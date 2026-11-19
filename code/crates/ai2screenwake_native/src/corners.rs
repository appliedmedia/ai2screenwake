// corners.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Minimal original-event deflection with revocable, operation-scoped authority.

use ai2screenwake_backend::contract::{
    Apply, CornerAuthorization, CornerFiltered, CornerLoss, Emitter, Error, Fault, Power,
    WeakEmitter,
};
use std::{
    sync::{
        Arc, Mutex, OnceLock,
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
    actions: [i64; 4],
    modifiers: [u64; 4],
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
            actions,
            modifiers,
            displays,
        })
    }

    /// Whether this validated configuration requires an installed input guard.
    pub fn required(&self) -> bool {
        self.actions.contains(&5)
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
                if self.actions[corner] == 5
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

/// Native permission result, distinguishing an actual request from a check.
pub enum PermissionOutcome {
    /// No request was attempted: already granted, denied or caller-ineligible.
    Checked(Result<(), Fault>),
    /// The eligible target-app request was attempted, whether granted or denied.
    Requested(Result<(), Fault>),
}

impl PermissionOutcome {
    /// Whether this operation consumed the owner's one-shot request allowance.
    pub fn request_attempted(&self) -> bool {
        matches!(self, Self::Requested(_))
    }

    /// Preserve the native permission result independently of request accounting.
    pub fn into_result(self) -> Result<(), Fault> {
        match self {
            Self::Checked(result) | Self::Requested(result) => result,
        }
    }
}

/// Injected public OS boundary; production supplies `system::SystemCorners`.
pub trait CornerApi: Send + Sync + 'static {
    /// Read and validate current configuration without requesting permissions.
    fn configuration(&self) -> Result<Configuration, Fault>;
    /// Check target-process filtering authority; only explicit On may request once.
    fn permission(&self, request: bool) -> PermissionOutcome;
    /// Install on this thread, returning only after source readiness is verified.
    fn install(
        &self,
        configuration: &Configuration,
        callback: CornerCallback,
    ) -> Result<Box<dyn CornerRegistration>, Fault>;
}

/// Thread-confined source; all calls must be bounded and stop releases it once.
pub trait CornerRegistration {
    /// Recheck current permission, configuration and stationary pointer at activation.
    fn validate_activation(&mut self) -> Result<(), Fault>;
    /// Pump at most 5ms and validate live permission/configuration/topology.
    fn poll(&mut self) -> Result<(), Fault>;
    /// Disable, remove, invalidate and release the owned source on this thread.
    fn stop(&mut self);
}

struct Authority {
    revision: u64,
    enabled: bool,
    retired: bool,
    activated: bool,
}

struct Shared {
    request: Apply,
    configuration: Configuration,
    authority: Mutex<Authority>,
    live: AtomicBool,
    stop: AtomicBool,
    entry_reported: AtomicBool,
    started: AtomicBool,
    notifications: SyncSender<Notice>,
    first_loss: OnceLock<Fault>,
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
        let mut result = None;
        self.deflect(point, |point| result = Some(point));
        result
    }

    /// Modify the original event while holding the same short revocation fence.
    ///
    /// The closure must only update that owned event; it must not dispatch or wait.
    pub fn deflect(&self, point: Point, apply: impl FnOnce(Point)) -> bool {
        if !self.0.live.load(Ordering::Acquire) || self.0.stop.load(Ordering::Acquire) {
            return false;
        }
        let Ok(authority) = self.0.authority.try_lock() else {
            return false;
        };
        if !authority.enabled || authority.retired {
            return false;
        }
        let Some((point, corner)) = self.0.configuration.deflect(point) else {
            self.0.entry_reported.store(false, Ordering::Release);
            return false;
        };
        if !self.0.live.load(Ordering::Acquire) {
            return false;
        }
        apply(point);
        // The authority lock linearizes filtering against Off/source retirement.
        if !self.0.entry_reported.swap(true, Ordering::AcqRel)
            && self
                .0
                .notifications
                .try_send(Notice::Filtered(corner))
                .is_err()
        {
            self.0.live.store(false, Ordering::Release);
        }
        true
    }

    /// A timeout, user disable, permission loss or changed config never re-enables.
    pub fn lost(&self, failure: Fault) {
        let _ = self.0.first_loss.set(failure);
        if self.0.live.swap(false, Ordering::AcqRel) {
            let _ = self
                .0
                .notifications
                .try_send(Notice::Lost(self.0.failure()));
        }
    }
}

