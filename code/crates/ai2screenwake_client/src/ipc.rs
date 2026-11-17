// ipc.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Private same-user owner transport; no caller-visible routes or returned core state.

use crate::{
    action_log::FileJournal,
    contract::{
        Abandoned, ControlResult, CorrelationId, Empty, Error, Fault, KvEnvelope, ObservationSink,
        OwnerControl, OwnerLocation, Power, PreparedAction, Registry, RegistryBuilder, Settlement,
        StartupError, State, Status, WeakEmitter,
    },
    runtime::RuntimeOptions,
};
use ai2screenwake_native::{NativePower, launch::OwnerLauncher};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Write},
    mem,
    net::Shutdown,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Maximum UTF-8 envelope body, excluding the four-byte big-endian prefix.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Maximum admitted but unsettled requests for one peer.
pub const MAX_OUTSTANDING: usize = 32;
const MAX_PEERS: usize = 16;
const MAX_QUEUED_FRAMES: usize = MAX_OUTSTANDING * 3 + 1;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TICK: Duration = Duration::from_millis(20);

/// Bind the OS effective user to an explicitly native-supplied path, never a guessed temp root.
pub fn for_current_user(directory: PathBuf) -> OwnerLocation {
    // SAFETY: geteuid has no arguments or failure sentinel.
    OwnerLocation {
        directory,
        uid: unsafe { libc::geteuid() },
    }
}

/// Encode the canonical KV envelope rather than a second transport request schema.
pub fn encode_frame(envelope: &KvEnvelope) -> Result<Vec<u8>, Fault> {
    let body = serde_json::to_vec(envelope)
        .map_err(|_| Fault::new("invalid_frame", "Cannot encode the owner frame.", true))?;
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(Fault::new(
            "frame_too_large",
            "The owner frame exceeds 64 KiB.",
            true,
        ));
    }
    let size = u32::try_from(body.len())
        .map_err(|_| Fault::new("frame_too_large", "The owner frame is too large.", true))?;
    let mut frame = size.to_be_bytes().to_vec();
    frame.extend(body);
    Ok(frame)
}

/// Read the real OS peer UID; this is also exercised on real Unix socket pairs.
pub fn validate_peer(stream: &UnixStream, expected: u32) -> Result<(), Fault> {
    #[cfg(target_os = "macos")]
    let uid = {
        let mut uid = 0;
        let mut gid = 0;
        // SAFETY: stream owns a valid socket and both output pointers are writable uid_t/gid_t.
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
            return Err(Fault::new(
                "peer_identity_failed",
                "Cannot verify the owner peer.",
                true,
            ));
        }
        uid
    };
    #[cfg(not(target_os = "macos"))]
    let uid = {
        // SAFETY: zero is a valid initial ucred and getsockopt receives its exact writable size.
        let mut credential: libc::ucred = unsafe { mem::zeroed() };
        let mut size = mem::size_of::<libc::ucred>() as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credential as *mut libc::ucred).cast(),
                &mut size,
            )
        } != 0
        {
            return Err(Fault::new(
                "peer_identity_failed",
                "Cannot verify the owner peer.",
                true,
            ));
        }
        credential.uid
    };
    if uid != expected {
        return Err(Fault::new(
            "foreign_peer",
            "The owner peer belongs to another user.",
            false,
        ));
    }
    Ok(())
}

fn socket_address(path: &Path) -> io::Result<libc::sockaddr_un> {
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: sockaddr_un consists only of integer/byte fields.
    let mut address: libc::sockaddr_un = unsafe { mem::zeroed() };
    if bytes.is_empty() || bytes.contains(&0) || bytes.len() >= address.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "owner socket path is too long or invalid",
        ));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = mem::size_of::<libc::sockaddr_un>() as u8;
    }
    for (out, byte) in address.sun_path.iter_mut().zip(bytes) {
        *out = *byte as libc::c_char;
    }
    Ok(address)
}

fn connect_socket(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let address = socket_address(path)?;
    // SAFETY: socket returns a new descriptor, transferred to OwnedFd on success.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is newly owned and not aliased by another owner.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: these commands set descriptor flags and do not read pointer arguments.
    if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0
        || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: address is a valid bounded sockaddr_un for the supplied exact size.
    let result = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            mem::size_of_val(&address) as libc::socklen_t,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if !matches!(error.raw_os_error(), Some(libc::EINPROGRESS | libc::EAGAIN)) {
            return Err(error);
        }
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: poll references one initialized pollfd for a bounded timeout.
        let ready = unsafe {
            libc::poll(
                &mut poll,
                1,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if ready == 0 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owner connect timed out",
            ));
        }
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut failure: libc::c_int = 0;
        let mut size = mem::size_of_val(&failure) as libc::socklen_t;
        // SAFETY: the writable integer and its size exactly match SO_ERROR.
        if unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut failure as *mut libc::c_int).cast(),
                &mut size,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        if failure != 0 {
            return Err(io::Error::from_raw_os_error(failure));
        }
    }
    Ok(UnixStream::from(fd))
}

