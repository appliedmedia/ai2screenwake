// ffi.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Generated native intent/observation port and Rust-owned menu rendering.

use crate::contract::{
    ControlResult, CorrelationId, Emitter, Empty, Error, Fault, KvEnvelope, Log, ObservationSink,
    Power, Registry, RegistryBuilder, State, Status, Subscription, Ui, WeakEmitter,
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Condvar, Mutex,
        atomic::Ordering,
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

mod shutdown;
use shutdown::{Signals, deliver_after_disposal, observe_cleanup, shutdown_retry};

const PREVIEW_INPUT_LIMIT: usize = 32;

/// Immutable compiled publication version, not a mutable owner-state getter.
#[uniffi::export]
pub fn build_version() -> String {
    crate::build_info::VERSION.into()
}

/// Canonical source/configuration/version identity for nonlaunching build verification.
#[uniffi::export]
pub fn build_stamp() -> String {
    crate::build_info::STAMP.into()
}

/// Typed native inputs; the generated shell never constructs a genus or reads state.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum NativeIntent {
    /// The owner, not Swift, chooses the opposite latest intent.
    Toggle,
    /// Activate the observed large-TV cycle; Rust confirms the required real power first.
    CyclePreview,
    /// Read the existing canonical log through its typed query.
    ShowLog,
    /// Dismiss only the auxiliary read-only presentation.
    CloseLog,
    /// Request the fixed product website through a logged native effect.
    OpenInfo,
    /// The native workspace accepted the logged website effect.
    InfoOpened,
    /// The native workspace refused the logged website effect.
    InfoFailed,
    /// Explicitly open optional Accessibility setup, never ordinary protection.
    OpenSetup,
    /// Workspace acceptance for this exact setup-window lifetime.
    SetupOpened { session: u64, opened: bool },
    /// Recheck actual nonprompting OS trust while that window remains open.
    CheckAccessibility { session: u64 },
    /// Close only the matching setup window, not the diagnostic log.
    CloseSetup { session: u64 },
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
    setup_activation: Mutex<Option<(CorrelationId, Option<u64>)>>,
}

impl ObservationSink for RenderSink {
    fn observed(&self, envelope: KvEnvelope) {
        let mut cancel_setup = false;
        let mut activation = self
            .setup_activation
            .lock()
            .expect("setup delivery poisoned");
        if envelope
            .payload::<Ui::Accessibility::Setup::Activated>()
            .expect("canonical setup input")
            .is_some()
        {
            activation
                .get_or_insert((envelope.correlation().expect("canonical correlation"), None));
        }
        if envelope
            .payload::<Power::Session::Protection::TurnOff>()
            .expect("canonical control")
            .is_some()
            || envelope
                .payload::<Power::Session::Protection::Toggle>()
                .expect("canonical control")
                .is_some()
            || envelope
                .payload::<Power::Session::Protection::Quit>()
                .expect("canonical control")
                .is_some()
        {
            *activation = None;
            cancel_setup = true;
        }
        if let Some(window) = envelope
            .payload::<Ui::Accessibility::Setup::Dismissed>()
            .expect("canonical setup input")
            && activation
                .as_ref()
                .is_some_and(|(_, session)| *session == Some(window.session))
        {
            *activation = None;
            cancel_setup = true;
        }
        if let Some(state) = envelope
            .payload::<Ui::Accessibility::Setup::Changed>()
            .expect("canonical setup observation")
        {
            if !state.visible
                && activation
                    .as_ref()
                    .is_some_and(|(_, session)| *session == Some(state.session))
            {
                *activation = None;
                cancel_setup = true;
            } else if let Some((origin, session)) = activation.as_mut()
                && *origin == envelope.correlation().expect("canonical correlation")
            {
                *session = Some(state.session);
            }
        }
        let setup_live = activation.as_ref().is_some_and(|(origin, _)| {
            *origin == envelope.correlation().expect("canonical correlation")
        });
        drop(activation);
        if cancel_setup {
            // This logged control revokes native in-flight delivery before the next main-queue turn.
            self.observer.observed(
                HashMap::new(),
                HashMap::from([("cancel_setup".into(), true)]),
            );
        }
        match envelope.payload::<Ui::Menu::Presentation::Render>() {
            Ok(Some(mut render)) => {
                // Admission can retire the effect before its queued render reaches native delivery.
                if render.flags.get("open_setup") == Some(&true) && !setup_live {
                    render.flags.insert("open_setup".into(), false);
                    render.flags.insert("setup_visible".into(), false);
                }
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
    confirmed_on: bool,
    error: String,
    error_recovery: bool,
    log: Option<String>,
    log_title: Option<String>,
    log_query: Option<CorrelationId>,
    quitting: Option<CorrelationId>,
    closing: bool,
    termination_failed: bool,
    preview_error: bool,
    preview_request: Option<(PreviewFace, CorrelationId)>,
    preview_setters: VecDeque<CorrelationId>,
    info_request: Option<WeakEmitter>,
    setup: Ui::SetupPresentation,
}

#[derive(Clone, Copy, PartialEq)]
enum PreviewFace {
    On,
    Off,
    Error,
}

impl PreviewFace {
    fn next(self) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::Error,
            Self::Error => Self::On,
        }
    }
}

impl Presentation {
    fn face(&self) -> PreviewFace {
        if !self.error.is_empty()
            || self.preview_error
            || self
                .status
                .as_ref()
                .is_some_and(|status| status.state == State::Error)
        {
            PreviewFace::Error
        } else if self.confirmed_on {
            PreviewFace::On
        } else {
            PreviewFace::Off
        }
    }

