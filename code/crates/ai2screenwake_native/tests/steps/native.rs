// native.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Native ownership requirements observed only through the typed Actions boundary.

use ai2screenwake_backend::contract::{
    Apply, AssertionsObserved, ClockSample, Empty, Error, Fault, IdGeneration, IdleObserved,
    IdleRequest, KvEnvelope, ObservationSink, Outcome, Power, Registry, RegistryBuilder,
};
use ai2screenwake_native::{
    NativePower, contracts,
    power::{AssertionKind, MacPower, PowerApi},
};
use cucumber::{World, given, then, when};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Calls {
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
struct Sink(Mutex<Vec<KvEnvelope>>);

impl ObservationSink for Sink {
    fn observed(&self, envelope: KvEnvelope) {
        self.0.lock().unwrap().push(envelope);
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
}

impl std::fmt::Debug for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Native").finish_non_exhaustive()
    }
}

fn request(enabled: bool) -> Apply {
    Apply {
        session_id: IdGeneration::next_id::<Power::Session::Query>(&Empty {}).unwrap(),
        generation: 1,
        operation_id: IdGeneration::next_id::<Power::Session::Query>(&Empty {}).unwrap(),
        enabled,
    }
}

impl Native {
    fn attach(&mut self, native: Arc<dyn NativePower>) {
        let mut builder = RegistryBuilder::default();
        contracts::register(&mut builder, native.clone()).unwrap();
        let bus = builder.build().unwrap();
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
            .find_map(|e| e.payload::<Power::Assertions::Observed>().unwrap())
            .expect("native outcome must be observed")
    }
}

#[given("a native owner with controlled OS calls")]
fn owner(world: &mut Native) {
    world.attach(Arc::new(MacPower::with_api(world.api.clone())));
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
        .observe_action::<Power::Assertions::Observed>(move |outcome, events| {
            if outcome.request.enabled && outcome.outcome == Outcome::Applied {
                events.emit::<Power::Assertions::Apply>(off.clone());
            }
        });
}

#[when("native protection is enabled")]
fn enable(world: &mut Native) {
    let request = request(true);
    world.last_request = Some(request.clone());
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Assertions::Apply>(request);
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
        .emit::<Power::Assertions::Apply>(request);
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
    let id = || IdGeneration::next_id::<Power::Session::Query>(&Empty {}).unwrap();
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
        .emit::<Power::Activity::Sample>(request);
}

fn idle(world: &Native) -> IdleObserved {
    world
        .sink
        .0
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|e| e.payload::<Power::Activity::Observed>().unwrap())
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
        .position(|e| e.payload::<Power::Activity::Observed>().unwrap().is_some())
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
    assert!(
        !entries
            .iter()
            .any(|e| e.payload::<Power::Activity::Observed>().unwrap().is_some())
    );
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
            e.payload::<Power::Assertions::Observed>()
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
        .payload::<Power::Assertions::Observed>()
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

fn main() {
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
    futures::executor::block_on(
        Native::cucumber()
            .max_concurrent_scenarios(3)
            .run_and_exit("code/crates/ai2screenwake_native/tests/features"),
    );
}
// end native.rs
