// corners.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Minimal original-event deflection with revocable, operation-scoped authority.

use ai2screenwake_backend::contract::{
    Apply, CornerAuthorization, CornerFiltered, CornerLoss, Emitter, Error, Fault, Power,
    WeakEmitter,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
#[path = "corners/system.rs"]
pub mod system;

const START_LIMIT: Duration = Duration::from_secs(1);
const STOP_LIMIT: Duration = Duration::from_millis(500);
const EDGE: f64 = 2.0;

fn fault(code: &str) -> Fault {
    Fault::new(
        code,
        "Manual screen saver corner protection is unavailable.",
        true,
    )
}

/// Quartz global coordinates, with the main display's upper-left as origin.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Point {
    /// Horizontal logical display coordinate.
    pub x: f64,
    /// Vertical logical display coordinate.
    pub y: f64,
}

/// Active display bounds in the same Quartz coordinate system as the event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    /// Upper-left origin, including negative secondary-display origins.
    pub origin: Point,
    /// Logical width; no backing-scale multiplication is performed.
    pub width: f64,
    /// Logical height; Quartz already accounts for display rotation.
    pub height: f64,
}

/// Validated saved actions: top-left, top-right, bottom-left, bottom-right.
#[derive(Clone, Debug, PartialEq)]
pub struct Configuration {
    savers: [bool; 4],
    displays: Vec<Display>,
}

impl Configuration {
    /// Decode the corroborated Dock preference schema; unknown values fail closed.
    ///
    /// Only value 5 is Start Screen Saver. Value 13 (Lock Screen) is never selected.
    /// Modifier masks accept only public shift/control/option/command bits. We
    /// deflect entry regardless of modifiers, so stationary modifier changes
    /// cannot activate a saver without first moving outside its corner.
    pub fn from_preferences(
        actions: [i64; 4],
        modifiers: [u64; 4],
        displays: Vec<Display>,
    ) -> Result<Self, Fault> {
        if actions
            .iter()
            .any(|action| !matches!(action, 0..=7 | 10..=14))
            || modifiers.iter().any(|mask| mask & !0x1e0000 != 0)
            || displays.is_empty()
            || displays.len() > 32
            || displays.iter().any(|display| {
                !display.origin.x.is_finite()
                    || !display.origin.y.is_finite()
                    || !display.width.is_finite()
                    || !display.height.is_finite()
                    || display.width <= EDGE * 3.0
                    || display.height <= EDGE * 3.0
                    || !(display.origin.x + display.width).is_finite()
                    || !(display.origin.y + display.height).is_finite()
            })
        {
            return Err(fault("corner_configuration_invalid"));
        }
        Ok(Self {
            savers: actions.map(|action| action == 5),
            displays,
        })
    }

    /// Whether this validated configuration requires an installed input guard.
    pub fn required(&self) -> bool {
        self.savers.iter().any(|saver| *saver)
    }

    /// Move only an original event inside a configured two-point corner square.
    pub fn deflect(&self, point: Point) -> Option<(Point, u8)> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return None;
        }
        for display in &self.displays {
            let x = point.x - display.origin.x;
            let y = point.y - display.origin.y;
            if x < 0.0 || y < 0.0 || x >= display.width || y >= display.height {
                continue;
            }
            for corner in 0..4 {
                let right = corner % 2 == 1;
                let bottom = corner >= 2;
                if self.savers[corner]
                    && if right {
                        x >= display.width - EDGE
                    } else {
                        x < EDGE
                    }
                    && if bottom {
                        y >= display.height - EDGE
                    } else {
                        y < EDGE
                    }
                {
                    return Some((
                        Point {
                            x: display.origin.x
                                + if right {
                                    display.width - EDGE - 1.0
                                } else {
                                    EDGE + 1.0
                                },
                            y: display.origin.y
                                + if bottom {
                                    display.height - EDGE - 1.0
                                } else {
                                    EDGE + 1.0
                                },
                        },
                        corner as u8,
                    ));
                }
            }
        }
        None
    }
}

/// Injected public OS boundary; production supplies `system::SystemCorners`.
pub trait CornerApi: Send + Sync + 'static {
    /// Read and validate current configuration without requesting permissions.
    fn configuration(&self) -> Result<Configuration, Fault>;
    /// Check target-process filtering authority; only explicit On may request once.
    fn permission(&self, request: bool) -> Result<(), Fault>;
    /// Install on this thread, returning only after source readiness is verified.
    fn install(
        &self,
        configuration: &Configuration,
        callback: CornerCallback,
    ) -> Result<Box<dyn CornerRegistration>, Fault>;
}

