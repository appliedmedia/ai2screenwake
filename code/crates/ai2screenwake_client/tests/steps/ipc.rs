// ipc.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Real local socket and framing requirements, isolated from app/OS power behavior.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, ControlResult, CorrelationId, Emitter, Empty,
        Error, Fault, IdleObserved, IdleRequest, KvEnvelope, ObservationSink, Outcome,
        OwnerLocation, Power, RegistryBuilder,
    },
    ipc::{
        MAX_FRAME_BYTES, OwnerConnection, OwnerServer, encode_frame, for_current_user,
        validate_peer,
    },
};
use ai2screenwake_native::{NativePower, launch::OwnerLauncher};
use cucumber::{World, given, then, when};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt, symlink},
        net::{UnixListener, UnixStream},
    },
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    reply: Option<KvEnvelope>,
    expected: Option<CorrelationId>,
    preserved: bool,
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
        self.native.hold.store(false, Ordering::SeqCst);
        self.native.fail_release.store(false, Ordering::SeqCst);
        self.native.waiting.lock().unwrap().clear();
        self.server.take();
        if let Some(launcher) = self.launcher.take() {
            launcher.server.lock().unwrap().take();
        }
    }
}

#[derive(Default)]
struct Native {
    applies: AtomicUsize,
    hold: AtomicBool,
    waiting: Mutex<Vec<(Apply, Emitter)>>,
    fail_release: AtomicBool,
    held: AtomicUsize,
}

impl Native {
    fn finish(&self, request: Apply, events: Emitter) {
        let failed = !request.enabled && self.fail_release.load(Ordering::SeqCst);
        if !failed {
            self.held
                .store(if request.enabled { 2 } else { 0 }, Ordering::SeqCst);
        }
        events.emit::<Power::Assertions::Observed>(AssertionsObserved {
            held_count: u8::try_from(self.held.load(Ordering::SeqCst)).unwrap(),
            request,
            outcome: if failed {
                Outcome::Failed
            } else {
                Outcome::Applied
            },
            fault_codes: if failed {
                vec!["release_failed".into()]
            } else {
                Vec::new()
            },
            clock: ClockSample {
                epoch_ms: 100_000,
                monotonic_ms: 1000,
            },
        });
    }

    fn complete(&self) {
        let (request, events) = self.waiting.lock().unwrap().pop().unwrap();
        self.finish(request, events);
    }
}

impl NativePower for Native {
    fn apply(&self, request: Apply, events: Emitter) {
        self.applies.fetch_add(1, Ordering::SeqCst);
        if self.hold.load(Ordering::SeqCst) {
            self.waiting.lock().unwrap().push((request, events));
        } else {
            self.finish(request, events);
        }
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

    fn raw(&self) -> UnixStream {
        let stream = UnixStream::connect(self.location().directory.join("owner.sock")).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
    }
}

fn read_reply(stream: &mut UnixStream) -> KvEnvelope {
    let mut prefix = [0_u8; 4];
    stream.read_exact(&mut prefix).unwrap();
    let size = u32::from_be_bytes(prefix) as usize;
    assert!(size > 0 && size <= MAX_FRAME_BYTES);
    let mut body = vec![0_u8; size];
    stream.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn wait_for_native(native: &Native) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while native.waiting.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(native.waiting.lock().unwrap().len(), 1);
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

#[when("malformed JSON is written to a real peer")]
fn malformed(world: &mut Ipc) {
    let mut stream = world.raw();
    stream.write_all(&5_u32.to_be_bytes()).unwrap();
    stream.write_all(b"nope!").unwrap();
    world.reply = Some(read_reply(&mut stream));
}

#[then("the peer receives an explicit invalid-frame error")]
fn malformed_error(world: &mut Ipc) {
    let error = world
        .reply
        .as_ref()
        .unwrap()
        .payload::<Error::Ipc::Protocol::Occurred>()
        .unwrap()
        .unwrap();
    assert_eq!(error.fault_code, "invalid_frame");
    assert!(!error.message.contains("nope!"));
}

#[when("an oversized length prefix is written without a body")]
fn oversized_input(world: &mut Ipc) {
    let mut stream = world.raw();
    stream
        .write_all(&u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes())
        .unwrap();
    world.reply = Some(read_reply(&mut stream));
}

#[then("the peer receives an explicit oversized-frame error")]
fn oversized_input_error(world: &mut Ipc) {
    assert_eq!(
        world
            .reply
            .as_ref()
            .unwrap()
            .payload::<Error::Ipc::Protocol::Occurred>()
            .unwrap()
            .unwrap()
            .fault_code,
        "frame_too_large"
    );
}

#[when("a native outcome is replayed through a real peer")]
fn forged_native(world: &mut Ipc) {
    let sink = Arc::new(Sink::default());
    let bus = world.server.as_ref().unwrap().actions();
    bus.observe(sink.clone());
    bus.emit::<Power::Session::TurnOn>(Empty {});
    let observed = sink
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|entry| {
            entry
                .payload::<Power::Assertions::Observed>()
                .unwrap()
                .is_some()
        })
        .unwrap()
        .clone();
    let mut stream = world.raw();
    stream.write_all(&encode_frame(&observed).unwrap()).unwrap();
    world.reply = Some(read_reply(&mut stream));
}

#[then("it is rejected without another native operation")]
fn forged_error(world: &mut Ipc) {
    assert_eq!(
        world
            .reply
            .as_ref()
            .unwrap()
            .payload::<Error::Ipc::Protocol::Occurred>()
            .unwrap()
            .unwrap()
            .fault_code,
        "inadmissible_action"
    );
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 1);
}

