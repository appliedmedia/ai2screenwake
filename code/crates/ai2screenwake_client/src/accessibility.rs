// accessibility.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Explicit setup policy; the native shell only executes effects and renders this copy.

use crate::contract::{Apply, Emitter, Error, Fault, Power, Ui};
use std::{
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Duration,
};

struct Schedule(Arc<(Mutex<bool>, Condvar)>);

impl Schedule {
    fn start(session: u64, events: &Emitter) -> Result<Self, Fault> {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = stop.clone();
        let origin = events.downgrade();
        thread::Builder::new().name("screenwake-setup-clock".into()).spawn(move || {
            let mut stopped = signal.0.lock().expect("setup clock poisoned");
            loop {
                let (state, wait) = signal.1.wait_timeout_while(stopped, Duration::from_secs(1), |stop| !*stop)
                    .expect("setup clock poisoned");
                stopped = state;
                if *stopped { return; }
                if wait.timed_out() {
                    drop(stopped);
                    let Some(events) = origin.upgrade() else { return; };
                    events.emit::<Ui::Accessibility::Setup::Tick>(Ui::SetupSession { session });
                    drop(events);
                    stopped = signal.0.lock().expect("setup clock poisoned");
                }
            }
        }).map_err(|_| Fault::new("accessibility_clock_unavailable",
            "Automatic Accessibility observation could not start. Close and reopen setup to retry.", true))?;
        Ok(Self(stop))
    }
}

impl Drop for Schedule {
    fn drop(&mut self) {
        *self.0.0.lock().expect("setup clock poisoned") = true;
        self.0.1.notify_one();
    }
}

/// One owner-scoped setup lifetime and its bounded outstanding observations.
#[derive(Default)]
pub(crate) struct Setup {
    pub view: Ui::SetupPresentation,
    opening: bool,
    checking: bool,
    settings_failed: bool,
    pub refresh: Option<Apply>,
    attempted: Option<Apply>,
    pub ready: Option<Apply>,
    schedule: Option<Schedule>,
}

impl Setup {
    /// Open only on explicit selection; revisions also fit the bounded input mailbox.
    pub fn activate(&mut self, events: &Emitter) {
        if self.view.visible {
            return;
        }
        let Some(session) = self
            .view
            .session
            .checked_add(1)
            .filter(|n| *n <= u64::MAX >> 2)
        else {
            events.error::<Error::Native::Corners::Occurred>(Fault::new(
                "accessibility_setup_exhausted",
                "Setup window identity is exhausted.",
                false,
            ));
            return;
        };
        *self = Self {
            view: Ui::SetupPresentation {
                session,
                visible: true,
                message: "Checking Accessibility access. Dragging alone does not grant permission."
                    .into(),
                ..Default::default()
            },
            opening: true,
            ..Default::default()
        };
        let mut view = self.view.clone();
        view.open_settings = true;
        events.emit::<Ui::Accessibility::Setup::Changed>(view);
    }

    /// All native completions must belong to the still-open explicit lifetime.
    pub fn current(&self, session: u64) -> bool {
        self.view.visible && self.view.session == session
    }

    /// Workspace refusal is optional and appears only in the requested setup window.
    pub fn completed(&mut self, result: Ui::SetupCompletion, events: &Emitter) {
        if !self.current(result.session) || !self.opening {
            return;
        }
        self.opening = false;
        if !result.opened {
            self.settings_failed = true;
            self.fail(Fault::new(
                "accessibility_settings_open_failed",
                "System Settings could not be opened. Open Privacy & Security > Accessibility manually.",
                true,
            ), events);
        }
        self.check(result.session, events);
        match Schedule::start(result.session, events) {
            Ok(schedule) => self.schedule = Some(schedule),
            Err(fault) => self.fail(fault, events),
        }
    }

    /// At most one OS observation is outstanding; closing stops further checks.
    pub fn check(&mut self, session: u64, events: &Emitter) {
        if self.current(session) && !self.opening && !self.checking {
            self.checking = true;
            events.emit::<Power::Corners::Access::Check>(Ui::SetupSession { session });
        }
    }

    /// Accept actual OS trust only; no drag or settings-open outcome authorizes filtering.
    pub fn observed(&mut self, result: Ui::AccessibilityObservation, events: &Emitter) -> bool {
        if !self.current(result.session) || !self.checking {
            return false;
        }
        self.checking = false;
        self.view.trusted = Some(result.trusted);
        if !result.trusted {
            self.view.guard_ready = false;
            self.ready = None;
            self.refresh = None;
            self.attempted = None;
            self.view.message = result.fault.map_or_else(
                || "Accessibility is not enabled. Enable AI2ScreenWake in the panel; macOS may require authentication.".into(),
                |fault| fault.message,
            );
        } else if !self.view.guard_ready {
            self.view.message =
                "Accessibility is enabled. No optional corner filter is confirmed active.".into();
        }
        if self.settings_failed && !result.trusted {
            self.view.message = format!(
                "System Settings could not be opened. Open Privacy & Security > Accessibility manually. {}",
                self.view.message
            );
        }
        self.publish(events);
        result.trusted
    }

    /// Retry an optional guard once per acquisition/grant transition, never on every timer tick.
    pub fn begin_refresh(&mut self, request: &Apply) -> bool {
        if self.attempted.as_ref() == Some(request) {
            return false;
        }
        self.attempted = Some(request.clone());
        self.refresh = Some(request.clone());
        true
    }

    /// Retire pending effects without changing ordinary protection.
    pub fn dismiss(&mut self, events: &Emitter) {
        if self.view.visible {
            self.schedule = None;
            self.view.visible = false;
            self.view.guard_ready = false;
            self.ready = None;
            self.opening = false;
            self.checking = false;
            self.refresh = None;
            self.publish(events);
        }
    }

    /// Native admission loss invalidates readiness without changing granted trust or power.
    pub fn invalidate(&mut self, events: &Emitter) {
        self.refresh = None;
        self.ready = None;
        if self.view.guard_ready {
            self.view.guard_ready = false;
            self.view.message = "No optional corner filter is confirmed active. Native protection admission was lost; explicit protection retry may be required.".into();
            self.publish(events);
        }
    }

    /// Publish current setup state through the same replayable Actions stream.
    pub fn publish(&self, events: &Emitter) {
        events.emit::<Ui::Accessibility::Setup::Changed>(self.view.clone());
    }

    /// Keep optional errors visible here, without adding ordinary menu attention.
    pub fn fail(&mut self, fault: Fault, events: &Emitter) {
        events.error::<Error::Native::Corners::Occurred>(fault.clone());
        self.view.message = fault.message;
        self.publish(events);
    }
}
// end accessibility.rs
