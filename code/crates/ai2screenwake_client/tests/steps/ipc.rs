// ipc.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Real local socket and framing requirements, isolated from app/OS power behavior.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, ControlResult, CorrelationId, Emitter, Empty,
        Fault, IdleObserved, IdleRequest, KvEnvelope, ObservationSink, Outcome, OwnerLocation,
        Power,
    },
    ipc::{
        MAX_FRAME_BYTES, OwnerConnection, OwnerServer, encode_frame, for_current_user,
        validate_peer,
    },
};
use ai2screenwake_native::{NativePower, launch::OwnerLauncher};
use cucumber::{World, given, then, when};
use std::{
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default, World)]
struct Ipc {
    failure: Option<Fault>,
    root: Option<tempfile::TempDir>,
    server: Option<OwnerServer>,
    native: Arc<Native>,
    outcomes: Vec<ControlResult>,
    lock_refused: bool,
    inode: u64,
    launcher: Option<Arc<Launch>>,
}

impl std::fmt::Debug for Ipc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ipc")
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl Drop for Ipc {
    fn drop(&mut self) {
        self.server.take();
        if let Some(launcher) = self.launcher.take() {
            launcher.server.lock().unwrap().take();
        }
    }
}

#[derive(Default)]
struct Native {
    applies: AtomicUsize,
}

