// ui.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Executable rendering requirements at the canonical Actions/generated-callback seam.

use ai2screenwake_client::{
    contract::{
        Action, Apply, AssertionsObserved, ClockSample, ControlResult, CorrelationId, Emitter,
        Empty, Error, FailedApply, Fault, IdleObserved, IdleRequest, KvEnvelope, Log, LogSnapshot,
        ObservationSink, Outcome, Power, Registry, RegistryBuilder, Settlement, State, Status,
        Subscription, Ui, WeakEmitter,
    },
    ffi::{NativeHost, NativeIntent, NativeObserver, observe_menu},
    ipc::{OwnerConnection, OwnerServer, for_current_user},
    telemetry::Configuration,
};
use ai2screenwake_native::{
    NativePower,
    corners::CornerApi,
    power::{AssertionKind, MacPower, PowerApi, SystemPower},
};
use cucumber::{World, given, then, when};
use std::{
    collections::{HashMap, VecDeque},
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

    fn sample(&self, request: IdleRequest, events: Emitter) {
        assert!(
            self.2.load(Ordering::SeqCst),
            "the queued Off witness must not sample"
        );
        events.emit::<Power::Activity::System::Observed>(IdleObserved {
            request,
            clock: ClockSample {
                epoch_ms: 1_800_000_001_000,
                monotonic_ms: 2000,
            },
            idle_age_ms: Some(1000),
            fault: None,
        });
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

/// Actual IOKit/clock/idle calls with an explicitly absent fixture corner capability.
#[derive(Default)]
struct AssertionOnlySystemPower {
    held: Mutex<Vec<u32>>,
    acquired: AtomicUsize,
    released: AtomicUsize,
    corner_reads: AtomicUsize,
}

impl PowerApi for AssertionOnlySystemPower {
    fn corners(&self) -> Option<Arc<dyn CornerApi>> {
        self.corner_reads.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn acquire(&self, kind: AssertionKind) -> Result<u32, Fault> {
        let id = SystemPower.acquire(kind)?;
        self.held.lock().unwrap().push(id);
        self.acquired.fetch_add(1, Ordering::SeqCst);
        Ok(id)
    }

    fn release(&self, id: u32) -> Result<(), Fault> {
        SystemPower.release(id)?;
        let mut held = self.held.lock().unwrap();
        let index = held
            .iter()
            .position(|held| *held == id)
            .expect("fixture-owned IOKit ID");
        held.remove(index);
        self.released.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn clock(&self) -> Result<ClockSample, Fault> {
        SystemPower.clock()
    }

    fn idle_seconds(&self) -> Result<f64, Fault> {
        SystemPower.idle_seconds()
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
    host_power: Arc<AssertionOnlySystemPower>,
    host_directory: Option<tempfile::TempDir>,
    host_parent_directory: Option<tempfile::TempDir>,
    host_project_directory: Option<tempfile::TempDir>,
    host_socket_budget: Option<usize>,
    second: Option<Arc<NativeHost>>,
    second_view: Arc<View>,
    quit_index: usize,
    toggle_index: usize,
    endpoint: Option<std::path::PathBuf>,
    control_count: Arc<AtomicUsize>,
    controls_before_fault: usize,
    queued_native: Arc<QueuedNative>,
    held_native: Option<(Apply, Emitter)>,
    queue_barrier: Arc<QueueBarrier>,
    queue_directory: Option<tempfile::TempDir>,
    queue_trace: Arc<IpcView>,
    queue_remote_trace: Arc<IpcView>,
    queue_remote: Option<OwnerConnection>,
    queue_server: Option<OwnerServer>,
    disposal_proved: bool,
    attention_result: Option<ControlResult>,
    ordinary_off_frame: Option<Ui::Projection>,
    expected_frames: Vec<(String, String)>,
    pending_frames: Vec<Ui::Projection>,
    cycle_frames: Vec<Ui::Projection>,
    queued_preview_ids: Vec<CorrelationId>,
    queued_preview_retirement: Option<String>,
    info_opened: Option<bool>,
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
            self.finish_held_native();
            for (request, events) in std::mem::take(&mut *self.queued_native.0.lock().unwrap()) {
                finish_apply(request, events);
            }
            self.bus
                .as_ref()
                .unwrap()
                .emit::<Power::Session::Protection::Quit>(Empty {});
            let entries = self.queue_trace.0.lock().unwrap();
            let (entries, _) = self
                .queue_trace
                .1
                .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
                    !entries.iter().any(|entry| {
                        entry
                            .payload::<Power::Session::Protection::Closed>()
                            .unwrap()
                            .is_some()
                    })
                })
                .unwrap();
            assert!(
                entries.iter().any(|entry| entry
                    .payload::<Power::Session::Protection::Closed>()
                    .unwrap()
                    .is_some()),
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

impl Menu {
    fn hold_native(&mut self, enabled: bool) {
        assert!(
            self.held_native.is_none(),
            "fixture already owns a held native operation"
        );
        self.held_native = Some(self.queued_native.pending(enabled));
    }

    fn finish_held_native(&mut self) {
        if let Some((request, events)) = self.held_native.take() {
            finish_apply(request, events);
        }
    }
}

fn last(world: &Menu) -> Ui::Projection {
    world.view.0.lock().unwrap().last().unwrap().clone()
}

#[given("a private menu whose current native acquisition fails permission and cleanup")]
fn failed_permission_menu(world: &mut Menu) {
    inactive_menu(world, "Unconfigured".into());
    world.queued_native.2.store(false, Ordering::SeqCst);
    let bus = world.bus.as_ref().unwrap();
    let control = bus
        .prepare::<Power::Session::Protection::TurnOn>(Empty {})
        .unwrap();
    let id = control.correlation();
    bus.emit_prepared(control);
    let (request, events) = world.queued_native.pending(true);
    let fault = Fault::new(
        "corner_accessibility_required",
        "Controlled original permission diagnostic, not display copy.",
        true,
    );
    events.error::<Error::Native::Assertions::Occurred>(fault.clone());
    events.error::<Error::Native::Assertions::Occurred>(Fault::new(
        "fixture_cleanup_failed",
        "Controlled secondary cleanup diagnostic.",
        true,
    ));
    events.emit::<Power::Assertions::Resources::Failed>(FailedApply {
        request,
        held_count: 0,
        fault,
    });
    let result = await_settlement(&world.queue_trace, &id);
    assert_eq!(
        result.error.unwrap().fault_code,
        "corner_accessibility_required"
    );
    wait_face(world, "error");
}

#[when("the failed owner is queried twice through its real private connection")]
fn query_failed_permission_menu(world: &mut Menu) {
    for _ in 0..2 {
        let remote = world.queue_remote.as_ref().unwrap();
        let query = remote
            .prepare::<Power::Session::Protection::Query>()
            .unwrap();
        let id = query.correlation();
        remote.emit(query);
        world.attention_result = Some(await_settlement(&world.queue_remote_trace, &id));
    }
}

#[then("permission recovery remains specific in the menu tooltip and remote result")]
fn retained_permission_menu(world: &mut Menu) {
    let result = world.attention_result.as_ref().unwrap();
    let fault = result.error.as_ref().unwrap();
    assert_eq!(fault.fault_code, "corner_accessibility_required");
    assert_eq!(
        fault.message,
        "Controlled original permission diagnostic, not display copy."
    );
    let status = result.status.as_ref().unwrap();
    assert_eq!(status.is_on, None);
    assert_eq!(status.state, State::Error);
    let frames = wait_view(&world.view, |frames| {
        frames
            .last()
            .is_some_and(|frame| frame.text["error"].starts_with("Allow AI2ScreenWake"))
    });
    let frame = frames.last().unwrap();
    assert_eq!(frame.text["status"], "Off: Error");
    assert_eq!(frame.text["icon_asset"], "menu-error");
    assert_eq!(
        frame.text["error"],
        "Allow AI2ScreenWake in System Settings > Privacy & Security > Accessibility, then turn No Sleep on again."
    );
    assert!(frame.text["tooltip"].contains(&frame.text["error"]));
    assert!(
        frame.text["tooltip"].contains("Idle sleep prevention itself does not need Accessibility.")
    );
    assert!(frame.text["tooltip"].contains(
        "The configured corner guard uses Accessibility to modify mouse movement, not to read keys."
    ));
    assert!(!frame.text["tooltip"].contains("Controlled"));
    assert!(frame.flags["toggle_enabled"] && frame.flags["quit_enabled"]);
    let first_recovery = frames
        .iter()
        .position(|frame| frame.text["error"].starts_with("Allow AI2ScreenWake"))
        .unwrap();
    assert!(
        frames[first_recovery..]
            .iter()
            .all(|frame| { frame.text["error"].starts_with("Allow AI2ScreenWake") }),
        "secondary cleanup or query errors must not replace actionable recovery with generic copy"
    );
}

#[when("explicit Off and On are retried after that real failure")]
fn retry_permission_menu(world: &mut Menu) {
    world.queued_native.2.store(true, Ordering::SeqCst);
    for enabled in [false, true, false] {
        let result = if enabled {
            owner_control::<Power::Session::Protection::TurnOn>(world)
        } else {
            owner_control::<Power::Session::Protection::TurnOff>(world)
        };
        assert!(result.error.is_none());
        assert_eq!(result.status.unwrap().is_on, Some(enabled));
        let expected = if enabled {
            "Observed protection: On"
        } else {
            "Observed protection: Off"
        };
        let frames = wait_view(&world.view, |frames| {
            frames
                .last()
                .is_some_and(|frame| frame.text["accessibility"].contains(expected))
        });
        world.cycle_frames.push(frames.last().unwrap().clone());
    }
}

#[then("the controls remain available and actual power changes without hiding the fault")]
fn retry_permission_menu_truth(world: &mut Menu) {
    assert_eq!(world.cycle_frames.len(), 3);
    for frame in &world.cycle_frames {
        assert_eq!(frame.text["status"], "Off: Error");
        assert_eq!(frame.text["icon_asset"], "menu-error");
        assert!(frame.text["error"].starts_with("Allow AI2ScreenWake"));
        assert!(frame.flags["toggle_enabled"] && frame.flags["quit_enabled"]);
    }
}

#[then(
    "capability help distinguishes the authorized corner guard from locking forced sleep and automatic screensavers"
)]
fn corner_capability_help(world: &mut Menu) {
    let frame = last(world);
    assert_eq!(frame.text["status"], "Mode: No Sleep");
    for required in [
        "configured manual Start Screen Saver corners",
        "authorization",
        "configuration changes",
        "Forced sleep and security locks remain available",
        "Automatic screensaver protection is unverified",
    ] {
        assert!(
            frame.text["tooltip"].contains(required),
            "missing capability boundary {required}: {:?}",
            frame.text
        );
    }
    assert!(!frame.text["tooltip"].contains("Manual screensavers, forced sleep"));
}

#[when("native corner authorization reports a permission denial")]
fn corner_permission_denied(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Error::Native::Assertions::Occurred>(Fault::new(
            "corner_accessibility_required",
            "private diagnostic must not become arbitrary user copy",
            true,
        ));
}

#[then(
    "the menu requests target-app Accessibility permission without inventing protection or a global grant"
)]
fn corner_permission_recovery(world: &mut Menu) {
    let frame = last(world);
    assert_eq!(frame.text["status"], "Off: Error");
    assert_eq!(frame.text["icon_asset"], "menu-error");
    assert_eq!(
        frame.text["error"],
        "Allow AI2ScreenWake in System Settings > Privacy & Security > Accessibility, then turn No Sleep on again."
    );
    assert!(frame.text["tooltip"].contains(&frame.text["error"]));
    assert!(!frame.text["tooltip"].contains("private diagnostic"));
    assert_eq!(world.control_count.load(Ordering::SeqCst), 0);
}

#[then("the footer matches the compiled build and website opening is not requested")]
fn build_and_info_projection(world: &mut Menu) {
    let frame = last(world);
    assert_eq!(
        frame.text["version"],
        ai2screenwake_client::build_info::VERSION
    );
    assert_eq!(frame.text["info_url"], "https://ai2sw.info");
    assert_eq!(frame.text["info_label"], "About AI2ScreenWake");
    assert!(!frame.text["info_help"].is_empty());
    assert!(!frame.flags["open_info"]);
    assert_eq!(world.control_count.load(Ordering::SeqCst), 0);
}

#[when(expr = "an explicit information activation completes with {word}")]
fn information_activation(world: &mut Menu, outcome: String) {
    let opened = match outcome.as_str() {
        "success" => true,
        "failure" => false,
        _ => panic!("unknown native website outcome"),
    };
    let bus = world.bus.as_ref().unwrap();
    bus.observe(world.queue_trace.clone());
    bus.emit::<Ui::Info::Website::Activated>(Empty {});
    assert!(last(world).flags["open_info"]);
    assert_eq!(last(world).text["info_url"], "https://ai2sw.info");
    bus.emit::<Ui::Info::Website::Completed>(Ui::WebsiteCompletion { opened });
    world.info_opened = Some(opened);
}

#[when(expr = "a native-channel overflow carries website {word}")]
fn website_overflow(world: &mut Menu, outcome: String) {
    let bus = world.bus.as_ref().unwrap();
    bus.observe(world.queue_trace.clone());
    bus.emit::<Ui::Info::Website::Activated>(Empty {});
    bus.emit::<Ui::Info::Website::Activated>(Empty {});
    assert!(world.queue_trace.0.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some_and(|fault| fault.fault_code == "ui_website_pending")
    }));
    let (commands, _receiver) = std::sync::mpsc::sync_channel(32);
    for _ in 0..32 {
        commands.try_send(NativeIntent::Toggle).unwrap();
    }
    let opened = outcome == "success";
    let signals = shutdown::Signals::default();
    assert!(
        shutdown::enqueue(
            &commands,
            &signals,
            if opened {
                NativeIntent::InfoOpened
            } else {
                NativeIntent::InfoFailed
            }
        )
        .is_ok()
    );
    shutdown::drain_inputs(&signals, bus);
    world.info_opened = Some(opened);
}

