// ipc.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Real local socket and framing requirements, isolated from app/OS power behavior.

use ai2screenwake_client::contract;
use ai2screenwake_client::{
    contract::{
        Abandoned, Apply, AssertionsObserved, CancellationObserved, ClockSample, ControlResult,
        CorrelationId, Emitter, Empty, Error, Fault, IdGeneration, IdleObserved, IdleRequest,
        KvEnvelope, Log, LogSnapshot, ObservationSink, Outcome, OwnerLocation, Power,
        RegistryBuilder, SuppressionKind, SuppressionRequest,
    },
    ipc::{
        MAX_FRAME_BYTES, OwnerConnection, OwnerServer, encode_frame, for_current_user,
        validate_peer,
    },
    runtime::RuntimeOptions,
};
use ai2screenwake_native::{NativePower, launch::OwnerLauncher};
use cucumber::{World, given, then, when};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt, symlink},
        net::{UnixListener, UnixStream},
    },
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[path = "../../src/action_log.rs"]
mod journal_codec;

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
    history_visible: bool,
    restart_off: bool,
    new_on_empty: bool,
    log_view: Option<serde_json::Value>,
    startup_error: Option<String>,
    rotated_bounded: bool,
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
struct Sink(Mutex<Vec<KvEnvelope>>, Condvar);

impl ObservationSink for Sink {
    fn observed(&self, entry: KvEnvelope) {
        self.0.lock().unwrap().push(entry);
        self.1.notify_all();
    }
}