impl NativePower for Native {
    fn apply(&self, request: Apply, events: Emitter) {
        self.applies.fetch_add(1, Ordering::SeqCst);
        events.emit::<Power::Assertions::Observed>(AssertionsObserved {
            held_count: if request.enabled { 2 } else { 0 },
            request,
            outcome: Outcome::Applied,
            fault_codes: Vec::new(),
            clock: ClockSample {
                epoch_ms: 100_000,
                monotonic_ms: 1000,
            },
        });
    }
    fn sample(&self, request: IdleRequest, events: Emitter) {
        events.emit::<Power::Activity::Observed>(IdleObserved {
            request,
            clock: ClockSample {
                epoch_ms: 110_000,
                monotonic_ms: 11_000,
            },
            idle_age_ms: Some(5000),
            fault: None,
        });
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<KvEnvelope>>);

impl ObservationSink for Sink {
    fn observed(&self, entry: KvEnvelope) {
        self.0.lock().unwrap().push(entry);
    }
}

impl Sink {
    fn result(&self, id: &CorrelationId) -> ControlResult {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = self.0.lock().unwrap().iter().find_map(|entry| {
                entry
                    .payload::<Power::Session::Settled>()
                    .unwrap()
                    .filter(|settled| &settled.correlation == id)
                    .map(|settled| settled.result)
            }) {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "real owner request did not settle"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

struct NoLaunch;
impl OwnerLauncher for NoLaunch {
    fn launch(&self) -> Result<(), Fault> {
        Err(Fault::new(
            "unexpected_launch",
            "An existing owner must not be launched again.",
            false,
        ))
    }
}

struct Launch {
    location: OwnerLocation,
    native: Arc<Native>,
    server: Mutex<Option<OwnerServer>>,
    count: AtomicUsize,
}
impl OwnerLauncher for Launch {
    fn launch(&self) -> Result<(), Fault> {
        self.count.fetch_add(1, Ordering::SeqCst);
        let server =
            OwnerServer::bind(self.location.clone(), self.native.clone()).map_err(|_| {
                Fault::new(
                    "test_owner_start",
                    "The real test owner did not start.",
                    false,
                )
            })?;
        *self.server.lock().unwrap() = Some(server);
        Ok(())
    }
}

impl Ipc {
    fn location(&self) -> OwnerLocation {
        for_current_user(self.root.as_ref().unwrap().path().join("owner"))
    }
}

#[when("an envelope exceeds the 64 KiB frame limit")]
fn oversized(world: &mut Ipc) {
    let envelope = serde_json::from_value(serde_json::json!({
        "_genus": "untrusted",
        "padding": "x".repeat(MAX_FRAME_BYTES),
    }))
    .unwrap();
    world.failure = encode_frame(&envelope).err();
}

#[then("framing returns an explicit oversized-frame error")]
fn rejected(world: &mut Ipc) {
    assert_eq!(
        world.failure.as_ref().map(|f| f.fault_code.as_str()),
        Some("frame_too_large")
    );
}

#[given("a private location without an owner")]
fn private_location(world: &mut Ipc) {
    world.root = Some(tempfile::Builder::new().prefix("sw-").tempdir().unwrap());
}

#[given("a real private owner socket")]
fn owner(world: &mut Ipc) {
    private_location(world);
    world.server = Some(OwnerServer::bind(world.location(), world.native.clone()).unwrap());
}

#[when("two socket clients interleave On query and native Off")]
fn interleave(world: &mut Ipc) {
    let first = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let second = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let first_sink = Arc::new(Sink::default());
    let second_sink = Arc::new(Sink::default());
    first.observe(first_sink.clone());
    second.observe(second_sink.clone());
    let on = first.prepare::<Power::Session::TurnOn>().unwrap();
    let on_id = on.correlation();
    first.emit(on);
    world.outcomes.push(first_sink.result(&on_id));
    let query = second.prepare::<Power::Session::Query>().unwrap();
    let query_id = query.correlation();
    second.emit(query);
    world.outcomes.push(second_sink.result(&query_id));
    world
        .server
        .as_ref()
        .unwrap()
        .actions()
        .emit::<Power::Session::TurnOff>(Empty {});
    let query = first.prepare::<Power::Session::Query>().unwrap();
    let query_id = query.correlation();
    first.emit(query);
    world.outcomes.push(first_sink.result(&query_id));
}

#[then("their correlated results agree with the shared native owner")]
fn agreed(world: &mut Ipc) {
    assert_eq!(world.outcomes.len(), 3);
    for (outcome, enabled) in world.outcomes.iter().zip([true, true, false]) {
        assert!(
            outcome.error.is_none(),
            "real IPC failed: {:?}",
            outcome.error
        );
        assert_eq!(outcome.status.as_ref().unwrap().is_on, Some(enabled));
    }
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 2);
}

#[when("a real socket pair is checked against its actual and a different UID")]
fn peer_uid(world: &mut Ipc) {
    let (left, _right) = UnixStream::pair().unwrap();
    let uid = for_current_user(std::path::PathBuf::new()).uid;
    validate_peer(&left, uid).unwrap();
    world.failure = validate_peer(&left, uid.checked_add(1).unwrap()).err();
}

#[then("only the actual peer UID is accepted")]
fn denied_uid(world: &mut Ipc) {
    assert_eq!(world.failure.as_ref().unwrap().fault_code, "foreign_peer");
}

#[when("another owner attempts the same login-session endpoint")]
fn duplicate_owner(world: &mut Ipc) {
    let socket = world.location().directory.join("owner.sock");
    world.inode = socket.metadata().unwrap().ino();
    world.lock_refused = OwnerServer::bind(world.location(), world.native.clone()).is_err();
}

#[then("the live socket identity and private modes are preserved")]
fn private_modes(world: &mut Ipc) {
    assert!(world.lock_refused);
    let root = world.location().directory;
    assert_eq!(
        root.metadata().unwrap().permissions().mode() & 0o7777,
        0o700
    );
    let socket = root.join("owner.sock").metadata().unwrap();
    assert_eq!(socket.ino(), world.inode);
    assert_eq!(socket.permissions().mode() & 0o7777, 0o600);
}

#[when("a query launches and connects to the real test owner")]
fn launch(world: &mut Ipc) {
    let launcher = Arc::new(Launch {
        location: world.location(),
        native: world.native.clone(),
        server: Mutex::new(None),
        count: AtomicUsize::new(0),
    });
    let connection = OwnerConnection::connect(world.location(), launcher.clone()).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let query = connection.prepare::<Power::Session::Query>().unwrap();
    let id = query.correlation();
    connection.emit(query);
    world.outcomes.push(sink.result(&id));
    world.launcher = Some(launcher);
}

#[then("the first observed owner status is Off and launch happened once")]
fn launched_off(world: &mut Ipc) {
    assert!(
        world.outcomes[0].error.is_none(),
        "{:?}",
        world.outcomes[0].error
    );
    assert_eq!(
        world.outcomes[0].status.as_ref().unwrap().is_on,
        Some(false)
    );
    assert_eq!(
        world
            .launcher
            .as_ref()
            .unwrap()
            .count
            .load(Ordering::SeqCst),
        1
    );
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 0);
}

fn main() {
    futures::executor::block_on(Ipc::cucumber().fail_on_skipped().run_and_exit(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/features/ipc.feature"
    )));
}
// end ipc.rs