impl Shared {
    /// Keep a specific first cause even if delivery observes loss before its notice.
    fn failure(&self) -> Fault {
        self.first_loss
            .get()
            .cloned()
            .unwrap_or_else(|| fault("corner_source_lost"))
    }
}

/// One operation-owned source; an expired owner cannot be retained by callbacks.
pub struct CornerGuard {
    shared: Arc<Shared>,
    source: Worker,
    delivery: Worker,
    activation: SyncSender<Activation>,
}

struct Activation {
    authorization: CornerAuthorization,
    reply: SyncSender<Result<(), Fault>>,
}

struct Worker {
    done: Receiver<Result<(), Fault>>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    fn spawn(
        name: &str,
        run: impl FnOnce() -> Result<(), Fault> + Send + 'static,
    ) -> Result<Self, Fault> {
        let (send, done) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let _ = send.send(run());
            })
            .map_err(|_| fault("corner_thread_failed"))?;
        Ok(Self {
            done,
            handle: Some(handle),
        })
    }

    fn finish(&mut self, deadline: Instant) -> Result<(), Fault> {
        let Some(handle) = self.handle.as_ref() else {
            return Ok(());
        };
        if handle.thread().id() == thread::current().id() {
            self.handle.take();
            return Ok(());
        }
        let result = self
            .done
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| fault("corner_stop_timeout"))?;
        let handle = self.handle.take().expect("checked guard worker");
        if handle.is_finished() && handle.join().is_err() {
            return Err(fault("corner_thread_failed"));
        }
        result
    }
}

impl CornerGuard {
    /// Prepare an initially unauthorized source before native Applied/On is emitted.
    pub fn start(
        api: Arc<dyn CornerApi>,
        request: Apply,
        events: &Emitter,
        permission_requested: &mut bool,
    ) -> Result<Self, Fault> {
        let configuration = api.configuration()?;
        if configuration.required() {
            let permission = api.permission(!*permission_requested);
            *permission_requested |= permission.request_attempted();
            permission.into_result()?;
        }
        let (send, receive) = mpsc::sync_channel(32);
        let shared = Arc::new(Shared {
            request,
            configuration,
            authority: Mutex::new(Authority {
                revision: 0,
                enabled: false,
                retired: false,
                activated: false,
            }),
            live: AtomicBool::new(true),
            stop: AtomicBool::new(false),
            entry_reported: AtomicBool::new(false),
            started: AtomicBool::new(false),
            notifications: send,
            first_loss: OnceLock::new(),
        });
        let callback = CornerCallback(shared.clone());
        let (ready_send, ready) = mpsc::sync_channel(1);
        let (activation, activations) = mpsc::sync_channel(1);
        let provenance = events.downgrade();
        let deliver_shared = shared.clone();
        let mut delivery = Worker::spawn("screenwake-corner-events", move || {
            deliver(deliver_shared, provenance, receive)
        })?;
        let worker_shared = shared.clone();
        let source = match Worker::spawn("screenwake-corners", move || {
            run(api, callback, worker_shared, ready_send, activations)
        }) {
            Ok(source) => source,
            Err(failure) => {
                shared.stop.store(true, Ordering::Release);
                if let Err(cleanup) = delivery.finish(Instant::now() + STOP_LIMIT) {
                    events.error::<Error::Native::Corners::Occurred>(cleanup);
                }
                return Err(failure);
            }
        };
        let mut guard = Self {
            shared,
            source,
            delivery,
            activation,
        };
        match ready.recv_timeout(START_LIMIT) {
            Ok(Ok(())) => Ok(guard),
            Ok(Err(failure)) => {
                if let Err(cleanup) = guard.stop() {
                    events.error::<Error::Native::Corners::Occurred>(cleanup);
                }
                Err(failure)
            }
            Err(_) => {
                guard.revoke();
                if let Err(cleanup) = guard.stop() {
                    events.error::<Error::Native::Corners::Occurred>(cleanup);
                }
                Err(fault("corner_start_timeout"))
            }
        }
    }