#[then("completion recovery preserves attribution and admits the next explicit website request")]
fn website_overflow_recovered(world: &mut Menu) {
    let entries = world.queue_trace.0.lock().unwrap();
    let activated = entries
        .iter()
        .find(|entry| {
            entry
                .payload::<Ui::Info::Website::Activated>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let overflow = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Error::Dispatch::Validation::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "ui_input_overflow")
        })
        .unwrap();
    let completed = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Ui::Info::Website::Completed>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let result = entries
        .iter()
        .find(|entry| {
            if world.info_opened == Some(true) {
                entry
                    .payload::<Ui::Info::Website::Opened>()
                    .unwrap()
                    .is_some()
            } else {
                entry
                    .payload::<Error::Dispatch::Validation::Occurred>()
                    .unwrap()
                    .is_some_and(|fault| fault.fault_code == "ui_website_open_failed")
            }
        })
        .unwrap();
    assert!(overflow < completed);
    assert_eq!(
        activated.correlation().unwrap(),
        result.correlation().unwrap()
    );
    let effects = entries
        .iter()
        .filter(|entry| {
            entry
                .payload::<Ui::Menu::Presentation::Render>()
                .unwrap()
                .is_some_and(|frame| frame.flags["open_info"])
        })
        .count();
    drop(entries);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Ui::Info::Website::Activated>(Empty {});
    assert_eq!(
        world
            .queue_trace
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry
                .payload::<Ui::Menu::Presentation::Render>()
                .unwrap()
                .is_some_and(|frame| frame.flags["open_info"]))
            .count(),
        effects + 1
    );
}

#[then("its logged native effect and correlated outcome never change protection")]
fn information_outcome(world: &mut Menu) {
    let entries = world.queue_trace.0.lock().unwrap();
    let (intent_index, intent) = entries
        .iter()
        .enumerate()
        .find(|(_, entry)| {
            entry
                .payload::<Ui::Info::Website::Activated>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let effect = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Ui::Menu::Presentation::Render>()
                .unwrap()
                .is_some_and(|frame| frame.flags["open_info"])
        })
        .unwrap();
    let completed = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Ui::Info::Website::Completed>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(intent_index < effect && effect < completed);
    let result_index = if world.info_opened == Some(true) {
        entries
            .iter()
            .position(|entry| {
                entry
                    .payload::<Ui::Info::Website::Opened>()
                    .unwrap()
                    .is_some()
            })
            .unwrap()
    } else {
        let index = entries
            .iter()
            .position(|entry| {
                entry
                    .payload::<Error::Dispatch::Validation::Occurred>()
                    .unwrap()
                    .is_some_and(|fault| {
                        fault.fault_code == "ui_website_open_failed" && fault.failed_genus.is_some()
                    })
            })
            .unwrap();
        let error_render = entries
            .iter()
            .position(|entry| {
                entry
                    .payload::<Ui::Menu::Presentation::Render>()
                    .unwrap()
                    .is_some_and(|frame| !frame.text["error"].is_empty())
            })
            .unwrap();
        assert!(
            index < error_render,
            "failure must be logged before error presentation"
        );
        index
    };
    assert!(completed < result_index);
    assert_eq!(
        entries[result_index].correlation().unwrap(),
        intent.correlation().unwrap()
    );
    assert_eq!(world.control_count.load(Ordering::SeqCst), 0);
    drop(entries);
    confirmed(world);
    assert!(
        !last(world).flags["open_info"],
        "ordinary rendering cannot repeat the native effect"
    );
}

#[then("visible values use sleep words while accessible Pending remains truthful")]
fn sleep_labels(world: &mut Menu) {
    let frames = world.view.0.lock().unwrap();
    assert_eq!(frames.len(), world.expected_frames.len());
    for (frame, (phase, face)) in frames.iter().zip(&world.expected_frames) {
        assert_eq!(
            frame.text["status"],
            if face == "on" {
                "Mode: No Sleep"
            } else {
                "Mode: Can Sleep"
            }
        );
        assert!(frame.text["accessibility"].contains(phase));
        assert!(frame.text["tooltip"].contains("Forced sleep and security locks remain available"));
        if phase == "Pending" {
            assert!(frame.text["tooltip"].contains("Pending"));
        }
    }
}

fn activate_preview(world: &Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Ui::Preview::Control::Activated>(Empty {});
}

#[when("the large TV is activated repeatedly after a genuine fault while confirmed On")]
fn preview_after_real_fault(world: &mut Menu) {
    assert_eq!(
        owner_control::<Power::Session::Protection::TurnOn>(world)
            .status
            .unwrap()
            .is_on,
        Some(true)
    );
    wait_face(world, "on");
    private_fault(world);
    wait_face(world, "error");
    for enabled in [false, true, false] {
        let bus = world.bus.as_ref().unwrap();
        let request = bus
            .prepare::<Ui::Preview::Control::Activated>(Empty {})
            .unwrap();
        let id = request.correlation();
        bus.emit_prepared(request);
        let result = await_settlement(&world.queue_trace, &id);
        assert!(result.error.is_none());
        assert_eq!(
            result.status.as_ref().unwrap().is_on,
            Some(enabled),
            "a genuine display fault cannot force every preview input to enable"
        );
        let remote = world.queue_remote.as_ref().unwrap();
        let query = remote
            .prepare::<Power::Session::Protection::Query>()
            .unwrap();
        let id = query.correlation();
        remote.emit(query);
        let result = await_settlement(&world.queue_remote_trace, &id);
        assert!(result.error.is_none());
        assert_eq!(result.status.unwrap().is_on, Some(enabled));
        world.cycle_frames.push(last(world));
    }
}