fn directory(location: &OwnerLocation, create: bool) -> Result<PathBuf, Fault> {
    // SAFETY: identity calls have no arguments or failure sentinel.
    if !location.directory.is_absolute()
        || location.uid != unsafe { libc::geteuid() }
        || unsafe { libc::getuid() } != unsafe { libc::geteuid() }
    {
        return Err(Fault::new(
            "runtime_identity",
            "The owner runtime identity is invalid.",
            false,
        ));
    }
    if create {
        match DirBuilder::new().mode(0o700).create(&location.directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => {
                return Err(Fault::new(
                    "runtime_directory",
                    "Cannot create the owner runtime directory.",
                    true,
                ));
            }
        }
    }
    let metadata = fs::symlink_metadata(&location.directory).map_err(|_| {
        Fault::new(
            "runtime_unavailable",
            "The owner runtime directory is unavailable.",
            true,
        )
    })?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != location.uid
        || metadata.mode() & 0o7777 != 0o700
    {
        return Err(Fault::new(
            "unsafe_runtime",
            "The owner runtime directory is not private.",
            false,
        ));
    }
    let path = location.directory.canonicalize().map_err(|_| {
        Fault::new(
            "runtime_directory",
            "Cannot resolve the owner runtime directory.",
            true,
        )
    })?;
    socket_address(&path.join("owner.sock"))
        .map_err(|_| Fault::new("socket_path", "The owner socket path is too long.", false))?;
    Ok(path)
}

fn socket_metadata(path: &Path, uid: u32) -> Result<fs::Metadata, Fault> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        Fault::new(
            "socket_unavailable",
            "The owner socket is unavailable.",
            true,
        )
    })?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != uid
        || metadata.mode() & 0o7777 != 0o600
    {
        return Err(Fault::new(
            "unsafe_socket",
            "The owner socket identity is invalid.",
            false,
        ));
    }
    Ok(metadata)
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.uid() == right.uid()
        && left.file_type() == right.file_type()
}

struct Lease {
    socket: PathBuf,
    identity: fs::Metadata,
    _lock: File,
}

fn bind(location: &OwnerLocation) -> Result<(Lease, UnixListener), StartupError> {
    let failed = |fault: Fault| StartupError(format!("{}: {}", fault.fault_code, fault.message));
    let root = directory(location, true).map_err(failed)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root.join("owner.lock"))
        .map_err(|_| StartupError("owner_lock: Cannot open the private owner lock.".into()))?;
    let metadata = lock
        .metadata()
        .map_err(|error| StartupError(error.to_string()))?;
    if !metadata.is_file() || metadata.uid() != location.uid || metadata.mode() & 0o7777 != 0o600 {
        return Err(StartupError(
            "unsafe_lock: The owner lock is not a private regular file.".into(),
        ));
    }
    // SAFETY: lock owns this valid file descriptor; LOCK_NB never waits for another process.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(StartupError(
            "owner_already_running: The owner lock is held.".into(),
        ));
    }
    let socket = root.join("owner.sock");
    match fs::symlink_metadata(&socket) {
        Ok(_) => {
            let before = socket_metadata(&socket, location.uid).map_err(failed)?;
            match connect_socket(&socket, Duration::from_millis(50)) {
                Ok(_) => {
                    return Err(StartupError(
                        "owner_already_running: An owner is listening.".into(),
                    ));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) => {}
                Err(_) => {
                    return Err(StartupError(
                        "owner_unverified: Cannot disprove a live owner listener.".into(),
                    ));
                }
            }
            match fs::symlink_metadata(&socket) {
                Ok(after) if same_file(&before, &after) && after.file_type().is_socket() => {
                    fs::remove_file(&socket).map_err(|error| StartupError(error.to_string()))?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                _ => {
                    return Err(StartupError(
                        "socket_changed: Stale owner identity changed.".into(),
                    ));
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(StartupError(error.to_string())),
    }
    let listener = UnixListener::bind(&socket).map_err(|error| StartupError(error.to_string()))?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
        .map_err(|error| StartupError(error.to_string()))?;
    let identity = socket_metadata(&socket, location.uid).map_err(failed)?;
    listener
        .set_nonblocking(true)
        .map_err(|error| StartupError(error.to_string()))?;
    Ok((
        Lease {
            socket,
            identity,
            _lock: lock,
        },
        listener,
    ))
}

#[derive(Default)]
struct Reader {
    header: [u8; 4],
    used: usize,
    body: Vec<u8>,
    expected: usize,
}

enum ReadFrame {
    Frame(KvEnvelope),
    Pending,
    Closed,
}

impl Reader {
    fn next(&mut self, stream: &mut UnixStream) -> Result<ReadFrame, Fault> {
        loop {
            if self.used < 4 {
                match stream.read(&mut self.header[self.used..]) {
                    Ok(0) if self.used == 0 => return Ok(ReadFrame::Closed),
                    Ok(0) => {
                        return Err(Fault::new(
                            "truncated_frame",
                            "The owner frame was truncated.",
                            true,
                        ));
                    }
                    Ok(size) => self.used += size,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        return Ok(ReadFrame::Pending);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        return Err(Fault::new(
                            "peer_read",
                            "Cannot read the owner connection.",
                            true,
                        ));
                    }
                }
                if self.used < 4 {
                    continue;
                }
                self.expected = u32::from_be_bytes(self.header) as usize;
                if self.expected == 0 || self.expected > MAX_FRAME_BYTES {
                    return Err(Fault::new(
                        "frame_too_large",
                        "The owner frame length is invalid.",
                        true,
                    ));
                }
                self.body = Vec::with_capacity(self.expected);
            }
            let remaining = self.expected - self.body.len();
            let mut chunk = [0_u8; 4096];
            match stream.read(&mut chunk[..remaining.min(4096)]) {
                Ok(0) => {
                    return Err(Fault::new(
                        "truncated_frame",
                        "The owner frame was truncated.",
                        true,
                    ));
                }
                Ok(size) => self.body.extend_from_slice(&chunk[..size]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(ReadFrame::Pending);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    return Err(Fault::new(
                        "peer_read",
                        "Cannot read the owner connection.",
                        true,
                    ));
                }
            }
            if self.body.len() == self.expected {
                let result = serde_json::from_slice(&self.body).map_err(|_| {
                    Fault::new(
                        "invalid_frame",
                        "The owner frame is not a KV envelope.",
                        true,
                    )
                });
                self.used = 0;
                self.expected = 0;
                self.body.clear();
                return result.map(ReadFrame::Frame);
            }
        }
    }
}

struct Writing {
    bytes: Vec<u8>,
    offset: usize,
}

fn write_ready(stream: &mut UnixStream, output: &mut VecDeque<Writing>) -> Result<(), Fault> {
    while let Some(frame) = output.front_mut() {
        match stream.write(&frame.bytes[frame.offset..]) {
            Ok(0) => {
                return Err(Fault::new(
                    "peer_write",
                    "The owner connection stopped accepting data.",
                    true,
                ));
            }
            Ok(size) => frame.offset += size,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                return Err(Fault::new(
                    "peer_write",
                    "Cannot write the owner connection.",
                    true,
                ));
            }
        }
        if frame.offset == frame.bytes.len() {
            output.pop_front();
        }
    }
    Ok(())
}

fn pause(fd: i32, writing: bool) -> Result<(), Fault> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN | if writing { libc::POLLOUT } else { 0 },
        revents: 0,
    };
    // SAFETY: one valid pollfd, with a bounded lifecycle/deadline wake interval.
    if unsafe { libc::poll(&mut poll, 1, IO_TICK.as_millis() as i32) } < 0
        && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
    {
        return Err(Fault::new(
            "peer_poll",
            "Cannot observe the owner connection.",
            true,
        ));
    }
    Ok(())
}

