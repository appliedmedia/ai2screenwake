// ui.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Executable rendering requirements at the canonical Actions/generated-callback seam.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, ControlResult, CorrelationId, Emitter, Empty,
        Error, Fault, IdleRequest, KvEnvelope, Log, LogSnapshot, ObservationSink, Outcome, Power,
        Registry, RegistryBuilder, Settlement, State, Status, Subscription, Ui, WeakEmitter,
    },
    ffi::{NativeHost, NativeIntent, NativeObserver, observe_menu},
    ipc::{OwnerConnection, OwnerServer, for_current_user},
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

#[path = "../../src/ffi/shutdown.rs"]
#[allow(dead_code)]
mod shutdown;

#[derive(Default)]
struct QueuedNative(Mutex<Vec<(Apply, Emitter)>>, Condvar, AtomicBool);

impl NativePower for QueuedNative {
    fn apply(&self, request: Apply, events: Emitter) {
        if self.2.load(Ordering::SeqCst) {
            finish_apply(request, events);
            return;
        }
        self.0.lock().unwrap().push((request, events));
        self.1.notify_all();
    }

    fn sample(&self, _: IdleRequest, _: Emitter) {
        panic!("this Off-settlement witness must not sample native idle");
    }
}

impl QueuedNative {
    fn pending(&self, enabled: bool) -> (Apply, Emitter) {
        let requests = self.0.lock().unwrap();
        let (mut requests, _) = self
            .1
            .wait_timeout_while(requests, Duration::from_secs(2), |requests| {
                !requests
                    .iter()
                    .any(|(request, _)| request.enabled == enabled)
            })
            .unwrap();
        let index = requests
            .iter()
            .position(|(request, _)| request.enabled == enabled)
            .expect("required native apply did not arrive");
        requests.remove(index)
    }
}

#[derive(Default)]
struct QueueBarrier {
    barrier: Mutex<(bool, bool)>,
    changed: Condvar,
}

#[derive(Default)]
struct View(
    Mutex<Vec<Ui::Projection>>,
    Condvar,
    Mutex<Option<std::path::PathBuf>>,
    Mutex<Option<bool>>,
);

impl NativeObserver for View {
    fn observed(&self, text: HashMap<String, String>, flags: HashMap<String, bool>) {
        if flags.get("terminate") == Some(&true) {
            if let Some(path) = self.2.lock().unwrap().as_ref() {
                *self.3.lock().unwrap() = Some(path.exists());
            }
        }
        self.0.lock().unwrap().push(Ui::Projection { text, flags });
        self.1.notify_all();
    }
}

struct Forward(Arc<View>);
impl NativeObserver for Forward {
    fn observed(&self, text: HashMap<String, String>, flags: HashMap<String, bool>) {
        self.0.observed(text, flags);
    }
}

#[derive(Default, World)]
struct Menu {
    bus: Option<Registry>,
    subscription: Option<Subscription>,
    view: Arc<View>,
    log_query: Arc<Mutex<Option<WeakEmitter>>>,
    quit: Option<CorrelationId>,
    after_dismiss: usize,
    host: Option<Arc<NativeHost>>,
    second: Option<Arc<NativeHost>>,
    second_view: Arc<View>,
    quit_index: usize,
    toggle_index: usize,
    endpoint: Option<std::path::PathBuf>,
    control_count: Arc<AtomicUsize>,
    controls_before_fault: usize,
    queued_native: Arc<QueuedNative>,
    queue_barrier: Arc<QueueBarrier>,
    queue_directory: Option<tempfile::TempDir>,
    queue_trace: Arc<IpcView>,
    queue_remote_trace: Arc<IpcView>,
    queue_remote: Option<OwnerConnection>,
    queue_server: Option<OwnerServer>,
    disposal_proved: bool,
}

