// ffi.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Generated native intent/observation port and Rust-owned menu rendering.

use crate::contract::{
    ControlResult, CorrelationId, Emitter, Empty, Error, Fault, KvEnvelope, Log, ObservationSink,
    Power, Registry, RegistryBuilder, State, Status, Subscription, Ui, WeakEmitter,
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::Ordering,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

mod shutdown;
use shutdown::{Signals, deliver_after_disposal, observe_cleanup, shutdown_retry};

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

type TerminalDelivery = Arc<(Mutex<Option<Ui::Projection>>, Condvar)>;

struct RenderSink {
    observer: Arc<dyn NativeObserver>,
    terminal: Option<TerminalDelivery>,
}

impl ObservationSink for RenderSink {
    fn observed(&self, envelope: KvEnvelope) {
        match envelope.payload::<Ui::Menu::Render>() {
            Ok(Some(render)) => {
                if render.flags.get("terminate") == Some(&true) {
                    if let Some(terminal) = &self.terminal {
                        *terminal.0.lock().expect("terminal delivery poisoned") = Some(render);
                        terminal.1.notify_one();
                        return;
                    }
                }
                self.observer.observed(render.text, render.flags);
            }
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
    log_query: Option<CorrelationId>,
    quitting: Option<CorrelationId>,
    closing: bool,
    termination_failed: bool,
}

impl Presentation {
    fn observe_status(&mut self, status: Status, events: &Emitter) {
        if self
            .status
            .as_ref()
            .is_some_and(|current| status.revision < current.revision)
        {
            return;
        }
        self.status = Some(status);
        self.render(events);
    }

    fn projection(&self, terminate: bool) -> Ui::Projection {
        let (phase, icon, badge) = match self.status.as_ref().map(|s| (s.state, s.is_on)) {
            Some((State::On, Some(true))) => ("On", "menu-on", ""),
            Some((State::Off, Some(false))) => ("Off", "menu-off", ""),
            Some((State::Pending, _)) => ("Pending", "menu-off", "..."),
            None if self.error.is_empty() => ("Pending", "menu-off", "..."),
            _ => ("Error", "menu-off", "!"),
        };
        let error = if self
            .status
            .as_ref()
            .is_some_and(|s| s.state == State::Error)
            && self.error.is_empty()
        {
            "Protection could not be confirmed. Try again.".to_owned()
        } else {
            self.error.clone()
        };
        let accessibility = if error.is_empty() {
            format!("ScreenWake protection: {phase}")
        } else {
            format!("ScreenWake protection: {phase}. {error}")
        };
        let text = HashMap::from([
            ("title".into(), "ScreenWake".into()),
            ("icon_asset".into(), icon.into()),
            ("badge".into(), if error.is_empty() { badge } else { "!" }.into()),
            ("status".into(), format!("Status: {phase}")),
            ("accessibility".into(), accessibility),
            ("show_log".into(), "Show Log...".into()),
            ("open_menu".into(), "Open menu".into()),
            ("quit".into(), "Quit".into()),
            ("log_title".into(), "ScreenWake Log".into()),
            ("log_text".into(), self.log.clone().unwrap_or_default()),
            ("error".into(), error),
            ("tooltip".into(), "Idle system/display sleep protection. Automatic screensaver protection is unverified.".into()),
        ]);
        Ui::Projection {
            text,
            flags: HashMap::from([
                (
                    "toggle_enabled".into(),
                    self.status.is_some() && !self.closing,
                ),
                ("log_visible".into(), self.log.is_some()),
                ("activate_log".into(), false),
                ("quit_enabled".into(), self.quitting.is_none()),
                ("terminate".into(), terminate),
                ("termination_failed".into(), self.termination_failed),
            ]),
        }
    }

    fn render(&self, events: &Emitter) {
        events.emit::<Ui::Menu::Render>(self.projection(false));
    }

    fn fault(&mut self, failure: Fault, events: &Emitter) {
        if matches!(
            failure.fault_code.as_str(),
            "quit_timeout"
                | "quit_unconfirmed"
                | "ui_shutdown_failed"
                | "ui_terminal_observation_timeout"
        ) {
            self.quitting = None;
            self.termination_failed = true;
        }
        self.error = "Last action reported an error. Check Show Log.".into();
        self.render(events);
    }
}

/// Subscribe before any input is emitted; dropping this token unregisters callbacks.
pub fn observe_menu(bus: &Registry, observer: Arc<dyn NativeObserver>) -> Subscription {
    observe_menu_deferred(bus, observer, None)
}

fn observe_menu_deferred(
    bus: &Registry,
    observer: Arc<dyn NativeObserver>,
    terminal: Option<TerminalDelivery>,
) -> Subscription {
    let subscription = bus.subscribe(Arc::new(RenderSink { observer, terminal }));
    let presentation = Arc::new(Mutex::new(Presentation::default()));
    let changed = presentation.clone();
    bus.observe_action::<Power::Session::Changed>(move |status, events| {
        let mut view = changed.lock().expect("menu projection poisoned");
        view.observe_status(status, events);
    });
    let snapshot = presentation.clone();
    bus.observe_action::<Power::Session::Snapshot>(move |status, events| {
        let mut view = snapshot.lock().expect("menu projection poisoned");
        view.observe_status(status, events);
    });
    let requested = presentation.clone();
    bus.observe_action::<Log::Session::Query>(move |_, events| {
        requested
            .lock()
            .expect("menu projection poisoned")
            .log_query = Some(events.correlation());
    });
    let log = presentation.clone();
    bus.observe_action::<Log::Session::Snapshot>(move |snapshot, events| {
        let mut view = log.lock().expect("menu projection poisoned");
        if view.log_query.as_ref() != Some(&events.correlation()) {
            return;
        }
        let omitted = snapshot.entries.len().saturating_sub(200);
        let mut text = String::new();
        if snapshot.earlier_entries_omitted {
            text.push_str("Earlier action history is omitted from this view.\n");
        }
        if omitted > 0 {
            use std::fmt::Write;
            writeln!(
                text,
                "{omitted} additional recent log entries are not shown."
            )
            .expect("String writes cannot fail");
        }
        if snapshot.storage_error.is_some() {
            text.push_str("Some history could not be saved; this view may be incomplete.\n");
        }
        for entry in snapshot.entries.iter().skip(omitted) {
            use std::fmt::Write;
            writeln!(text, "{}  {}", entry.sequence, entry.display)
                .expect("String writes cannot fail");
        }
        if snapshot.storage_error.is_some() {
            view.error = "Some history could not be saved.".into();
        }
        view.log = Some(text);
        let mut projection = view.projection(false);
        projection.flags.insert("activate_log".into(), true);
        events.emit::<Ui::Menu::Render>(projection);
    });
    let closed = presentation.clone();
    bus.observe_action::<Ui::Log::Dismissed>(move |_, events| {
        let mut view = closed.lock().expect("menu projection poisoned");
        view.log_query = None;
        if view.log.take().is_some() {
            view.render(events);
        }
    });
    let quit = presentation.clone();
    bus.observe_action::<Power::Session::Quit>(move |_, events| {
        let mut view = quit.lock().expect("menu projection poisoned");
        view.closing = true;
        view.termination_failed = false;
        view.quitting = Some(events.correlation());
        view.render(events);
    });
    let settled = presentation.clone();
    bus.observe_action::<Power::Session::Settled>(move |settlement, events| {
        let mut view = settled.lock().expect("menu projection poisoned");
        if view.quitting.as_ref() == Some(&settlement.correlation) {
            if let Some(error) = settlement.result.error {
                view.quitting = None;
                view.termination_failed = true;
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
    error_observer!(Error::Telemetry::Configuration::Occurred);
    error_observer!(Error::Telemetry::Transport::Occurred);
    error_observer!(Error::Telemetry::Buffer::Occurred);
    subscription
}
/// Failure before an operational host thread exists; normal startup outcomes are observed.
#[derive(Debug, uniffi::Error)]
pub enum HostFailure {
    /// No state is available and no private owner has been admitted.
    Unavailable { message: String },
}

impl std::fmt::Display for HostFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for HostFailure {}

#[derive(Default)]
struct QuitTracking {
    correlation: Option<CorrelationId>,
    events: Option<WeakEmitter>,
    result: Option<ControlResult>,
    closed: bool,
}

/// The generated port owns only bounded input delivery, never a Swift-readable core object.
#[derive(uniffi::Object)]
pub struct NativeHost {
    commands: SyncSender<NativeIntent>,
    signals: Arc<Signals>,
    failures: Registry,
    _failed_observation: Subscription,
}

#[uniffi::export]
impl NativeHost {
    /// Register the observed stream before asynchronous OS admission or any native completion.
    #[uniffi::constructor]
    pub fn new(observer: Box<dyn NativeObserver>) -> Result<Arc<Self>, HostFailure> {
        let bootstrap =
            RegistryBuilder::default()
                .build()
                .map_err(|error| HostFailure::Unavailable {
                    message: error.to_string(),
                })?;
        let observer: Arc<dyn NativeObserver> = Arc::from(observer);
        let failed_observation = observe_menu(&bootstrap, observer.clone());
        bootstrap.emit::<Ui::Menu::Render>(Presentation::default().projection(false));
        let (commands, receiver) = mpsc::sync_channel(32);
        let signals = Arc::new(Signals::default());
        let worker_signals = signals.clone();
        let failure_bus = bootstrap.clone();
        let failure_port = bootstrap.clone();
        thread::Builder::new()
            .name("screenwake-native-host".into())
            .spawn(move || run_host(observer, bootstrap, receiver, worker_signals))
            .map_err(|_| {
                failure_bus.emit::<Error::Dispatch::Validation::Occurred>(Fault::new(
                    "ui_start_failed",
                    "Cannot start the native host worker.",
                    false,
                ));
                HostFailure::Unavailable {
                    message: "Cannot start the native host worker.".into(),
                }
            })?;
        Ok(Arc::new(Self {
            commands,
            signals,
            failures: failure_port,
            _failed_observation: failed_observation,
        }))
    }

    /// Fire-and-forget typed input; state, errors and exit instructions are observed.
    pub fn emit(&self, intent: NativeIntent) {
        match self.commands.try_send(intent) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.signals.overflow.store(true, Ordering::Release),
            Err(TrySendError::Disconnected(_)) => {
                self.signals.disposed.store(true, Ordering::Release);
                self.failures
                    .emit::<Error::Dispatch::Validation::Occurred>(Fault::new(
                        "ui_host_closed",
                        "The native host has stopped and cannot accept input.",
                        false,
                    ));
            }
        }
    }
}

impl Drop for NativeHost {
    fn drop(&mut self) {
        // Never block a Swift deinit or foreign callback. Normal exit retains this
        // port until the observed confirmed-Off shutdown has completed.
        self.signals.disposed.store(true, Ordering::Release);
    }
}

struct Resources {
    server: Option<crate::ipc::OwnerServer>,
    monitor: ai2screenwake_native::lifecycle::LifecycleMonitor,
    _native: Arc<ai2screenwake_native::power::MacPower>,
}

fn start_resources(diagnostics: &Registry) -> Result<Resources, Fault> {
    use ai2screenwake_native::{identity, lifecycle::LifecycleMonitor, power::MacPower};
    let location = crate::ipc::for_current_user(identity::runtime_directory()?);
    let native = Arc::new(MacPower::new());
    // Install interruption observation before exposing any IPC control.
    let mut monitor = LifecycleMonitor::start(native.clone())?;
    let server = match crate::ipc::OwnerServer::bind(location, native.clone()) {
        Ok(server) => server,
        Err(error) => {
            if let Err(failure) = monitor.stop() {
                diagnostics.emit::<Error::Native::Activity::Occurred>(failure);
            }
            return Err(Fault::new("owner_start_failed", &error.to_string(), false));
        }
    };
    Ok(Resources {
        server: Some(server),
        monitor,
        _native: native,
    })
}

fn emit_quit_error(bus: &Registry, tracking: &Arc<Mutex<QuitTracking>>, error: Fault) {
    let events = tracking
        .lock()
        .expect("quit tracking poisoned")
        .events
        .clone();
    if let Some(events) = events.and_then(|events| events.upgrade()) {
        events.error::<Error::Dispatch::Validation::Occurred>(error);
    } else {
        bus.emit::<Error::Dispatch::Validation::Occurred>(error);
    }
}

fn request_quit(bus: &Registry, tracking: &Arc<Mutex<QuitTracking>>) -> Result<(), Fault> {
    let prepared = bus.prepare::<Power::Session::Quit>(Empty {})?;
    {
        let mut quit = tracking.lock().expect("quit tracking poisoned");
        quit.correlation = Some(prepared.correlation());
        quit.events = None;
        quit.result = None;
    }
    bus.emit_prepared(prepared);
    Ok(())
}

fn termination(bus: &Registry, tracking: &Arc<Mutex<QuitTracking>>) {
    let projection = Ui::Projection {
        text: HashMap::new(),
        flags: HashMap::from([("terminate".into(), true)]),
    };
    let events = tracking
        .lock()
        .expect("quit tracking poisoned")
        .events
        .clone();
    if let Some(events) = events.and_then(|events| events.upgrade()) {
        events.emit::<Ui::Menu::Render>(projection);
    } else {
        bus.emit::<Ui::Menu::Render>(projection);
    }
}

struct DisposalWitness(SyncSender<()>);

impl ObservationSink for DisposalWitness {
    fn observed(&self, _envelope: KvEnvelope) {}
}

impl Drop for DisposalWitness {
    fn drop(&mut self) {
        // The last registry owner is gone, including the private service worker.
        // This is transport lifetime acknowledgement, never a new core-state stream.
        let _ = self.0.send(());
    }
}

fn observed_terminal(
    terminal: &TerminalDelivery,
    commands: &Receiver<NativeIntent>,
    diagnostics: &Registry,
) -> Option<Ui::Projection> {
    loop {
        let cell = terminal.0.lock().expect("terminal delivery poisoned");
        let (mut cell, _) = terminal
            .1
            .wait_timeout_while(cell, Duration::from_secs(2), |cell| cell.is_none())
            .expect("terminal delivery poisoned");
        if cell.is_some() {
            return cell.take();
        }
        drop(cell);
        diagnostics.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
            "ui_terminal_observation_timeout",
            "Shutdown rendering has not been observed. Quit can retry.",
            true,
        ));
        if !shutdown_retry(commands, diagnostics) {
            return None;
        }
    }
}

fn run_host(
    observer: Arc<dyn NativeObserver>,
    bootstrap: Registry,
    receiver: Receiver<NativeIntent>,
    signals: Arc<Signals>,
) {
    let diagnostics = bootstrap.clone();
    let startup = start_resources(&bootstrap);
    let (mut resources, bus, startup_error) = match startup {
        Ok(resources) => {
            let bus = resources
                .server
                .as_ref()
                .expect("admitted server")
                .actions();
            (Some(resources), bus, None)
        }
        Err(error) => (None, bootstrap, Some(error)),
    };
    let terminal = Arc::new((Mutex::new(None), Condvar::new()));
    let (disposal_sender, disposal) = mpsc::sync_channel(1);
    if resources.is_some() {
        bus.observe(Arc::new(DisposalWitness(disposal_sender)));
    }
    let _observation = resources
        .as_ref()
        .map(|_| observe_menu_deferred(&bus, observer.clone(), Some(terminal.clone())));
    if let Some(error) = startup_error {
        bus.emit::<Error::Dispatch::Validation::Occurred>(error);
    } else {
        bus.emit::<Power::Session::Query>(Empty {});
    }
    let tracking = Arc::new(Mutex::new(QuitTracking::default()));
    let admitted = tracking.clone();
    bus.observe_action::<Power::Session::Quit>(move |_, events| {
        let mut quit = admitted.lock().expect("quit tracking poisoned");
        if quit.correlation.as_ref() == Some(&events.correlation()) {
            quit.events = Some(events.downgrade());
        }
    });
    let settled = tracking.clone();
    bus.observe_action::<Power::Session::Settled>(move |settlement, _| {
        let mut quit = settled.lock().expect("quit tracking poisoned");
        if quit.correlation.as_ref() == Some(&settlement.correlation) {
            quit.result = Some(settlement.result);
        }
    });
    let closed = tracking.clone();
    bus.observe_action::<Power::Session::Closed>(move |_, _| {
        closed.lock().expect("quit tracking poisoned").closed = true;
    });
    observe_cleanup(&bus, signals.clone());
    let mut quit_deadline = None;
    let mut terminal_requested = false;
    let mut shutdown_attempted = false;
    loop {
        if signals.overflow.swap(false, Ordering::AcqRel) {
            bus.emit::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "ui_input_overflow",
                "Native inputs arrived faster than they could be handled.",
                true,
            ));
        }
        let disposed = signals.disposed.load(Ordering::Acquire);
        let command = if disposed && quit_deadline.is_none() {
            Some(NativeIntent::Quit)
        } else {
            receiver.recv_timeout(Duration::from_millis(20)).ok()
        };
        if let Some(intent) = command {
            match intent {
                NativeIntent::Quit if resources.is_none() => {
                    termination(&bus, &tracking);
                    break;
                }
                NativeIntent::Quit if quit_deadline.is_none() => {
                    match request_quit(&bus, &tracking) {
                        Ok(()) => {
                            shutdown_attempted = false;
                            quit_deadline = Some(Instant::now() + Duration::from_secs(6));
                        }
                        Err(error) => {
                            bus.emit::<Error::Dispatch::Validation::Occurred>(error);
                            if disposed {
                                break;
                            }
                        }
                    }
                }
                NativeIntent::Quit => {}
                NativeIntent::ShowLog => bus.emit::<Log::Session::Query>(Empty {}),
                NativeIntent::CloseLog => bus.emit::<Ui::Log::Dismissed>(Empty {}),
                NativeIntent::RendererFailed => {
                    bus.emit::<Error::Dispatch::Validation::Occurred>(Fault::new(
                        "ui_renderer_failed",
                        "The native view could not render its resources.",
                        true,
                    ))
                }
                NativeIntent::Toggle if resources.is_some() && quit_deadline.is_none() => {
                    bus.emit::<Power::Session::Toggle>(Empty {});
                }
                NativeIntent::Toggle => {
                    bus.emit::<Error::Dispatch::Validation::Occurred>(Fault::new(
                        "owner_unavailable",
                        "The private owner is unavailable or shutting down.",
                        true,
                    ))
                }
            }
        }
        let (closed, result) = {
            let mut quit = tracking.lock().expect("quit tracking poisoned");
            let closed = quit.closed && quit.correlation.is_some() && !shutdown_attempted;
            let failed = quit.result.as_ref().is_some_and(|result| {
                result.error.is_some()
                    || !result.status.as_ref().is_some_and(|status| {
                        status.is_on == Some(false) && status.state == State::Off
                    })
            });
            let result = if failed { quit.result.take() } else { None };
            (closed, result)
        };
        if closed {
            quit_deadline = None;
            shutdown_attempted = true;
            signals.cleaning.store(true, Ordering::Release);
            let cleanup = match resources.as_mut() {
                Some(owner) => owner.monitor.stop(),
                None => Ok(()),
            };
            if let Err(error) = cleanup {
                signals.cleaning.store(false, Ordering::Release);
                emit_quit_error(&bus, &tracking, error);
                emit_quit_error(
                    &bus,
                    &tracking,
                    Fault::new(
                        "ui_shutdown_failed",
                        "The native observer could not finish shutdown. Try Quit again.",
                        true,
                    ),
                );
                if disposed {
                    break;
                }
            } else {
                if let Some(owner) = resources.as_mut() {
                    drop(owner.server.take());
                }
                if !signals.cleanup_failed.load(Ordering::Acquire) {
                    termination(&bus, &tracking);
                    terminal_requested = true;
                    break;
                }
                signals.cleaning.store(false, Ordering::Release);
                emit_quit_error(
                    &bus,
                    &tracking,
                    Fault::new(
                        "ui_shutdown_failed",
                        "The private owner could not finish shutdown.",
                        true,
                    ),
                );
                if disposed {
                    break;
                }
            }
        } else if let Some(result) = result {
            quit_deadline = None;
            if disposed {
                // Exceptional port disposal is not successful normal Quit:
                // failure was already observed; native Drop is fallback.
                break;
            } else if result.error.is_none() {
                emit_quit_error(
                    &bus,
                    &tracking,
                    Fault::new(
                        "quit_unconfirmed",
                        "Protection cleanup did not confirm Off. Try Quit again.",
                        true,
                    ),
                );
            }
        }
        if quit_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let failure = Fault::new(
                "quit_timeout",
                "Protection cleanup did not finish. Try Quit again.",
                true,
            );
            emit_quit_error(&bus, &tracking, failure.clone());
            diagnostics.emit::<Error::Ipc::Protocol::Occurred>(failure);
            quit_deadline = None;
            if disposed {
                break;
            }
        }
    }
    let packet = if terminal_requested {
        observed_terminal(&terminal, &receiver, &diagnostics)
    } else {
        None
    };
    // Queueing preserves the single logged observation path while preventing
    // NSApp termination from racing the private service's asynchronous disposal.
    drop(_observation);
    drop(resources);
    drop(bus);
    if let Some(packet) = packet {
        deliver_after_disposal(disposal, receiver, diagnostics, observer, signals, packet);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    struct Observed(mpsc::Sender<bool>);
    impl NativeObserver for Observed {
        fn observed(&self, _: HashMap<String, String>, flags: HashMap<String, bool>) {
            if flags.get("terminate") == Some(&true) {
                self.0.send(true).unwrap();
            }
        }
    }

    #[test]
    fn given_completed_disposal_when_quit_retries_then_completion_stays_latched() {
        let diagnostics = RegistryBuilder::default().build().unwrap();
        let (failed, failures) = mpsc::channel();
        diagnostics.observe_action::<Error::Ipc::Protocol::Occurred>(move |fault, _| {
            if fault.fault_code == "ui_shutdown_failed" {
                failed.send(()).unwrap();
            }
        });
        let signals = Arc::new(Signals::default());
        signals.cleaning.store(true, Ordering::Release);
        signals.cleanup_failed.store(true, Ordering::Release);
        let worker_signals = signals.clone();
        let (witness, disposal) = mpsc::sync_channel(1);
        let (commands, receiver) = mpsc::sync_channel(1);
        let (observed, terminal) = mpsc::channel();
        let worker = thread::spawn(move || {
            deliver_after_disposal(
                disposal,
                receiver,
                diagnostics,
                Arc::new(Observed(observed)),
                worker_signals,
                Ui::Projection {
                    text: HashMap::new(),
                    flags: HashMap::from([("terminate".into(), true)]),
                },
            )
        });
        witness.send(()).unwrap();
        failures.recv_timeout(Duration::from_secs(3)).unwrap();
        signals.cleanup_failed.store(false, Ordering::Release);
        commands.send(NativeIntent::Quit).unwrap();
        let result = terminal.recv_timeout(Duration::from_secs(3));
        drop(commands);
        worker.join().unwrap();
        assert_eq!(
            result.unwrap(),
            true,
            "the completion witness was consumed by the first attempt"
        );
    }

    #[test]
    fn given_pending_disposal_when_wait_times_out_then_cleanup_collection_remains_active() {
        let diagnostics = RegistryBuilder::default().build().unwrap();
        let (failed, failures) = mpsc::channel();
        diagnostics.observe_action::<Error::Ipc::Protocol::Occurred>(move |fault, _| {
            if fault.fault_code == "ui_shutdown_failed" {
                failed.send(()).unwrap();
            }
        });
        let signals = Arc::new(Signals::default());
        signals.cleaning.store(true, Ordering::Release);
        let worker_signals = signals.clone();
        let (_witness, disposal) = mpsc::sync_channel(1);
        let (commands, receiver) = mpsc::sync_channel(1);
        let (observed, _) = mpsc::channel();
        let worker = thread::spawn(move || {
            deliver_after_disposal(
                disposal,
                receiver,
                diagnostics,
                Arc::new(Observed(observed)),
                worker_signals,
                Ui::Projection {
                    text: HashMap::new(),
                    flags: HashMap::new(),
                },
            )
        });
        failures.recv_timeout(Duration::from_secs(3)).unwrap();
        let collecting = signals.cleaning.load(Ordering::Acquire);
        drop(commands);
        worker.join().unwrap();
        assert!(collecting, "timeout is not a disposal acknowledgement");
    }
}
// end ffi.rs