#[then("each real power target alternates while the genuine fault remains visible")]
fn preview_fault_stays_visible(world: &mut Menu) {
    assert_eq!(world.cycle_frames.len(), 3);
    for frame in &world.cycle_frames {
        assert_eq!(frame.text["status"], "Off: Error");
        assert!(!frame.text["error"].is_empty());
        assert!(!frame.text["tooltip"].contains("Error preview"));
        assert!(!frame.text["tooltip"].contains("private"));
    }
}

#[when("an isolated owner fixture unwinds while holding native Off")]
fn fixture_unwinds_held_off(world: &mut Menu) {
    let mut fixture = Menu::default();
    inactive_menu(&mut fixture, "Unconfigured".into());
    owner_control::<Power::Session::Protection::TurnOn>(&fixture);
    wait_face(&fixture, "on");
    fixture.queued_native.2.store(false, Ordering::SeqCst);
    fixture
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOff>(Empty {});
    fixture.hold_native(false);
    let operation = fixture.held_native.as_ref().unwrap().0.operation_id.clone();
    let trace = fixture.queue_trace.clone();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _owned_fixture = fixture;
        panic!("intentional step failure before native completion");
    }));
    assert!(
        unwind.is_err(),
        "the original simulated step failure is retained"
    );
    let entries = trace.0.lock().unwrap().clone();
    let completions: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            entry
                .payload::<Power::Assertions::Resources::Observed>()
                .unwrap()
                .is_some_and(|result| result.request.operation_id == operation)
        })
        .collect();
    assert_eq!(completions.len(), 1);
    let closed = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Protection::Closed>()
                .unwrap()
                .is_some()
        })
        .expect("fixture teardown must observe real Closed");
    assert!(completions[0].0 < closed);
    world.disposal_proved = true;
}

#[then("its held operation completes exactly once before the real Closed observation")]
fn held_fixture_closed(world: &mut Menu) {
    assert!(world.disposal_proved);
}

fn queued_preview_roots(world: &mut Menu, retirement: Option<String>, count: usize) {
    let initial = owner_control::<Power::Session::Protection::TurnOff>(world);
    assert!(initial.error.is_none());
    wait_face(world, "off");
    let bus = world.bus.as_ref().unwrap();
    let pause = bus
        .prepare::<Power::Session::Protection::Query>(Empty {})
        .unwrap();
    let pause_id = pause.correlation();
    let barrier = world.queue_barrier.clone();
    bus.observe_action::<Power::Session::Protection::Query>(move |_, events| {
        if events.correlation() == pause_id {
            let mut state = barrier.barrier.lock().unwrap();
            state.0 = true;
            barrier.changed.notify_all();
            let (state, _) = barrier
                .changed
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.1)
                .unwrap();
            assert!(
                state.1,
                "the persistent journal actor's query barrier must be released"
            );
        }
    });
    bus.emit_prepared(pause);
    let state = world.queue_barrier.barrier.lock().unwrap();
    let (mut state, _) = world
        .queue_barrier
        .changed
        .wait_timeout_while(state, Duration::from_secs(2), |state| !state.0)
        .unwrap();
    assert!(
        state.0,
        "the real owner must reach the logged query observer before root enqueue"
    );
    for _ in 0..count {
        let activated = bus
            .prepare::<Ui::Preview::Control::Activated>(Empty {})
            .unwrap();
        world.queued_preview_ids.push(activated.correlation());
        bus.emit_prepared(activated);
    }
    assert_ne!(world.queued_preview_ids[0], world.queued_preview_ids[1]);
    let ordinary_id = match retirement.as_deref() {
        None => None,
        Some("setter") => {
            let request = bus
                .prepare::<Power::Session::Protection::TurnOff>(Empty {})
                .unwrap();
            let id = request.correlation();
            bus.emit_prepared(request);
            Some(id)
        }
        Some("toggle") => {
            let request = bus
                .prepare::<Power::Session::Protection::Toggle>(Empty {})
                .unwrap();
            let id = request.correlation();
            bus.emit_prepared(request);
            Some(id)
        }
        Some("quit") => {
            let request = bus
                .prepare::<Power::Session::Protection::Quit>(Empty {})
                .unwrap();
            let id = request.correlation();
            bus.emit_prepared(request);
            Some(id)
        }
        Some(other) => panic!("unknown ordinary control {other}"),
    };
    state.1 = true;
    world.queue_barrier.changed.notify_all();
    drop(state);
    for id in world.queued_preview_ids.iter().take(32) {
        let result = await_settlement(&world.queue_trace, id);
        assert!(
            result.error.is_none(),
            "both real Off setters must settle successfully: {result:?}"
        );
        let status = result.status.unwrap();
        assert_eq!(status.state, State::Off);
        assert_eq!(status.is_on, Some(false));
    }
    if let Some(id) = ordinary_id {
        let result = await_settlement(&world.queue_trace, &id);
        assert!(result.error.is_none());
        assert_eq!(
            result.status.unwrap().is_on,
            Some(retirement.as_deref() == Some("toggle"))
        );
    }
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    let id = query.correlation();
    remote.emit(query);
    let result = await_settlement(&world.queue_remote_trace, &id);
    assert!(
        result.error.is_none(),
        "remote query must retain error-null truth from admitted controls"
    );
    assert_eq!(
        result.status.as_ref().unwrap().is_on,
        Some(retirement.as_deref() == Some("toggle"))
    );
    world.attention_result = Some(result);
    world.queued_preview_retirement = retirement;
}

#[when("two distinct TV activation roots precede both derived Off setters")]
fn queued_distinct_preview(world: &mut Menu) {
    queued_preview_roots(world, None, 2);
}

#[when(
    regex = r"^an ordinary (setter|toggle|quit) is queued between distinct TV roots and their setters$"
)]
fn queued_retired_preview(world: &mut Menu, control: String) {
    queued_preview_roots(world, Some(control), 2);
}

#[when("queued TV activation roots exceed the native input capacity")]
fn queued_preview_capacity(world: &mut Menu) {
    queued_preview_roots(world, None, 33);
}

#[then("excess preview input is an explicit fault and a later activation still settles normally")]
fn bounded_preview_origins(world: &mut Menu) {
    let entries = world.queue_trace.0.lock().unwrap().clone();
    let rejected = world.queued_preview_ids.last().unwrap();
    let overflows: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .payload::<Error::Dispatch::Validation::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "preview_input_overflow")
        })
        .collect();
    assert_eq!(overflows.len(), 1);
    assert_eq!(overflows[0].correlation().unwrap(), *rejected);
    let setters: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .payload::<Power::Session::Protection::TurnOff>()
                .unwrap()
                .is_some()
                && world
                    .queued_preview_ids
                    .contains(&entry.correlation().unwrap())
        })
        .collect();
    assert_eq!(
        setters.len(),
        32,
        "the overflow input must not emit an untracked setter"
    );
    assert!(
        setters
            .iter()
            .all(|entry| entry.correlation().unwrap() != *rejected)
    );
    let frame = last(world);
    assert!(!frame.text["error"].is_empty());
    assert!(
        !frame.text["tooltip"].contains("Error preview"),
        "real input failure wins over preview"
    );
    let bus = world.bus.as_ref().unwrap();
    let next = bus
        .prepare::<Ui::Preview::Control::Activated>(Empty {})
        .unwrap();
    let id = next.correlation();
    bus.emit_prepared(next);
    assert!(
        await_settlement(&world.queue_trace, &id).error.is_none(),
        "admitted origins must be removed so later activation can run"
    );
}

fn assert_distinct_preview_order(world: &Menu) {
    let entries = world.queue_trace.0.lock().unwrap().clone();
    let mut roots = Vec::new();
    let mut setters = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let id = entry.correlation().unwrap();
        if world.queued_preview_ids.contains(&id) {
            if entry
                .payload::<Ui::Preview::Control::Activated>()
                .unwrap()
                .is_some()
            {
                roots.push(index);
            }
            if entry
                .payload::<Power::Session::Protection::TurnOff>()
                .unwrap()
                .is_some()
            {
                setters.push(index);
            }
        }
    }
    assert_eq!(roots.len(), 2);
    assert_eq!(setters.len(), 2);
    assert!(
        roots[1] < setters[0],
        "both distinct roots must precede either derived setter"
    );
    println!("QUEUED_PREVIEW_ORDER roots={roots:?} setters={setters:?}");
}

#[then("both successful Off results produce only the latest intentional Error preview")]
fn latest_queued_preview(world: &mut Menu) {
    assert_distinct_preview_order(world);
    let entries = world.queue_trace.0.lock().unwrap().clone();
    let latest = world.queued_preview_ids.last().unwrap();
    let settlement = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .is_some_and(|result| {
                    result.correlation == *latest && result.result.error.is_none()
                })
        })
        .unwrap();
    let previews: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            entry
                .payload::<Ui::Menu::Presentation::Render>()
                .unwrap()
                .filter(|frame| frame.text["tooltip"].contains("Error preview"))
                .map(|_| (index, entry.correlation().unwrap()))
        })
        .collect();
    assert!(
        !previews.is_empty(),
        "latest successful Off activation must render Error preview"
    );
    assert_eq!(
        &previews[0].1, latest,
        "the committing render must continue the latest activation"
    );
    assert!(
        previews.iter().all(|(index, _)| *index > settlement),
        "queries may retain the committed face, never introduce it before settlement"
    );
    let frame = last(world);
    assert_eq!(frame.text["status"], "Off: Error");
    assert_eq!(frame.text["icon_asset"], "menu-error");
    assert!(frame.text["error"].is_empty());
    assert!(frame.text["accessibility"].contains("Observed protection: Off"));
    assert!(
        !entries.iter().any(|entry| entry
            .payload::<Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some()),
        "successful preview input must not create a core fault"
    );
}