impl std::fmt::Debug for Menu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Menu").finish_non_exhaustive()
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        if self.queue_server.is_some() {
            self.queue_barrier.barrier.lock().unwrap().1 = true;
            self.queue_barrier.changed.notify_all();
            self.queued_native.2.store(true, Ordering::SeqCst);
            for (request, events) in std::mem::take(&mut *self.queued_native.0.lock().unwrap()) {
                finish_apply(request, events);
            }
            self.bus
                .as_ref()
                .unwrap()
                .emit::<Power::Session::Quit>(Empty {});
            let entries = self.queue_trace.0.lock().unwrap();
            let (entries, _) = self
                .queue_trace
                .1
                .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
                    !entries
                        .iter()
                        .any(|entry| entry.payload::<Power::Session::Closed>().unwrap().is_some())
                })
                .unwrap();
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.payload::<Power::Session::Closed>().unwrap().is_some()),
                "controlled owner fixture did not close"
            );
            drop(entries);
            self.queue_remote.take();
            self.subscription.take();
            self.bus.take();
            self.queue_server.take();
        }
        for (host, view) in [(&self.second, &self.second_view), (&self.host, &self.view)] {
            if let Some(host) = host {
                let frames = view.0.lock().unwrap();
                if !frames
                    .iter()
                    .any(|frame| frame.flags.get("terminate") == Some(&true))
                {
                    drop(frames);
                    host.emit(NativeIntent::Quit);
                    let frames = view.0.lock().unwrap();
                    drop(
                        view.1
                            .wait_timeout_while(frames, Duration::from_secs(2), |frames| {
                                !frames
                                    .iter()
                                    .any(|frame| frame.flags.get("terminate") == Some(&true))
                            })
                            .unwrap(),
                    );
                }
            }
        }
    }
}

fn last(world: &Menu) -> Ui::Projection {
    world.view.0.lock().unwrap().last().unwrap().clone()
}

fn wait_view(view: &View, predicate: impl Fn(&[Ui::Projection]) -> bool) -> Vec<Ui::Projection> {
    let frames = view.0.lock().unwrap();
    let (frames, _) = view
        .1
        .wait_timeout_while(frames, Duration::from_secs(2), |frames| !predicate(frames))
        .unwrap();
    let valid = predicate(&frames);
    let result = frames.clone();
    drop(frames);
    assert!(valid, "required host observation did not arrive");
    result
}

#[given("a typed native menu observation is registered")]
fn observing(world: &mut Menu) {
    let mut builder = RegistryBuilder::default();
    // This projection-only fixture deliberately supplies no native completion.
    // Real owner/IPC/resource behavior is exercised by the serial host scenarios.
    builder.on::<Power::Session::Quit>(|_, _| Ok(())).unwrap();
    let bus = builder.build().unwrap();
    world.subscription = Some(observe_menu(&bus, world.view.clone()));
    let query = world.log_query.clone();
    bus.observe_action::<Log::Session::Query>(move |_, events| {
        *query.lock().unwrap() = Some(events.downgrade());
    });
    macro_rules! watch_control {
        ($action:ty) => {{
            let count = world.control_count.clone();
            bus.observe_action::<$action>(move |_, _| {
                count.fetch_add(1, Ordering::SeqCst);
            });
        }};
    }
    watch_control!(Power::Session::TurnOn);
    watch_control!(Power::Session::TurnOff);
    watch_control!(Power::Session::Toggle);
    watch_control!(Power::Session::Quit);
    world.bus = Some(bus);
}

#[when("the owner observes confirmed protection On")]
fn confirmed(world: &mut Menu) {
    let mut status = Status::off();
    status.state = State::On;
    status.desired_on = true;
    status.is_on = Some(true);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Changed>(status);
}

#[then("the generated observer receives the awake TV and observed On label")]
fn awake(world: &mut Menu) {
    let views = world.view.0.lock().unwrap();
    assert_eq!(views.len(), 1, "a canonical observed render is required");
    assert_eq!(views[0].text["icon_asset"], "menu-on");
    assert_eq!(views[0].text["status"], "Status: On");
}

#[when("the owner observes a pending enable intent")]
fn pending(world: &mut Menu) {
    let mut status = Status::off();
    status.state = State::Pending;
    status.desired_on = true;
    status.is_on = None;
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Changed>(status);
}

#[then("the observed menu remains pending with a distinct non-color badge")]
fn not_optimistic(world: &mut Menu) {
    let view = last(world);
    assert_eq!(view.text["icon_asset"], "menu-off");
    assert_eq!(view.text["status"], "Status: Pending");
    assert!(!view.text["badge"].is_empty());
}

#[when("the owner observes unresolved protection")]
fn unresolved(world: &mut Menu) {
    let mut status = Status::off();
    status.state = State::Error;
    status.is_on = None;
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Changed>(status);
}

#[then("the menu labels Error rather than falsely claiming Off")]
fn truthful_error(world: &mut Menu) {
    let view = last(world);
    assert_eq!(view.text["status"], "Status: Error");
    assert!(!view.text["badge"].is_empty());
    assert!(!view.text["error"].is_empty());
}

#[when("a native activity fault with private details is dispatched")]
fn private_fault(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Error::Native::Activity::Occurred>(Fault::new(
            "test_sample",
            "private raw native chain not for display",
            true,
        ));
}