#[when("a caller disconnects before its admitted On operation completes")]
fn disconnect(world: &mut Ipc) {
    world.native.hold.store(true, Ordering::SeqCst);
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let request = connection.prepare::<Power::Session::TurnOn>().unwrap();
    let id = request.correlation();
    connection.emit(request);
    wait_for_native(&world.native);
    drop(connection);
    let closed = sink.result(&id);
    assert!(closed.error.is_some());
    world.native.complete();
}

#[then("a new client observes the completed On intent")]
fn after_disconnect(world: &mut Ipc) {
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let request = connection.prepare::<Power::Session::Query>().unwrap();
    let id = request.correlation();
    connection.emit(request);
    let result = sink.result(&id);
    assert!(result.error.is_none());
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(true));
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 1);
}

#[when("one client emits 33 setters before native completion")]
fn capacity(world: &mut Ipc) {
    world.native.hold.store(true, Ordering::SeqCst);
    let owner_sink = Arc::new(Sink::default());
    world
        .server
        .as_ref()
        .unwrap()
        .actions()
        .observe(owner_sink.clone());
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let mut ids = Vec::new();
    for _ in 0..33 {
        let request = connection.prepare::<Power::Session::TurnOn>().unwrap();
        ids.push(request.correlation());
        connection.emit(request);
    }
    let last = sink.result(ids.last().unwrap());
    assert_eq!(last.error.as_ref().unwrap().fault_code, "peer_capacity");
    wait_for_native(&world.native);
    let deadline = Instant::now() + Duration::from_secs(1);
    let admitted = || {
        owner_sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.payload::<Power::Session::TurnOn>().unwrap().is_some())
            .count()
    };
    while admitted() < 32 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(admitted(), 32);
    assert_eq!(
        sink.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|entry| entry.payload::<Power::Session::Settled>().unwrap())
            .count(),
        1
    );
    world.native.complete();
    for id in ids {
        world.outcomes.push(sink.result(&id));
    }
}

#[then("exactly 32 await convergence and the extra request has an explicit error")]
fn capacity_result(world: &mut Ipc) {
    assert_eq!(world.outcomes.len(), 33);
    assert_eq!(
        world
            .outcomes
            .iter()
            .filter(|result| result.error.is_none())
            .count(),
        32
    );
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 1);
    assert!(
        world.outcomes[..32]
            .iter()
            .all(|result| result.status.as_ref().unwrap().is_on == Some(true))
    );
}

#[when("an owned inactive socket is present at startup")]
fn stale_socket(world: &mut Ipc) {
    let root = world.location().directory;
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let socket = root.join("owner.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    drop(listener);
    world.server = Some(OwnerServer::bind(world.location(), world.native.clone()).unwrap());
}

#[then("a real client can query the replacement Off owner")]
fn stale_replaced(world: &mut Ipc) {
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let request = connection.prepare::<Power::Session::Query>().unwrap();
    let id = request.correlation();
    connection.emit(request);
    let result = sink.result(&id);
    assert!(result.error.is_none());
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(false));
}

#[when("a socket symlink points at an unrelated test file")]
fn socket_symlink(world: &mut Ipc) {
    let root = world.location().directory;
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let keep = root.join("keep.txt");
    fs::write(&keep, b"preserve this exact test content").unwrap();
    let socket = root.join("owner.sock");
    symlink(&keep, &socket).unwrap();
    world.lock_refused = OwnerServer::bind(world.location(), world.native.clone()).is_err();
    world.preserved = fs::symlink_metadata(&socket)
        .unwrap()
        .file_type()
        .is_symlink()
        && fs::read(&keep).unwrap() == b"preserve this exact test content";
}

#[then("owner startup rejects it and preserves the exact file and link")]
fn symlink_preserved(world: &mut Ipc) {
    assert!(world.lock_refused && world.preserved);
}