#[then("later preview-origin setters cannot restore a retired preview")]
fn retired_queued_preview(world: &mut Menu) {
    assert_distinct_preview_order(world);
    let entries = world.queue_trace.0.lock().unwrap().clone();
    assert!(!entries.iter().any(|entry| {
        entry
            .payload::<Ui::Menu::Presentation::Render>()
            .unwrap()
            .is_some_and(|frame| frame.text["tooltip"].contains("Error preview"))
    }));
    let frame = last(world);
    assert!(!frame.text["tooltip"].contains("Error preview"));
    if world.queued_preview_retirement.as_deref() == Some("quit") {
        assert!(!frame.flags["toggle_enabled"]);
    }
}

fn wait_face(world: &Menu, face: &str) -> Ui::Projection {
    wait_view(&world.view, |frames| {
        frames.last().is_some_and(|frame| {
            frame.text["icon_asset"] == format!("menu-{face}")
                && !frame.text["accessibility"].contains("Pending")
        })
    })
    .last()
    .unwrap()
    .clone()
}

#[when("the large TV cycles from confirmed On through Off Error and On")]
fn cycle_confirmed(world: &mut Menu) {
    assert_eq!(
        owner_control::<Power::Session::Protection::TurnOn>(world)
            .status
            .unwrap()
            .is_on,
        Some(true)
    );
    wait_face(world, "on");
    for face in ["off", "error", "on"] {
        activate_preview(world);
        world.cycle_frames.push(wait_face(world, face));
        let remote = world.queue_remote.as_ref().unwrap();
        let query = remote
            .prepare::<Power::Session::Protection::Query>()
            .unwrap();
        let id = query.correlation();
        remote.emit(query);
        let result = await_settlement(&world.queue_remote_trace, &id);
        assert!(result.error.is_none());
        let status = result.status.unwrap();
        assert_eq!(status.is_on, Some(face == "on"));
        assert_eq!(
            status.state,
            if face == "on" { State::On } else { State::Off }
        );
    }
}

#[then("each preview follows its actual correlated power and remote truth stays ordinary")]
fn preview_real_truth(world: &mut Menu) {
    assert_eq!(world.cycle_frames.len(), 3);
    for (frame, value) in
        world
            .cycle_frames
            .iter()
            .zip(["Mode: Can Sleep", "Off: Error", "Mode: No Sleep"])
    {
        assert_eq!(frame.text["status"], value);
        assert!(
            frame.text["error"].is_empty(),
            "a preview is not a fabricated fault"
        );
    }
    assert!(world.cycle_frames[1].text["tooltip"].contains("Error preview"));
    assert!(world.cycle_frames[1].text["accessibility"].contains("Off"));
    let entries = world.queue_trace.0.lock().unwrap();
    let activations: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .payload::<Ui::Preview::Control::Activated>()
                .unwrap()
                .is_some()
        })
        .map(|entry| entry.correlation().unwrap())
        .collect();
    assert_eq!(activations.len(), 3);
    for id in activations {
        assert!(entries.iter().any(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .is_some_and(|result| result.correlation == id && result.result.error.is_none())
        }));
    }
    assert!(!entries.iter().any(|entry| {
        entry
            .payload::<Error::Native::Assertions::Occurred>()
            .unwrap()
            .is_some()
    }));
}

#[when("repeated large TV activations wait for native Off")]
fn rapid_preview(world: &mut Menu) {
    owner_control::<Power::Session::Protection::TurnOn>(world);
    wait_face(world, "on");
    world.queued_native.2.store(false, Ordering::SeqCst);
    let start = world.view.0.lock().unwrap().len();
    activate_preview(world);
    world.hold_native(false);
    activate_preview(world);
    // A real remote query is a barrier after both native inputs, not a sleep.
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    let id = query.correlation();
    remote.emit(query);
    assert_eq!(
        await_settlement(&world.queue_remote_trace, &id)
            .status
            .unwrap()
            .is_on,
        None
    );
    world.pending_frames = world.view.0.lock().unwrap()[start..].to_vec();
    world.finish_held_native();
    wait_face(world, "off");
    world.queued_native.2.store(true, Ordering::SeqCst);
    activate_preview(world);
    world.cycle_frames.push(wait_face(world, "error"));
}

#[then("no Error preview appears before the next separately confirmed Off cycle")]
fn rapid_preview_truth(world: &mut Menu) {
    assert!(!world.pending_frames.is_empty());
    for frame in &world.pending_frames {
        assert_eq!(frame.text["icon_asset"], "menu-on");
        assert_eq!(frame.text["status"], "Mode: No Sleep");
        assert!(!frame.text["tooltip"].contains("Error preview"));
    }
    assert_eq!(world.cycle_frames[0].text["status"], "Off: Error");
}

#[when("an ordinary control supersedes a pending Error preview and a genuine fault follows")]
fn superseded_preview(world: &mut Menu) {
    owner_control::<Power::Session::Protection::TurnOff>(world);
    wait_face(world, "off");
    let bus = world.bus.as_ref().unwrap();
    let preview = bus
        .prepare::<Ui::Preview::Control::Activated>(Empty {})
        .unwrap();
    let stale = preview.correlation();
    let barrier = world.queue_barrier.clone();
    let preview_id = stale.clone();
    bus.observe_action::<Power::Session::Protection::TurnOff>(move |_, events| {
        if events.correlation() == preview_id {
            let mut state = barrier.barrier.lock().unwrap();
            state.0 = true;
            barrier.changed.notify_all();
            let (state, _) = barrier
                .changed
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.1)
                .unwrap();
            assert!(
                state.1,
                "the admitted preview setter barrier must be released"
            );
        }
    });
    bus.emit_prepared(preview);
    let state = world.queue_barrier.barrier.lock().unwrap();
    let (mut state, _) = world
        .queue_barrier
        .changed
        .wait_timeout_while(state, Duration::from_secs(2), |state| !state.0)
        .unwrap();
    assert!(
        state.0,
        "the actual preview setter must be admitted before the newer control"
    );
    let on = bus
        .prepare::<Power::Session::Protection::TurnOn>(Empty {})
        .unwrap();
    let on_id = on.correlation();
    bus.emit_prepared(on);
    state.1 = true;
    world.queue_barrier.changed.notify_all();
    drop(state);
    assert_eq!(
        await_settlement(&world.queue_trace, &on_id)
            .status
            .unwrap()
            .is_on,
        Some(true)
    );
    wait_face(world, "on");
    bus.emit::<Power::Session::Protection::Settled>(Settlement {
        correlation: stale,
        result: ControlResult {
            status: Some(Status::off()),
            error: None,
        },
    });
    private_fault(world);
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    let id = query.correlation();
    remote.emit(query);
    world.attention_result = Some(await_settlement(&world.queue_remote_trace, &id));
}

#[then("the stale preview cannot hide the genuine fault or rewrite remote power")]
fn superseded_preview_truth(world: &mut Menu) {
    let frame = wait_face(world, "error");
    assert_eq!(frame.text["status"], "Off: Error");
    assert!(!frame.text["tooltip"].contains("Error preview"));
    assert!(!frame.text["error"].is_empty());
    assert!(frame.text["accessibility"].contains("Observed protection: On"));
    let result = world.attention_result.as_ref().unwrap();
    assert!(result.error.is_none());
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(true));
}

#[when("the preview Off target fails with actual retained resources")]
fn preview_failed_off(world: &mut Menu) {
    owner_control::<Power::Session::Protection::TurnOn>(world);
    wait_face(world, "on");
    world.queued_native.2.store(false, Ordering::SeqCst);
    activate_preview(world);
    let (request, events) = world.queued_native.pending(false);
    events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
        request,
        held_count: 2,
        outcome: Outcome::Failed,
        fault_codes: vec!["fixture_release_failed".into()],
        clock: ClockSample {
            epoch_ms: 1_800_000_000_000,
            monotonic_ms: 1000,
        },
    });
    wait_face(world, "error");
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    let id = query.correlation();
    remote.emit(query);
    world.attention_result = Some(await_settlement(&world.queue_remote_trace, &id));
}

#[then("genuine cleanup error remains distinct from an intentional Error preview")]
fn preview_failed_truth(world: &mut Menu) {
    let frame = last(world);
    assert_eq!(frame.text["status"], "Off: Error");
    assert!(!frame.text["tooltip"].contains("Error preview"));
    assert!(!frame.text["error"].is_empty());
    let result = world.attention_result.as_ref().unwrap();
    assert_eq!(result.status.as_ref().unwrap().state, State::Error);
    assert_eq!(result.status.as_ref().unwrap().is_on, None);
}

#[when("Quit overtakes a held preview Off and a later TV activation")]
fn preview_during_quit(world: &mut Menu) {
    owner_control::<Power::Session::Protection::TurnOn>(world);
    wait_face(world, "on");
    world.queued_native.2.store(false, Ordering::SeqCst);
    activate_preview(world);
    world.hold_native(false);
    let bus = world.bus.as_ref().unwrap();
    let quit = bus
        .prepare::<Power::Session::Protection::Quit>(Empty {})
        .unwrap();
    let id = quit.correlation();
    bus.emit_prepared(quit);
    wait_view(&world.view, |frames| {
        frames
            .last()
            .is_some_and(|frame| !frame.flags["toggle_enabled"])
    });
    activate_preview(world);
    world.finish_held_native();
    assert_eq!(
        await_settlement(&world.queue_trace, &id)
            .status
            .unwrap()
            .is_on,
        Some(false)
    );
    world.quit = Some(id);
}

