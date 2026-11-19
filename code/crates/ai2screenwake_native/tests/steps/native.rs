// native.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Native ownership requirements observed only through the typed Actions boundary.

use ai2screenwake_backend::contract::{
    Apply, AssertionsObserved, ClockSample, CornerAuthorization, Empty, Error, Fault, IdGeneration,
    IdleObserved, IdleRequest, InterruptionObserved, KvEnvelope, ObservationSink, Outcome, Power,
    Registry, RegistryBuilder, SleepPhase,
};
use ai2screenwake_native::{
    NativePower, contracts,
    corners::{Configuration, CornerApi, CornerCallback, CornerRegistration, Display, Point},
    lifecycle::{
        LifecycleMonitor, NotificationCallback, NotificationRegistration, NotificationSource,
        SleepNotification,
    },
    power::{AssertionKind, MacPower, PowerApi},
};
use cucumber::{World, given, then, when};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct Notifications {
    callback: Mutex<Option<NotificationCallback>>,
    starts: AtomicUsize,
    stops: AtomicUsize,
    acknowledgements: AtomicUsize,
    fail_registration: std::sync::atomic::AtomicBool,
    fail_poll: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct ControlledNotifications(Arc<Notifications>);

struct ControlledRegistration(Arc<Notifications>);

impl NotificationSource for ControlledNotifications {
    fn register(
        &self,
        callback: NotificationCallback,
    ) -> Result<Box<dyn NotificationRegistration>, Fault> {
        if self.0.fail_registration.load(Ordering::SeqCst) {
            return Err(Fault::new(
                "test_lifecycle_register",
                "Controlled registration fault.",
                true,
            ));
        }
        *self.0.callback.lock().unwrap() = Some(callback);
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(ControlledRegistration(self.0.clone())))
    }
}