fn is_control(envelope: &KvEnvelope) -> Result<bool, Fault> {
    Ok(envelope.payload::<Power::Session::Query>()?.is_some()
        || envelope.payload::<Power::Session::TurnOn>()?.is_some()
        || envelope.payload::<Power::Session::TurnOff>()?.is_some())
}

fn error_payload(envelope: &KvEnvelope) -> Result<Option<Fault>, Fault> {
    macro_rules! error {
        ($action:ty) => {
            if let Some(fault) = envelope.payload::<$action>()? {
                return Ok(Some(fault));
            }
        };
    }
    error!(Error::Ipc::Protocol::Occurred);
    error!(Error::Dispatch::Validation::Occurred);
    error!(Error::Native::Assertions::Occurred);
    error!(Error::Native::Activity::Occurred);
    error!(Error::Log::Storage::Occurred);
    Ok(None)
}

#[derive(Default)]
struct PeerState {
    pending: HashSet<CorrelationId>,
    errors: HashSet<CorrelationId>,
    outgoing: VecDeque<KvEnvelope>,
    failure: Option<Fault>,
}

struct PeerSink {
    peer: Weak<Mutex<PeerState>>,
}

impl ObservationSink for PeerSink {
    fn observed(&self, envelope: KvEnvelope) {
        let Some(peer) = self.peer.upgrade() else {
            return;
        };
        let mut peer = peer.lock().expect("peer state poisoned");
        let forward = match envelope.payload::<Power::Session::Settled>() {
            Ok(Some(settled)) => peer.pending.remove(&settled.correlation),
            Ok(None) => match error_payload(&envelope) {
                Ok(Some(_)) => match envelope.correlation() {
                    Ok(id) => peer.errors.remove(&id) || peer.pending.contains(&id),
                    Err(fault) => {
                        peer.failure = Some(fault);
                        false
                    }
                },
                Ok(None) => false,
                Err(fault) => {
                    peer.failure = Some(fault);
                    false
                }
            },
            Err(fault) => {
                peer.failure = Some(fault);
                false
            }
        };
        if forward {
            if peer.outgoing.len() >= MAX_QUEUED_FRAMES {
                peer.failure = Some(Fault::new(
                    "peer_backpressure",
                    "The owner peer is not reading results.",
                    true,
                ));
            } else {
                peer.outgoing.push_back(envelope);
            }
        }
    }
}