#[then("confirmed cleanup never permits a new native enable or preview control")]
fn preview_quit_fence(world: &mut Menu) {
    let frame = last(world);
    assert!(!frame.flags["toggle_enabled"]);
    assert!(!frame.text["tooltip"].contains("Error preview"));
    let trace = world.queue_trace.0.lock().unwrap();
    let (trace, _) = world
        .queue_trace
        .1
        .wait_timeout_while(trace, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Protection::Closed>()
                    .unwrap()
                    .is_some()
            })
        })
        .unwrap();
    let enables = trace
        .iter()
        .filter(|entry| {
            entry
                .payload::<Power::Assertions::Resources::Apply>()
                .unwrap()
                .is_some_and(|request| request.enabled)
        })
        .count();
    let closed = trace.iter().any(|entry| {
        entry
            .payload::<Power::Session::Protection::Closed>()
            .unwrap()
            .is_some()
    });
    drop(trace);
    assert_eq!(enables, 1);
    assert!(
        closed,
        "actual Closed observation, not an earlier Quit settlement, is required"
    );
}

#[then("actual human rows descend below the path while canonical history still ascends")]
fn newest_log_rows(world: &mut Menu) {
    let views = world.view.0.lock().unwrap();
    let logs: Vec<_> = views
        .iter()
        .filter(|view| view.flags["activate_log"])
        .collect();
    assert_eq!(logs.len(), 2);
    for frame in logs {
        let mut lines = frame.text["log_text"].lines();
        assert!(std::path::Path::new(lines.next().unwrap()).is_absolute());
        let rows: Vec<_> = lines
            .filter_map(|line| line.split_once("  "))
            .map(|(sequence, label)| (sequence.parse::<u64>().unwrap(), label))
            .collect();
        assert!(rows.len() > 5);
        assert!(rows.windows(2).all(|pair| pair[0].0 > pair[1].0));
        assert!(rows[0].1.starts_with("Log:Session:History:Query  ["));
        assert!(
            rows.iter()
                .any(|row| row.1.starts_with("Power:Session:Protection:TurnOn  ["))
        );
        assert!(
            rows.iter()
                .any(|row| row.1.starts_with("Power:Session:Protection:TurnOff  ["))
        );
        assert_eq!(rows.len(), 200);
        assert!(frame.text["log_text"].len() <= 64 * 1024);
        assert!(
            frame.text["log_text"]
                .find("Earlier action history")
                .unwrap()
                > frame.text["log_text"].find(rows[0].1).unwrap()
        );
    }
    let trace = world.queue_trace.0.lock().unwrap();
    let snapshots: Vec<_> = trace
        .iter()
        .filter_map(|entry| entry.payload::<Log::Session::History::Snapshot>().unwrap())
        .collect();
    for snapshot in snapshots {
        assert!(
            snapshot
                .entries
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
    }
}

#[when("the private menu generates a bounded overflowing diagnostic history")]
fn bounded_overflowing_history(world: &mut Menu) {
    for _ in 0..70 {
        assert!(
            owner_control::<Power::Session::Protection::Query>(world)
                .error
                .is_none()
        );
    }
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
    assert!(
        valid,
        "required host observation did not arrive: {:?}",
        result.iter().map(|frame| &frame.text).collect::<Vec<_>>()
    );
    result
}

#[given("a typed native menu observation is registered")]
fn observing(world: &mut Menu) {
    let mut builder = RegistryBuilder::default();
    // This projection-only fixture deliberately supplies no native completion.
    // Real owner/IPC/resource behavior is exercised by the serial host scenarios.
    builder
        .on::<Power::Session::Protection::Quit>(|_, _| Ok(()))
        .unwrap();
    let bus = builder.build().unwrap();
    world.subscription = Some(observe_menu(&bus, world.view.clone()));
    let query = world.log_query.clone();
    bus.observe_action::<Log::Session::History::Query>(move |_, events| {
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
    watch_control!(Power::Session::Protection::TurnOn);
    watch_control!(Power::Session::Protection::TurnOff);
    watch_control!(Power::Session::Protection::Toggle);
    watch_control!(Power::Session::Protection::Quit);
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
        .emit::<Power::Session::Protection::Changed>(status);
}

#[then("the generated observer receives the awake TV and observed On label")]
fn awake(world: &mut Menu) {
    let views = world.view.0.lock().unwrap();
    assert_eq!(views.len(), 1, "a canonical observed render is required");
    assert_eq!(views[0].text["icon_asset"], "menu-on");
    assert_eq!(views[0].text["status"], "Mode: No Sleep");
    assert_eq!(views[0].text["title"], "AI2ScreenWake");
    assert_eq!(
        views[0].text.get("preview_asset").map(String::as_str),
        Some("preview-on")
    );
    assert_eq!(views[0].text["log_title"], "ai2sw.log");
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
        .emit::<Power::Session::Protection::Changed>(status);
}

#[then("the observed menu remains explicitly pending without a moving punctuation badge")]
fn not_optimistic(world: &mut Menu) {
    let view = last(world);
    assert_eq!(view.text["icon_asset"], "menu-off");
    assert_eq!(view.text["status"], "Mode: Can Sleep");
    assert!(view.text["badge"].is_empty());
    assert!(view.text["accessibility"].contains("Pending"));
}

#[when(
    regex = r"^confirmed (Off|On) is followed by Pending retries stale frames and opposite settlement$"
)]
fn transition_frames(world: &mut Menu, initial: String) {
    let initially_on = initial == "On";
    let bus = world.bus.as_ref().unwrap();
    let mut status = Status::off();
    status.revision = 10;
    status.state = if initially_on { State::On } else { State::Off };
    status.is_on = Some(initially_on);
    status.desired_on = initially_on;
    bus.emit::<Power::Session::Protection::Changed>(status.clone());
    world
        .expected_frames
        .push((initial.clone(), initial.to_lowercase()));
    status.revision = 11;
    status.state = State::Pending;
    status.is_on = None;
    status.desired_on = !initially_on;
    for _ in 0..2 {
        bus.emit::<Power::Session::Protection::Changed>(status.clone());
        world
            .expected_frames
            .push(("Pending".into(), initial.to_lowercase()));
    }
    let mut stale = Status::off();
    stale.revision = 9;
    stale.state = if initially_on { State::Off } else { State::On };
    stale.is_on = Some(!initially_on);
    bus.emit::<Power::Session::Protection::Snapshot>(stale);
    // A withdrawn request reconfirms the original face, without an opposite flash.
    status.revision = 12;
    status.state = if initially_on { State::On } else { State::Off };
    status.is_on = Some(initially_on);
    bus.emit::<Power::Session::Protection::Snapshot>(status.clone());
    world
        .expected_frames
        .push((initial.clone(), initial.to_lowercase()));
    status.revision = 13;
    status.state = State::Pending;
    status.is_on = None;
    bus.emit::<Power::Session::Protection::Changed>(status.clone());
    world
        .expected_frames
        .push(("Pending".into(), initial.to_lowercase()));
    status.state = if initially_on { State::Off } else { State::On };
    status.is_on = Some(!initially_on);
    bus.emit::<Power::Session::Protection::Snapshot>(status);
    world.expected_frames.push((
        if initially_on { "Off" } else { "On" }.into(),
        if initially_on { "off" } else { "on" }.into(),
    ));
}

#[then("every emitted frame retains truthful status and changes its face only at confirmation")]
fn confirmed_face_sequence(world: &mut Menu) {
    let frames = world.view.0.lock().unwrap();
    assert_eq!(
        frames.len(),
        world.expected_frames.len(),
        "no accepted observation may disappear or stale observation render"
    );
    for (frame, (phase, face)) in frames.iter().zip(&world.expected_frames) {
        assert_eq!(
            frame.text["status"],
            if face == "on" {
                "Mode: No Sleep"
            } else {
                "Mode: Can Sleep"
            }
        );
        assert_eq!(frame.text["icon_asset"], format!("menu-{face}"));
        assert_eq!(frame.text["preview_asset"], format!("preview-{face}"));
        assert!(
            frame.text["badge"].is_empty(),
            "Pending must not move the image with title dots"
        );
        assert!(frame.text["accessibility"].contains(phase));
        assert!(frame.flags["toggle_enabled"] && frame.flags["quit_enabled"]);
    }
    assert_eq!(
        frames
            .windows(2)
            .filter(|pair| pair[0].text["icon_asset"] != pair[1].text["icon_asset"])
            .count(),
        1
    );
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
        .emit::<Power::Session::Protection::Changed>(status);
}

#[then("the menu labels Error rather than falsely claiming Off")]
fn truthful_error(world: &mut Menu) {
    let view = last(world);
    error_face(&view);
    assert!(!view.text["error"].is_empty());
}

fn error_face(view: &Ui::Projection) {
    assert_eq!(view.text["status"], "Off: Error");
    assert_eq!(view.text["icon_asset"], "menu-error");
    assert_eq!(view.text["preview_asset"], "preview-error");
    assert!(
        view.text["badge"].is_empty(),
        "attention has its own face, never punctuation"
    );
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

#[then("the menu shows the Error face with observed On detail and no private chain")]
fn no_raw_errors(world: &mut Menu) {
    let view = last(world);
    error_face(&view);
    assert!(view.text["accessibility"].contains("Observed protection: On"));
    assert!(!view.text["error"].is_empty());
    assert!(!view.text["error"].contains("private"));
    assert!(view.text["tooltip"].contains(&view.text["error"]));
    assert!(view.text["tooltip"].contains("Observed protection: On"));
    assert!(!view.flags["terminate"]);
}

#[given(regex = r"^a real private menu with (Unconfigured|Disabled) forwarding$")]
fn inactive_menu(world: &mut Menu, mode: String) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target");
    let directory = tempfile::Builder::new()
        .prefix("u")
        .tempdir_in(root)
        .unwrap();
    world.queued_native.2.store(true, Ordering::SeqCst);
    let location = for_current_user(directory.path().join("o"));
    let server = if mode == "Disabled" {
        OwnerServer::bind_with_telemetry(
            location.clone(),
            world.queued_native.clone(),
            Configuration::Disabled,
        )
    } else {
        OwnerServer::bind(location.clone(), world.queued_native.clone())
    }
    .unwrap();
    let bus = server.actions();
    world.subscription = Some(observe_menu(&bus, world.view.clone()));
    bus.observe(world.queue_trace.clone());
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

fn owner_control<A: Action<Payload = Empty>>(world: &Menu) -> ControlResult {
    let bus = world.bus.as_ref().unwrap();
    let request = bus.prepare::<A>(Empty {}).unwrap();
    let id = request.correlation();
    bus.emit_prepared(request);
    await_settlement(&world.queue_trace, &id)
}

fn await_settlement(trace: &IpcView, id: &CorrelationId) -> ControlResult {
    let entries = trace.0.lock().unwrap();
    let (entries, _) = trace
        .1
        .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Protection::Settled>()
                    .unwrap()
                    .is_some_and(|settled| settled.correlation == *id)
            })
        })
        .unwrap();
    entries
        .iter()
        .find_map(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .filter(|settled| settled.correlation == *id)
                .map(|settled| settled.result)
        })
        .expect("the real private owner must settle the exact correlated control")
}