#[then("the menu keeps On with short error copy and no private chain")]
fn no_raw_errors(world: &mut Menu) {
    let view = last(world);
    assert_eq!(view.text["status"], "Status: On");
    assert_eq!(view.text["icon_asset"], "menu-on");
    assert!(!view.text["error"].is_empty());
    assert!(!view.text["error"].contains("private"));
    assert!(!view.flags["terminate"]);
}

#[given(regex = r"^an observed (On|Off) native menu$")]
fn observed_menu_phase(world: &mut Menu, phase: String) {
    observing(world);
    match phase.as_str() {
        "On" => confirmed(world),
        "Off" => observed_off(world),
        _ => unreachable!("the executable requirement restricts observed phases"),
    }
}

#[when(regex = r"^the canonical telemetry (Configuration|Transport|Buffer) fault is dispatched$")]
fn telemetry_fault(world: &mut Menu, family: String) {
    world.controls_before_fault = world.control_count.load(Ordering::SeqCst);
    let fault = Fault::new(
        "fixture_telemetry_failure",
        "fixture-private-key; fixture-private-endpoint; raw failure chain",
        true,
    );
    let bus = world.bus.as_ref().unwrap();
    match family.as_str() {
        "Configuration" => bus.emit::<Error::Telemetry::Configuration::Occurred>(fault),
        "Transport" => bus.emit::<Error::Telemetry::Transport::Occurred>(fault),
        "Buffer" => bus.emit::<Error::Telemetry::Buffer::Occurred>(fault),
        _ => unreachable!("the executable requirement restricts canonical telemetry families"),
    }
}

fn safe_telemetry_error(world: &Menu) -> Ui::Projection {
    let view = last(world);
    assert!(!view.text["error"].is_empty());
    assert!(!view.text["error"].contains("fixture-private"));
    assert!(!view.text["accessibility"].contains("fixture-private"));
    assert!(view.text["accessibility"].contains(&view.text["error"]));
    assert!(!view.text["badge"].is_empty());
    assert!(!view.flags["terminate"]);
    assert!(!view.flags["log_visible"]);
    assert!(!view.flags["activate_log"]);
    assert_eq!(
        world.control_count.load(Ordering::SeqCst),
        world.controls_before_fault
    );
    view
}

#[then(
    regex = r"^observed (On|Off) is retained with a safe accessible telemetry error and unchanged controls$"
)]
fn telemetry_keeps_phase(world: &mut Menu, phase: String) {
    let view = safe_telemetry_error(world);
    assert_eq!(view.text["status"], format!("Status: {phase}"));
    assert_eq!(
        view.text["icon_asset"],
        if phase == "On" { "menu-on" } else { "menu-off" }
    );
    assert!(view.flags["toggle_enabled"]);
    assert!(view.flags["quit_enabled"]);
}

#[then("telemetry error visibility preserves in-progress Quit and protection truth")]
fn telemetry_keeps_quit(world: &mut Menu) {
    let view = safe_telemetry_error(world);
    assert_eq!(view.text["status"], "Status: On");
    assert_eq!(view.text["icon_asset"], "menu-on");
    assert!(!view.flags["toggle_enabled"]);
    assert!(!view.flags["quit_enabled"]);
}

#[when("the existing log query is emitted")]
fn log(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Log::Session::Query>(Empty {});
}

#[then("the auxiliary log is visible with redacted ordered metadata")]
fn redacted_log(world: &mut Menu) {
    let view = last(world);
    assert!(view.flags["log_visible"]);
    assert!(!view.text["log_text"].is_empty());
    assert!(!view.text["log_text"].contains("private"));
    let sequences: Vec<usize> = view.text["log_text"]
        .lines()
        .map(|line| line.split_whitespace().next().unwrap().parse().unwrap())
        .collect();
    assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
}

#[then("the log stays visible without a new activation instruction")]
fn no_focus_steal(world: &mut Menu) {
    let view = last(world);
    assert!(view.flags["log_visible"]);
    assert_eq!(view.flags.get("activate_log"), Some(&false));
}

#[when("the canonical log snapshot reports earlier omitted history")]
fn omitted_history(world: &mut Menu) {
    log(world);
    world
        .log_query
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .emit::<Log::Session::Snapshot>(LogSnapshot {
            earlier_entries_omitted: true,
            ..Default::default()
        });
}