impl Sink {
    fn log(&self) -> LogSnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(view) = self
                .0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find_map(|entry| entry.payload::<Log::Session::Snapshot>().unwrap())
            {
                return view;
            }
            assert!(
                Instant::now() < deadline,
                "the native log query did not produce an observed view"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn log_for(&self, id: &CorrelationId) -> LogSnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let view = self.0.lock().unwrap().iter().rev().find_map(|entry| {
                (entry.correlation().unwrap() == *id)
                    .then(|| entry.payload::<Log::Session::Snapshot>().unwrap())
                    .flatten()
            });
            if let Some(view) = view {
                return view;
            }
            assert!(
                Instant::now() < deadline,
                "the matching native log view did not arrive"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[track_caller]
    fn result(&self, id: &CorrelationId) -> ControlResult {
        let timeout = RuntimeOptions::default().operation_timeout;
        let caller = std::panic::Location::caller();
        let result = |entries: &[KvEnvelope]| {
            entries.iter().find_map(|entry| {
                entry
                    .payload::<Power::Session::Settled>()
                    .unwrap()
                    .filter(|settled| &settled.correlation == id)
                    .map(|settled| settled.result)
            })
        };
        let (entries, _) = self
            .1
            .wait_timeout_while(self.0.lock().unwrap(), timeout, |entries| {
                result(entries).is_none()
            })
            .unwrap();
        result(&entries).unwrap_or_else(|| {
            panic!(
                "real owner request {id:?} did not settle within the {timeout:?} owner operation \
                 contract at {caller}; observed {} actions",
                entries.len()
            )
        })
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
    let actions = world.server.as_ref().unwrap().actions();
    let native_sink = Arc::new(Sink::default());
    actions.observe(native_sink.clone());
    let off = actions
        .prepare::<Power::Session::TurnOff>(Empty {})
        .unwrap();
    let off_id = off.correlation();
    actions.emit_prepared(off);
    assert_eq!(
        native_sink.result(&off_id).status.as_ref().unwrap().is_on,
        Some(false)
    );
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
    let on = bus.prepare::<Power::Session::TurnOn>(Empty {}).unwrap();
    let id = on.correlation();
    bus.emit_prepared(on);
    assert_eq!(sink.result(&id).status.as_ref().unwrap().is_on, Some(true));
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

#[when("verified On history is closed and the same owner location restarts")]
fn restart_history(world: &mut Ipc) {
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let on = connection.prepare::<Power::Session::TurnOn>().unwrap();
    let id = on.correlation();
    connection.emit(on);
    assert_eq!(sink.result(&id).status.as_ref().unwrap().is_on, Some(true));
    {
        let bus = world.server.as_ref().unwrap().actions();
        let native_sink = Arc::new(Sink::default());
        bus.observe(native_sink.clone());
        let query = connection.prepare::<Power::Session::Query>().unwrap();
        let id = query.correlation();
        connection.emit(query);
        assert!(sink.result(&id).error.is_none());
        let guard = native_sink
            .0
            .lock()
            .unwrap()
            .iter()
            .find_map(|entry| entry.payload::<Power::Activity::Sample>().unwrap())
            .unwrap();
        let request_id = IdGeneration::next_id::<Power::Session::Query>(&Empty {}).unwrap();
        bus.emit::<Power::Suppression::Requested>(SuppressionRequest {
            request: guard.clone(),
            native_request_id: request_id.clone(),
            sequence: 1,
            kind: SuppressionKind::IdleSleepRequest,
            clock: ClockSample {
                epoch_ms: 107999,
                monotonic_ms: 7999,
            },
            idle_age_ms: 1999,
        });
        bus.emit::<Power::Suppression::Recorded>(CancellationObserved {
            request: guard,
            native_request_id: request_id,
            cancellation_id: IdGeneration::next_id::<Power::Session::Query>(&Empty {}).unwrap(),
            clock: ClockSample {
                epoch_ms: 108000,
                monotonic_ms: 8000,
            },
            idle_age_ms: 2000,
            cancelled: true,
        });
        let barrier = connection.prepare::<Power::Session::Query>().unwrap();
        let id = barrier.correlation();
        connection.emit(barrier);
        let prior = sink.result(&id);
        assert_eq!(
            prior
                .status
                .as_ref()
                .unwrap()
                .protected_idle_suppressed_events
                .len(),
            1
        );
        let quit = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
        let id = quit.correlation();
        bus.emit_prepared(quit);
        assert_eq!(
            native_sink.result(&id).status.as_ref().unwrap().is_on,
            Some(false)
        );
    }
    drop(connection);
    drop(world.server.take());
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
            Err(error) => panic!("confirmed-clean restart failed: {error}"),
        }
    }
    let native_count = world.native.applies.load(Ordering::SeqCst);
    let connection = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let query = connection.prepare::<Power::Session::Query>().unwrap();
    let id = query.correlation();
    connection.emit(query);
    world.restart_off = sink.result(&id).status.as_ref().unwrap().is_on == Some(false)
        && world.native.applies.load(Ordering::SeqCst) == native_count;
    let bus = world.server.as_ref().unwrap().actions();
    let history = Arc::new(Sink::default());
    bus.observe(history.clone());
    bus.emit::<Log::Session::Query>(Empty {});
    let log = history.log();
    world.history_visible = log
        .entries
        .iter()
        .any(|entry| entry.display.starts_with("Power:Session:TurnOn:"))
        && log
            .entries
            .iter()
            .any(|entry| entry.display.starts_with("Power:Suppression:Recorded:"));
    let on = connection.prepare::<Power::Session::TurnOn>().unwrap();
    let id = on.correlation();
    connection.emit(on);
    let result = sink.result(&id);
    world.new_on_empty = result.error.is_none()
        && result
            .status
            .as_ref()
            .unwrap()
            .protected_idle_suppressed_events
            .is_empty()
        && world.native.applies.load(Ordering::SeqCst) == native_count + 1;
}

#[then("history remains visible while startup is Off and a new On has no old events")]
fn diagnostic_only_replay(world: &mut Ipc) {
    assert!(
        world.history_visible,
        "a new owner lost the retained canonical diagnostic history"
    );
    assert!(
        world.restart_off,
        "diagnostic replay restored native On intent or resources"
    );
    assert!(
        world.new_on_empty,
        "a new On interval restored historical live suppression records"
    );
}

#[when("many ordered observations exceed the recent log window")]
fn recent_window(world: &mut Ipc) {
    let bus = world.server.as_ref().unwrap().actions();
    let sink = Arc::new(Sink::default());
    bus.observe(sink.clone());
    let mut last = None;
    for _ in 0..100 {
        let query = bus.prepare::<Power::Session::Query>(Empty {}).unwrap();
        last = Some(query.correlation());
        bus.emit_prepared(query);
    }
    sink.result(last.as_ref().unwrap());
    bus.emit::<Log::Session::Query>(Empty {});
    world.log_view = Some(serde_json::to_value(sink.log()).unwrap());
}

#[then("the log view is bounded and explicitly reports earlier entries omitted")]
fn recent_window_bounded(world: &mut Ipc) {
    let view = world.log_view.as_ref().unwrap();
    assert!(view["entries"].as_array().unwrap().len() <= 256);
    assert_eq!(view["earlier_entries_omitted"], true);
    assert!(
        view.get("storage_error")
            .is_none_or(serde_json::Value::is_null)
    );
}

#[when("the durable journal contains a malformed record at startup")]
fn corrupt_history(world: &mut Ipc) {
    let root = world.location().directory;
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.join("actions.journal");
    fs::write(&path, b"preserve corrupt journal bytes").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    match OwnerServer::bind(world.location(), world.native.clone()) {
        Ok(server) => {
            world.server = Some(server);
        }
        Err(error) => world.startup_error = Some(error.0),
    }
    world.preserved = fs::read(&path).unwrap() == b"preserve corrupt journal bytes";
}

#[then("startup reports journal corruption without changing those bytes or enabling protection")]
fn corrupt_rejected(world: &mut Ipc) {
    assert!(
        world
            .startup_error
            .as_ref()
            .is_some_and(|error| error.contains("journal")),
        "corrupt history was accepted instead of an explicit startup error"
    );
    assert!(world.preserved);
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 0);
}

#[when("repeated native log views rotate the retained journal")]
fn rotate_history(world: &mut Ipc) {
    let bus = world.server.as_ref().unwrap().actions();
    let sink = Arc::new(Sink::default());
    bus.observe(sink.clone());
    let mut last = None;
    for _ in 0..100 {
        let query = bus.prepare::<Power::Session::Query>(Empty {}).unwrap();
        last = Some(query.correlation());
        bus.emit_prepared(query);
    }
    let result = sink.result(last.as_ref().unwrap());
    assert!(result.error.is_none(), "{result:?}");
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(false));
    let mut last = None;
    for _ in 0..80 {
        let query = bus.prepare::<Log::Session::Query>(Empty {}).unwrap();
        last = Some(query.correlation());
        bus.emit_prepared(query);
    }
    let view = sink.log_for(last.as_ref().unwrap());
    assert!(view.storage_error.is_none(), "{view:?}");
    assert!(view.earlier_entries_omitted);
    let root = world.location().directory;
    world.rotated_bounded = root.join("actions.previous").metadata().unwrap().len() <= 1024 * 1024
        && root.join("actions.journal").metadata().unwrap().len() <= 1024 * 1024;
    let quit = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
    let id = quit.correlation();
    bus.emit_prepared(quit);
    let result = sink.result(&id);
    assert!(result.error.is_none(), "{result:?}");
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(false));
    drop(bus);
    drop(world.server.take());
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
            Err(error) => panic!("rotated journal restart failed: {error}"),
        }
    }
    let bus = world.server.as_ref().unwrap().actions();
    let restored = Arc::new(Sink::default());
    bus.observe(restored.clone());
    let query = bus.prepare::<Log::Session::Query>(Empty {}).unwrap();
    let id = query.correlation();
    bus.emit_prepared(query);
    world.log_view = Some(serde_json::to_value(restored.log_for(&id)).unwrap());
}