#[when("the private menu owner enables then disables protection")]
fn ordinary_private_faces(world: &mut Menu) {
    assert_eq!(
        owner_control::<Power::Session::Protection::TurnOn>(world)
            .status
            .unwrap()
            .is_on,
        Some(true)
    );
    wait_view(&world.view, |frames| {
        frames
            .iter()
            .any(|frame| frame.text["status"] == "Mode: No Sleep")
    });
    let start = world.view.0.lock().unwrap().len();
    assert_eq!(
        owner_control::<Power::Session::Protection::TurnOff>(world)
            .status
            .unwrap()
            .is_on,
        Some(false)
    );
    let confirmed_off = |frame: &Ui::Projection| {
        frame.text["accessibility"] == "AI2ScreenWake protection: Off"
            && frame.text["status"] == "Mode: Can Sleep"
            && frame.text["icon_asset"] == "menu-off"
    };
    let frames = wait_view(&world.view, |frames| {
        frames.iter().skip(start).any(confirmed_off)
    });
    world.ordinary_off_frame = Some(
        frames
            .into_iter()
            .skip(start)
            .find(confirmed_off)
            .expect("this TurnOff must render its own confirmed Off frame"),
    );
}

#[when("the real owner requests Off while native confirmation is held")]
fn real_pending_off(world: &mut Menu) {
    assert_eq!(
        owner_control::<Power::Session::Protection::TurnOn>(world)
            .status
            .unwrap()
            .is_on,
        Some(true)
    );
    wait_view(&world.view, |frames| {
        frames
            .last()
            .is_some_and(|frame| frame.text["status"] == "Mode: No Sleep")
    });
    let start = world.view.0.lock().unwrap().len();
    world.queued_native.2.store(false, Ordering::SeqCst);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOff>(Empty {});
    let (request, events) = world.queued_native.pending(false);
    wait_view(&world.view, |frames| {
        frames
            .iter()
            .skip(start)
            .any(|frame| frame.text["accessibility"].contains("Pending"))
    });
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    let id = query.correlation();
    remote.emit(query);
    world.attention_result = Some(await_settlement(&world.queue_remote_trace, &id));
    world.pending_frames = world.view.0.lock().unwrap()[start..].to_vec();
    finish_apply(request, events);
    wait_view(&world.view, |frames| {
        frames
            .last()
            .is_some_and(|frame| frame.text["status"] == "Mode: Can Sleep")
    });
}

#[then("remote Pending remains nullable while the menu retains On until confirmed Off")]
fn real_pending_truth(world: &mut Menu) {
    let result = world.attention_result.as_ref().unwrap();
    assert!(result.error.is_none());
    let status = result.status.as_ref().unwrap();
    assert_eq!(status.state, State::Pending);
    assert_eq!(status.is_on, None);
    assert!(!status.desired_on);
    assert!(!world.pending_frames.is_empty());
    let mut pending_seen = false;
    for frame in &world.pending_frames {
        if frame.text["accessibility"].contains("Pending") {
            pending_seen = true;
        } else {
            assert!(
                !pending_seen,
                "accepted Pending cannot regress to an older On frame"
            );
            assert_eq!(frame.text["status"], "Mode: No Sleep");
        }
        assert_eq!(frame.text["icon_asset"], "menu-on");
        assert_eq!(frame.text["preview_asset"], "preview-on");
        assert!(frame.text["badge"].is_empty());
        assert_eq!(frame.text["status"], "Mode: No Sleep");
        assert!(frame.text["accessibility"].contains(if pending_seen { "Pending" } else { "On" }));
    }
    assert!(pending_seen);
    let frame = last(world);
    assert_eq!(frame.text["status"], "Mode: Can Sleep");
    assert_eq!(frame.text["icon_asset"], "menu-off");
}

#[then("both observed faces are ordinary without telemetry attention or delivery claims")]
fn no_inactive_attention(world: &mut Menu) {
    let frames = world.view.0.lock().unwrap().clone();
    for (phase, asset) in [("On", "on"), ("Off", "off")] {
        let frame = if phase == "Off" {
            world
                .ordinary_off_frame
                .as_ref()
                .expect("use the exact post-TurnOff frame selected by the wait")
        } else {
            frames
                .iter()
                .rev()
                .find(|frame| {
                    frame.text["accessibility"] == "AI2ScreenWake protection: On"
                        && frame.text["status"] == "Mode: No Sleep"
                })
                .unwrap()
        };
        assert_eq!(frame.text["icon_asset"], format!("menu-{asset}"));
        assert_eq!(frame.text["preview_asset"], format!("preview-{asset}"));
        assert!(
            frame.text["badge"].is_empty() && frame.text["error"].is_empty(),
            "intentional inactivity must not ask the user for attention: {:?}",
            frame.text
        );
    }
    let entries = world.queue_trace.0.lock().unwrap().clone();
    assert!(!entries.iter().any(|entry| {
        entry
            .payload::<Error::Telemetry::Configuration::Occurred>()
            .unwrap()
            .is_some()
            || entry
                .payload::<ai2screenwake_client::contract::Telemetry::Delivery::Forwarding::Confirmed>()
                .unwrap()
                .is_some()
    }));
}

#[when("a genuine fault precedes another remote confirmed On query")]
fn fault_then_remote_truth(world: &mut Menu) {
    assert_eq!(
        owner_control::<Power::Session::Protection::TurnOn>(world)
            .status
            .unwrap()
            .is_on,
        Some(true)
    );
    private_fault(world);
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    let id = query.correlation();
    remote.emit(query);
    world.attention_result = Some(await_settlement(&world.queue_remote_trace, &id));
}

#[then("the Error face retains the real owner's confirmed On truth and controls")]
fn genuine_attention_truth(world: &mut Menu) {
    let result = world.attention_result.as_ref().unwrap();
    assert!(result.error.is_none());
    let status = result.status.as_ref().unwrap();
    assert_eq!(status.is_on, Some(true));
    assert_eq!(status.state, State::On);
    assert!(status.desired_on);
    let view = last(world);
    error_face(&view);
    assert!(view.text["accessibility"].contains("Observed protection: On"));
    assert!(view.flags["toggle_enabled"] && view.flags["quit_enabled"]);
    assert!(!view.text["error"].contains("private"));
    let entries = world.queue_trace.0.lock().unwrap();
    let fault = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Error::Native::Activity::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "test_sample")
        })
        .unwrap();
    let render = entries
        .iter()
        .rposition(|entry| {
            entry
                .payload::<Ui::Menu::Presentation::Render>()
                .unwrap()
                .is_some_and(|frame| frame.text["status"] == "Off: Error")
        })
        .unwrap();
    assert!(
        fault < render,
        "the canonical fault must be logged before its attention render"
    );
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
    error_face(&view);
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
    assert!(view.text["accessibility"].contains(&format!("Observed protection: {phase}")));
    assert!(view.flags["toggle_enabled"]);
    assert!(view.flags["quit_enabled"]);
}

#[then("telemetry error visibility preserves in-progress Quit and protection truth")]
fn telemetry_keeps_quit(world: &mut Menu) {
    let view = safe_telemetry_error(world);
    assert!(view.text["accessibility"].contains("Observed protection: On"));
    assert!(!view.flags["toggle_enabled"]);
    assert!(!view.flags["quit_enabled"]);
}

#[when("the existing log query is emitted")]
fn log(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Log::Session::History::Query>(Empty {});
}

#[when("its readable snapshot supplies a timestamped absolute path")]
fn named_readable_snapshot(world: &mut Menu) {
    world
        .log_query
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .emit::<Log::Session::History::Snapshot>(LogSnapshot {
            readable_path: Some(
                "/private/owned/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log".into(),
            ),
            ..Default::default()
        });
}

#[then("the viewer title is the actual exported basename and the first line is its full path")]
fn exported_viewer_name(world: &mut Menu) {
    let view = last(world);
    assert_eq!(view.text["log_title"], "20261117tu100720_ai2sw.log");
    assert_eq!(
        view.text["log_text"].lines().next(),
        Some("/private/owned/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log")
    );
    assert!(view.flags["log_visible"] && view.flags["activate_log"]);
}

#[when("the real private menu opens and refreshes its human log")]
fn real_viewer_log(world: &mut Menu) {
    let bus = world.bus.as_ref().unwrap();
    for _ in 0..2 {
        let previous = world.view.0.lock().unwrap().len();
        bus.emit::<Log::Session::History::Query>(Empty {});
        wait_view(&world.view, |frames| {
            frames
                .iter()
                .skip(previous)
                .any(|frame| frame.flags["log_visible"] && frame.flags["activate_log"])
        });
    }
}

