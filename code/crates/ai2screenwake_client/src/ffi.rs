// ffi.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Generated native intent/observation port and Rust-owned menu rendering.

use crate::contract::{
    CorrelationId, Emitter, Error, Fault, KvEnvelope, Log, ObservationSink, Power, Registry, State,
    Status, Subscription, Ui,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Typed native inputs; the generated shell never constructs a genus or reads state.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum NativeIntent {
    /// The owner, not Swift, chooses the opposite latest intent.
    Toggle,
    /// Read the existing canonical log through its typed query.
    ShowLog,
    /// Dismiss only the auxiliary read-only presentation.
    CloseLog,
    /// Request existing explicit native resource cleanup.
    Quit,
    /// Report a native rendering fault through canonical errors.
    RendererFailed,
}

/// One observed stream, containing only canonical rendering attributes.
#[uniffi::export(callback_interface)]
pub trait NativeObserver: Send + Sync {
    /// Apply these maps on the native main thread; never fold domain state in Swift.
    fn observed(&self, text: HashMap<String, String>, flags: HashMap<String, bool>);
}

struct RenderSink(Arc<dyn NativeObserver>);

impl ObservationSink for RenderSink {
    fn observed(&self, envelope: KvEnvelope) {
        match envelope.payload::<Ui::Menu::Render>() {
            Ok(Some(render)) => self.0.observed(render.text, render.flags),
            Ok(None) => {}
            Err(_) => {
                unreachable!("only canonical internally encoded render events reach this sink")
            }
        }
    }
}

#[derive(Default)]
struct Presentation {
    status: Option<Status>,
    error: String,
    log: Option<String>,
    quitting: Option<CorrelationId>,
}

impl Presentation {
    fn projection(&self, terminate: bool) -> Ui::Projection {
        let (phase, icon, badge) = match self.status.as_ref().map(|s| (s.state, s.is_on)) {
            Some((State::On, Some(true))) => ("On", "menu-on", ""),
            Some((State::Off, Some(false))) => ("Off", "menu-off", ""),
            Some((State::Pending, _)) => ("Pending", "menu-off", "..."),
            None if self.error.is_empty() => ("Pending", "menu-off", "..."),
            _ => ("Error", "menu-off", "!"),
        };
        let mut text = HashMap::from([
            ("title".into(), "ScreenWake".into()),
            ("icon_asset".into(), icon.into()),
            ("badge".into(), if self.error.is_empty() { badge } else { "!" }.into()),
            ("status".into(), format!("Status: {phase}")),
            ("accessibility".into(), format!("ScreenWake protection: {phase}")),
            ("show_log".into(), "Show Log...".into()),
            ("quit".into(), "Quit".into()),
            ("log_title".into(), "ScreenWake Log".into()),
            ("log_text".into(), self.log.clone().unwrap_or_default()),
            ("error".into(), self.error.clone()),
            ("tooltip".into(), "Idle system/display sleep protection. Automatic screensaver protection is unverified.".into()),
        ]);
        if self
            .status
            .as_ref()
            .is_some_and(|s| s.state == State::Error)
            && self.error.is_empty()
        {
            text.insert(
                "error".into(),
                "Protection could not be confirmed. Try again.".into(),
            );
        }
        Ui::Projection {
            text,
            flags: HashMap::from([
                (
                    "toggle_enabled".into(),
                    self.status.is_some() && self.quitting.is_none(),
                ),
                ("log_visible".into(), self.log.is_some()),
                ("quit_enabled".into(), self.quitting.is_none()),
                ("terminate".into(), terminate),
            ]),
        }
    }

    fn render(&self, events: &Emitter) {
        events.emit::<Ui::Menu::Render>(self.projection(false));
    }

    fn fault(&mut self, _fault: Fault, events: &Emitter) {
        self.error = "ScreenWake could not complete that action. Try again.".into();
        self.render(events);
    }
}

/// Subscribe before any input is emitted; dropping this token unregisters callbacks.
pub fn observe_menu(bus: &Registry, observer: Arc<dyn NativeObserver>) -> Subscription {
    let subscription = bus.subscribe(Arc::new(RenderSink(observer)));
    let presentation = Arc::new(Mutex::new(Presentation::default()));
    let changed = presentation.clone();
    bus.observe_action::<Power::Session::Changed>(move |status, events| {
        let mut view = changed.lock().expect("menu projection poisoned");
        view.status = Some(status);
        view.render(events);
    });
    let snapshot = presentation.clone();
    bus.observe_action::<Power::Session::Snapshot>(move |status, events| {
        let mut view = snapshot.lock().expect("menu projection poisoned");
        view.status = Some(status);
        view.render(events);
    });
    let log = presentation.clone();
    bus.observe_action::<Log::Session::Snapshot>(move |snapshot, events| {
        let omitted = snapshot.entries.len().saturating_sub(200);
        let mut text = if omitted > 0 {
            format!("{omitted} earlier log entries are not shown.\n")
        } else {
            String::new()
        };
        for entry in snapshot.entries.iter().skip(omitted) {
            use std::fmt::Write;
            writeln!(text, "{}  {}", entry.sequence, entry.display)
                .expect("String writes cannot fail");
        }
        let mut view = log.lock().expect("menu projection poisoned");
        view.log = Some(text);
        view.render(events);
    });
    let closed = presentation.clone();
    bus.observe_action::<Ui::Log::Dismissed>(move |_, events| {
        let mut view = closed.lock().expect("menu projection poisoned");
        if view.log.take().is_some() {
            view.render(events);
        }
    });
    let quit = presentation.clone();
    bus.observe_action::<Power::Session::Quit>(move |_, events| {
        let mut view = quit.lock().expect("menu projection poisoned");
        view.quitting = Some(events.correlation());
        view.render(events);
    });
    let settled = presentation.clone();
    bus.observe_action::<Power::Session::Settled>(move |settlement, events| {
        let mut view = settled.lock().expect("menu projection poisoned");
        if view.quitting.as_ref() == Some(&settlement.correlation) {
            view.quitting = None;
            if let Some(error) = settlement.result.error {
                view.fault(error, events);
            } else {
                view.render(events);
            }
        }
    });
    macro_rules! error_observer {
        ($action:ty) => {{
            let errors = presentation.clone();
            bus.observe_action::<$action>(move |failure, events| {
                errors
                    .lock()
                    .expect("menu projection poisoned")
                    .fault(failure, events);
            });
        }};
    }
    error_observer!(Error::Native::Assertions::Occurred);
    error_observer!(Error::Native::Activity::Occurred);
    error_observer!(Error::Dispatch::Validation::Occurred);
    error_observer!(Error::Ipc::Protocol::Occurred);
    error_observer!(Error::Log::Storage::Occurred);
    subscription
}
// end ffi.rs