impl NotificationRegistration for ControlledRegistration {
    fn poll(&mut self) -> Result<(), Fault> {
        std::thread::sleep(Duration::from_millis(2));
        if self.0.fail_poll.load(Ordering::SeqCst) {
            return Err(Fault::new(
                "test_lifecycle_poll",
                "Controlled source failure.",
                true,
            ));
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<(), Fault> {
        self.0.callback.lock().unwrap().take();
        self.0.stops.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Default)]
struct Calls {
    corners: Option<Arc<TestCorners>>,
    acquired: Vec<(AssertionKind, u32)>,
    held: Vec<u32>,
    releases: Vec<u32>,
    fail_display: bool,
    fail_release_once: bool,
    idle: Option<Result<f64, Fault>>,
    fail_clock: bool,
}

#[derive(Default)]
struct Controlled(Mutex<Calls>);

impl PowerApi for Controlled {
    fn corners(&self) -> Option<Arc<dyn CornerApi>> {
        self.0
            .lock()
            .unwrap()
            .corners
            .clone()
            .map(|api| api as Arc<dyn CornerApi>)
    }
    fn acquire(&self, kind: AssertionKind) -> Result<u32, Fault> {
        let mut calls = self.0.lock().unwrap();
        if kind == AssertionKind::Display && calls.fail_display {
            return Err(Fault::new(
                "test_acquire",
                "Controlled acquisition fault.",
                true,
            ));
        }
        let id = u32::try_from(calls.acquired.len() + 1).unwrap();
        calls.acquired.push((kind, id));
        calls.held.push(id);
        Ok(id)
    }

    fn release(&self, id: u32) -> Result<(), Fault> {
        let mut calls = self.0.lock().unwrap();
        calls.releases.push(id);
        if std::mem::take(&mut calls.fail_release_once) {
            return Err(Fault::new(
                "test_release",
                "Controlled release fault.",
                true,
            ));
        }
        assert!(calls.held.contains(&id), "released an unowned ID");
        calls.held.retain(|held| *held != id);
        Ok(())
    }

    fn clock(&self) -> Result<ClockSample, Fault> {
        if self.0.lock().unwrap().fail_clock {
            return Err(Fault::new("test_clock", "Controlled clock fault.", true));
        }
        Ok(ClockSample {
            epoch_ms: 1_800_000_000_000,
            monotonic_ms: 1000,
        })
    }

    fn idle_seconds(&self) -> Result<f64, Fault> {
        self.0.lock().unwrap().idle.clone().unwrap_or(Ok(1.234))
    }
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct Host(Mutex<Vec<u32>>);

#[cfg(target_os = "macos")]
impl PowerApi for Host {
    fn acquire(&self, kind: AssertionKind) -> Result<u32, Fault> {
        let id = ai2screenwake_native::power::SystemPower.acquire(kind)?;
        self.0.lock().unwrap().push(id);
        Ok(id)
    }

    fn release(&self, id: u32) -> Result<(), Fault> {
        ai2screenwake_native::power::SystemPower.release(id)
    }

    fn clock(&self) -> Result<ClockSample, Fault> {
        ai2screenwake_native::power::SystemPower.clock()
    }

    fn idle_seconds(&self) -> Result<f64, Fault> {
        ai2screenwake_native::power::SystemPower.idle_seconds()
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<KvEnvelope>>, Condvar);

impl ObservationSink for Sink {
    fn observed(&self, envelope: KvEnvelope) {
        self.0.lock().unwrap().push(envelope);
        self.1.notify_all();
    }
}

#[derive(Default, World)]
struct Native {
    bus: Option<Registry>,
    native: Option<Arc<dyn NativePower>>,
    api: Arc<Controlled>,
    sink: Arc<Sink>,
    last_request: Option<Apply>,
    idle_request: Option<IdleRequest>,
    #[cfg(target_os = "macos")]
    host: Arc<Host>,
    runtime_paths: Option<(std::path::PathBuf, std::path::PathBuf)>,
    launch_errors: Vec<Fault>,
    power: Option<Arc<MacPower>>,
    notifications: Arc<ControlledNotifications>,
    monitor: Option<Arc<Mutex<LifecycleMonitor>>>,
    lifecycle_failure: Option<Fault>,
    prompt_acknowledgement: bool,
    real_monitors_stopped: usize,
    bounded_stop: bool,
    log_names: Vec<(std::path::PathBuf, std::path::PathBuf)>,
    log_home: Option<std::path::PathBuf>,
    corners: Arc<TestCorners>,
}

#[derive(Default)]
struct TestCorners {
    denied: std::sync::atomic::AtomicBool,
    fail_install: std::sync::atomic::AtomicBool,
    prompts: AtomicUsize,
    starts: AtomicUsize,
    stops: Arc<AtomicUsize>,
    callback: Mutex<Option<CornerCallback>>,
    hold_poll: std::sync::atomic::AtomicBool,
    poll_gate: Arc<(Mutex<bool>, Condvar)>,
    poll_entered: Arc<(Mutex<bool>, Condvar)>,
    fail_activation: std::sync::atomic::AtomicBool,
}

fn corner_configuration() -> Configuration {
    Configuration::from_preferences(
        [1, 5, 1, 13],
        [0; 4],
        vec![Display {
            origin: Point { x: 0.0, y: 0.0 },
            width: 100.0,
            height: 100.0,
        }],
    )
    .unwrap()
}

impl CornerApi for TestCorners {
    fn configuration(&self) -> Result<Configuration, Fault> {
        Ok(corner_configuration())
    }
    fn permission(&self, request: bool) -> Result<(), Fault> {
        if request {
            self.prompts.fetch_add(1, Ordering::SeqCst);
        }
        if self.denied.load(Ordering::SeqCst) {
            Err(Fault::new(
                "test_corner_denied",
                "Controlled corner permission denial.",
                true,
            ))
        } else {
            Ok(())
        }
    }
    fn install(
        &self,
        _: &Configuration,
        callback: CornerCallback,
    ) -> Result<Box<dyn CornerRegistration>, Fault> {
        if self.fail_install.load(Ordering::SeqCst) {
            return Err(Fault::new(
                "test_corner_install",
                "Controlled nil source.",
                true,
            ));
        }
        *self.callback.lock().unwrap() = Some(callback);
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(TestCornerRegistration {
            stops: self.stops.clone(),
            hold_poll: self.hold_poll.load(Ordering::SeqCst),
            poll_gate: self.poll_gate.clone(),
            poll_entered: self.poll_entered.clone(),
            fail_activation: self.fail_activation.load(Ordering::SeqCst),
            activated: false,
        }))
    }
}

struct TestCornerRegistration {
    stops: Arc<AtomicUsize>,
    hold_poll: bool,
    poll_gate: Arc<(Mutex<bool>, Condvar)>,
    poll_entered: Arc<(Mutex<bool>, Condvar)>,
    fail_activation: bool,
    activated: bool,
}

impl CornerRegistration for TestCornerRegistration {
    fn validate_activation(&mut self) -> Result<(), Fault> {
        if self.fail_activation {
            return Err(Fault::new(
                "test_corner_activation",
                "Controlled final readiness failure.",
                true,
            ));
        }
        self.activated = true;
        Ok(())
    }
    fn poll(&mut self) -> Result<(), Fault> {
        if self.hold_poll && self.activated {
            let (entered, condition) = &*self.poll_entered;
            *entered.lock().unwrap() = true;
            condition.notify_all();
            let (gate, condition) = &*self.poll_gate;
            let (_guard, timeout) = condition
                .wait_timeout_while(gate.lock().unwrap(), Duration::from_secs(2), |released| {
                    !*released
                })
                .unwrap();
            if timeout.timed_out() {
                return Err(Fault::new(
                    "test_corner_hold_expired",
                    "Controlled poll hold expired.",
                    true,
                ));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }
    fn stop(&mut self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

impl std::fmt::Debug for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Native").finish_non_exhaustive()
    }
}

fn request(enabled: bool) -> Apply {
    Apply {
        session_id: IdGeneration::next_id::<Power::Session::Protection::Query>(&Empty {}).unwrap(),
        generation: 1,
        operation_id: IdGeneration::next_id::<Power::Session::Protection::Query>(&Empty {})
            .unwrap(),
        enabled,
    }
}

impl Native {
    fn attach(&mut self, native: Arc<dyn NativePower>) {
        let mut builder = RegistryBuilder::default();
        contracts::register(&mut builder, native.clone()).unwrap();
        builder
            .on::<Power::Session::Protection::TurnOff>(|_, _| Ok(()))
            .unwrap();
        builder
            .on::<Power::Session::Protection::Toggle>(|_, _| Ok(()))
            .unwrap();
        builder
            .on::<Power::Session::Protection::Quit>(|_, _| Ok(()))
            .unwrap();
        let bus = builder.build().unwrap();
        contracts::observe_controls(&bus, &native);
        bus.observe(self.sink.clone());
        self.native = Some(native);
        self.bus = Some(bus);
    }

    fn observed(&self) -> AssertionsObserved {
        self.sink
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|e| {
                e.payload::<Power::Assertions::Resources::Observed>()
                    .unwrap()
            })
            .expect("native outcome must be observed")
    }
}

#[given("a native owner with controlled OS calls")]
fn owner(world: &mut Native) {
    world.attach(Arc::new(MacPower::with_api(world.api.clone())));
}

#[given("a native owner whose manual-corner permission is denied")]
fn corner_denied(world: &mut Native) {
    world.corners.denied.store(true, Ordering::SeqCst);
    guarded_owner(world);
}

#[given("an admitted native owner with a controlled saver corner")]
fn guarded_owner(world: &mut Native) {
    world.api.0.lock().unwrap().corners = Some(world.corners.clone());
    owner(world);
}

#[given("a native owner whose corner source installation fails")]
fn guard_install_failure(world: &mut Native) {
    world.corners.fail_install.store(true, Ordering::SeqCst);
    guarded_owner(world);
}

#[given("a corner source that loses readiness at final activation")]
fn corner_activation_failure(world: &mut Native) {
    world.corners.fail_activation.store(true, Ordering::SeqCst);
    guarded_owner(world);
}

#[given("a native corner source whose poll is deliberately held")]
fn held_corner_source(world: &mut Native) {
    world.corners.hold_poll.store(true, Ordering::SeqCst);
    guarded_owner(world);
}

#[when("the held corner source is released and cleanup is retried")]
fn release_corner_source(world: &mut Native) {
    let (gate, condition) = &*world.corners.poll_gate;
    *gate.lock().unwrap() = true;
    condition.notify_all();
    assert_eq!(
        corner_callback(world).filter(Point { x: 99.0, y: 0.0 }),
        None
    );
    disable(world);
}

#[then(expr = "an attributed corner failure retains {int} resources")]
fn corner_failed(world: &mut Native, held: u8) {
    let entries = world.sink.0.lock().unwrap();
    let index = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Assertions::Resources::Failed>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let failure = entries[index]
        .payload::<Power::Assertions::Resources::Failed>()
        .unwrap()
        .unwrap();
    assert_eq!(failure.held_count, held);
    assert_eq!(&failure.request, world.last_request.as_ref().unwrap());
    assert!(entries[..index].iter().any(|entry| {
        entry
            .payload::<Error::Native::Assertions::Occurred>()
            .unwrap()
            .is_some_and(|fault| {
                fault.fault_code == failure.fault.fault_code && fault.failed_genus.is_some()
            })
    }));
    assert_eq!(world.api.0.lock().unwrap().held.len(), usize::from(held));
}

fn corner_callback(world: &Native) -> CornerCallback {
    world
        .corners
        .callback
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone()
}

#[then("corner permission was requested only once and no source was installed")]
fn one_corner_prompt(world: &mut Native) {
    assert_eq!(world.corners.prompts.load(Ordering::SeqCst), 1);
    assert_eq!(world.corners.starts.load(Ordering::SeqCst), 0);
    assert_eq!(world.observed().held_count, 0);
}

#[then("the saver corner is deflected and the lock corner is unchanged")]
fn guard_filter(world: &mut Native) {
    assert_eq!(world.observed().outcome, Outcome::Applied);
    assert_eq!(world.observed().held_count, 2);
    let callback = corner_callback(world);
    assert_eq!(
        callback.filter(Point { x: 99.0, y: 0.0 }),
        Some(Point { x: 97.0, y: 3.0 })
    );
    assert_eq!(callback.filter(Point { x: 99.0, y: 99.0 }), None);
    assert_eq!(callback.filter(Point { x: 99.0, y: 50.0 }), None);
}

#[then("the retired callback passes through and its source stopped once")]
fn guard_retired(world: &mut Native) {
    assert_eq!(
        corner_callback(world).filter(Point { x: 99.0, y: 0.0 }),
        None
    );
    assert_eq!(world.corners.stops.load(Ordering::SeqCst), 1);
    assert!(world.api.0.lock().unwrap().held.is_empty());
}

#[when("the corner input source is disabled")]
fn guard_source_loss(world: &mut Native) {
    corner_callback(world).lost(Fault::new(
        "test_corner_disabled",
        "Controlled tap disabled.",
        true,
    ));
    assert_eq!(
        corner_callback(world).filter(Point { x: 99.0, y: 0.0 }),
        None
    );
}

#[then("the callback passes through and a typed source loss follows its error")]
fn guard_lost(world: &mut Native) {
    let entries = wait_for_entries(world, |entries| {
        entries.iter().any(|entry| {
            entry
                .payload::<Power::Corners::Guard::Lost>()
                .unwrap()
                .is_some()
        })
    });
    let lost = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Corners::Guard::Lost>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(entries[..lost].iter().any(|entry| {
        entry
            .payload::<Error::Native::Assertions::Occurred>()
            .unwrap()
            .is_some()
    }));
    let loss = entries[lost]
        .payload::<Power::Corners::Guard::Lost>()
        .unwrap()
        .unwrap();
    assert_eq!(&loss.request, world.last_request.as_ref().unwrap());
    assert_eq!(
        corner_callback(world).filter(Point { x: 99.0, y: 0.0 }),
        None
    );
    disable(world);
}

#[when("Off is observed before stale corner authorization")]
fn stale_corner_authorization(world: &mut Native) {
    let request = world.last_request.clone();
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOff>(Empty {});
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Corners::Guard::Authorize>(CornerAuthorization {
            revision: u64::MAX,
            request,
        });
    assert_eq!(
        corner_callback(world).filter(Point { x: 99.0, y: 0.0 }),
        None
    );
    disable(world);
}

#[then("corner geometry preserves ordinary edges modifiers and security corners")]
fn corner_geometry(_: &mut Native) {
    let displays = vec![
        Display {
            origin: Point {
                x: -1200.0,
                y: -200.0,
            },
            width: 1200.0,
            height: 1920.0,
        },
        Display {
            origin: Point { x: 0.0, y: 0.0 },
            width: 2560.0,
            height: 1440.0,
        },
    ];
    let configuration =
        Configuration::from_preferences([5, 5, 13, 10], [0, 0x100000, 0, 0], displays.clone())
            .unwrap();
    assert_eq!(
        configuration.deflect(Point {
            x: -1200.0,
            y: -200.0
        }),
        Some((
            Point {
                x: -1197.0,
                y: -197.0
            },
            0
        ))
    );
    assert_eq!(
        configuration.deflect(Point { x: -1.0, y: -199.0 }),
        Some((Point { x: -3.0, y: -197.0 }, 1))
    );
    assert_eq!(
        configuration.deflect(Point { x: 2559.0, y: 0.0 }),
        Some((Point { x: 2557.0, y: 3.0 }, 1))
    );
    for point in [
        Point {
            x: -1200.0,
            y: 1719.0,
        },
        Point {
            x: 2559.0,
            y: 1439.0,
        },
        Point {
            x: 2559.0,
            y: 100.0,
        },
        Point {
            x: -1201.0,
            y: -201.0,
        },
        Point {
            x: f64::NAN,
            y: 0.0,
        },
    ] {
        assert!(configuration.deflect(point).is_none());
    }
    assert!(Configuration::from_preferences([99, 5, 1, 1], [0; 4], displays.clone()).is_err());
    assert!(Configuration::from_preferences([1, 5, 1, 1], [1; 4], displays).is_err());
}

#[cfg(target_os = "macos")]
#[then("unposted movement and drag events are deflected without changing keys or clicks")]
fn real_corner_events(world: &mut Native) {
    let callback = corner_callback(world);
    for kind in [5, 6, 7, 27] {
        assert_eq!(
            ai2screenwake_native::corners::system::unposted_event(
                &callback,
                kind,
                Point { x: 99.0, y: 0.0 }
            )
            .unwrap(),
            Point { x: 97.0, y: 3.0 }
        );
    }
    for kind in [1, 2, 10, 11, 12, 22] {
        assert_eq!(
            ai2screenwake_native::corners::system::unposted_event(
                &callback,
                kind,
                Point { x: 99.0, y: 0.0 }
            )
            .unwrap(),
            Point { x: 99.0, y: 0.0 }
        );
    }
}

#[cfg(target_os = "macos")]
#[then("unposted movement passes through the retired callback")]
fn real_retired_corner(world: &mut Native) {
    assert_eq!(
        ai2screenwake_native::corners::system::unposted_event(
            &corner_callback(world),
            5,
            Point { x: 99.0, y: 0.0 }
        )
        .unwrap(),
        Point { x: 99.0, y: 0.0 }
    );
}

#[when("a native toggle is observed while Off")]
fn corner_toggle_off(world: &mut Native) {
    assert_eq!(world.corners.prompts.load(Ordering::SeqCst), 0);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Toggle>(Empty {});
    assert_eq!(world.corners.prompts.load(Ordering::SeqCst), 0);
}

#[when("native resources are requested without corner admission")]
fn corner_unadmitted(world: &mut Native) {
    let request = request(true);
    world.last_request = Some(request.clone());
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Assertions::Resources::Apply>(request);
    assert_eq!(
        corner_callback(world).filter(Point { x: 99.0, y: 0.0 }),
        None
    );
}

#[when("Quit is observed before native cleanup")]
fn corner_quit(world: &mut Native) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Quit>(Empty {});
    assert_eq!(
        corner_callback(world).filter(Point { x: 99.0, y: 0.0 }),
        None
    );
    disable(world);
}

#[cfg(target_os = "macos")]
struct PreferenceFixture(std::path::PathBuf);

#[cfg(target_os = "macos")]
impl PreferenceFixture {
    fn new() -> Self {
        let id = IdGeneration::next_id::<Power::Session::Protection::Query>(&Empty {}).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let directory = root.join("target").join(format!("corner-prefs-{id}"));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("com.apple.dock.plist")
    }
    fn write(&self, action: i64, extra: &str) {
        std::fs::write(self.path(), Self::bytes(action, extra)).unwrap();
    }
    fn replace(&self, action: i64, extra: &str) {
        let replacement = self.0.join("replacement");
        std::fs::write(&replacement, Self::bytes(action, extra)).unwrap();
        std::fs::rename(replacement, self.path()).unwrap();
    }
    fn bytes(action: i64, extra: &str) -> String {
        let bytes = format!(
            "<plist version=\"1.0\"><dict><key>wvous-tr-corner</key><integer>{action}</integer><key>wvous-br-corner</key><integer>13</integer>{extra}</dict></plist>"
        );
        bytes
    }
}

#[cfg(target_os = "macos")]
impl Drop for PreferenceFixture {
    fn drop(&mut self) {
        for name in ["com.apple.dock.plist", "replacement", "unrelated"] {
            let path = self.0.join(name);
            if path.symlink_metadata().is_ok() {
                std::fs::remove_file(path).unwrap();
            }
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

#[cfg(target_os = "macos")]
#[then("unrelated files and unrelated Dock settings preserve the exact corner assignment")]
fn unrelated_preferences(_: &mut Native) {
    use ai2screenwake_native::corners::system::PreferenceFence;
    let fixture = PreferenceFixture::new();
    fixture.write(5, "");
    let fence = PreferenceFence::open(fixture.path(), &corner_configuration()).unwrap();
    std::fs::write(fixture.0.join("unrelated"), b"owned fixture").unwrap();
    fence.revalidate(&corner_configuration()).unwrap();
    assert!(fence.fresh());
    fixture.write(5, "<key>unrelatedSetting</key><true/>");
    fence.revalidate(&corner_configuration()).unwrap();
    assert!(fence.fresh());
    fixture.replace(5, "<key>anotherUnrelatedSetting</key><false/>");
    fence.revalidate(&corner_configuration()).unwrap();
    assert!(fence.fresh());
}

#[cfg(target_os = "macos")]
#[then("an atomic saver to Lock replacement fails configuration revalidation")]
fn lock_preferences(_: &mut Native) {
    use ai2screenwake_native::corners::system::PreferenceFence;
    let fixture = PreferenceFixture::new();
    fixture.write(5, "");
    let fence = PreferenceFence::open(fixture.path(), &corner_configuration()).unwrap();
    fixture.replace(13, "");
    assert!(!fence.fresh());
    assert_eq!(
        fence
            .revalidate(&corner_configuration())
            .unwrap_err()
            .fault_code,
        "corner_configuration_changed"
    );
    assert!(!fence.fresh());
}

#[cfg(target_os = "macos")]
#[then("malformed and symlink preference replacements fail closed")]
fn unsafe_preferences(_: &mut Native) {
    use ai2screenwake_native::corners::system::PreferenceFence;
    let fixture = PreferenceFixture::new();
    fixture.write(5, "");
    let fence = PreferenceFence::open(fixture.path(), &corner_configuration()).unwrap();
    std::fs::write(fixture.path(), b"not a property list").unwrap();
    assert!(fence.revalidate(&corner_configuration()).is_err());
    std::fs::remove_file(fixture.path()).unwrap();
    std::os::unix::fs::symlink(fixture.0.join("unrelated"), fixture.path()).unwrap();
    assert!(fence.revalidate(&corner_configuration()).is_err());
    assert!(!fence.fresh());
}

#[given("an enabled native owner with controlled sleep notifications")]
fn lifecycle_owner(world: &mut Native) {
    let power = Arc::new(MacPower::with_api(world.api.clone()));
    world.attach(power.clone());
    enable(world);
    world.monitor = Some(Arc::new(Mutex::new(
        LifecycleMonitor::with_source(power.clone(), world.notifications.clone()).unwrap(),
    )));
    world.power = Some(power);
}

fn notify(world: &Native, message: SleepNotification) {
    let callback = world
        .notifications
        .0
        .callback
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    callback.receive(message, || {
        world
            .notifications
            .0
            .acknowledgements
            .fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
}

#[when("the source reports impending sleep and completed wake")]
fn sleep_wake(world: &mut Native) {
    notify(world, SleepNotification::WillSleep);
    notify(world, SleepNotification::DidWake);
}

fn interruptions(entries: &[KvEnvelope]) -> Vec<InterruptionObserved> {
    entries
        .iter()
        .filter_map(|entry| {
            entry
                .payload::<Power::Activity::System::Interrupted>()
                .unwrap()
        })
        .collect()
}

#[then("both typed interruptions carry the exact active operation count and clocks")]
fn interrupted(world: &mut Native) {
    let entries = world.sink.0.lock().unwrap();
    let (entries, _) = world
        .sink
        .1
        .wait_timeout_while(entries, Duration::from_secs(1), |entries| {
            interruptions(entries).len() < 2
        })
        .unwrap();
    let observations = interruptions(&entries);
    assert_eq!(
        observations.len(),
        2,
        "sleep/wake must reach the typed observe stream"
    );
    assert_eq!(observations[0].phase, SleepPhase::WillSleep);
    assert_eq!(observations[1].phase, SleepPhase::DidWake);
    for observed in observations {
        assert_eq!(&observed.request, world.last_request.as_ref().unwrap());
        assert_eq!(observed.held_count, 2);
        assert_eq!(observed.clock, world.api.clock().unwrap());
    }
}

#[then("only the impending sleep was acknowledged without veto")]
fn allow_only_sleep(world: &mut Native) {
    assert_eq!(
        world
            .notifications
            .0
            .acknowledgements
            .load(Ordering::SeqCst),
        1
    );
}

fn wait_for_entries(world: &Native, predicate: impl Fn(&[KvEnvelope]) -> bool) -> Vec<KvEnvelope> {
    let entries = world.sink.0.lock().unwrap();
    let (entries, _) = world
        .sink
        .1
        .wait_timeout_while(entries, Duration::from_secs(1), |entries| {
            !predicate(entries)
        })
        .unwrap();
    assert!(
        predicate(&entries),
        "required typed observation did not arrive"
    );
    entries.clone()
}

fn stop_monitor(world: &Native) {
    world
        .monitor
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .stop()
        .unwrap();
}

#[when("the source asks permission to sleep then reports completed wake")]
fn can_sleep(world: &mut Native) {
    notify(world, SleepNotification::CanSleep);
    notify(world, SleepNotification::DidWake);
}

#[then("only completed wake is observed and permission was allowed")]
fn only_wake(world: &mut Native) {
    let entries = wait_for_entries(world, |entries| !interruptions(entries).is_empty());
    let observed = interruptions(&entries);
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].phase, SleepPhase::DidWake);
    assert_eq!(
        world
            .notifications
            .0
            .acknowledgements
            .load(Ordering::SeqCst),
        1
    );
}

#[when("sleep is reported while off before a new enable and wake")]
fn off_then_wake(world: &mut Native) {
    disable(world);
    notify(world, SleepNotification::WillSleep);
    enable(world);
    notify(world, SleepNotification::DidWake);
}

#[then("only completed wake carries the replacement active operation")]
fn replacement_wake(world: &mut Native) {
    only_wake(world);
    let entries = world.sink.0.lock().unwrap();
    assert_eq!(
        interruptions(&entries)[0].request,
        *world.last_request.as_ref().unwrap()
    );
}

#[when("the registry is disposed before a sleep notification")]
fn expired_context(world: &mut Native) {
    world.bus.take();
    world.native.take();
    notify(world, SleepNotification::WillSleep);
    stop_monitor(world);
}

#[then("no interruption is fabricated and native ownership is acyclic")]
fn no_context(world: &mut Native) {
    assert_eq!(Arc::strong_count(world.power.as_ref().unwrap()), 1);
    assert!(interruptions(&world.sink.0.lock().unwrap()).is_empty());
    assert_eq!(
        world
            .notifications
            .0
            .acknowledgements
            .load(Ordering::SeqCst),
        1
    );
}

#[given("a partially acquired native owner with controlled sleep notifications")]
fn partial_lifecycle(world: &mut Native) {
    world.api.0.lock().unwrap().fail_display = true;
    world.api.0.lock().unwrap().fail_release_once = true;
    lifecycle_owner(world);
}

#[then("both interruptions truthfully report one retained resource")]
fn partial_interruptions(world: &mut Native) {
    let entries = wait_for_entries(world, |entries| interruptions(entries).len() == 2);
    for observed in interruptions(&entries) {
        assert_eq!(observed.held_count, 1);
        assert_eq!(observed.request, *world.last_request.as_ref().unwrap());
    }
}

#[when("an interruption observer requests explicit Off")]
fn interrupted_off(world: &mut Native) {
    let off = request(false);
    world
        .bus
        .as_ref()
        .unwrap()
        .observe_action::<Power::Activity::System::Interrupted>(move |_, events| {
            events.emit::<Power::Assertions::Resources::Apply>(off.clone());
        });
    notify(world, SleepNotification::WillSleep);
}

#[then("resource cleanup completes without blocking acknowledgement")]
fn interrupted_cleanup(world: &mut Native) {
    wait_for_entries(world, |entries| {
        entries.iter().any(|e| {
            e.payload::<Power::Assertions::Resources::Observed>()
                .unwrap()
                .is_some_and(|o| !o.request.enabled && o.held_count == 0)
        })
    });
    assert!(world.api.0.lock().unwrap().held.is_empty());
    assert_eq!(
        world
            .notifications
            .0
            .acknowledgements
            .load(Ordering::SeqCst),
        1
    );
}

#[when("the sleep source fails before explicit Off and another enable")]
fn source_fails(world: &mut Native) {
    world
        .notifications
        .0
        .fail_poll
        .store(true, Ordering::SeqCst);
    wait_for_entries(world, |entries| {
        entries.iter().any(|entry| {
            entry
                .payload::<Error::Native::Activity::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "test_lifecycle_poll")
        })
    });
    disable(world);
    enable(world);
}

#[then("native acquisition is refused until a new source starts and cleanup stays available")]
fn unavailable_source(world: &mut Native) {
    assert!(
        world.api.0.lock().unwrap().held.is_empty(),
        "failed observation must not permit reacquisition"
    );
    assert_eq!(world.api.0.lock().unwrap().acquired.len(), 2);
    assert!(world.sink.0.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<Power::Assertions::Resources::Failed>()
            .unwrap()
            .is_some_and(|failure| {
                failure.fault.fault_code == "lifecycle_unavailable" && failure.held_count == 0
            })
    }));
    world
        .notifications
        .0
        .fail_poll
        .store(false, Ordering::SeqCst);
    let mut restarted = LifecycleMonitor::with_source(
        world.power.as_ref().unwrap().clone(),
        world.notifications.clone(),
    )
    .unwrap();
    // The failed monitor cannot invalidate its newer replacement.
    let _ = world.monitor.as_ref().unwrap().lock().unwrap().stop();
    enable(world);
    assert_eq!(world.observed().held_count, 2);
    disable(world);
    assert!(world.api.0.lock().unwrap().held.is_empty());
    restarted.stop().unwrap();
}

#[when("a wake observer pauses delivery while a new sleep notification arrives")]
fn slow_observer(world: &mut Native) {
    let (entered_sender, entered) = std::sync::mpsc::sync_channel(1);
    let pause = Arc::new((Mutex::new(false), Condvar::new()));
    let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let expired = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observer_exited = exited.clone();
    let observer_expired = expired.clone();
    let observer_pause = pause.clone();
    world
        .bus
        .as_ref()
        .unwrap()
        .observe_action::<Power::Activity::System::Interrupted>(move |observed, _| {
            if observed.phase == SleepPhase::DidWake {
                entered_sender.send(()).unwrap();
                let guard = observer_pause.0.lock().unwrap();
                let (guard, timeout) = observer_pause
                    .1
                    .wait_timeout_while(guard, Duration::from_millis(500), |released| !*released)
                    .unwrap();
                observer_expired.store(timeout.timed_out() && !*guard, Ordering::SeqCst);
                observer_exited.store(true, Ordering::SeqCst);
                drop(guard);
            }
        });
    notify(world, SleepNotification::DidWake);
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    let started = std::time::Instant::now();
    notify(world, SleepNotification::WillSleep);
    world.prompt_acknowledgement = started.elapsed() < Duration::from_millis(100)
        && world
            .notifications
            .0
            .acknowledgements
            .load(Ordering::SeqCst)
            == 1
        && !exited.load(Ordering::SeqCst)
        && !expired.load(Ordering::SeqCst);
    *pause.0.lock().unwrap() = true;
    pause.1.notify_all();
    wait_for_entries(world, |entries| interruptions(entries).len() == 2);
    assert!(
        !expired.load(Ordering::SeqCst),
        "observer hold expired before the explicit release"
    );
}

#[then("sleep was promptly allowed before that observer resumed")]
fn prompt_ack(world: &mut Native) {
    assert!(world.prompt_acknowledgement);
}

#[when("a wake observer stops the monitor and a late sleep callback arrives")]
fn reentrant_stop(world: &mut Native) {
    let monitor = world.monitor.as_ref().unwrap().clone();
    let (sender, completed) = std::sync::mpsc::sync_channel(1);
    world
        .bus
        .as_ref()
        .unwrap()
        .observe_action::<Power::Activity::System::Interrupted>(move |_, _| {
            sender.send(monitor.lock().unwrap().stop()).unwrap();
        });
    let late = world
        .notifications
        .0
        .callback
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    notify(world, SleepNotification::DidWake);
    completed
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    late.receive(SleepNotification::WillSleep, || {
        world
            .notifications
            .0
            .acknowledgements
            .fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
}

#[then("the source is removed once and the late callback only allows sleep")]
fn stopped_late(world: &mut Native) {
    assert_eq!(world.notifications.0.stops.load(Ordering::SeqCst), 1);
    assert!(world.notifications.0.callback.lock().unwrap().is_none());
    assert_eq!(interruptions(&world.sink.0.lock().unwrap()).len(), 1);
    assert_eq!(
        world
            .notifications
            .0
            .acknowledgements
            .load(Ordering::SeqCst),
        1
    );
}

#[when("the monitor is stopped and restarted twice")]
fn restart_monitor(world: &mut Native) {
    stop_monitor(world);
    stop_monitor(world);
    for _ in 0..2 {
        let mut monitor = LifecycleMonitor::with_source(
            world.power.as_ref().unwrap().clone(),
            world.notifications.clone(),
        )
        .unwrap();
        monitor.stop().unwrap();
        monitor.stop().unwrap();
    }
}

#[then("all three registrations are removed exactly once")]
fn all_stopped(world: &mut Native) {
    assert_eq!(world.notifications.0.starts.load(Ordering::SeqCst), 3);
    assert_eq!(world.notifications.0.stops.load(Ordering::SeqCst), 3);
}

#[when("a new notification registration fails")]
fn registration_failure(world: &mut Native) {
    stop_monitor(world);
    world
        .notifications
        .0
        .fail_registration
        .store(true, Ordering::SeqCst);
    world.lifecycle_failure = LifecycleMonitor::with_source(
        world.power.as_ref().unwrap().clone(),
        world.notifications.clone(),
    )
    .err();
}

#[then("startup returns the native registration fault")]
fn startup_failure(world: &mut Native) {
    assert_eq!(
        world.lifecycle_failure.as_ref().unwrap().fault_code,
        "test_lifecycle_register"
    );
}

#[when("the occurrence clock fails during an impending sleep notification")]
fn lifecycle_clock_failure(world: &mut Native) {
    world.api.0.lock().unwrap().fail_clock = true;
    notify(world, SleepNotification::WillSleep);
}

#[then("an attributed lifecycle timing fault is observed without an interruption")]
fn lifecycle_error(world: &mut Native) {
    let entries = wait_for_entries(world, |entries| {
        entries.iter().any(|e| {
            e.payload::<Error::Native::Activity::Occurred>()
                .unwrap()
                .is_some()
        })
    });
    let fault = entries
        .iter()
        .find_map(|e| e.payload::<Error::Native::Activity::Occurred>().unwrap())
        .unwrap();
    assert_eq!(fault.fault_code, "test_clock");
    assert!(fault.failed_genus.is_some());
    assert!(interruptions(&entries).is_empty());
    assert_eq!(
        world
            .notifications
            .0
            .acknowledgements
            .load(Ordering::SeqCst),
        1
    );
}

#[when("the OS allowance fails for an impending sleep notification")]
fn allow_failure(world: &mut Native) {
    let callback = world
        .notifications
        .0
        .callback
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    callback.receive(SleepNotification::WillSleep, || {
        Err(Fault::new(
            "test_allow",
            "Controlled allowance failure.",
            true,
        ))
    });
}

#[then("the allowance error precedes the typed interruption")]
fn allowance_before_interruption(world: &mut Native) {
    let entries = wait_for_entries(world, |entries| !interruptions(entries).is_empty());
    let error = entries
        .iter()
        .position(|e| {
            e.payload::<Error::Native::Activity::Occurred>()
                .unwrap()
                .is_some_and(|f| f.fault_code == "test_allow" && f.failed_genus.is_some())
        })
        .unwrap();
    let observation = entries
        .iter()
        .position(|e| {
            e.payload::<Power::Activity::System::Interrupted>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(error < observation);
}

#[when("an external observer exceeds the monitor stop deadline")]
fn stop_deadline(world: &mut Native) {
    let (sender, entered) = std::sync::mpsc::sync_channel(1);
    let pause = Arc::new((Mutex::new(false), Condvar::new()));
    let observer_pause = pause.clone();
    world
        .bus
        .as_ref()
        .unwrap()
        .observe_action::<Power::Activity::System::Interrupted>(move |_, _| {
            sender.send(()).unwrap();
            let guard = observer_pause.0.lock().unwrap();
            drop(
                observer_pause
                    .1
                    .wait_timeout_while(guard, Duration::from_secs(2), |released| !*released)
                    .unwrap(),
            );
        });
    notify(world, SleepNotification::DidWake);
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    let started = std::time::Instant::now();
    let stopped = world.monitor.as_ref().unwrap().lock().unwrap().stop();
    world.bounded_stop = stopped.is_err_and(|error| error.fault_code == "lifecycle_stop_timeout")
        && started.elapsed() < Duration::from_millis(750);
    *pause.0.lock().unwrap() = true;
    pause.1.notify_all();
    stop_monitor(world);
}

#[then("stop fails explicitly within its budget and succeeds after observer release")]
fn stopped_with_deadline(world: &mut Native) {
    assert!(world.bounded_stop);
    assert_eq!(world.notifications.0.stops.load(Ordering::SeqCst), 1);
}

#[cfg(target_os = "macos")]
#[when("actual macOS sleep monitoring is started and stopped twice")]
fn real_registration(world: &mut Native) {
    let power = Arc::new(MacPower::new());
    for _ in 0..2 {
        let mut monitor = LifecycleMonitor::start(power.clone()).unwrap();
        monitor.stop().unwrap();
        world.real_monitors_stopped += 1;
    }
}

#[then("both real registrations completed bounded explicit teardown")]
fn real_stopped(world: &mut Native) {
    assert_eq!(world.real_monitors_stopped, 2);
}

#[given(regex = r"^a native owner reporting input age (.+)$")]
fn input_age(world: &mut Native, age: String) {
    world.api.0.lock().unwrap().idle = Some(Ok(age.parse().unwrap()));
    owner(world);
}

#[given("a native owner whose input source fails")]
fn input_fault(world: &mut Native) {
    world.api.0.lock().unwrap().idle = Some(Err(Fault::new(
        "test_idle",
        "Controlled input source fault.",
        true,
    )));
    owner(world);
}

#[given("a native owner whose clock fails")]
fn clock_fault(world: &mut Native) {
    world.api.0.lock().unwrap().fail_clock = true;
    owner(world);
}

#[cfg(target_os = "macos")]
#[given("a native owner using actual macOS APIs")]
fn host_owner(world: &mut Native) {
    world.attach(Arc::new(MacPower::with_api(world.host.clone())));
}

#[given("a native owner whose display acquisition fails")]
fn partial(world: &mut Native) {
    world.api.0.lock().unwrap().fail_display = true;
    owner(world);
}

#[given("a native owner whose display acquisition and first rollback fail")]
fn rollback(world: &mut Native) {
    {
        let mut calls = world.api.0.lock().unwrap();
        calls.fail_display = true;
        calls.fail_release_once = true;
    }
    owner(world);
}

#[given("a native owner with an observer that turns off confirmed resources")]
fn reentrant(world: &mut Native) {
    owner(world);
    let off = request(false);
    world
        .bus
        .as_ref()
        .unwrap()
        .observe_action::<Power::Assertions::Resources::Observed>(move |outcome, events| {
            if outcome.request.enabled && outcome.outcome == Outcome::Applied {
                events.emit::<Power::Assertions::Resources::Apply>(off.clone());
            }
        });
}

#[when("native protection is enabled")]
fn enable(world: &mut Native) {
    let request = request(true);
    if world.api.0.lock().unwrap().corners.is_some() {
        let revision = world.sink.0.lock().unwrap().len() as u64 + 1;
        world
            .bus
            .as_ref()
            .unwrap()
            .emit::<Power::Corners::Guard::Authorize>(CornerAuthorization {
                revision,
                request: Some(request.clone()),
            });
    }
    world.last_request = Some(request.clone());
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Assertions::Resources::Apply>(request);
    if world.corners.hold_poll.load(Ordering::SeqCst) {
        let (entered, condition) = &*world.corners.poll_entered;
        let (_guard, timeout) = condition
            .wait_timeout_while(entered.lock().unwrap(), Duration::from_secs(1), |entered| {
                !*entered
            })
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "controlled source did not enter its poll"
        );
    }
}

#[when("native protection is enabled twice")]
fn twice(world: &mut Native) {
    enable(world);
    enable(world);
}

#[when("native protection is disabled")]
fn disable(world: &mut Native) {
    let request = request(false);
    world.last_request = Some(request.clone());
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Assertions::Resources::Apply>(request);
}

#[when("the next release fails")]
fn fail_release(world: &mut Native) {
    world.api.0.lock().unwrap().fail_release_once = true;
}

#[when("the registry and native owner are dropped")]
fn drop_owner(world: &mut Native) {
    world.bus.take();
    world.native.take();
}

#[when("native input age is sampled")]
fn sample(world: &mut Native) {
    let id = || IdGeneration::next_id::<Power::Session::Protection::Query>(&Empty {}).unwrap();
    let request = IdleRequest {
        session_id: id(),
        generation: 1,
        episode_id: id(),
        sample_id: id(),
    };
    world.idle_request = Some(request.clone());
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Activity::System::Sample>(request);
}

fn idle(world: &Native) -> IdleObserved {
    world
        .sink
        .0
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|e| e.payload::<Power::Activity::System::Observed>().unwrap())
        .expect("an explicit idle observation is required")
}

#[then("1234 milliseconds and the original request clocks are observed")]
fn idle_valid(world: &mut Native) {
    let observed = idle(world);
    assert_eq!(observed.idle_age_ms, Some(1234));
    assert_eq!(&observed.request, world.idle_request.as_ref().unwrap());
    assert_eq!(observed.clock, world.api.clock().unwrap());
    assert!(observed.fault.is_none());
}

#[then("an attributed idle error precedes a null observation")]
fn idle_failed(world: &mut Native) {
    let entries = world.sink.0.lock().unwrap();
    let error = entries
        .iter()
        .position(|e| {
            e.payload::<Error::Native::Activity::Occurred>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let observed = entries
        .iter()
        .position(|e| {
            e.payload::<Power::Activity::System::Observed>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(error < observed);
    let fault = entries[error]
        .payload::<Error::Native::Activity::Occurred>()
        .unwrap()
        .unwrap();
    assert!(fault.failed_genus.is_some());
    drop(entries);
    let observed = idle(world);
    assert_eq!(&observed.request, world.idle_request.as_ref().unwrap());
    assert!(observed.idle_age_ms.is_none());
    assert!(observed.fault.is_some());
}

#[then("a timing error is observed without a fabricated sample")]
fn no_clock(world: &mut Native) {
    let entries = world.sink.0.lock().unwrap();
    assert!(entries.iter().any(|e| {
        e.payload::<Error::Native::Activity::Occurred>()
            .unwrap()
            .is_some()
    }));
    assert!(!entries.iter().any(|e| {
        e.payload::<Power::Activity::System::Observed>()
            .unwrap()
            .is_some()
    }));
}

#[cfg(target_os = "macos")]
fn properties_exist(id: u32) -> bool {
    use core_foundation::{base::CFRelease, dictionary::CFDictionaryRef};
    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCopyProperties(id: u32) -> CFDictionaryRef;
    }
    // Test queries only IDs allocated in this same witness and releases copied CF data.
    let properties = unsafe { IOPMAssertionCopyProperties(id) };
    if properties.is_null() {
        false
    } else {
        unsafe { CFRelease(properties.cast()) };
        true
    }
}

#[cfg(target_os = "macos")]
#[then("both native assertion IDs have live IOKit properties")]
fn live(world: &mut Native) {
    assert_eq!(world.observed().outcome, Outcome::Applied);
    let ids = world.host.0.lock().unwrap();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert!(ids.iter().copied().all(properties_exist));
    println!("Actual IOKit acquisition and live-property query: two distinct IDs.");
}

#[cfg(target_os = "macos")]
#[then("neither native assertion ID has live IOKit properties")]
fn released(world: &mut Native) {
    assert_eq!(world.observed().outcome, Outcome::Applied);
    assert_eq!(world.observed().held_count, 0);
    assert!(
        world
            .host
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|id| !properties_exist(*id))
    );
    println!("Actual IOKit release and absent-property query: both IDs released.");
}

#[cfg(target_os = "macos")]
#[then("a genuine nonnegative input age and current clocks are observed")]
fn genuine(world: &mut Native) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let observed = idle(world);
    assert!(observed.idle_age_ms.is_some());
    assert!(observed.fault.is_none());
    let now = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    assert!(now.abs_diff(observed.clock.epoch_ms) < 2000);
    assert!(observed.clock.monotonic_ms > 0);
    let later = world.host.clock().unwrap();
    assert!(later.monotonic_ms >= observed.clock.monotonic_ms);
    println!("Actual combined-session input sample and paired continuous/UTC clocks observed.");
}

#[cfg(target_os = "macos")]
#[when("two processes resolve the native runtime location")]
fn identity(world: &mut Native) {
    let local = ai2screenwake_native::identity::runtime_directory().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--identity-probe")
        .env("TMPDIR", "/tmp/untrusted-tmpdir")
        .output()
        .unwrap();
    assert!(output.status.success());
    let child = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    world.runtime_paths = Some((local, child));
}

#[then("both paths agree without using the TMPDIR override")]
fn identity_matches(world: &mut Native) {
    let (local, child) = world.runtime_paths.as_ref().unwrap();
    assert_eq!(local, child);
    assert!(local.is_absolute());
    assert!(!local.starts_with("/tmp/untrusted-tmpdir"));
    assert!(
        local
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("ai2screenwake-")
    );
}

#[cfg(target_os = "macos")]
#[when("invalid owner bundle paths are submitted")]
fn invalid_launch(world: &mut Native) {
    use ai2screenwake_native::launch::{BundleLauncher, OwnerLauncher};
    use std::path::Path;
    for path in [
        Path::new("relative/ScreenWake.app"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    ] {
        world.launch_errors.push(
            BundleLauncher::new(path.to_path_buf())
                .launch()
                .unwrap_err(),
        );
    }
}

#[then("each launch is explicitly refused")]
fn launch_failed(world: &mut Native) {
    assert_eq!(world.launch_errors.len(), 2);
    assert!(
        world
            .launch_errors
            .iter()
            .all(|error| error.fault_code == "owner_bundle_invalid")
    );
}

#[then("both distinct resources are held with an applied observation")]
fn applied(world: &mut Native) {
    let outcome = world.observed();
    assert_eq!(outcome.outcome, Outcome::Applied);
    assert_eq!(outcome.held_count, 2);
    assert_eq!(&outcome.request, world.last_request.as_ref().unwrap());
    let calls = world.api.0.lock().unwrap();
    assert_eq!(calls.held.len(), 2);
    assert_ne!(calls.held[0], calls.held[1]);
}

#[then("only two acquisitions occurred")]
fn no_duplicates(world: &mut Native) {
    assert_eq!(world.api.0.lock().unwrap().acquired.len(), 2);
}

#[then(regex = r"^an attributed error precedes a failed observation holding (\d+) resources$")]
fn failed(world: &mut Native, held: u8) {
    let entries = world.sink.0.lock().unwrap();
    let outcome_index = entries
        .iter()
        .rposition(|e| {
            e.payload::<Power::Assertions::Resources::Observed>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let fault = entries[..outcome_index]
        .iter()
        .rev()
        .find_map(|e| e.payload::<Error::Native::Assertions::Occurred>().unwrap())
        .expect("explicit error must precede failed outcome");
    assert!(fault.failed_genus.is_some());
    let outcome = entries[outcome_index]
        .payload::<Power::Assertions::Resources::Observed>()
        .unwrap()
        .unwrap();
    assert_eq!(outcome.outcome, Outcome::Failed);
    assert_eq!(outcome.held_count, held);
    assert_eq!(&outcome.request, world.last_request.as_ref().unwrap());
    assert!(!outcome.fault_codes.is_empty());
    assert_eq!(world.api.0.lock().unwrap().held.len(), usize::from(held));
}

#[then("no resources remain and the retained release was retried")]
fn retried(world: &mut Native) {
    clean(world);
    let calls = world.api.0.lock().unwrap();
    assert!(
        calls
            .releases
            .iter()
            .any(|id| { calls.releases.iter().filter(|other| *other == id).count() == 2 })
    );
}

#[then("no resources remain and cleanup was observed")]
fn clean(world: &mut Native) {
    assert_eq!(world.observed().outcome, Outcome::Applied);
    assert_eq!(world.observed().held_count, 0);
    assert!(world.api.0.lock().unwrap().held.is_empty());
}

#[then("the OS boundary has no retained resources")]
fn dropped(world: &mut Native) {
    assert!(world.api.0.lock().unwrap().held.is_empty());
}

#[when("native local log names are resolved around a local midnight in an owned fixture home")]
fn local_log_names(world: &mut Native) {
    use ai2screenwake_native::identity::readable_log_path_at;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target");
    let home = root
        .canonicalize()
        .unwrap()
        .join(format!("native-log-clock-{}", std::process::id()));
    assert!(
        !home.exists(),
        "clock-only fixture must not reuse an existing home"
    );
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    local.tm_year = 2026 - 1900;
    local.tm_mon = 10;
    local.tm_mday = 17;
    local.tm_hour = 23;
    local.tm_min = 59;
    local.tm_sec = 59;
    local.tm_isdst = -1;
    let instant = unsafe { libc::mktime(&mut local) };
    assert!(instant > 0);
    for (seconds, name) in [
        (instant, "20261117tu235959_ai2sw.log"),
        (instant + 1, "20261118we000000_ai2sw.log"),
        (instant - 12 * 3600, "20261117tu115959_ai2sw.log"),
    ] {
        world.log_names.push((
            readable_log_path_at(&home, seconds).unwrap(),
            home.join("Library/Logs/AI2ScreenWake").join(name),
        ));
    }
    for (offset, day) in [
        (0, "tu"),
        (1, "we"),
        (2, "th"),
        (3, "fr"),
        (4, "sa"),
        (5, "su"),
        (6, "mo"),
    ] {
        let path = readable_log_path_at(&home, instant + offset * 86400).unwrap();
        assert_eq!(&path.file_name().unwrap().to_str().unwrap()[8..10], day);
    }
    assert!(
        !home.join("Library").exists(),
        "native resolution must not create files"
    );
    assert!(readable_log_path_at(std::path::Path::new("relative-home"), instant).is_err());
    world.log_home = Some(home);
}

#[then(
    "filenames carry the exact local date lowercase weekday and 24-hour time without creating files"
)]
fn local_log_format(world: &mut Native) {
    assert_eq!(world.log_names.len(), 3);
    for (actual, expected) in &world.log_names {
        assert_eq!(
            actual, expected,
            "native local instant must determine exact filename"
        );
    }
    assert!(
        !world.log_home.as_ref().unwrap().exists(),
        "formatting a native destination must create no file or directory"
    );
}

fn main() {
    #[cfg(target_os = "macos")]
    if std::env::args().any(|arg| arg == "--corner-pass-through-probe") {
        match ai2screenwake_native::corners::system::pass_through_probe() {
            Ok(()) => println!(
                "CORNER_PASS_THROUGH_PROBE_OK target_app_permission=UNVERIFIED filtering=NEVER_ENABLED"
            ),
            Err(error) => {
                eprintln!(
                    "CORNER_PASS_THROUGH_PROBE_UNAVAILABLE code={} target_app_permission=UNVERIFIED filtering=NEVER_ENABLED",
                    error.fault_code
                );
                std::process::exit(2);
            }
        }
        return;
    }
    #[cfg(target_os = "macos")]
    if std::env::args().any(|arg| arg == "--identity-probe") {
        match ai2screenwake_native::identity::runtime_directory() {
            Ok(path) => println!("{}", path.display()),
            Err(error) => {
                eprintln!("{}: {}", error.fault_code, error.message);
                std::process::exit(1);
            }
        }
        return;
    }
    let features = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/features");
    futures::executor::block_on(
        Native::cucumber()
            .max_concurrent_scenarios(3)
            .run_and_exit(features),
    );
}
// end native.rs