    /// Adopt only a newer owner revision for this exact installed acquisition.
    pub fn authorize(&self, authorization: &CornerAuthorization) {
        if let Ok(mut authority) = self.shared.authority.lock() {
            if authorization.revision > authority.revision {
                authority.revision = authorization.revision;
                authority.enabled = authority.activated
                    && !authority.retired
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

    /// Activate only after native resource acquisition and exact owner admission.
    pub fn activate(&self, authorization: &CornerAuthorization) -> Result<(), Fault> {
        let (reply, result) = mpsc::sync_channel(1);
        self.activation
            .try_send(Activation {
                authorization: authorization.clone(),
                reply,
            })
            .map_err(|_| fault("corner_activation_unavailable"))?;
        result
            .recv_timeout(START_LIMIT)
            .map_err(|_| fault("corner_activation_timeout"))?
    }

    /// Request bounded same-thread teardown without joining an unfinished worker.
    pub fn stop(&mut self) -> Result<(), Fault> {
        self.revoke();
        self.shared.stop.store(true, Ordering::Release);
        let deadline = Instant::now() + STOP_LIMIT;
        let source = self.source.finish(deadline);
        let delivery = self.delivery.finish(deadline);
        source.and(delivery)
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
    ready: SyncSender<Result<(), Fault>>,
    activations: Receiver<Activation>,
) -> Result<(), Fault> {
    let mut registration = match api.install(&shared.configuration, callback.clone()) {
        Ok(registration) => registration,
        Err(failure) => {
            shared.live.store(false, Ordering::Release);
            let _ = ready.send(Err(failure.clone()));
            return Ok(());
        }
    };
    shared.started.store(true, Ordering::Release);
    if ready.send(Ok(())).is_err() {
        shared.stop.store(true, Ordering::Release);
    }
    while !shared.stop.load(Ordering::Acquire) && shared.live.load(Ordering::Acquire) {
        while let Ok(activation) = activations.try_recv() {
            let result = registration.validate_activation().and_then(|()| {
                let mut authority = shared
                    .authority
                    .lock()
                    .map_err(|_| fault("corner_authority_poisoned"))?;
                let authorization = &activation.authorization;
                if authority.retired
                    || shared.stop.load(Ordering::Acquire)
                    || !shared.live.load(Ordering::Acquire)
                    || authorization.revision < authority.revision
                    || authorization.request.as_ref() != Some(&shared.request)
                {
                    return Err(fault("corner_activation_retired"));
                }
                authority.revision = authorization.revision;
                authority.activated = true;
                authority.enabled = true;
                Ok(())
            });
            let _ = activation.reply.send(result);
        }
        if let Err(failure) = registration.poll() {
            callback.lost(failure);
        }
    }
    shared.live.store(false, Ordering::Release);
    registration.stop();
    Ok(())
}

fn deliver(
    shared: Arc<Shared>,
    events: WeakEmitter,
    receive: Receiver<Notice>,
) -> Result<(), Fault> {
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if events.upgrade().is_none() {
            shared.live.store(false, Ordering::Release);
            return Ok(());
        }
        let notice = match receive.recv_timeout(Duration::from_millis(5)) {
            Ok(notice) => notice,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(fault("corner_delivery_lost")),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if shared.started.load(Ordering::Acquire) && !shared.live.load(Ordering::Acquire) {
                    Notice::Lost(shared.failure())
                } else {
                    continue;
                }
            }
        };
        let Some(events) = events.upgrade() else {
            return Ok(());
        };
        match notice {
            Notice::Filtered(corner) => {
                events.emit::<Power::Corners::Guard::Filtered>(CornerFiltered {
                    request: shared.request.clone(),
                    corner,
                })
            }
            Notice::Lost(failure) => {
                events.error::<Error::Native::Corners::Occurred>(failure.clone());
                events.emit::<Power::Corners::Guard::Lost>(CornerLoss {
                    request: shared.request.clone(),
                    fault: failure,
                });
                return Ok(());
            }
        }
    }
}
// end corners.rs