#[then("both actual viewer projections use the same exported basename path and bytes")]
fn real_viewer_export(world: &mut Menu) {
    let frames = world.view.0.lock().unwrap().clone();
    let views: Vec<_> = frames
        .iter()
        .filter(|frame| frame.flags["log_visible"] && frame.flags["activate_log"])
        .collect();
    assert_eq!(views.len(), 2);
    let paths: Vec<_> = views
        .iter()
        .map(|frame| frame.text["log_text"].lines().next().unwrap())
        .collect();
    assert_eq!(paths[0], paths[1]);
    let path = std::path::Path::new(paths[1]);
    assert!(
        path.starts_with(
            world
                .queue_directory
                .as_ref()
                .unwrap()
                .path()
                .canonicalize()
                .unwrap()
        )
    );
    assert_eq!(
        views[1].text["log_title"],
        path.file_name().unwrap().to_str().unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        views[1].text["log_text"]
    );
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
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
    assert!(sequences.windows(2).all(|pair| pair[0] > pair[1]));
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
        .emit::<Log::Session::History::Snapshot>(LogSnapshot {
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
        .emit::<Log::Session::History::Snapshot>(LogSnapshot {
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
    error_face(&view);
    assert!(view.text["accessibility"].contains("Observed protection: On"));
}

#[when("the log is dismissed twice")]
fn dismiss(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Ui::Log::Window::Dismissed>(Empty {});
    world.after_dismiss = world.view.0.lock().unwrap().len();
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Ui::Log::Window::Dismissed>(Empty {});
}

#[then("it is closed once without changing the observed On state")]
fn closed_once(world: &mut Menu) {
    assert_eq!(world.view.0.lock().unwrap().len(), world.after_dismiss);
    let view = last(world);
    assert!(!view.flags["log_visible"]);
    assert_eq!(view.text["status"], "Mode: No Sleep");
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
        .emit::<Log::Session::History::Snapshot>(LogSnapshot::default());
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
    let prepared = bus
        .prepare::<Power::Session::Protection::Quit>(Empty {})
        .unwrap();
    world.quit = Some(prepared.correlation());
    bus.emit_prepared(prepared);
}

#[when("the owner observes confirmed Off")]
fn observed_off(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Changed>(Status::off());
}

#[then("Quit remains in progress without a terminal instruction")]
fn quit_not_ready(world: &mut Menu) {
    let view = last(world);
    assert_eq!(view.text["status"], "Mode: Can Sleep");
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
        .emit::<Power::Session::Protection::Settled>(Settlement {
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target");
    actual_host_in(world, &root);
}

#[given("an actual generated native host uses a missing fixture parent")]
fn actual_host_missing_parent(world: &mut Menu) {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    actual_host_missing_parent_in(world, &project);
}

#[given(
    expr = "an actual generated native host has a missing parent and a {int}-byte ordinary socket budget"
)]
fn actual_host_near_path_limit(world: &mut Menu, budget: usize) {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let container = tempfile::Builder::new()
        .prefix("u")
        .rand_bytes(5)
        .tempdir_in(&project)
        .unwrap();
    let current = container.path().join("target/uXXXXXX/o/owner.sock");
    let extra = budget
        .checked_sub(current.as_os_str().as_encoded_bytes().len())
        .expect("the assigned checkout must fit the explicit Darwin path witness");
    let simulated_project = if extra == 0 {
        container.path().to_path_buf()
    } else {
        let padding = extra
            .checked_sub(1)
            .filter(|padding| *padding > 0)
            .expect("a nested fixture needs both its separator and a nonempty name");
        let path = container.path().join("x".repeat(padding));
        std::fs::create_dir(&path).unwrap();
        path
    };
    world.host_project_directory = Some(container);
    actual_host_missing_parent_in(world, &simulated_project);
    assert_eq!(world.host_socket_budget, Some(budget));
}

fn actual_host_missing_parent_in(world: &mut Menu, project: &std::path::Path) {
    let root = project.join("target");
    world.host_socket_budget = Some(
        root.join("uXXXXXX/o/owner.sock")
            .as_os_str()
            .as_encoded_bytes()
            .len(),
    );
    let parent = tempfile::Builder::new()
        .prefix("u")
        .rand_bytes(5)
        .tempdir_in(project)
        .unwrap();
    let missing = parent.path().to_path_buf();
    assert_eq!(missing.parent(), Some(project));
    assert_eq!(
        missing.file_name().unwrap().as_encoded_bytes().len(),
        b"target".len()
    );
    assert!(
        !std::fs::symlink_metadata(&missing)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(std::fs::read_dir(&missing).unwrap().next().is_none());
    world.host_parent_directory = Some(parent);
    std::fs::remove_dir(&missing).unwrap();
    assert!(
        !missing.exists(),
        "the owned fixture parent must start absent"
    );
    actual_host_in(world, &missing);
    assert!(
        world
            .host_directory
            .as_ref()
            .unwrap()
            .path()
            .starts_with(&missing),
        "the real host must use the newly created fixture parent"
    );
    println!("NATIVE_FIXTURE_PARENT_OK missing_at_start=true observed_off=true");
}

fn actual_host_in(world: &mut Menu, root: &std::path::Path) {
    std::fs::create_dir_all(root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("u")
        .tempdir_in(root)
        .unwrap();
    if let Some(budget) = world.host_socket_budget {
        let socket = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("o/owner.sock");
        let bytes = socket.as_os_str().as_encoded_bytes().len();
        assert!(
            bytes <= budget && bytes < 104,
            "regression fixture socket uses {bytes} bytes, exceeding ordinary budget {budget} or Darwin capacity 104"
        );
        println!(
            "NATIVE_FIXTURE_PATH_OK socket_bytes={bytes} ordinary_budget={budget} darwin_capacity=104"
        );
    }
    let location = for_current_user(directory.path().join("o"));
    assert_ne!(
        location.directory,
        ai2screenwake_native::identity::runtime_directory().unwrap(),
        "the real-resource fixture must never control the user's login-session owner"
    );
    world.endpoint = Some(location.directory.join("owner.sock"));
    *world.view.2.lock().unwrap() = world.endpoint.clone();
    world.host_directory = Some(directory);
    world.host = Some(
        NativeHost::with_native(
            Box::new(Forward(world.view.clone())),
            location,
            Arc::new(MacPower::with_api(world.host_power.clone())),
        )
        .unwrap(),
    );
    wait_view(&world.view, |frames| {
        frames.iter().any(|frame| {
            frame
                .text
                .get("accessibility")
                .is_some_and(|s| s == "AI2ScreenWake protection: Off")
        })
    });
    assert!(world.host_power.held.lock().unwrap().is_empty());
    assert_eq!(world.host_power.acquired.load(Ordering::SeqCst), 0);
    println!(
        "ASSERTION_ONLY_HOST startup_off=true real_iokit=true configured_corner=explicitly_absent permission_prompt=false"
    );
}

#[when("its typed primary toggle is emitted")]
fn real_toggle(world: &mut Menu) {
    world.toggle_index = world.view.0.lock().unwrap().len();
    world.host.as_ref().unwrap().emit(NativeIntent::Toggle);
}

#[when("its typed preview cycles confirmed Off through Error and On")]
fn real_preview(world: &mut Menu) {
    let host = world.host.as_ref().unwrap();
    host.emit(NativeIntent::CyclePreview);
    let error = wait_face(world, "error");
    assert!(error.text["tooltip"].contains("Error preview"));
    assert!(error.text["accessibility"].contains("Observed protection: Off"));
    assert!(error.text["error"].is_empty());
    world.toggle_index = world.view.0.lock().unwrap().len();
    host.emit(NativeIntent::CyclePreview);
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
        frames.iter().any(|frame| {
            frame
                .text
                .get("status")
                .is_some_and(|s| s == "Mode: No Sleep")
        })
    });
    let tail = &frames[world.toggle_index..];
    let pending = tail
        .iter()
        .position(|f| {
            f.text
                .get("accessibility")
                .is_some_and(|s| s.contains("Pending"))
        })
        .unwrap();
    let on = tail
        .iter()
        .position(|f| f.text.get("status").is_some_and(|s| s == "Mode: No Sleep"))
        .unwrap();
    assert!(pending < on);
    let location = for_current_user(world.endpoint.as_ref().unwrap().parent().unwrap().into());
    // The existing owner must connect; this nonexistent bundle must never be launched.
    let launcher = Arc::new(ai2screenwake_native::launch::BundleLauncher::new(
        std::path::PathBuf::from("/nonexistent/ScreenWake.app"),
    ));
    let connection = OwnerConnection::connect(location, launcher).unwrap();
    let observed = Arc::new(IpcView::default());
    connection.observe(observed.clone());
    let query = connection
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    let correlation = query.correlation();
    connection.emit(query);
    let entries = observed.0.lock().unwrap();
    let (entries, _) = observed
        .1
        .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Protection::Settled>()
                    .unwrap()
                    .is_some_and(|s| s.correlation == correlation)
            })
        })
        .unwrap();
    let settled = entries
        .iter()
        .find_map(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .filter(|s| s.correlation == correlation)
        })
        .unwrap();
    assert!(settled.result.error.is_none());
    assert_eq!(settled.result.status.unwrap().is_on, Some(true));
    assert_eq!(world.host_power.held.lock().unwrap().len(), 2);
    assert!(world.host_power.acquired.load(Ordering::SeqCst) >= 2);
    assert!(world.host_power.corner_reads.load(Ordering::SeqCst) > 0);
    println!(
        "ASSERTION_ONLY_HOST confirmed_on=true real_held_iokit=2 configured_corner=explicitly_absent physical_corner_proof=false"
    );
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
        .position(|f| f.text.get("status").is_some_and(|s| s == "Mode: Can Sleep"))
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
    assert!(world.host_power.held.lock().unwrap().is_empty());
    assert_eq!(
        world.host_power.acquired.load(Ordering::SeqCst),
        world.host_power.released.load(Ordering::SeqCst)
    );
    println!(
        "ASSERTION_ONLY_HOST terminal_after_off=true real_iokit_released=true private_endpoint_absent=true"
    );
}