#[then("the log visibly discloses earlier missing history")]
fn omitted_warning(world: &mut Menu) {
    let view = last(world);
    assert!(view.flags["log_visible"]);
    assert!(view.text["log_text"].to_lowercase().contains("earlier"));
    assert!(view.text["log_text"].to_lowercase().contains("omitted"));
}

#[when("the canonical log snapshot reports a storage fault")]
fn storage_fault(world: &mut Menu) {
    log(world);
    world
        .log_query
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .emit::<Log::Session::Snapshot>(LogSnapshot {
            storage_error: Some(Fault::new(
                "test_storage",
                "private disk path and native error chain",
                true,
            )),
            ..Default::default()
        });
}

#[then("the log warns about unsaved history while preserving On and privacy")]
fn unhealthy_warning(world: &mut Menu) {
    let view = last(world);
    assert!(view.flags["log_visible"]);
    assert!(view.text["log_text"].contains("could not be saved"));
    assert!(!view.text["error"].is_empty());
    assert!(!view.text["log_text"].contains("private"));
    assert!(!view.text["error"].contains("private"));
    assert_eq!(view.text["status"], "Status: On");
}

#[when("the log is dismissed twice")]
fn dismiss(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Ui::Log::Dismissed>(Empty {});
    world.after_dismiss = world.view.0.lock().unwrap().len();
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Ui::Log::Dismissed>(Empty {});
}

#[then("it is closed once without changing the observed On state")]
fn closed_once(world: &mut Menu) {
    assert_eq!(world.view.0.lock().unwrap().len(), world.after_dismiss);
    let view = last(world);
    assert!(!view.flags["log_visible"]);
    assert_eq!(view.text["status"], "Status: On");
}

#[when("the old log query emits a late snapshot")]
fn late_log(world: &mut Menu) {
    world
        .log_query
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .emit::<Log::Session::Snapshot>(LogSnapshot::default());
}

#[then("the dismissed log stays closed without another activation")]
fn late_stays_closed(world: &mut Menu) {
    let view = last(world);
    assert!(!view.flags["log_visible"]);
    assert!(!view.flags["activate_log"]);
}

#[when("the generated observation token is dropped")]
fn unsubscribe(world: &mut Menu) {
    world.subscription.take();
}

#[then("the foreign observer is no longer retained or invoked")]
fn no_callbacks(world: &mut Menu) {
    assert!(world.view.0.lock().unwrap().is_empty());
    assert_eq!(Arc::strong_count(&world.view), 1);
}

#[when("native Quit is admitted without a terminal result")]
fn quit_pending(world: &mut Menu) {
    let bus = world.bus.as_ref().unwrap();
    let prepared = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
    world.quit = Some(prepared.correlation());
    bus.emit_prepared(prepared);
}

#[when("the owner observes confirmed Off")]
fn observed_off(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Changed>(Status::off());
}

#[then("Quit remains in progress without a terminal instruction")]
fn quit_not_ready(world: &mut Menu) {
    let view = last(world);
    assert_eq!(view.text["status"], "Status: Off");
    assert!(!view.flags["quit_enabled"]);
    assert!(!view.flags["terminate"]);
}

#[when("that Quit settles with a retained-resource error")]
fn quit_failed(world: &mut Menu) {
    let mut status = Status::off();
    status.state = State::Error;
    status.is_on = None;
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Settled>(Settlement {
            correlation: world.quit.as_ref().unwrap().clone(),
            result: ControlResult {
                status: Some(status),
                error: Some(Fault::new("test_release", "Retained resource.", true)),
            },
        });
}

#[then("Quit is retryable without termination or re-enabled primary control")]
fn retry_quit(world: &mut Menu) {
    let view = last(world);
    assert!(view.flags["quit_enabled"]);
    assert!(!view.flags["toggle_enabled"]);
    assert!(!view.flags["terminate"]);
    assert!(!view.text["error"].is_empty());
}

#[given("an actual generated native host is observed")]
fn actual_host(world: &mut Menu) {
    world.host = Some(NativeHost::new(Box::new(Forward(world.view.clone()))).unwrap());
    wait_view(&world.view, |frames| {
        frames
            .iter()
            .any(|frame| frame.text.get("status").is_some_and(|s| s == "Status: Off"))
    });
    world.endpoint = Some(
        ai2screenwake_native::identity::runtime_directory()
            .unwrap()
            .join("owner.sock"),
    );
    *world.view.2.lock().unwrap() = world.endpoint.clone();
}

#[when("its typed primary toggle is emitted")]
fn real_toggle(world: &mut Menu) {
    world.toggle_index = world.view.0.lock().unwrap().len();
    world.host.as_ref().unwrap().emit(NativeIntent::Toggle);
}