#[then("both retained files are bounded and restart reads their contiguous diagnostic tail")]
fn rotation_restored(world: &mut Ipc) {
    assert!(world.rotated_bounded);
    let view = world.log_view.as_ref().unwrap();
    assert!(view.get("storage_error").is_none(), "{view}");
    let entries = view["entries"].as_array().unwrap();
    assert!(entries.len() <= 256);
    assert!(entries[0]["sequence"].as_u64().unwrap() > 0);
    for pair in entries.windows(2) {
        assert_eq!(
            pair[0]["sequence"].as_u64().unwrap() + 1,
            pair[1]["sequence"]
        );
    }
    assert_eq!(view["earlier_entries_omitted"], true);
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 0);
}

#[when("a real peer sends an unexpected private value in a control request")]
fn private_bad_parameter(world: &mut Ipc) {
    let bus = world.server.as_ref().unwrap().actions();
    let sink = Arc::new(Sink::default());
    bus.observe(sink.clone());
    let query = bus.prepare::<Power::Session::Query>(Empty {}).unwrap();
    let id = query.correlation();
    bus.emit_prepared(query);
    sink.result(&id);
    let entry = sink
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|entry| entry.payload::<Power::Session::Query>().unwrap().is_some())
        .unwrap()
        .clone();
    let mut value = serde_json::to_value(entry).unwrap();
    value["password"] = serde_json::json!("PRIVATE_TEST_VALUE_NEVER_LOG");
    let entry = serde_json::from_value(value).unwrap();
    let mut stream = world.raw();
    stream.write_all(&encode_frame(&entry).unwrap()).unwrap();
    world.reply = Some(read_reply(&mut stream));
}