    fn activate_preview(&mut self, events: &Emitter) {
        if self.closing || self.status.is_none() {
            events.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "preview_unavailable",
                "The owner is unavailable or shutting down.",
                true,
            ));
            return;
        }
        if self.preview_setters.len() == PREVIEW_INPUT_LIMIT {
            events.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "preview_input_overflow",
                "TV preview inputs arrived faster than they could be admitted.",
                true,
            ));
            return;
        }
        let target = if self.confirmed_on
            && (!self.error.is_empty()
                || self
                    .status
                    .as_ref()
                    .is_some_and(|status| status.state == State::Error))
        {
            PreviewFace::Off
        } else {
            self.face().next()
        };
        let id = events.correlation();
        self.preview_setters.push_back(id.clone());
        self.preview_request = Some((target, id));
        if target == PreviewFace::On {
            events.emit::<Power::Session::Protection::TurnOn>(Empty {});
        } else {
            events.emit::<Power::Session::Protection::TurnOff>(Empty {});
        }
    }

    fn ordinary_control(&mut self, events: &Emitter, setter: bool) {
        if setter
            && let Some(index) = self
                .preview_setters
                .iter()
                .position(|id| *id == events.correlation())
        {
            // Older derived setters are still internal after a newer activation.
            // Consume their origin at admission, never at delayed settlement.
            self.preview_setters.remove(index);
            return;
        }
        self.preview_request = None;
        if std::mem::take(&mut self.preview_error) {
            self.render(events);
        }
    }

    fn settle_preview(&mut self, settlement: &crate::contract::Settlement, events: &Emitter) {
        let Some((target, id)) = &self.preview_request else {
            return;
        };
        if *id != settlement.correlation {
            return;
        }
        let target = *target;
        self.preview_request = None;
        let enabled = target == PreviewFace::On;
        let confirmed = settlement.result.error.is_none()
            && settlement.result.status.as_ref().is_some_and(|status| {
                status.is_on == Some(enabled)
                    && status.desired_on == enabled
                    && status.state == if enabled { State::On } else { State::Off }
                    && self.status.as_ref().is_some_and(|current| {
                        current.revision == status.revision
                            && current.is_on == status.is_on
                            && current.state == status.state
                    })
            });
        if confirmed {
            self.preview_error = target == PreviewFace::Error;
            self.render(events);
        }
    }

    fn observe_status(&mut self, status: Status, events: &Emitter) {
        if self
            .status
            .as_ref()
            .is_some_and(|current| status.revision < current.revision)
        {
            return;
        }
        if status.operation_id.is_some()
            && self
                .status
                .as_ref()
                .and_then(|current| current.operation_id.as_ref())
                != status.operation_id.as_ref()
        {
            self.error_recovery = false;
        }
        match (status.state, status.is_on) {
            (State::On, Some(true)) => {
                self.confirmed_on = true;
                self.preview_error = false;
            }
            (State::Off, Some(false)) => self.confirmed_on = false,
            _ => {}
        }
        if status.state == State::Error {
            self.preview_error = false;
        }
        self.status = Some(status);
        self.render(events);
    }

    fn projection(&self, terminate: bool) -> Ui::Projection {
        let (protection, icon) = match self.status.as_ref().map(|s| (s.state, s.is_on)) {
            Some((State::On, Some(true))) => ("On", "menu-on"),
            Some((State::Off, Some(false))) => ("Off", "menu-off"),
            Some((State::Pending, _)) | None => (
                "Pending",
                if self.confirmed_on {
                    "menu-on"
                } else {
                    "menu-off"
                },
            ),
            _ => ("Error", "menu-error"),
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
        let attention = !error.is_empty() || protection == "Error";
        let intentional_preview = self.preview_error && !attention;
        let (phase, icon) = if attention || intentional_preview {
            ("Error", "menu-error")
        } else {
            (protection, icon)
        };
        let accessibility = if intentional_preview {
            format!(
                "AI2ScreenWake Error preview. Observed protection: {protection}. No fault is being simulated."
            )
        } else if !attention {
            format!("AI2ScreenWake protection: {phase}")
        } else {
            format!("AI2ScreenWake status: Error. Observed protection: {protection}. {error}")
        };
        let tooltip = format!(
            "{accessibility}\nNo Sleep requires both idle system/display assertions and current native admission. Idle sleep prevention itself does not need Accessibility. Optional protection for configured manual Start Screen Saver corners requires authorization. The configured corner guard uses Accessibility to modify mouse movement, not to read keys. Detected relevant configuration changes retire only that guard; concurrent external settings changes are not atomic. Forced sleep and security locks remain available. Automatic screensaver protection is unverified."
        );
        let text = HashMap::from([
            ("title".into(), "AI2ScreenWake".into()),
            ("icon_asset".into(), icon.into()),
            (
                "preview_asset".into(),
                icon.replacen("menu-", "preview-", 1),
            ),
            ("badge".into(), String::new()),
            (
                "status".into(),
                match phase {
                    "On" => "Mode: No Sleep",
                    "Off" => "Mode: Can Sleep",
                    "Pending" if self.confirmed_on => "Mode: No Sleep",
                    "Pending" => "Mode: Can Sleep",
                    _ => "Off: Error",
                }
                .into(),
            ),
            ("accessibility".into(), accessibility),
            ("show_log".into(), "Show Log...".into()),
            ("open_menu".into(), "Open menu".into()),
            ("version".into(), crate::build_info::VERSION.into()),
            ("info_label".into(), "About AI2ScreenWake".into()),
            ("info_help".into(), "Open the AI2ScreenWake website".into()),
            ("info_url".into(), "https://ai2sw.info".into()),
            ("setup_label".into(), "Install active (beyond idle) sleep preventer".into()),
            ("setup_title".into(), "Optional corner protection".into()),
            ("setup_instruction".into(), "Drag app icon to panel".into()),
            ("setup_explanation".into(), "This optional Accessibility access supports filtering configured Start Screen Saver corners, not arbitrary sleep or security locks. Drag AI2ScreenWake into the panel, then enable it. macOS may require authentication; dropping alone does not grant access.".into()),
            ("setup_url".into(), "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility".into()),
            ("setup_session".into(), self.setup.session.to_string()),
            ("setup_message".into(), self.setup.message.clone()),
            (
                "cycle_preview".into(),
                "Cycle TV: No Sleep, Can Sleep, Error preview".into(),
            ),
            ("quit".into(), "Quit".into()),
            (
                "log_title".into(),
                self.log_title.clone().unwrap_or_else(|| "ai2sw.log".into()),
            ),
            ("log_text".into(), self.log.clone().unwrap_or_default()),
            ("error".into(), error),
            ("tooltip".into(), tooltip),
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
                ("open_info".into(), false),
                ("open_setup".into(), false),
                ("setup_visible".into(), self.setup.visible && !self.closing),
                (
                    "setup_enabled".into(),
                    self.status.is_some() && !self.closing,
                ),
                ("setup_trusted".into(), self.setup.trusted == Some(true)),
                ("setup_guard_ready".into(), self.setup.guard_ready),
            ]),
        }
    }

    fn render(&self, events: &Emitter) {
        events.emit::<Ui::Menu::Presentation::Render>(self.projection(false));
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
        let actionable = matches!(
            failure.fault_code.as_str(),
            "corner_accessibility_required" | "corner_pointer_inside"
        );
        let copy = match failure.fault_code.as_str() {
            "corner_accessibility_required" => {
                "Allow AI2ScreenWake in System Settings > Privacy & Security > Accessibility, then turn No Sleep on again."
            }
            "corner_pointer_inside" => {
                "Move the pointer away from the screen saver corner, then turn No Sleep on again."
            }
            code if code.starts_with("corner_") => {
                "Screen saver corner protection is unavailable. Turn No Sleep off, then on again. Check Show Log."
            }
            _ => "Last action reported an error. Check Show Log.",
        };
        if actionable || !self.error_recovery {
            self.error = copy.into();
            self.error_recovery = actionable;
        }
        self.preview_error = false;
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
    let subscription = bus.subscribe(Arc::new(RenderSink {
        observer,
        terminal,
        setup_activation: Mutex::new(None),
    }));
    let presentation = Arc::new(Mutex::new(Presentation::default()));
    let setup = presentation.clone();
    bus.observe_action::<Ui::Accessibility::Setup::Changed>(move |state, events| {
        let mut view = setup.lock().expect("menu projection poisoned");
        if state.session < view.setup.session {
            return;
        }
        let open = state.open_settings && state.visible && !view.closing;
        view.setup = state;
        let mut projection = view.projection(false);
        projection.flags.insert("open_setup".into(), open);
        events.emit::<Ui::Menu::Presentation::Render>(projection);
    });
    let info = presentation.clone();
    bus.observe_action::<Ui::Info::Website::Activated>(move |_, events| {
        let mut view = info.lock().expect("menu projection poisoned");
        if view.info_request.is_some() {
            events.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "ui_website_pending",
                "The website request has not completed.",
                true,
            ));
            return;
        }
        view.info_request = Some(events.downgrade());
        let mut projection = view.projection(false);
        projection.flags.insert("open_info".into(), true);
        events.emit::<Ui::Menu::Presentation::Render>(projection);
    });
    let info = presentation.clone();
    bus.observe_action::<Ui::Info::Website::Completed>(move |completion, events| {
        let pending = info
            .lock()
            .expect("menu projection poisoned")
            .info_request
            .take();
        let Some(pending) = pending.and_then(|pending| pending.upgrade()) else {
            events.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "ui_website_unrequested",
                "No website request is awaiting completion.",
                true,
            ));
            return;
        };
        if completion.opened {
            pending.emit::<Ui::Info::Website::Opened>(Empty {});
        } else {
            pending.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "ui_website_open_failed",
                "The website could not be opened. Try again.",
                true,
            ));
        }
    });
    let activated = presentation.clone();
    bus.observe_action::<Ui::Preview::Control::Activated>(move |_, events| {
        activated
            .lock()
            .expect("menu projection poisoned")
            .activate_preview(events);
    });
    macro_rules! ordinary_control {
        ($action:ty, $setter:expr) => {{
            let controls = presentation.clone();
            bus.observe_action::<$action>(move |_, events| {
                controls
                    .lock()
                    .expect("menu projection poisoned")
                    .ordinary_control(events, $setter);
            });
        }};
    }
    ordinary_control!(Power::Session::Protection::TurnOn, true);
    ordinary_control!(Power::Session::Protection::TurnOff, true);
    ordinary_control!(Power::Session::Protection::Toggle, false);
    ordinary_control!(Power::Session::Protection::Quit, false);
    let changed = presentation.clone();
    bus.observe_action::<Power::Session::Protection::Changed>(move |status, events| {
        let mut view = changed.lock().expect("menu projection poisoned");
        view.observe_status(status, events);
    });
    let snapshot = presentation.clone();
    bus.observe_action::<Power::Session::Protection::Snapshot>(move |status, events| {
        let mut view = snapshot.lock().expect("menu projection poisoned");
        view.observe_status(status, events);
    });
    let requested = presentation.clone();
    bus.observe_action::<Log::Session::History::Query>(move |_, events| {
        requested
            .lock()
            .expect("menu projection poisoned")
            .log_query = Some(events.correlation());
    });
    let log = presentation.clone();
    bus.observe_action::<Log::Session::History::Snapshot>(move |snapshot, events| {
        let mut view = log.lock().expect("menu projection poisoned");
        if view.log_query.as_ref() != Some(&events.correlation()) {
            return;
        }
        let text = snapshot.text();
        if snapshot.storage_error.is_some() {
            view.error = "Some history could not be saved.".into();
            view.error_recovery = false;
        }
        view.log_title = snapshot.readable_path.as_ref().and_then(|path| {
            std::path::Path::new(path)
                .file_name()?
                .to_str()
                .map(str::to_owned)
        });
        view.log = Some(text);
        let mut projection = view.projection(false);
        projection.flags.insert("activate_log".into(), true);
        events.emit::<Ui::Menu::Presentation::Render>(projection);
    });
    let closed = presentation.clone();
    bus.observe_action::<Ui::Log::Window::Dismissed>(move |_, events| {
        let mut view = closed.lock().expect("menu projection poisoned");
        view.log_query = None;
        if view.log.take().is_some() {
            view.render(events);
        }
    });
    let quit = presentation.clone();
    bus.observe_action::<Power::Session::Protection::Quit>(move |_, events| {
        let mut view = quit.lock().expect("menu projection poisoned");
        view.closing = true;
        view.termination_failed = false;
        view.quitting = Some(events.correlation());
        view.render(events);
    });
    let settled = presentation.clone();
    bus.observe_action::<Power::Session::Protection::Settled>(move |settlement, events| {
        let mut view = settled.lock().expect("menu projection poisoned");
        view.settle_preview(&settlement, events);
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

impl NativeHost {
    /// Explicit Rust-only location AND native collaborator, never an exported production bypass.
    ///
    /// Assertion-only fixtures supply real IOKit with a controlled corner boundary.
    /// Production `new` uses actual native admission and optional guarding; paths never select policy.
    pub fn with_native(
        observer: Box<dyn NativeObserver>,
        location: crate::contract::OwnerLocation,
        native: Arc<ai2screenwake_native::power::MacPower>,
    ) -> Result<Arc<Self>, HostFailure> {
        Self::start(observer, Some((location, native)))
    }

    fn start(
        observer: Box<dyn NativeObserver>,
        location: Option<(
            crate::contract::OwnerLocation,
            Arc<ai2screenwake_native::power::MacPower>,
        )>,
    ) -> Result<Arc<Self>, HostFailure> {
        let bootstrap =
            RegistryBuilder::default()
                .build()
                .map_err(|error| HostFailure::Unavailable {
                    message: error.to_string(),
                })?;
        let observer: Arc<dyn NativeObserver> = Arc::from(observer);
        let failed_observation = observe_menu(&bootstrap, observer.clone());
        bootstrap.emit::<Ui::Menu::Presentation::Render>(Presentation::default().projection(false));
        let (commands, receiver) = mpsc::sync_channel(32);
        let signals = Arc::new(Signals::default());
        let worker_signals = signals.clone();
        let failure_bus = bootstrap.clone();
        let failure_port = bootstrap.clone();
        thread::Builder::new()
            .name("screenwake-native-host".into())
            .spawn(move || run_host(observer, bootstrap, receiver, worker_signals, location))
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
}

#[uniffi::export]
impl NativeHost {
    /// Register observation before asynchronous admission at the real login-session location.
    #[uniffi::constructor]
    pub fn new(observer: Box<dyn NativeObserver>) -> Result<Arc<Self>, HostFailure> {
        Self::start(observer, None)
    }

    /// Fire-and-forget typed input; state, errors and exit instructions are observed.
    pub fn emit(&self, intent: NativeIntent) {
        if shutdown::enqueue(&self.commands, &self.signals, intent).is_err() {
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

fn start_resources(
    diagnostics: &Registry,
    location: Option<(
        crate::contract::OwnerLocation,
        Arc<ai2screenwake_native::power::MacPower>,
    )>,
) -> Result<Resources, Fault> {
    use ai2screenwake_native::{identity, lifecycle::LifecycleMonitor, power::MacPower};
    let (location, readable, native) = match location {
        Some((location, native)) => {
            let readable = identity::readable_log_path_in(&location.directory);
            (location, readable, native)
        }
        None => (
            crate::ipc::for_current_user(identity::runtime_directory()?),
            identity::readable_log_path(),
            Arc::new(MacPower::new()),
        ),
    };
    // Install interruption observation before exposing any IPC control.
    let mut monitor = LifecycleMonitor::start(native.clone())?;
    let server =
        match crate::ipc::OwnerServer::bind_with_readable_log(location, native.clone(), readable) {
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
    let prepared = bus.prepare::<Power::Session::Protection::Quit>(Empty {})?;
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
        events.emit::<Ui::Menu::Presentation::Render>(projection);
    } else {
        bus.emit::<Ui::Menu::Presentation::Render>(projection);
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
    location: Option<(
        crate::contract::OwnerLocation,
        Arc<ai2screenwake_native::power::MacPower>,
    )>,
) {
    let diagnostics = bootstrap.clone();
    let startup = start_resources(&bootstrap, location);
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
        bus.emit::<Power::Session::Protection::Query>(Empty {});
    }
    let tracking = Arc::new(Mutex::new(QuitTracking::default()));
    let admitted = tracking.clone();
    bus.observe_action::<Power::Session::Protection::Quit>(move |_, events| {
        let mut quit = admitted.lock().expect("quit tracking poisoned");
        if quit.correlation.as_ref() == Some(&events.correlation()) {
            quit.events = Some(events.downgrade());
        }
    });
    let settled = tracking.clone();
    bus.observe_action::<Power::Session::Protection::Settled>(move |settlement, _| {
        let mut quit = settled.lock().expect("quit tracking poisoned");
        if quit.correlation.as_ref() == Some(&settlement.correlation) {
            quit.result = Some(settlement.result);
        }
    });
    let closed = tracking.clone();
    bus.observe_action::<Power::Session::Protection::Closed>(move |_, _| {
        closed.lock().expect("quit tracking poisoned").closed = true;
    });
    observe_cleanup(&bus, signals.clone());
    let mut quit_deadline = None;
    let mut terminal_requested = false;
    let mut shutdown_attempted = false;
    loop {
        shutdown::drain_inputs(&signals, &bus);
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
                NativeIntent::ShowLog => bus.emit::<Log::Session::History::Query>(Empty {}),
                NativeIntent::CloseLog => bus.emit::<Ui::Log::Window::Dismissed>(Empty {}),
                NativeIntent::OpenInfo => bus.emit::<Ui::Info::Website::Activated>(Empty {}),
                NativeIntent::InfoOpened | NativeIntent::InfoFailed => {
                    bus.emit::<Ui::Info::Website::Completed>(Ui::WebsiteCompletion {
                        opened: matches!(intent, NativeIntent::InfoOpened),
                    });
                }
                NativeIntent::OpenSetup => {
                    bus.emit::<Ui::Accessibility::Setup::Activated>(Empty {})
                }
                NativeIntent::SetupOpened { session, opened } => {
                    bus.emit::<Ui::Accessibility::Setup::Completed>(Ui::SetupCompletion {
                        session,
                        opened,
                    });
                }
                NativeIntent::CheckAccessibility { session } => {
                    bus.emit::<Ui::Accessibility::Setup::Check>(Ui::SetupSession { session });
                }
                NativeIntent::CloseSetup { session } => {
                    bus.emit::<Ui::Accessibility::Setup::Dismissed>(Ui::SetupSession { session });
                }
                NativeIntent::CyclePreview if resources.is_some() && quit_deadline.is_none() => {
                    bus.emit::<Ui::Preview::Control::Activated>(Empty {});
                }
                NativeIntent::RendererFailed => {
                    bus.emit::<Error::Dispatch::Validation::Occurred>(Fault::new(
                        "ui_renderer_failed",
                        "The native view could not render its resources.",
                        true,
                    ))
                }
                NativeIntent::Toggle if resources.is_some() && quit_deadline.is_none() => {
                    bus.emit::<Power::Session::Protection::Toggle>(Empty {});
                }
                NativeIntent::Toggle | NativeIntent::CyclePreview => {
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
        assert!(
            result.unwrap(),
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