#[derive(Default)]
struct IpcView(Mutex<Vec<KvEnvelope>>, Condvar);
impl ObservationSink for IpcView {
    fn observed(&self, envelope: KvEnvelope) {
        self.0.lock().unwrap().push(envelope);
        self.1.notify_all();
    }
}

#[then("confirmed On follows pending and the same IPC owner reports On")]
fn shared_on(world: &mut Menu) {
    let frames = wait_view(&world.view, |frames| {
        frames
            .iter()
            .any(|frame| frame.text.get("status").is_some_and(|s| s == "Status: On"))
    });
    let tail = &frames[world.toggle_index..];
    let pending = tail
        .iter()
        .position(|f| f.text.get("status").is_some_and(|s| s == "Status: Pending"))
        .unwrap();
    let on = tail
        .iter()
        .position(|f| f.text.get("status").is_some_and(|s| s == "Status: On"))
        .unwrap();
    assert!(pending < on);
    let location = for_current_user(ai2screenwake_native::identity::runtime_directory().unwrap());
    // The existing owner must connect; this nonexistent bundle must never be launched.
    let launcher = Arc::new(ai2screenwake_native::launch::BundleLauncher::new(
        std::path::PathBuf::from("/nonexistent/ScreenWake.app"),
    ));
    let connection = OwnerConnection::connect(location, launcher).unwrap();
    let observed = Arc::new(IpcView::default());
    connection.observe(observed.clone());
    let query = connection.prepare::<Power::Session::Query>().unwrap();
    let correlation = query.correlation();
    connection.emit(query);
    let entries = observed.0.lock().unwrap();
    let (entries, _) = observed
        .1
        .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Settled>()
                    .unwrap()
                    .is_some_and(|s| s.correlation == correlation)
            })
        })
        .unwrap();
    let settled = entries
        .iter()
        .find_map(|entry| {
            entry
                .payload::<Power::Session::Settled>()
                .unwrap()
                .filter(|s| s.correlation == correlation)
        })
        .unwrap();
    assert!(settled.result.error.is_none());
    assert_eq!(settled.result.status.unwrap().is_on, Some(true));
}

#[when("its typed Quit is emitted")]
fn actual_quit(world: &mut Menu) {
    world.quit_index = world.view.0.lock().unwrap().len();
    world.host.as_ref().unwrap().emit(NativeIntent::Quit);
}

#[then("confirmed Off precedes terminal rendering and the private endpoint is removed")]
fn real_cleaned(world: &mut Menu) {
    let frames = wait_view(&world.view, |frames| {
        frames
            .iter()
            .any(|f| f.flags.get("terminate") == Some(&true))
    });
    let tail = &frames[world.quit_index..];
    let off = tail
        .iter()
        .position(|f| f.text.get("status").is_some_and(|s| s == "Status: Off"))
        .unwrap();
    let terminate = tail
        .iter()
        .position(|f| f.flags.get("terminate") == Some(&true))
        .unwrap();
    assert!(off < terminate);
    assert_eq!(
        *world.view.3.lock().unwrap(),
        Some(false),
        "foreign terminal delivery must follow private-service disposal"
    );
    assert!(!world.endpoint.as_ref().unwrap().exists());
}

#[when("a second generated host attempts admission")]
fn second_host(world: &mut Menu) {
    world.second = Some(NativeHost::new(Box::new(Forward(world.second_view.clone()))).unwrap());
    wait_view(&world.second_view, |frames| {
        frames
            .iter()
            .any(|f| f.text.get("status").is_some_and(|s| s == "Status: Error"))
    });
}

#[then("its error observation never claims Off and its Quit leaves the first owner available")]
fn second_failed(world: &mut Menu) {
    assert!(
        !world
            .second_view
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|f| f.text.get("status").is_some_and(|s| s == "Status: Off"))
    );
    world.second.as_ref().unwrap().emit(NativeIntent::Quit);
    wait_view(&world.second_view, |frames| {
        frames
            .iter()
            .any(|f| f.flags.get("terminate") == Some(&true))
    });
    assert!(world.endpoint.as_ref().unwrap().exists());
    actual_quit(world);
    wait_view(&world.view, |frames| {
        frames
            .iter()
            .any(|f| f.flags.get("terminate") == Some(&true))
    });
    assert!(!world.endpoint.as_ref().unwrap().exists());
}