#[then("its error and durable files omit that private value")]
fn no_private_diagnostics(world: &mut Ipc) {
    let reply = world.reply.as_ref().unwrap();
    assert_eq!(
        reply
            .payload::<Error::Ipc::Protocol::Occurred>()
            .unwrap()
            .unwrap()
            .fault_code,
        "invalid_payload"
    );
    assert!(
        !serde_json::to_string(reply)
            .unwrap()
            .contains("PRIVATE_TEST_VALUE_NEVER_LOG")
    );
    let root = world.location().directory;
    for path in [root.join("actions.journal"), root.join("actions.previous")] {
        match fs::read(path) {
            Ok(bytes) => {
                assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE_TEST_VALUE_NEVER_LOG"))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("cannot inspect retained diagnostics: {error}"),
        }
    }
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 0);
}

#[when("two clients overlap repeated On with a newer Off request")]
fn concurrent_clients(world: &mut Ipc) {
    world.native.hold.store(true, Ordering::SeqCst);
    let first = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let second = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let first_sink = Arc::new(Sink::default());
    let second_sink = Arc::new(Sink::default());
    let owner_sink = Arc::new(Sink::default());
    first.observe(first_sink.clone());
    second.observe(second_sink.clone());
    world
        .server
        .as_ref()
        .unwrap()
        .actions()
        .observe(owner_sink.clone());
    let on = first.prepare::<Power::Session::TurnOn>().unwrap();
    let first_id = on.correlation();
    first.emit(on);
    wait_for_native(&world.native);
    let on = second.prepare::<Power::Session::TurnOn>().unwrap();
    let second_id = on.correlation();
    second.emit(on);
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
    while admitted() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(admitted(), 2);
    let off = first.prepare::<Power::Session::TurnOff>().unwrap();
    let off_id = off.correlation();
    first.emit(off);
    world.outcomes.push(first_sink.result(&first_id));
    world.outcomes.push(second_sink.result(&second_id));
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 1);
    world.native.complete();
    wait_for_native(&world.native);
    assert!(!world.native.waiting.lock().unwrap()[0].0.enabled);
    world.native.complete();
    world.outcomes.push(first_sink.result(&off_id));
}

#[then("both On callers receive superseded errors and only confirmed Off succeeds")]
fn concurrent_results(world: &mut Ipc) {
    assert_eq!(world.outcomes.len(), 3);
    for result in &world.outcomes[..2] {
        assert_eq!(result.error.as_ref().unwrap().fault_code, "superseded");
    }
    assert!(world.outcomes[2].error.is_none());
    assert_eq!(
        world.outcomes[2].status.as_ref().unwrap().is_on,
        Some(false)
    );
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 2);
}

#[when(expr = "the {word} journal candidate is a FIFO without a writer")]
fn fifo_candidate(world: &mut Ipc, candidate: String) {
    use std::os::unix::fs::OpenOptionsExt;
    let location = world.location();
    fs::create_dir(&location.directory).unwrap();
    fs::set_permissions(&location.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let path = location.directory.join(candidate);
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let native = world.native.clone();
    let (sender, completed) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        sender
            .send(
                OwnerServer::bind(location, native)
                    .err()
                    .map(|error| error.0),
            )
            .unwrap();
    });
    let outcome = completed.recv_timeout(Duration::from_millis(200));
    world.lock_refused = outcome.is_ok();
    // Unblock only this exact test-owned FIFO on old behavior; always join the fixture.
    let rescue = if outcome.is_err() {
        Some(
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
                .unwrap(),
        )
    } else {
        None
    };
    world.startup_error =
        outcome.unwrap_or_else(|_| completed.recv_timeout(Duration::from_secs(2)).unwrap());
    worker.join().unwrap();
    drop(rescue);
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_fifo());
}

#[then("startup promptly reports an unsafe journal without acquiring resources")]
fn fifo_rejected(world: &mut Ipc) {
    assert!(
        world.lock_refused,
        "journal open waited for a FIFO writer before validating its type"
    );
    assert!(
        world
            .startup_error
            .as_ref()
            .unwrap()
            .contains("journal_unsafe")
    );
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 0);
}

