// ui.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Executable rendering requirements at the canonical Actions/generated-callback seam.

use ai2screenwake_client::{
    contract::{
        ControlResult, CorrelationId, Empty, Error, Fault, KvEnvelope, Log, LogSnapshot,
        ObservationSink, Power, Registry, RegistryBuilder, Settlement, State, Status, Subscription,
        Ui, WeakEmitter,
    },
    ffi::{NativeHost, NativeIntent, NativeObserver, observe_menu},
    ipc::{OwnerConnection, for_current_user},
};
use cucumber::{World, given, then, when};
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

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
}

impl std::fmt::Debug for Menu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Menu").finish_non_exhaustive()
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
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

fn main() {
    let features =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/features/ui.feature");
    futures::executor::block_on(
        Menu::cucumber()
            .max_concurrent_scenarios(3)
            .run_and_exit(features),
    );
}
// end ui.rs