#[when("newer Off is followed by older On snapshots and changes")]
fn stale_projection(world: &mut Menu) {
    let bus = world.bus.as_ref().unwrap();
    let mut off = Status::off();
    off.revision = 12;
    bus.emit::<Power::Session::Changed>(off);
    let mut on = Status::off();
    on.revision = 11;
    on.state = State::On;
    on.desired_on = true;
    on.is_on = Some(true);
    bus.emit::<Power::Session::Snapshot>(on.clone());
    bus.emit::<Power::Session::Changed>(on);
}

#[then("the menu never renders the older awake state")]
fn stale_ignored(world: &mut Menu) {
    let frames = world.view.0.lock().unwrap();
    assert_eq!(
        frames.len(),
        1,
        "older revisions must not emit stale rendering packets"
    );
    assert_eq!(frames[0].text["status"], "Status: Off");
}

#[when("a same-revision snapshot supplies confirmed On after Pending")]
fn equal_projection(world: &mut Menu) {
    let bus = world.bus.as_ref().unwrap();
    let mut status = Status::off();
    status.revision = 12;
    status.state = State::Pending;
    status.is_on = None;
    bus.emit::<Power::Session::Changed>(status.clone());
    status.state = State::On;
    status.is_on = Some(true);
    status.desired_on = true;
    bus.emit::<Power::Session::Snapshot>(status);
}

#[then("the same-revision observation is rendered rather than discarded")]
fn equal_accepted(world: &mut Menu) {
    assert_eq!(last(world).text["status"], "Status: On");
}

#[given("a persistent owner queue with a native completion journal barrier")]
fn persistent_projection(world: &mut Menu) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target");
    let directory = tempfile::Builder::new()
        .prefix("u")
        .tempdir_in(root)
        .unwrap();
    let location = for_current_user(directory.path().join("o"));
    let server = OwnerServer::bind(location.clone(), world.queued_native.clone()).unwrap();
    let bus = server.actions();
    world.subscription = Some(observe_menu(&bus, world.view.clone()));
    bus.observe(world.queue_trace.clone());
    let barrier = world.queue_barrier.clone();
    bus.observe_action::<Power::Assertions::Observed>(move |result, _| {
        if result.request.enabled {
            let mut state = barrier.barrier.lock().unwrap();
            state.0 = true;
            barrier.changed.notify_all();
            let (state, _) = barrier
                .changed
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.1)
                .unwrap();
            assert!(
                state.1,
                "the journalled completion barrier was not released"
            );
        }
    });
    let remote = OwnerConnection::connect(
        location,
        Arc::new(ai2screenwake_native::launch::BundleLauncher::new(
            "/nonexistent/ScreenWake.app".into(),
        )),
    )
    .unwrap();
    remote.observe(world.queue_remote_trace.clone());
    world.queue_remote = Some(remote);
    world.queue_server = Some(server);
    world.queue_directory = Some(directory);
    world.bus = Some(bus);
}

fn finish_apply(request: Apply, events: Emitter) {
    let enabled = request.enabled;
    events.emit::<Power::Assertions::Observed>(AssertionsObserved {
        request,
        held_count: if enabled { 2 } else { 0 },
        outcome: Outcome::Applied,
        fault_codes: Vec::new(),
        clock: ClockSample {
            epoch_ms: 1_800_000_000_000,
            monotonic_ms: 1000,
        },
    });
}

#[when("Off is queued behind the journalled On completion before derived state drains")]
fn queued_off(world: &mut Menu) {
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Session::TurnOn>(Empty {});
    let (request, events) = world.queued_native.pending(true);
    finish_apply(request, events);
    let journal = &world.queue_barrier;
    let state = journal.barrier.lock().unwrap();
    let (mut state, _) = journal
        .changed
        .wait_timeout_while(state, Duration::from_secs(2), |state| !state.0)
        .unwrap();
    assert!(
        state.0,
        "the actual persistent actor did not reach its append barrier"
    );
    bus.emit::<Power::Session::TurnOff>(Empty {});
    state.1 = true;
    journal.changed.notify_all();
    drop(state);
    let (request, events) = world.queued_native.pending(false);
    finish_apply(request, events);
    wait_view(&world.view, |frames| {
        frames
            .iter()
            .any(|frame| frame.text["status"] == "Status: Off")
    });
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote.prepare::<Power::Session::Query>().unwrap();
    world.quit = Some(query.correlation());
    remote.emit(query);
    let entries = world.queue_remote_trace.0.lock().unwrap();
    let (entries, _) = world
        .queue_remote_trace
        .1
        .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Settled>()
                    .unwrap()
                    .is_some_and(|settled| Some(&settled.correlation) == world.quit.as_ref())
            })
        })
        .unwrap();
    assert!(entries.iter().any(|entry| {
        entry
            .payload::<Power::Session::Settled>()
            .unwrap()
            .is_some_and(|settled| Some(&settled.correlation) == world.quit.as_ref())
    }));
}