fn peer_error(bus: &Registry, peer: &Arc<Mutex<PeerState>>, fault: Fault) {
    if let Ok(error) = bus.prepare::<Error::Ipc::Protocol::Occurred>(fault) {
        {
            let mut state = peer.lock().expect("peer state poisoned");
            if state.errors.len() >= MAX_OUTSTANDING {
                state.failure = Some(Fault::new(
                    "peer_backpressure",
                    "The owner peer is not reading errors.",
                    true,
                ));
                return;
            }
            state.errors.insert(error.correlation());
        }
        bus.emit_prepared(error);
    }
}

fn serve_peer(mut stream: UnixStream, uid: u32, bus: Registry, stopping: Arc<AtomicBool>) {
    let peer = Arc::new(Mutex::new(PeerState::default()));
    let _observation = bus.subscribe(Arc::new(PeerSink {
        peer: Arc::downgrade(&peer),
    }));
    let mut reader = Reader::default();
    let mut output = VecDeque::new();
    let mut closing = None;
    if let Err(fault) = stream
        .set_nonblocking(true)
        .map_err(|_| {
            Fault::new(
                "peer_configuration",
                "Cannot configure the owner peer.",
                true,
            )
        })
        .and_then(|_| validate_peer(&stream, uid))
    {
        peer_error(&bus, &peer, fault);
        closing = Some(Instant::now() + IO_TICK);
    }
    loop {
        if stopping.load(Ordering::Acquire) && closing.is_none() {
            closing = Some(Instant::now() + Duration::from_millis(100));
        }
        let failure = peer.lock().expect("peer state poisoned").failure.take();
        if let Some(fault) = failure {
            bus.emit::<Error::Ipc::Protocol::Occurred>(fault);
            break;
        }
        if closing.is_none() {
            for _ in 0..MAX_OUTSTANDING {
                match reader.next(&mut stream) {
                    Ok(ReadFrame::Frame(envelope)) => {
                        match envelope.payload::<Power::Session::Abandoned>() {
                            Ok(Some(abandoned)) => {
                                if abandoned.correlations.len() > MAX_OUTSTANDING {
                                    peer_error(
                                        &bus,
                                        &peer,
                                        Fault::new(
                                            "invalid_abandon",
                                            "Too many abandoned peer requests.",
                                            true,
                                        ),
                                    );
                                } else {
                                    let correlations = {
                                        let mut state = peer.lock().expect("peer state poisoned");
                                        abandoned
                                            .correlations
                                            .into_iter()
                                            .filter(|id| state.pending.remove(id))
                                            .collect()
                                    };
                                    bus.emit::<Power::Session::Abandoned>(Abandoned {
                                        correlations,
                                    });
                                }
                                continue;
                            }
                            Ok(None) => {}
                            Err(fault) => {
                                peer_error(&bus, &peer, fault);
                                continue;
                            }
                        }
                        let admission = is_control(&envelope).and_then(|allowed| {
                            if !allowed {
                                return Err(Fault::new(
                                    "inadmissible_action",
                                    "IPC accepts only the three owner controls.",
                                    true,
                                ));
                            }
                            let id = envelope.correlation()?;
                            let mut state = peer.lock().expect("peer state poisoned");
                            if state.pending.len() >= MAX_OUTSTANDING {
                                return Err(Fault::new(
                                    "peer_capacity",
                                    "The owner peer has 32 outstanding requests.",
                                    true,
                                ));
                            }
                            if !state.pending.insert(id) {
                                return Err(Fault::new(
                                    "duplicate_request",
                                    "This peer request is already pending.",
                                    true,
                                ));
                            }
                            Ok(())
                        });
                        match admission {
                            Ok(()) => bus.emit_envelope(envelope),
                            Err(fault) => peer_error(&bus, &peer, fault),
                        }
                    }
                    Ok(ReadFrame::Pending) => break,
                    Ok(ReadFrame::Closed) => {
                        closing = Some(Instant::now());
                        break;
                    }
                    Err(fault) => {
                        peer_error(&bus, &peer, fault);
                        closing = Some(Instant::now() + Duration::from_millis(100));
                        break;
                    }
                }
            }
        }
        {
            let mut state = peer.lock().expect("peer state poisoned");
            while output.len() < MAX_OUTSTANDING {
                let Some(entry) = state.outgoing.pop_front() else {
                    break;
                };
                match encode_frame(&entry) {
                    Ok(bytes) => output.push_back(Writing { bytes, offset: 0 }),
                    Err(fault) => state.failure = Some(fault),
                }
            }
        }
        if let Err(fault) = write_ready(&mut stream, &mut output) {
            bus.emit::<Error::Ipc::Protocol::Occurred>(fault);
            break;
        }
        if closing.is_some_and(|at| {
            let state = peer.lock().expect("peer state poisoned");
            (output.is_empty() && state.outgoing.is_empty() && state.errors.is_empty())
                || Instant::now() >= at
        }) {
            break;
        }
        if let Err(fault) = pause(stream.as_raw_fd(), !output.is_empty()) {
            bus.emit::<Error::Ipc::Protocol::Occurred>(fault);
            break;
        }
    }
    let correlations = peer
        .lock()
        .expect("peer state poisoned")
        .pending
        .drain()
        .collect();
    bus.emit::<Power::Session::Abandoned>(Abandoned { correlations });
}