#[when("a second generated host attempts admission")]
fn second_host(world: &mut Menu) {
    let location = for_current_user(world.endpoint.as_ref().unwrap().parent().unwrap().into());
    world.second = Some(
        NativeHost::with_native(
            Box::new(Forward(world.second_view.clone())),
            location,
            Arc::new(MacPower::with_api(Arc::new(
                AssertionOnlySystemPower::default(),
            ))),
        )
        .unwrap(),
    );
    wait_view(&world.second_view, |frames| {
        frames
            .iter()
            .any(|f| f.text.get("status").is_some_and(|s| s == "Off: Error"))
    });
}

#[then("its error observation never claims Off and its Quit leaves the first owner available")]
fn second_failed(world: &mut Menu) {
    assert!(!world.second_view.0.lock().unwrap().iter().any(|f| {
        f.text
            .get("accessibility")
            .is_some_and(|s| s == "AI2ScreenWake protection: Off")
    }));
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
    bus.emit::<Power::Session::Protection::Changed>(off);
    let mut on = Status::off();
    on.revision = 11;
    on.state = State::On;
    on.desired_on = true;
    on.is_on = Some(true);
    bus.emit::<Power::Session::Protection::Snapshot>(on.clone());
    bus.emit::<Power::Session::Protection::Changed>(on);
}

#[then("the menu never renders the older awake state")]
fn stale_ignored(world: &mut Menu) {
    let frames = world.view.0.lock().unwrap();
    assert_eq!(
        frames.len(),
        1,
        "older revisions must not emit stale rendering packets"
    );
    assert_eq!(frames[0].text["status"], "Mode: Can Sleep");
}

#[when("a same-revision snapshot supplies confirmed On after Pending")]
fn equal_projection(world: &mut Menu) {
    let bus = world.bus.as_ref().unwrap();
    let mut status = Status::off();
    status.revision = 12;
    status.state = State::Pending;
    status.is_on = None;
    bus.emit::<Power::Session::Protection::Changed>(status.clone());
    status.state = State::On;
    status.is_on = Some(true);
    status.desired_on = true;
    bus.emit::<Power::Session::Protection::Snapshot>(status);
}

#[then("the same-revision observation is rendered rather than discarded")]
fn equal_accepted(world: &mut Menu) {
    assert_eq!(last(world).text["status"], "Mode: No Sleep");
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
    bus.observe_action::<Power::Assertions::Resources::Observed>(move |result, _| {
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
    events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
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
    bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
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
    bus.emit::<Power::Session::Protection::TurnOff>(Empty {});
    state.1 = true;
    journal.changed.notify_all();
    drop(state);
    let (request, events) = world.queued_native.pending(false);
    let before_off = world.view.0.lock().unwrap().len();
    finish_apply(request, events);
    wait_view(&world.view, |frames| {
        frames.iter().skip(before_off).any(|frame| {
            frame.text["status"] == "Mode: Can Sleep"
                && frame.text["accessibility"] == "AI2ScreenWake protection: Off"
        })
    });
    let remote = world.queue_remote.as_ref().unwrap();
    let query = remote
        .prepare::<Power::Session::Protection::Query>()
        .unwrap();
    world.quit = Some(query.correlation());
    remote.emit(query);
    let entries = world.queue_remote_trace.0.lock().unwrap();
    let (entries, _) = world
        .queue_remote_trace
        .1
        .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Protection::Settled>()
                    .unwrap()
                    .is_some_and(|settled| Some(&settled.correlation) == world.quit.as_ref())
            })
        })
        .unwrap();
    assert!(entries.iter().any(|entry| {
        entry
            .payload::<Power::Session::Protection::Settled>()
            .unwrap()
            .is_some_and(|settled| Some(&settled.correlation) == world.quit.as_ref())
    }));
    drop(entries);
    let entries = world.queue_trace.0.lock().unwrap();
    let (entries, _) = world
        .queue_trace
        .1
        .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                Some(&entry.correlation().unwrap()) == world.quit.as_ref()
                    && entry
                        .payload::<Ui::Menu::Presentation::Render>()
                        .unwrap()
                        .is_some()
            })
        })
        .unwrap();
    assert!(
        entries.iter().any(|entry| {
            Some(&entry.correlation().unwrap()) == world.quit.as_ref()
                && entry
                    .payload::<Ui::Menu::Presentation::Render>()
                    .unwrap()
                    .is_some()
        }),
        "the final remote query's own render must reach the observation ledger"
    );
}

#[given("a typed menu with a queued projection source")]
fn queued_projection_source(world: &mut Menu) {
    let mut builder = RegistryBuilder::default();
    builder
        .on::<Power::Session::Protection::Query>(|_, events| {
            for (revision, state, is_on) in [
                (2, State::On, Some(true)),
                (3, State::Pending, None),
                (4, State::Off, Some(false)),
                (3, State::On, Some(true)),
                (4, State::Off, Some(false)),
            ] {
                let mut status = Status::off();
                status.revision = revision;
                status.state = state;
                status.is_on = is_on;
                status.desired_on = state == State::On;
                events.emit::<Power::Session::Protection::Snapshot>(status);
            }
            Ok(())
        })
        .unwrap();
    let bus = builder.build().unwrap();
    world.subscription = Some(observe_menu(&bus, world.view.clone()));
    bus.observe(world.queue_trace.clone());
    world.bus = Some(bus);
}

#[when("one root queues On Pending Off and stale observations before rendering")]
fn queued_projection_observations(world: &mut Menu) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Query>(Empty {});
    let entries = world.queue_trace.0.lock().unwrap();
    let last_observation = entries
        .iter()
        .rposition(|entry| {
            entry
                .payload::<Power::Session::Protection::Snapshot>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let first_render = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Ui::Menu::Presentation::Render>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(
        last_observation < first_render,
        "the fixture must force observations ahead of renders"
    );
    for (index, entry) in entries.iter().enumerate() {
        if let Some(status) = entry
            .payload::<Power::Session::Protection::Snapshot>()
            .unwrap()
        {
            eprintln!(
                "CAUSAL_OBSERVATION index={index} revision={} state={:?}",
                status.revision, status.state
            );
        }
        if let Some(render) = entry.payload::<Ui::Menu::Presentation::Render>().unwrap() {
            eprintln!(
                "CAUSAL_RENDER index={index} label={} ax={}",
                render.text["status"], render.text["accessibility"]
            );
        }
    }
}

#[then("each queued render matches its causal observation and confirmed mode")]
fn queued_projection_matches(world: &mut Menu) {
    assert_ordered_projection(&world.queue_trace.0.lock().unwrap());
    let frames = world.view.0.lock().unwrap();
    assert_eq!(
        frames.len(),
        4,
        "stale revisions must not render; equal revisions must render"
    );
    assert_eq!(frames.last().unwrap().text["status"], "Mode: Can Sleep");
}

fn assert_ordered_projection(entries: &[KvEnvelope]) {
    let mut expected = VecDeque::new();
    let mut rendered = Vec::new();
    let mut confirmed_label = "Mode: Can Sleep";
    let mut accepted_revision = None;
    for (index, entry) in entries.iter().enumerate() {
        let status = entry
            .payload::<Power::Session::Protection::Changed>()
            .unwrap()
            .or_else(|| {
                entry
                    .payload::<Power::Session::Protection::Snapshot>()
                    .unwrap()
            });
        if let Some(status) = status {
            if accepted_revision.is_some_and(|revision| status.revision < revision) {
                continue;
            }
            accepted_revision = Some(status.revision);
            let label = match (status.state, status.is_on) {
                (State::On, Some(true)) => {
                    confirmed_label = "Mode: No Sleep";
                    confirmed_label
                }
                (State::Off, Some(false)) => {
                    confirmed_label = "Mode: Can Sleep";
                    confirmed_label
                }
                (State::Pending, _) => confirmed_label,
                _ => "Off: Error",
            };
            // These callbacks synchronously enqueue one render per accepted observation.
            expected.push_back((index, entry.correlation().unwrap(), status, label));
        }
        if let Some(render) = entry.payload::<Ui::Menu::Presentation::Render>().unwrap() {
            let (observation, correlation, status, label) = expected
                .pop_front()
                .expect("render has no accepted emitting observation");
            assert_eq!(
                entry.correlation().unwrap(),
                correlation,
                "render origin differs from the next causal observation"
            );
            let phase = match status.state {
                State::On => "On",
                State::Off => "Off",
                State::Pending => "Pending",
                State::Error => "Error",
            };
            eprintln!(
                "CAUSAL_PAIR observation={observation} render={index} revision={} phase={phase} expected_label={label} actual_label={}",
                status.revision, render.text["status"]
            );
            assert!(render.text["accessibility"].contains(phase));
            assert_eq!(
                render.text["status"], label,
                "visible label must match its emitting observation, not a later shared-chain status"
            );
            rendered.push((status.revision, phase.to_owned()));
        }
    }
    assert!(
        expected.is_empty(),
        "accepted observations lost their corresponding renders"
    );
    assert!(rendered.iter().any(|(_, phase)| phase == "Pending"));
    assert!(rendered.iter().any(|(_, phase)| phase == "On"));
    assert!(
        rendered.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "rendered revisions regressed: {rendered:?}"
    );
}

#[then("rendered protection revisions never regress and menu and remote status agree Off")]
fn ordered_persistent_projection(world: &mut Menu) {
    assert_ordered_projection(&world.queue_trace.0.lock().unwrap());
    let remote_entries = world.queue_remote_trace.0.lock().unwrap();
    let result = remote_entries
        .iter()
        .find_map(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .filter(|settled| Some(&settled.correlation) == world.quit.as_ref())
        })
        .unwrap();
    assert!(result.result.error.is_none());
    assert_eq!(result.result.status.unwrap().is_on, Some(false));
    assert_eq!(last(world).text["status"], "Mode: Can Sleep");
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