#[when("a valid query is written in separate header and body fragments")]
fn fragmented(world: &mut Ipc) {
    let mut builder = RegistryBuilder::default();
    builder.on::<Power::Session::Query>(|_, _| Ok(())).unwrap();
    let bus = builder.build().unwrap();
    let sink = Arc::new(Sink::default());
    bus.observe(sink.clone());
    bus.emit::<Power::Session::Query>(Empty {});
    let entry = sink.0.lock().unwrap().first().unwrap().clone();
    world.expected = Some(entry.correlation().unwrap());
    let frame = encode_frame(&entry).unwrap();
    let mut stream = world.raw();
    for chunk in frame[..4].chunks(1).chain(frame[4..].chunks(7)) {
        stream.write_all(chunk).unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    world.reply = Some(read_reply(&mut stream));
}

#[then("the peer receives its matching Off settlement")]
fn fragmented_result(world: &mut Ipc) {
    let settled = world
        .reply
        .as_ref()
        .unwrap()
        .payload::<Power::Session::Settled>()
        .unwrap()
        .unwrap();
    assert_eq!(Some(settled.correlation), world.expected);
    assert!(settled.result.error.is_none());
    assert_eq!(settled.result.status.unwrap().is_on, Some(false));
}

#[when("the declared runtime directory has public permissions")]
fn unsafe_directory(world: &mut Ipc) {
    let root = world.location().directory;
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let request = connection.prepare::<Power::Session::Query>().unwrap();
    let id = request.correlation();
    connection.emit(request);
    world.outcomes.push(sink.result(&id));
    assert_eq!(
        root.metadata().unwrap().permissions().mode() & 0o7777,
        0o755
    );
}

#[then("the query receives an explicit unsafe-runtime error")]
fn unsafe_error(world: &mut Ipc) {
    assert_eq!(
        world.outcomes[0].error.as_ref().unwrap().fault_code,
        "unsafe_runtime"
    );
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 0);
}

#[when("Quit confirms Off and late MCP On and native Toggle are attempted")]
fn late_enable(world: &mut Ipc) {
    let bus = world.server.as_ref().unwrap().actions();
    let native_sink = Arc::new(Sink::default());
    bus.observe(native_sink.clone());
    let quit = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
    let id = quit.correlation();
    bus.emit_prepared(quit);
    assert_eq!(
        native_sink.result(&id).status.as_ref().unwrap().is_on,
        Some(false)
    );
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let on = connection.prepare::<Power::Session::TurnOn>().unwrap();
    let id = on.correlation();
    connection.emit(on);
    world.outcomes.push(sink.result(&id));
    let toggle = bus.prepare::<Power::Session::Toggle>(Empty {}).unwrap();
    let id = toggle.correlation();
    bus.emit_prepared(toggle);
    world.outcomes.push(native_sink.result(&id));
}

#[then("both late enables have explicit closing errors without acquiring resources")]
fn late_enable_rejected(world: &mut Ipc) {
    assert_eq!(world.outcomes.len(), 2);
    for outcome in &world.outcomes {
        assert_eq!(
            outcome
                .error
                .as_ref()
                .map(|fault| fault.fault_code.as_str()),
            Some("owner_closing")
        );
    }
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 0);
    assert_eq!(world.native.held.load(Ordering::SeqCst), 0);
}

#[when("native cleanup fails and its transport owner is disposed")]
fn failed_cleanup(world: &mut Ipc) {
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let on = connection.prepare::<Power::Session::TurnOn>().unwrap();
    let id = on.correlation();
    connection.emit(on);
    assert_eq!(sink.result(&id).status.as_ref().unwrap().is_on, Some(true));
    world.native.fail_release.store(true, Ordering::SeqCst);
    {
        let bus = world.server.as_ref().unwrap().actions();
        let native_sink = Arc::new(Sink::default());
        bus.observe(native_sink.clone());
        let quit = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
        let id = quit.correlation();
        bus.emit_prepared(quit);
        assert!(native_sink.result(&id).error.is_some());
    }
    assert_eq!(world.native.held.load(Ordering::SeqCst), 2);
    drop(connection);
    drop(world.server.take());
    let other = OwnerServer::bind(world.location(), world.native.clone());
    world.lock_refused = other.is_err();
    if let Ok(other) = other {
        drop(other);
        return;
    }
    world.native.fail_release.store(false, Ordering::SeqCst);
    let cleanup = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    cleanup.observe(sink.clone());
    let off = cleanup.prepare::<Power::Session::TurnOff>().unwrap();
    let id = off.correlation();
    cleanup.emit(off);
    world.outcomes.push(sink.result(&id));
}

#[then("a second owner is refused until the retained owner confirms Off cleanup")]
fn cleanup_owned(world: &mut Ipc) {
    assert!(
        world.lock_refused,
        "transport disposal released a lease while native IDs remained held"
    );
    let result = world.outcomes.last().unwrap();
    assert!(
        result.error.is_none(),
        "retained owner did not serve cleanup: {:?}",
        result.error
    );
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(false));
    assert_eq!(world.native.held.load(Ordering::SeqCst), 0);
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match OwnerServer::bind(world.location(), world.native.clone()) {
            Ok(server) => {
                world.server = Some(server);
                break;
            }
            Err(error)
                if error.0.starts_with("owner_already_running") && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => {
                panic!("confirmed-clean owner disposal did not release ownership: {error}")
            }
        }
    }
}

fn main() {
    futures::executor::block_on(Ipc::cucumber().fail_on_skipped().run_and_exit(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/features/ipc.feature"
    )));
}
// end ipc.rs