struct OwnerLease {
    lease: Lease,
    disposing: AtomicBool,
    closing: AtomicBool,
    closed: AtomicBool,
    stopping: Arc<AtomicBool>,
    lifecycle: Mutex<()>,
    changed: Condvar,
}

impl OwnerLease {
    fn stop(&self) {
        let _lifecycle = self.lifecycle.lock().expect("owner lifecycle poisoned");
        self.stopping.store(true, Ordering::Release);
        self.changed.notify_one();
    }

    fn retain_until_stopped(&self) {
        let mut lifecycle = self.lifecycle.lock().expect("owner lifecycle poisoned");
        while !self.stopping.load(Ordering::Acquire) {
            lifecycle = self
                .changed
                .wait(lifecycle)
                .expect("owner lifecycle poisoned");
        }
    }
}

struct OwnershipSink(Arc<OwnerLease>);

impl ObservationSink for OwnershipSink {
    fn observed(&self, envelope: KvEnvelope) {
        if envelope
            .payload::<Power::Session::Quit>()
            .expect("typed Quit envelope")
            .is_some()
        {
            self.0.closing.store(true, Ordering::Release);
        }
        if envelope
            .payload::<Power::Session::Closed>()
            .expect("typed Closed envelope")
            .is_some()
        {
            // These one-shot signals can race; neither side may miss the other.
            self.0.closed.store(true, Ordering::SeqCst);
            if self.0.disposing.load(Ordering::SeqCst) {
                self.0.stop();
            }
        }
    }
}

fn cleanup_socket(lease: &Lease, actions: &Registry) {
    match fs::symlink_metadata(&lease.socket) {
        Ok(current) if same_file(&lease.identity, &current) => {
            if fs::remove_file(&lease.socket).is_err() {
                actions.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                    "socket_cleanup",
                    "Cannot remove the delivered owner socket.",
                    true,
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => actions.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
            "socket_changed",
            "Owner socket cleanup identity changed.",
            false,
        )),
    }
}

/// The app's sole private owner endpoint. Obtain state only through its Actions port.
pub struct OwnerServer {
    actions: Registry,
    worker: Option<JoinHandle<()>>,
    ownership: Arc<OwnerLease>,
}

impl OwnerServer {
    /// Acquire the private owner lock, bind, then serve the same injected native owner.
    pub fn bind(
        location: OwnerLocation,
        native: Arc<dyn NativePower>,
    ) -> Result<Self, StartupError> {
        Self::bind_with_options(location, native, RuntimeOptions::default())
    }

    /// Host-test deadline injection; the production constructor always uses five seconds.
    pub fn bind_with_options(
        location: OwnerLocation,
        native: Arc<dyn NativePower>,
        options: RuntimeOptions,
    ) -> Result<Self, StartupError> {
        Self::bind_configured(
            location,
            native,
            options,
            crate::telemetry::Configuration::Unconfigured,
        )
    }

    /// Install forwarding once for this owner, never once per view or remote caller.
    /// Authorized configuration does not imply consent; the host must verify it first.
    pub fn bind_with_telemetry(
        location: OwnerLocation,
        native: Arc<dyn NativePower>,
        configuration: crate::telemetry::Configuration,
    ) -> Result<Self, StartupError> {
        Self::bind_configured(location, native, RuntimeOptions::default(), configuration)
    }

    fn bind_configured(
        location: OwnerLocation,
        native: Arc<dyn NativePower>,
        options: RuntimeOptions,
        configuration: crate::telemetry::Configuration,
    ) -> Result<Self, StartupError> {
        let (lease, listener) = bind(&location)?;
        let journal = Arc::new(
            FileJournal::open(
                lease.socket.parent().expect("owner directory"),
                location.uid,
            )
            .map_err(|fault| StartupError(format!("{}: {}", fault.fault_code, fault.message)))?,
        );
        let actions = crate::runtime::build_owner(Some(native), options, Some(journal))?;
        let forwarding = crate::telemetry::install(&actions, configuration);
        let stopping = Arc::new(AtomicBool::new(false));
        let ownership = Arc::new(OwnerLease {
            lease,
            disposing: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            stopping: stopping.clone(),
            lifecycle: Mutex::new(()),
            changed: Condvar::new(),
        });
        actions.observe(Arc::new(OwnershipSink(ownership.clone())));
        let worker_ownership = ownership.clone();
        let stop = stopping.clone();
        let bus = actions.clone();
        let worker = thread::Builder::new()
            .name("screenwake-owner".into())
            .spawn(move || {
                let mut forwarding = forwarding;
                forwarding.start(&bus);
                let mut peers: Vec<JoinHandle<()>> = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    peers.retain(|peer| !peer.is_finished());
                    match listener.accept() {
                        Ok((stream, _)) if peers.len() < MAX_PEERS => {
                            let peer_bus = bus.clone();
                            let peer_stop = stop.clone();
                            match thread::Builder::new().name("screenwake-peer".into()).spawn(
                                move || serve_peer(stream, location.uid, peer_bus, peer_stop),
                            ) {
                                Ok(worker) => peers.push(worker),
                                Err(_) => bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                                    "peer_start",
                                    "Cannot start the owner peer.",
                                    true,
                                )),
                            }
                        }
                        Ok((stream, _)) => {
                            bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                                "owner_capacity",
                                "The owner has too many peers.",
                                true,
                            ));
                            drop(stream);
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            if let Err(fault) = pause(listener.as_raw_fd(), false) {
                                bus.emit::<Error::Ipc::Protocol::Occurred>(fault);
                                worker_ownership.retain_until_stopped();
                                break;
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                        Err(_) => {
                            bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                                "owner_accept",
                                "Cannot accept owner peers.",
                                true,
                            ));
                            worker_ownership.retain_until_stopped();
                            break;
                        }
                    }
                }
                for peer in peers {
                    if peer.join().is_err() {
                        bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                            "peer_failed",
                            "An owner peer stopped unexpectedly.",
                            false,
                        ));
                    }
                }
                cleanup_socket(&worker_ownership.lease, &bus);
                drop(bus);
                drop(worker_ownership);
                #[cfg(feature = "telemetry")]
                drop(forwarding);
            })
            .map_err(|error| StartupError(error.to_string()))?;
        Ok(Self {
            actions,
            worker: Some(worker),
            ownership,
        })
    }

    /// Clone the emit/observe port for NativeSeam, not a core-state reference.
    pub fn actions(&self) -> Registry {
        self.actions.clone()
    }
}