/// Thread-confined source; all calls must be bounded and stop releases it once.
pub trait CornerRegistration {
    /// Pump at most 5ms and validate live permission/configuration/topology.
    fn poll(&mut self) -> Result<(), Fault>;
    /// Disable, remove, invalidate and release the owned source on this thread.
    fn stop(&mut self);
}

struct Authority {
    revision: u64,
    enabled: bool,
    retired: bool,
}

struct Shared {
    request: Apply,
    configuration: Configuration,
    authority: Mutex<Authority>,
    live: AtomicBool,
    stop: AtomicBool,
    entry_reported: AtomicBool,
    notifications: SyncSender<Notice>,
}

enum Notice {
    Filtered(u8),
    Lost(Fault),
}

/// Nonblocking native callback policy. No bus, UI, preference or disk calls.
#[derive(Clone)]
pub struct CornerCallback(Arc<Shared>);

impl CornerCallback {
    /// Deflect a movement/drag event only while this exact source is authorized.
    pub fn filter(&self, point: Point) -> Option<Point> {
        if !self.0.live.load(Ordering::Acquire) || self.0.stop.load(Ordering::Acquire) {
            return None;
        }
        let Ok(authority) = self.0.authority.try_lock() else {
            return None;
        };
        if !authority.enabled || authority.retired {
            return None;
        }
        let Some((point, corner)) = self.0.configuration.deflect(point) else {
            self.0.entry_reported.store(false, Ordering::Release);
            return None;
        };
        // The authority lock linearizes filtering against Off/source retirement.
        if !self.0.entry_reported.swap(true, Ordering::AcqRel)
            && self
                .0
                .notifications
                .try_send(Notice::Filtered(corner))
                .is_err()
        {
            self.0.live.store(false, Ordering::Release);
            return None;
        }
        Some(point)
    }

    /// A timeout, user disable, permission loss or changed config never re-enables.
    pub fn lost(&self, failure: Fault) {
        if self.0.live.swap(false, Ordering::AcqRel) {
            let _ = self.0.notifications.try_send(Notice::Lost(failure));
        }
    }
}

/// One operation-owned source; an expired owner cannot be retained by callbacks.
pub struct CornerGuard {
    shared: Arc<Shared>,
    done: Receiver<Result<(), Fault>>,
    worker: Option<JoinHandle<()>>,
}

impl CornerGuard {
    /// Prepare an initially unauthorized source before native Applied/On is emitted.
    pub fn start(
        api: Arc<dyn CornerApi>,
        request: Apply,
        events: &Emitter,
        may_prompt: bool,
    ) -> Result<Option<Self>, Fault> {
        let configuration = api.configuration()?;
        if configuration.required() {
            api.permission(may_prompt)?;
        }
        let (send, receive) = mpsc::sync_channel(32);
        let shared = Arc::new(Shared {
            request,
            configuration,
            authority: Mutex::new(Authority {
                revision: 0,
                enabled: false,
                retired: false,
            }),
            live: AtomicBool::new(true),
            stop: AtomicBool::new(false),
            entry_reported: AtomicBool::new(false),
            notifications: send,
        });
        let callback = CornerCallback(shared.clone());
        let worker_shared = shared.clone();
        let (ready_send, ready) = mpsc::sync_channel(1);
        let (done_send, done) = mpsc::sync_channel(1);
        let events = events.downgrade();
        let worker = thread::Builder::new()
            .name("screenwake-corners".into())
            .spawn(move || {
                let result = run(api, callback, worker_shared, events, receive, ready_send);
                let _ = done_send.send(result);
            })
            .map_err(|_| fault("corner_thread_failed"))?;
        let mut guard = Self {
            shared,
            done,
            worker: Some(worker),
        };
        match ready.recv_timeout(START_LIMIT) {
            Ok(Ok(())) => Ok(Some(guard)),
            Ok(Err(failure)) => {
                let _ = guard.stop();
                Err(failure)
            }
            Err(_) => {
                guard.revoke();
                let _ = guard.stop();
                Err(fault("corner_start_timeout"))
            }
        }
    }