fn main() {
    futures::executor::block_on(Ipc::cucumber().fail_on_skipped().run_and_exit(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/features/ipc.feature"
    )));
}
#[when(expr = "the owner sends a {word} error after a retired request")]
fn retired_error(world: &mut Ipc, scope: String) {
    let location = world.location();
    fs::create_dir(&location.directory).unwrap();
    fs::set_permissions(&location.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let socket = location.directory.join("owner.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    let protocol = scope == "protocol";
    world.preserved = protocol;
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let (send, received) = std::sync::mpsc::channel();
        let mut builder = RegistryBuilder::default();
        builder
            .on::<Power::Session::Query>(move |_, events| {
                send.send(events.clone()).unwrap();
                Ok(())
            })
            .unwrap();
        let bus = builder.build().unwrap();
        let output = Arc::new(Sink::default());
        bus.observe(output.clone());
        let send_result = |events: &Emitter, stream: &mut UnixStream| {
            events.emit::<Power::Session::Settled>(ai2screenwake_client::contract::Settlement {
                correlation: events.correlation(),
                result: ControlResult {
                    status: Some(ai2screenwake_client::contract::Status::off()),
                    error: None,
                },
            });
            let entry = output
                .0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|entry| {
                    entry
                        .payload::<Power::Session::Settled>()
                        .unwrap()
                        .is_some()
                })
                .unwrap()
                .clone();
            stream.write_all(&encode_frame(&entry).unwrap()).unwrap();
        };
        bus.emit_envelope(read_reply(&mut stream));
        let retired = received.recv().unwrap();
        send_result(&retired, &mut stream);
        bus.emit_envelope(read_reply(&mut stream));
        let pending = received.recv().unwrap();
        if protocol {
            bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                "invalid_frame",
                "Malformed owner protocol.",
                true,
            ));
        } else {
            retired.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "operation_timed_out",
                "Retired owner request expired.",
                true,
            ));
        }
        let entry = output.0.lock().unwrap().last().unwrap().clone();
        stream.write_all(&encode_frame(&entry).unwrap()).unwrap();
        if protocol {
            return;
        }
        // A subsequent request on this same socket proves the client survived the late error.
        let mut size = [0; 4];
        if stream.read_exact(&mut size).is_err() {
            return;
        }
        let mut body = vec![0; u32::from_be_bytes(size) as usize];
        stream.read_exact(&mut body).unwrap();
        bus.emit_envelope(serde_json::from_slice(&body).unwrap());
        let barrier = received.recv().unwrap();
        send_result(&pending, &mut stream);
        send_result(&barrier, &mut stream);
    });
    let connection = OwnerConnection::connect(location, Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    connection.observe(sink.clone());
    let first = connection.prepare::<Power::Session::Query>().unwrap();
    let first_id = first.correlation();
    connection.emit(first);
    assert!(sink.result(&first_id).error.is_none());
    let second = connection.prepare::<Power::Session::Query>().unwrap();
    let second_id = second.correlation();
    connection.emit(second);
    let entries = sink.0.lock().unwrap();
    let (entries, _) = sink
        .1
        .wait_timeout_while(entries, Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Error::Dispatch::Validation::Occurred>()
                    .unwrap()
                    .is_some_and(|fault| fault.fault_code == "operation_timed_out")
                    || entry
                        .payload::<Error::Ipc::Protocol::Occurred>()
                        .unwrap()
                        .is_some_and(|fault| fault.fault_code == "invalid_frame")
            })
        })
        .unwrap();
    drop(entries);
    if !protocol {
        let barrier = connection.prepare::<Power::Session::Query>().unwrap();
        connection.emit(barrier);
    }
    world.outcomes.push(sink.result(&second_id));
    drop(connection);
    worker.join().unwrap();
}