#[then("rendered protection revisions never regress and menu and remote status agree Off")]
fn ordered_persistent_projection(world: &mut Menu) {
    let entries = world.queue_trace.0.lock().unwrap();
    let mut statuses = Vec::new();
    let mut rendered = Vec::new();
    for entry in entries.iter() {
        let status = entry
            .payload::<Power::Session::Changed>()
            .unwrap()
            .or_else(|| entry.payload::<Power::Session::Snapshot>().unwrap());
        if let Some(status) = status {
            statuses.push((entry.correlation().unwrap(), status));
        }
        if let Some(render) = entry.payload::<Ui::Menu::Render>().unwrap() {
            if rendered.is_empty() && render.text["status"] == "Status: Error" {
                continue;
            }
            if let Some((_, status)) = statuses
                .iter()
                .rev()
                .find(|(correlation, _)| *correlation == entry.correlation().unwrap())
            {
                let expected = match status.state {
                    State::On => "Status: On",
                    State::Off => "Status: Off",
                    State::Pending => "Status: Pending",
                    State::Error => "Status: Error",
                };
                assert_eq!(render.text["status"], expected);
                rendered.push((status.revision, render.text["status"].clone()));
            }
        }
    }
    assert!(rendered.iter().any(|(_, phase)| phase == "Status: Pending"));
    assert!(rendered.iter().any(|(_, phase)| phase == "Status: On"));
    assert!(
        rendered.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "rendered revisions regressed: {rendered:?}"
    );
    let remote_entries = world.queue_remote_trace.0.lock().unwrap();
    let result = remote_entries
        .iter()
        .find_map(|entry| {
            entry
                .payload::<Power::Session::Settled>()
                .unwrap()
                .filter(|settled| Some(&settled.correlation) == world.quit.as_ref())
        })
        .unwrap();
    assert!(result.result.error.is_none());
    assert_eq!(result.result.status.unwrap().is_on, Some(false));
    assert_eq!(last(world).text["status"], "Status: Off");
}

fn main() {
    let features =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/features/ui.feature");
    futures::executor::block_on(
        Menu::cucumber()
            .max_concurrent_scenarios(3)
            .run_and_exit(features),
    );
}
#[when(expr = "the private disposal gate exercises {word}")]
fn disposal_gate(world: &mut Menu, gate: String) {
    use std::sync::mpsc;
    let bus = RegistryBuilder::default().build().unwrap();
    let signals = Arc::new(shutdown::Signals::default());
    signals.cleaning.store(true, Ordering::Release);
    shutdown::observe_cleanup(&bus, signals.clone());
    if gate == "peer_error" {
        bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
            "peer_write",
            "Peer disconnected.",
            true,
        ));
        assert!(
            !signals.cleanup_failed.load(Ordering::Acquire),
            "peer delivery is not endpoint disposal"
        );
    }
    let (failed, failures) = mpsc::channel();
    bus.observe_action::<Error::Ipc::Protocol::Occurred>(move |fault, _| {
        if fault.fault_code == "ui_shutdown_failed" {
            failed.send(()).unwrap();
        }
    });
    let (witness, disposal) = mpsc::sync_channel(1);
    let (commands, receiver) = mpsc::sync_channel(1);
    let worker_bus = bus.clone();
    let worker_signals = signals.clone();
    let observer = world.view.clone();
    if gate == "completion" {
        signals.cleanup_failed.store(true, Ordering::Release);
        witness.send(()).unwrap();
    } else if gate == "peer_error" {
        witness.send(()).unwrap();
    }
    let worker = std::thread::spawn(move || {
        shutdown::deliver_after_disposal(
            disposal,
            receiver,
            worker_bus,
            observer,
            worker_signals,
            Ui::Projection {
                text: HashMap::new(),
                flags: HashMap::from([("terminate".into(), true)]),
            },
        )
    });
    if gate != "peer_error" {
        failures.recv_timeout(Duration::from_secs(3)).unwrap();
        if gate == "late_error" {
            let collecting = signals.cleaning.load(Ordering::Acquire);
            bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                "socket_cleanup",
                "Endpoint removal failed.",
                true,
            ));
            let retained_error = signals.cleanup_failed.load(Ordering::Acquire);
            witness.send(()).unwrap();
            commands.send(NativeIntent::Quit).unwrap();
            failures.recv_timeout(Duration::from_secs(3)).unwrap();
            drop(commands);
            worker.join().unwrap();
            assert!(
                collecting && retained_error,
                "late disposal errors must stay observable after timeout"
            );
            assert!(
                world.view.0.lock().unwrap().is_empty(),
                "failed endpoint disposal must never approve termination"
            );
            world.disposal_proved = true;
            return;
        }
        signals.cleanup_failed.store(false, Ordering::Release);
        commands.send(NativeIntent::Quit).unwrap();
    }
    let views = world.view.0.lock().unwrap();
    let (views, _) = world
        .view
        .1
        .wait_timeout_while(views, Duration::from_secs(3), |views| views.is_empty())
        .unwrap();
    let terminated = views
        .iter()
        .any(|view| view.flags.get("terminate") == Some(&true));
    drop(views);
    drop(commands);
    worker.join().unwrap();
    assert!(
        terminated,
        "completed disposal must remain usable across explicit retry"
    );
    assert!(!signals.cleaning.load(Ordering::Acquire));
    world.disposal_proved = true;
}