    /// Adopt only a newer owner revision for this exact installed acquisition.
    pub fn authorize(&self, authorization: &CornerAuthorization) {
        if let Ok(mut authority) = self.shared.authority.lock() {
            if authorization.revision > authority.revision {
                authority.revision = authorization.revision;
                authority.enabled = !authority.retired
                    && self.shared.live.load(Ordering::Acquire)
                    && authorization.request.as_ref() == Some(&self.shared.request);
            }
        }
    }

    /// Retire this source permanently before Off/Quit starts releasing resources.
    pub fn revoke(&self) {
        if let Ok(mut authority) = self.shared.authority.lock() {
            authority.enabled = false;
            authority.retired = true;
        }
    }

    /// Ready means actually installed and not subsequently disabled or retired.
    pub fn ready(&self) -> bool {
        self.shared.live.load(Ordering::Acquire) && !self.shared.stop.load(Ordering::Acquire)
    }

    /// Activate only after native resource acquisition and exact owner admission.
    pub fn activate(&self, authorization: &CornerAuthorization) -> Result<(), Fault> {
        self.authorize(authorization);
        let authority = self
            .shared
            .authority
            .lock()
            .map_err(|_| fault("corner_authority_poisoned"))?;
        if !self.ready() || !authority.enabled || authority.retired {
            return Err(fault("corner_activation_retired"));
        }
        Ok(())
    }

    /// Request bounded same-thread teardown without joining an unfinished worker.
    pub fn stop(&mut self) -> Result<(), Fault> {
        self.revoke();
        self.shared.stop.store(true, Ordering::Release);
        let Some(worker) = self.worker.as_ref() else {
            return Ok(());
        };
        if worker.thread().id() == thread::current().id() {
            return Ok(());
        }
        let _source_outcome = self
            .done
            .recv_timeout(STOP_LIMIT)
            .map_err(|_| fault("corner_stop_timeout"))?;
        if let Some(worker) = self.worker.take() {
            if worker.is_finished() && worker.join().is_err() {
                return Err(fault("corner_thread_failed"));
            }
        }
        Ok(())
    }
}

impl Drop for CornerGuard {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run(
    api: Arc<dyn CornerApi>,
    callback: CornerCallback,
    shared: Arc<Shared>,
    events: WeakEmitter,
    receive: Receiver<Notice>,
    ready: SyncSender<Result<(), Fault>>,
) -> Result<(), Fault> {
    let mut registration = match api.install(&shared.configuration, callback.clone()) {
        Ok(registration) => registration,
        Err(failure) => {
            shared.live.store(false, Ordering::Release);
            let _ = ready.send(Err(failure.clone()));
            return Err(failure);
        }
    };
    if ready.send(Ok(())).is_err() {
        shared.stop.store(true, Ordering::Release);
    }
    let mut last_report = None;
    let result = loop {
        if shared.stop.load(Ordering::Acquire) {
            break Ok(());
        }
        if let Err(failure) = registration.poll() {
            callback.lost(failure);
        }
        while let Ok(notice) = receive.try_recv() {
            match notice {
                Notice::Filtered(corner) => {
                    if last_report
                        .is_none_or(|last: Instant| last.elapsed() >= Duration::from_secs(1))
                    {
                        if let Some(events) = events.upgrade() {
                            events.emit::<Power::Corners::Guard::Filtered>(CornerFiltered {
                                request: shared.request.clone(),
                                corner,
                            });
                        }
                        last_report = Some(Instant::now());
                    }
                }
                Notice::Lost(failure) => {
                    if let Some(events) = events.upgrade() {
                        events.error::<Error::Native::Assertions::Occurred>(failure.clone());
                        events.emit::<Power::Corners::Guard::Lost>(CornerLoss {
                            request: shared.request.clone(),
                            fault: failure.clone(),
                        });
                    }
                    registration.stop();
                    return Err(failure);
                }
            }
        }
        if !shared.live.load(Ordering::Acquire) {
            let failure = fault("corner_source_lost");
            if let Some(events) = events.upgrade() {
                events.error::<Error::Native::Assertions::Occurred>(failure.clone());
                events.emit::<Power::Corners::Guard::Lost>(CornerLoss {
                    request: shared.request.clone(),
                    fault: failure.clone(),
                });
            }
            break Err(failure);
        }
    };
    shared.live.store(false, Ordering::Release);
    registration.stop();
    result
}
// end corners.rs