#[then("only genuine protocol failure terminates the unrelated request")]
fn retired_error_result(world: &mut Ipc) {
    let result = world.outcomes.last().unwrap();
    assert_eq!(
        result.error.is_some(),
        world.preserved,
        "late request errors must not poison unrelated pending work: {result:?}"
    );
    if !world.preserved {
        assert_eq!(result.status.as_ref().unwrap().is_on, Some(false));
    }
}
#[when("another peer submits duplicate foreign and oversized abandonment metadata")]
fn foreign_abandon(world: &mut Ipc) {
    world.native.hold.store(true, Ordering::SeqCst);
    let first = OwnerConnection::connect(world.location(), Arc::new(NoLaunch)).unwrap();
    let sink = Arc::new(Sink::default());
    first.observe(sink.clone());
    let on = first.prepare::<Power::Session::TurnOn>().unwrap();
    let id = on.correlation();
    first.emit(on);
    wait_for_native(&world.native);
    let (send, received) = std::sync::mpsc::channel();
    world
        .server
        .as_ref()
        .unwrap()
        .actions()
        .observe_action::<Power::Session::Abandoned>(move |notification, _| {
            if notification.correlations.is_empty() {
                let _ = send.send(());
            }
        });
    let bus = RegistryBuilder::default().build().unwrap();
    let output = Arc::new(Sink::default());
    bus.observe(output.clone());
    let envelope = |correlations| {
        bus.emit::<Power::Session::Abandoned>(Abandoned { correlations });
        output.0.lock().unwrap().last().unwrap().clone()
    };
    let mut peer = world.raw();
    for _ in 0..2 {
        peer.write_all(&encode_frame(&envelope(vec![id.clone()])).unwrap())
            .unwrap();
        received.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    peer.write_all(&encode_frame(&envelope(vec![id.clone(); 33])).unwrap())
        .unwrap();
    assert_eq!(
        read_reply(&mut peer)
            .payload::<Error::Ipc::Protocol::Occurred>()
            .unwrap()
            .unwrap()
            .fault_code,
        "invalid_abandon"
    );
    world.native.complete();
    world.outcomes.push(sink.result(&id));
    drop(peer);
    drop(first);
}

#[then("the original admitted control still settles once without another native apply")]
fn foreign_abandon_preserved(world: &mut Ipc) {
    let result = world.outcomes.last().unwrap();
    assert!(result.error.is_none());
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(true));
    assert_eq!(world.native.applies.load(Ordering::SeqCst), 1);
}
#[when("the rotation archive becomes a FIFO before the active journal fills")]
fn rotation_fifo(world: &mut Ipc) {
    use ai2screenwake_client::contract::{ActionJournal, JournalHistory, Registry};
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::atomic::AtomicU64;
    struct Traced {
        journal: journal_codec::FileJournal,
        next: AtomicU64,
    }
    impl ActionJournal for Traced {
        fn history(&self) -> Result<JournalHistory, Fault> {
            self.journal.history()
        }
        fn append(&self, sequence: u64, envelope: &KvEnvelope) -> Result<(), Fault> {
            self.journal.append(sequence, envelope)?;
            self.next.store(sequence + 1, Ordering::SeqCst);
            Ok(())
        }
    }
    let root = world.location().directory;
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let traced = Arc::new(Traced {
        journal: journal_codec::FileJournal::open(&root, unsafe { libc::geteuid() }).unwrap(),
        next: AtomicU64::new(0),
    });
    let bus: Registry = ai2screenwake_client::runtime::start_with_journal(
        Some(world.native.clone()),
        RuntimeOptions::default(),
        Some(traced.clone()),
    )
    .unwrap();
    let sink = Arc::new(Sink::default());
    bus.observe(sink.clone());
    let query = bus.prepare::<Power::Session::Query>(Empty {}).unwrap();
    let id = query.correlation();
    bus.emit_prepared(query);
    assert!(sink.result(&id).error.is_none());
    let source = RegistryBuilder::default().build().unwrap();
    let source_sink = Arc::new(Sink::default());
    source.observe(source_sink.clone());
    source.emit::<Log::Session::Query>(Empty {});
    let next_query = source_sink
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|entry| entry.payload::<Log::Session::Query>().unwrap().is_some())
        .unwrap()
        .clone();
    let emit_filler = |size| {
        let before = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| {
                entry
                    .payload::<contract::Ui::Menu::Render>()
                    .unwrap()
                    .is_some()
            })
            .count();
        bus.emit::<ai2screenwake_client::contract::Ui::Menu::Render>(
            ai2screenwake_client::contract::Ui::Projection {
                text: std::collections::HashMap::from([("diagnostic".into(), "x".repeat(size))]),
                flags: std::collections::HashMap::new(),
            },
        );
        let entries = sink.0.lock().unwrap();
        let (entries, _) = sink
            .1
            .wait_timeout_while(entries, Duration::from_secs(1), |entries| {
                entries
                    .iter()
                    .filter(|entry| {
                        entry
                            .payload::<contract::Ui::Menu::Render>()
                            .unwrap()
                            .is_some()
                    })
                    .count()
                    <= before
            })
            .unwrap();
        assert!(
            entries
                .iter()
                .filter(|entry| entry
                    .payload::<contract::Ui::Menu::Render>()
                    .unwrap()
                    .is_some())
                .count()
                > before,
            "each filler must be persisted and observed before calculating the next boundary"
        );
        entries
            .iter()
            .rev()
            .find(|entry| {
                entry
                    .payload::<contract::Ui::Menu::Render>()
                    .unwrap()
                    .is_some()
            })
            .unwrap()
            .clone()
    };
    let mut last = None;
    while journal_codec::FILE_LIMIT - fs::metadata(root.join("actions.journal")).unwrap().len()
        >= 64 * 1024
    {
        last = Some(emit_filler(50 * 1024));
    }
    let sequence = traced.next.load(Ordering::SeqCst);
    let next_frame = journal_codec::encode_entry(sequence + 1, &next_query)
        .unwrap()
        .len() as u64
        + journal_codec::FRAME_HEADER_BYTES;
    let target = journal_codec::FILE_LIMIT - next_frame + 1;
    let active = fs::metadata(root.join("actions.journal")).unwrap().len();
    let overhead = journal_codec::encode_entry(sequence, last.as_ref().unwrap())
        .unwrap()
        .len() as u64
        + journal_codec::FRAME_HEADER_BYTES
        - 50 * 1024;
    let padding = target
        .checked_sub(active + overhead)
        .expect("a final admitted filler must fit");
    emit_filler(usize::try_from(padding).unwrap());
    let active = fs::metadata(root.join("actions.journal")).unwrap().len();
    let next_frame = journal_codec::encode_entry(traced.next.load(Ordering::SeqCst), &next_query)
        .unwrap()
        .len() as u64
        + journal_codec::FRAME_HEADER_BYTES;
    assert_eq!(
        active, target,
        "actual persisted filler bytes must match the planned boundary"
    );
    assert!(active <= journal_codec::FILE_LIMIT && active + next_frame > journal_codec::FILE_LIMIT);
    let path = root.join("actions.previous");
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    eprintln!(
        "ROTATION_BOUNDARY active={active} next_frame={next_frame} limit={} fifo_installed_before_crossing=true",
        journal_codec::FILE_LIMIT
    );
    bus.emit_envelope(next_query);
    let entries = sink.0.lock().unwrap();
    let (entries, _) = sink
        .1
        .wait_timeout_while(entries, Duration::from_secs(1), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Error::Log::Storage::Occurred>()
                    .unwrap()
                    .is_some_and(|fault| fault.fault_code == "journal_unsafe")
            })
        })
        .unwrap();
    world.preserved = entries.iter().any(|entry| {
        entry
            .payload::<Error::Log::Storage::Occurred>()
            .unwrap()
            .is_some_and(|fault| fault.fault_code == "journal_unsafe")
    });
    let admitted = entries
        .iter()
        .filter(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Ui::Menu::Render>()
                .unwrap()
                .is_some()
        })
        .count();
    drop(entries);
    let rescue = if !world.preserved {
        Some(
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)
                .unwrap(),
        )
    } else {
        None
    };
    let quit = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
    let id = quit.correlation();
    bus.emit_prepared(quit);
    world.outcomes.push(sink.result(&id));
    drop(rescue);
    eprintln!(
        "ROTATION_FIFO admitted={admitted} timely_unsafe={}",
        world.preserved
    );
    assert!(
        admitted >= 20,
        "the fixture must fill the active journal with valid admitted entries"
    );
}

#[then("rotation promptly reports the unsafe archive and the owner can still dispatch cleanup")]
fn rotation_fifo_rejected(world: &mut Ipc) {
    assert!(
        world.preserved,
        "rotation waited for a FIFO writer before descriptor validation"
    );
    assert_eq!(
        world
            .outcomes
            .last()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .fault_code,
        "journal_unsafe"
    );
    assert_eq!(world.native.held.load(Ordering::SeqCst), 0);
}
// end ipc.rs