#[then("terminal delivery requires the retained completion and genuine cleanup evidence")]
fn disposal_evidence(world: &mut Menu) {
    assert!(world.disposal_proved);
}
#[when("the final journal cleanup fault reaches the native shutdown helper")]
fn queued_cleanup_fault(world: &mut Menu) {
    use std::sync::mpsc;
    struct Journal;
    impl ai2screenwake_client::contract::ActionJournal for Journal {
        fn history(&self) -> Result<ai2screenwake_client::contract::JournalHistory, Fault> {
            Ok(Default::default())
        }
        fn append(&self, _: u64, _: &KvEnvelope) -> Result<(), Fault> {
            Ok(())
        }
    }
    struct Witness {
        completed: mpsc::SyncSender<()>,
        signals: Arc<shutdown::Signals>,
        error_before_disposal: Arc<AtomicBool>,
    }
    impl ObservationSink for Witness {
        fn observed(&self, _: KvEnvelope) {}
    }
    impl Drop for Witness {
        fn drop(&mut self) {
            self.error_before_disposal.store(
                self.signals.cleanup_failed.load(Ordering::Acquire),
                Ordering::Release,
            );
            self.completed.send(()).unwrap();
        }
    }
    let signals = Arc::new(shutdown::Signals::default());
    signals.cleaning.store(true, Ordering::Release);
    let before_disposal = Arc::new(AtomicBool::new(false));
    let mut builder = RegistryBuilder::default();
    builder.journal(Arc::new(Journal));
    let bus = builder.build().unwrap();
    shutdown::observe_cleanup(&bus, signals.clone());
    let (completed, disposal) = mpsc::sync_channel(1);
    bus.observe(Arc::new(Witness {
        completed,
        signals: signals.clone(),
        error_before_disposal: before_disposal.clone(),
    }));
    bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
        "socket_cleanup",
        "Owned endpoint removal failed.",
        true,
    ));
    drop(bus);
    let diagnostics = RegistryBuilder::default().build().unwrap();
    let (failed, failure) = mpsc::sync_channel(1);
    diagnostics.observe_action::<Error::Ipc::Protocol::Occurred>(move |fault, _| {
        if fault.fault_code == "ui_shutdown_failed" {
            failed.send(()).unwrap();
        }
    });
    let (commands, receiver) = mpsc::sync_channel(1);
    let observer = world.view.clone();
    let worker_signals = signals.clone();
    let worker = std::thread::spawn(move || {
        shutdown::deliver_after_disposal(
            disposal,
            receiver,
            diagnostics,
            observer,
            worker_signals,
            Ui::Projection {
                text: HashMap::new(),
                flags: HashMap::from([("terminate".into(), true)]),
            },
        )
    });
    let rejected = failure.recv_timeout(Duration::from_secs(2)).is_ok();
    drop(commands);
    worker.join().unwrap();
    assert!(
        rejected && before_disposal.load(Ordering::Acquire),
        "the actual cleanup collector must observe the queued fault before disposal acknowledgement"
    );
    assert!(signals.cleanup_failed.load(Ordering::Acquire));
    assert!(
        world.view.0.lock().unwrap().is_empty(),
        "a genuine unresolved disposal failure cannot approve termination"
    );
    world.disposal_proved = true;
}
// end ui.rs