impl Drop for OwnerServer {
    fn drop(&mut self) {
        self.ownership.disposing.store(true, Ordering::SeqCst);
        if !self.ownership.closing.load(Ordering::Acquire) {
            self.actions.emit::<Power::Session::Quit>(Empty {});
        }
        if self.ownership.closed.load(Ordering::SeqCst) {
            self.ownership.stop();
        } else {
            self.actions
                .emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                    "cleanup_pending",
                    "Owner disposal preceded cleanup; the owner lease remains held.",
                    true,
                ));
        }
        // The owned service remains reachable for explicit Off retries on failed cleanup.
        // Never join from a bus callback: that callback may be one of its peer workers.
        drop(self.worker.take());
    }
}

struct ClientPending {
    events: WeakEmitter,
    sent: Option<Instant>,
}

#[derive(Default)]
struct ClientState {
    pending: HashMap<CorrelationId, ClientPending>,
    outgoing: VecDeque<KvEnvelope>,
    last_status: Option<Status>,
}

#[derive(Default)]
struct ClientShared {
    state: Mutex<ClientState>,
    changed: Condvar,
    stopping: AtomicBool,
}

fn transport_failure(events: &crate::contract::Emitter, status: Option<Status>, fault: Fault) {
    events.error::<Error::Ipc::Protocol::Occurred>(fault.clone());
    events.emit::<Power::Session::Settled>(Settlement {
        correlation: events.correlation(),
        result: ControlResult {
            status,
            error: Some(fault),
        },
    });
}

fn unavailable_status(mut status: Option<Status>) -> Option<Status> {
    if let Some(status) = &mut status {
        status.is_on = None;
        status.state = State::Error;
        status.protected_idle_duration_seconds = None;
        status.protected_idle_suppressed_events.clear();
        status.suppressed_events_truncated = false;
    }
    status
}

fn fail_pending(shared: &ClientShared, fault: Fault) {
    let (pending, status) = {
        let mut state = shared.state.lock().expect("connection state poisoned");
        state.outgoing.clear();
        (
            mem::take(&mut state.pending),
            unavailable_status(state.last_status.clone()),
        )
    };
    for (_, request) in pending {
        if let Some(events) = request.events.upgrade() {
            transport_failure(&events, status.clone(), fault.clone());
        }
    }
}

fn connect_owner(
    location: &OwnerLocation,
    launcher: &dyn OwnerLauncher,
    shared: &ClientShared,
) -> Result<UnixStream, Fault> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    let mut launched = false;
    while Instant::now() < deadline && !shared.stopping.load(Ordering::Acquire) {
        match directory(location, false) {
            Ok(root) => {
                let socket = root.join("owner.sock");
                match fs::symlink_metadata(&socket) {
                    Ok(_) => {
                        socket_metadata(&socket, location.uid)?;
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(_) => {
                        return Err(Fault::new(
                            "socket_identity",
                            "Cannot inspect the owner socket.",
                            true,
                        ));
                    }
                }
                match connect_socket(
                    &socket,
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(IO_TICK),
                ) {
                    Ok(stream) => {
                        validate_peer(&stream, location.uid)?;
                        return Ok(stream);
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::NotFound
                                | io::ErrorKind::ConnectionRefused
                                | io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => {
                        return Err(Fault::new(
                            "owner_connect",
                            "Cannot connect to the owner.",
                            true,
                        ));
                    }
                }
            }
            Err(fault) if fault.fault_code == "runtime_unavailable" => {}
            Err(fault) => return Err(fault),
        }
        if !launched {
            launcher.launch()?;
            launched = true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(Fault::new(
        "owner_unavailable",
        "The owner did not become available within ten seconds.",
        true,
    ))
}

fn connection_worker(
    location: OwnerLocation,
    launcher: Arc<dyn OwnerLauncher>,
    shared: Arc<ClientShared>,
    bus: Registry,
) {
    while !shared.stopping.load(Ordering::Acquire) {
        {
            let mut state = shared.state.lock().expect("connection state poisoned");
            while state.outgoing.is_empty() && !shared.stopping.load(Ordering::Acquire) {
                state = shared
                    .changed
                    .wait(state)
                    .expect("connection state poisoned");
            }
        }
        if shared.stopping.load(Ordering::Acquire) {
            break;
        }
        let mut stream = match connect_owner(&location, launcher.as_ref(), &shared) {
            Ok(stream) => stream,
            Err(fault) => {
                fail_pending(&shared, fault);
                continue;
            }
        };
        let mut reader = Reader::default();
        let mut output = VecDeque::new();
        let failure = loop {
            if shared.stopping.load(Ordering::Acquire) {
                break None;
            }
            let mut encoding_failure = None;
            {
                let mut state = shared.state.lock().expect("connection state poisoned");
                while output.len() < MAX_OUTSTANDING {
                    let Some(entry) = state.outgoing.pop_front() else {
                        break;
                    };
                    let id = entry.correlation().expect("framework-made envelope");
                    if let Some(pending) = state.pending.get_mut(&id) {
                        pending.sent = Some(Instant::now());
                    }
                    match encode_frame(&entry) {
                        Ok(bytes) => output.push_back(Writing { bytes, offset: 0 }),
                        Err(fault) => {
                            encoding_failure = Some(fault);
                            break;
                        }
                    }
                }
            }
            if encoding_failure.is_some() {
                break encoding_failure;
            }
            if let Err(fault) = write_ready(&mut stream, &mut output) {
                break Some(fault);
            }
            let mut received_failure = None;
            for _ in 0..MAX_OUTSTANDING {
                match reader.next(&mut stream) {
                    Ok(ReadFrame::Frame(envelope)) => {
                        match envelope.payload::<Power::Session::Settled>() {
                            Ok(Some(settled)) => {
                                if envelope.correlation().ok().as_ref()
                                    != Some(&settled.correlation)
                                {
                                    received_failure = Some(Fault::new(
                                        "invalid_correlation",
                                        "Owner result correlation does not match its envelope.",
                                        true,
                                    ));
                                    break;
                                }
                                let accepted = {
                                    let mut state =
                                        shared.state.lock().expect("connection state poisoned");
                                    if state.pending.remove(&settled.correlation).is_some() {
                                        state.last_status = settled.result.status.clone();
                                        true
                                    } else {
                                        false
                                    }
                                };
                                if accepted {
                                    bus.receive_owner(envelope);
                                } else {
                                    bus.emit::<Error::Ipc::Protocol::Occurred>(
                                        envelope.attributed::<Power::Session::Settled>(Fault::new(
                                            "stale_settlement",
                                            "A completed owner request replied again.",
                                            true,
                                        )),
                                    );
                                }
                            }
                            Ok(None) => match error_payload(&envelope) {
                                Ok(Some(fault)) => {
                                    let active = envelope.correlation().is_ok_and(|id| {
                                        shared
                                            .state
                                            .lock()
                                            .expect("connection state poisoned")
                                            .pending
                                            .contains_key(&id)
                                    });
                                    let connection_wide = fault.failed_genus.is_none()
                                        && envelope
                                            .payload::<Error::Ipc::Protocol::Occurred>()
                                            .is_ok_and(|error| error.is_some());
                                    bus.receive_owner(envelope);
                                    if !active && connection_wide {
                                        received_failure = Some(fault);
                                    }
                                }
                                Ok(None) => {
                                    received_failure = Some(Fault::new(
                                        "invalid_observation",
                                        "The owner returned an invalid observation.",
                                        true,
                                    ))
                                }
                                Err(fault) => received_failure = Some(fault),
                            },
                            Err(fault) => received_failure = Some(fault),
                        }
                    }
                    Ok(ReadFrame::Pending) => break,
                    Ok(ReadFrame::Closed) => {
                        received_failure = Some(Fault::new(
                            "owner_disconnected",
                            "The owner connection closed.",
                            true,
                        ));
                        break;
                    }
                    Err(fault) => {
                        received_failure = Some(fault);
                        break;
                    }
                }
            }
            if received_failure.is_some() {
                break received_failure;
            }
            let expired = {
                let mut state = shared.state.lock().expect("connection state poisoned");
                let ids: Vec<_> = state
                    .pending
                    .iter()
                    .filter(|(_, pending)| {
                        pending
                            .sent
                            .is_some_and(|at| at.elapsed() >= OPERATION_TIMEOUT)
                    })
                    .map(|(id, _)| id.clone())
                    .collect();
                let status = unavailable_status(state.last_status.clone());
                ids.into_iter()
                    .filter_map(|id| state.pending.remove(&id))
                    .map(|request| (request, status.clone()))
                    .collect::<Vec<_>>()
            };
            for (request, status) in expired {
                if let Some(events) = request.events.upgrade() {
                    transport_failure(
                        &events,
                        status,
                        Fault::new(
                            "operation_timed_out",
                            "The owner operation exceeded five seconds.",
                            true,
                        ),
                    );
                }
            }
            if let Err(fault) = pause(stream.as_raw_fd(), !output.is_empty()) {
                break Some(fault);
            }
        };
        if let Some(fault) = failure {
            fail_pending(&shared, fault);
        }
        match stream.shutdown(Shutdown::Both) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotConnected => {}
            Err(_) => bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                "connection_cleanup",
                "Cannot close the owner connection.",
                true,
            )),
        }
    }
    fail_pending(
        &shared,
        Fault::new(
            "connection_closed",
            "The owner connection was disposed.",
            true,
        ),
    );
}

/// Native-independent connector with typed emission and observed eventual settlement.
pub struct OwnerConnection {
    actions: Registry,
    shared: Arc<ClientShared>,
    worker: Option<JoinHandle<()>>,
}

impl OwnerConnection {
    /// Prepare a lazy launch/connect worker; register observation before the first emission.
    pub fn connect(
        location: OwnerLocation,
        launcher: Arc<dyn OwnerLauncher>,
    ) -> Result<Self, StartupError> {
        let shared = Arc::new(ClientShared::default());
        let mut builder = RegistryBuilder::default();
        fn control<C: OwnerControl>(
            builder: &mut RegistryBuilder,
            shared: Arc<ClientShared>,
        ) -> Result<(), StartupError> {
            builder.on::<C>(move |_, events| {
                let mut state = shared.state.lock().expect("connection state poisoned");
                if shared.stopping.load(Ordering::Acquire)
                    || state.pending.len() >= MAX_OUTSTANDING
                    || state.outgoing.len() >= MAX_QUEUED_FRAMES
                {
                    let status = unavailable_status(state.last_status.clone());
                    drop(state);
                    transport_failure(
                        events,
                        status,
                        Fault::new(
                            "peer_capacity",
                            "The owner connection cannot admit another request.",
                            true,
                        ),
                    );
                } else {
                    let id = events.correlation();
                    state.pending.insert(
                        id,
                        ClientPending {
                            events: events.downgrade(),
                            sent: None,
                        },
                    );
                    state.outgoing.push_back(events.envelope());
                    shared.changed.notify_one();
                }
                Ok(())
            })
        }
        control::<Power::Session::Query>(&mut builder, shared.clone())?;
        control::<Power::Session::TurnOn>(&mut builder, shared.clone())?;
        control::<Power::Session::TurnOff>(&mut builder, shared.clone())?;
        let abandoned = shared.clone();
        builder.on::<Power::Session::Abandoned>(move |notification, events| {
            if notification.correlations.len() > MAX_OUTSTANDING {
                return Err(Fault::new(
                    "invalid_abandon",
                    "Too many abandoned connection requests.",
                    true,
                ));
            }
            let mut state = abandoned.state.lock().expect("connection state poisoned");
            if state.outgoing.len() >= MAX_QUEUED_FRAMES {
                return Err(Fault::new(
                    "abandon_backpressure",
                    "The connection cannot deliver more cancellation metadata.",
                    true,
                ));
            }
            let mut removed = false;
            for id in notification.correlations {
                removed |= state.pending.remove(&id).is_some();
            }
            if removed {
                state.outgoing.push_back(events.envelope());
                abandoned.changed.notify_one();
            }
            Ok(())
        })?;
        let actions = builder.build()?;
        let worker_shared = shared.clone();
        let bus = actions.clone();
        let worker = thread::Builder::new()
            .name("screenwake-connection".into())
            .spawn(move || connection_worker(location, launcher, worker_shared, bus))
            .map_err(|error| StartupError(error.to_string()))?;
        Ok(Self {
            actions,
            shared,
            worker: Some(worker),
        })
    }

    /// Reserve only request metadata; no core state is queried or returned.
    pub fn prepare<C: OwnerControl>(&self) -> Result<PreparedAction<C>, Fault> {
        self.actions.prepare::<C>(Empty {})
    }

    /// Emit after registering interest in the prepared opaque correlation token.
    pub fn emit<C: OwnerControl>(&self, request: PreparedAction<C>) {
        self.actions.emit_prepared(request);
    }

    /// Relinquish this peer's notification/capacity only, never its admitted physical intent.
    pub fn abandon(&self, correlation: CorrelationId) {
        self.actions.emit::<Power::Session::Abandoned>(Abandoned {
            correlations: vec![correlation],
        });
    }

    /// Observe canonical settlements/errors through the same envelope shape as native UI.
    pub fn observe(&self, sink: Arc<dyn ObservationSink>) {
        self.actions.observe(sink);
    }
}

impl Drop for OwnerConnection {
    fn drop(&mut self) {
        {
            let _state = self.shared.state.lock().expect("connection state poisoned");
            self.shared.stopping.store(true, Ordering::Release);
            self.shared.changed.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            if worker.thread().id() != thread::current().id() && worker.join().is_err() {
                self.actions
                    .emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
                        "connection_failed",
                        "The owner connector stopped unexpectedly.",
                        false,
                    ));
            }
        }
    }
}
// end ipc.rs
