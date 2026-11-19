// telemetry.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Owner-only forwarding requirements; HTTP witnesses use owned loopback listeners only.

use ai2screenwake_client::{
    contract::{
        Action, Apply, AssertionsObserved, ClockSample, ControlResult, CorrelationId, Emitter,
        Empty, Error, Fault, IdleObserved, IdleRequest, KvEnvelope, Log, ObservationSink, Outcome,
        Power, Telemetry as TelemetryActions,
    },
    ipc::{OwnerServer, for_current_user},
    runtime::RuntimeOptions,
    telemetry::{AnalyticsDestination, Configuration, Destinations},
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::fd::AsRawFd,
    os::unix::net::UnixStream,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

struct Native;
impl NativePower for Native {
    fn apply(&self, request: Apply, events: Emitter) {
        events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
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
        events.emit::<Power::Activity::System::Observed>(IdleObserved {
            request,
            clock: ClockSample {
                epoch_ms: 101_000,
                monotonic_ms: 2000,
            },
            idle_age_ms: Some(1000),
            fault: None,
        });
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Phase {
    Quit,
    NativeZero,
    Closed,
    Terminal,
    Stopped,
}

fn terminal(entry: &KvEnvelope) -> bool {
    entry
        .payload::<TelemetryActions::Delivery::Forwarding::Confirmed>()
        .unwrap()
        .is_some()
        || entry
            .payload::<Error::Telemetry::Transport::Occurred>()
            .unwrap()
            .is_some()
        || entry
            .payload::<Error::Telemetry::Buffer::Occurred>()
            .unwrap()
            .is_some()
}

struct ForwardingBarrier {
    pending: HashSet<CorrelationId>,
    signal: UnixStream,
}

#[derive(Default)]
struct Sink {
    entries: Mutex<Vec<KvEnvelope>>,
    changed: Condvar,
    times: Mutex<HashMap<(Phase, CorrelationId), Instant>>,
    forwarding: Mutex<Option<ForwardingBarrier>>,
}

impl ObservationSink for Sink {
    fn observed(&self, entry: KvEnvelope) {
        let phase = if terminal(&entry) {
            Some(Phase::Terminal)
        } else if entry
            .payload::<Power::Session::Protection::Quit>()
            .unwrap()
            .is_some()
        {
            Some(Phase::Quit)
        } else if entry
            .payload::<Power::Session::Protection::Closed>()
            .unwrap()
            .is_some()
        {
            Some(Phase::Closed)
        } else if entry
            .payload::<TelemetryActions::Session::Worker::Stopped>()
            .unwrap()
            .is_some()
        {
            Some(Phase::Stopped)
        } else if entry
            .payload::<Power::Assertions::Resources::Observed>()
            .unwrap()
            .is_some_and(|result| {
                !result.request.enabled
                    && result.held_count == 0
                    && result.outcome == Outcome::Applied
                    && result.fault_codes.is_empty()
            })
        {
            Some(Phase::NativeZero)
        } else {
            None
        };
        if let Some(phase) = phase {
            let id = entry.correlation().unwrap();
            self.times
                .lock()
                .unwrap()
                .entry((phase, id.clone()))
                .or_insert_with(Instant::now);
            if phase == Phase::Terminal {
                if let Some(barrier) = self.forwarding.lock().unwrap().as_mut() {
                    if barrier.pending.remove(&id) && barrier.pending.is_empty() {
                        (&barrier.signal).write_all(&[1]).unwrap();
                    }
                }
            }
        }
        self.entries.lock().unwrap().push(entry);
        self.changed.notify_all();
    }
}

impl Sink {
    #[track_caller]
    fn wait(&self, observation: &str, predicate: impl Fn(&[KvEnvelope]) -> bool) {
        let started = Instant::now();
        let (entries, _) = self
            .changed
            .wait_timeout_while(
                self.entries.lock().unwrap(),
                RuntimeOptions::default().operation_timeout,
                |entries| !predicate(entries),
            )
            .unwrap();
        assert!(
            predicate(&entries),
            "expected {observation} at {} after {:?}: {} actions",
            std::panic::Location::caller(),
            started.elapsed(),
            entries.len()
        );
    }

    fn fault(&self, code: &str) -> bool {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .any(|entry| serde_json::to_value(entry).unwrap()["fault_code"] == code)
    }

    fn at(&self, phase: Phase, id: &CorrelationId) -> Instant {
        *self
            .times
            .lock()
            .unwrap()
            .get(&(phase, id.clone()))
            .expect("required correlated phase missing")
    }
}

struct Collector {
    listener: TcpListener,
    requests: Vec<(String, serde_json::Value)>,
    held: Option<TcpStream>,
    status: u16,
}

impl Collector {
    fn new(status: u16) -> Self {
        Self {
            listener: TcpListener::bind(("127.0.0.1", 0)).unwrap(),
            requests: Vec::new(),
            held: None,
            status,
        }
    }

    fn origin(&self) -> String {
        format!("http://{}", self.listener.local_addr().unwrap())
    }

    fn receive(&mut self, hold: bool) -> Instant {
        let mut descriptor = libc::pollfd {
            fd: self.listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one live listener descriptor, with an explicit bounded local-test deadline.
        assert_eq!(
            unsafe { libc::poll(&mut descriptor, 1, 2000) },
            1,
            "real HTTP post missing"
        );
        let (mut stream, _) = self.listener.accept().unwrap();
        let accepted = Instant::now();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut bytes = Vec::new();
        let (path, body) = loop {
            let mut chunk = [0; 1024];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "incomplete local HTTP request");
            bytes.extend_from_slice(&chunk[..count]);
            assert!(bytes.len() <= 16 * 1024);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let header = std::str::from_utf8(&bytes[..end]).unwrap();
                let size: usize = header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + size {
                    break (
                        header
                            .lines()
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .nth(1)
                            .unwrap()
                            .to_string(),
                        serde_json::from_slice(&bytes[end + 4..end + 4 + size]).unwrap(),
                    );
                }
            }
        };
        self.requests.push((path, body));
        if hold {
            self.held = Some(stream);
        } else {
            let reply = format!(
                "HTTP/1.1 {} Result\r\nContent-Length: 2\r\nConnection: close\r\nRetry-After: 999\r\n\r\n{{}}",
                self.status
            );
            stream.write_all(reply.as_bytes()).unwrap();
        }
        accepted
    }

    fn drain(&mut self, finished: &UnixStream) {
        let deadline = Instant::now() + RuntimeOptions::default().operation_timeout;
        loop {
            let mut descriptors = [
                libc::pollfd {
                    fd: finished.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.listener.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("forwarding drain deadline expired");
            let milliseconds = i32::try_from(remaining.as_millis()).unwrap().max(1);
            // SAFETY: two live owned descriptors; the absolute test deadline bounds all I/O.
            assert!(
                unsafe { libc::poll(descriptors.as_mut_ptr(), 2, milliseconds) } > 0,
                "forwarding completion or HTTP request missing"
            );
            if descriptors[0].revents & libc::POLLIN != 0 {
                return;
            }
            assert_ne!(descriptors[1].revents & libc::POLLIN, 0);
            self.receive(false);
        }
    }
}

#[derive(Default, World)]
struct Telemetry {
    root: Option<tempfile::TempDir>,
    server: Option<OwnerServer>,
    sink: Arc<Sink>,
    collector: Option<Collector>,
    last: Option<ControlResult>,
    cleanup_elapsed: Option<Duration>,
    held_source: Option<CorrelationId>,
    quit_source: Option<CorrelationId>,
    held_accepted: Option<Instant>,
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry").finish_non_exhaustive()
    }
}

impl Telemetry {
    fn finish_listener(&mut self, active: bool) {
        struct Disposed(std::sync::mpsc::Sender<()>);
        impl ObservationSink for Disposed {
            fn observed(&self, _: KvEnvelope) {}
        }
        impl Drop for Disposed {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let mut collector = self.collector.take().unwrap();
        let (mut stop, finished) = UnixStream::pair().unwrap();
        let listener = std::thread::spawn(move || {
            loop {
                let mut descriptors = [
                    libc::pollfd {
                        fd: collector.listener.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: finished.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                assert!(
                    unsafe { libc::poll(descriptors.as_mut_ptr(), 2, 5000) } > 0,
                    "listener shutdown deadline"
                );
                if descriptors[0].revents & libc::POLLIN != 0 {
                    collector.receive(false);
                    continue;
                }
                if descriptors[1].revents != 0 {
                    // The preceding poll drained every already-pending connection first.
                    return collector;
                }
            }
        });
        let bus = self.server.as_ref().unwrap().actions();
        let (disposed, disposal) = std::sync::mpsc::channel();
        bus.observe(Arc::new(Disposed(disposed)));
        self.control::<Power::Session::Protection::Quit>();
        self.sink.wait("owner Closed", |entries| {
            entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Protection::Closed>()
                    .unwrap()
                    .is_some()
            })
        });
        self.server.take();
        if active {
            self.sink.wait(
                "forwarding Stopped while the listener remains active",
                |entries| {
                    entries.iter().any(|entry| {
                        entry
                            .payload::<TelemetryActions::Session::Worker::Stopped>()
                            .unwrap()
                            .is_some()
                    })
                },
            );
        }
        drop(bus);
        disposal.recv_timeout(Duration::from_secs(2)).unwrap();
        stop.write_all(&[1]).unwrap();
        self.collector = Some(listener.join().unwrap());
    }

    fn assert_request_count(&mut self, expected: usize, active: bool, permit_quit: bool) {
        self.finish_listener(active);
        let requests = &self.collector.as_ref().unwrap().requests;
        let mut shutdown = 0;
        for action in ["quit", "turn_off"] {
            let count = requests
                .iter()
                .filter(|(path, body)| {
                    path == "/batch/" && body["batch"][0]["properties"]["action"] == action
                })
                .count();
            assert!(
                count <= usize::from(permit_quit),
                "only the admitted Quit and its single derived Off may add analytics"
            );
            shutdown += count;
        }
        assert_eq!(
            requests.len() - shutdown,
            expected,
            "the actual listener observed an unauthorized or recursive POST: {requests:?}"
        );
    }

    fn start(&mut self, configuration: Configuration) {
        let root = Self::owner_directory();
        let server = OwnerServer::bind_with_telemetry(
            for_current_user(root.path().join("o")),
            Arc::new(Native),
            configuration,
        )
        .unwrap();
        server.actions().observe(self.sink.clone());
        self.root = Some(root);
        self.server = Some(server);
    }

    fn owner_directory() -> tempfile::TempDir {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target");
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let directory = Self::owner_directory_in(&parent);
        if parent
            .join("uXXXXXX/o/owner.sock")
            .as_os_str()
            .as_encoded_bytes()
            .len()
            < 104
        {
            assert!(
                directory.path().starts_with(&parent),
                "a fitting worktree fixture must not use the fallback"
            );
        }
        directory
    }

    fn owner_directory_in(preferred: &std::path::Path) -> tempfile::TempDir {
        let parent = if preferred
            .join("uXXXXXX/o/owner.sock")
            .as_os_str()
            .as_encoded_bytes()
            .len()
            < 104
        {
            preferred.to_path_buf()
        } else {
            std::path::PathBuf::from("/tmp")
        };
        std::fs::create_dir_all(&parent).unwrap();
        let directory = tempfile::Builder::new()
            .prefix("u")
            .tempdir_in(parent.canonicalize().unwrap())
            .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            directory
                .path()
                .join("o/owner.sock")
                .as_os_str()
                .as_encoded_bytes()
                .len()
                < 104,
            "the owned forwarding fixture must fit Darwin without changing production validation"
        );
        directory
    }

    fn control<A: Action<Payload = Empty>>(&mut self) -> CorrelationId {
        let bus = self.server.as_ref().unwrap().actions();
        let request = bus.prepare::<A>(Empty {}).unwrap();
        let id = request.correlation();
        bus.emit_prepared(request);
        self.sink.wait(
            &format!("{} settlement for {id:?}", std::any::type_name::<A>()),
            |entries| {
                entries.iter().any(|entry| {
                    entry
                        .payload::<Power::Session::Protection::Settled>()
                        .unwrap()
                        .is_some_and(|result| result.correlation == id)
                })
            },
        );
        self.last = self.sink.entries.lock().unwrap().iter().find_map(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .filter(|result| result.correlation == id)
                .map(|result| result.result)
        });
        id
    }
}

#[given("a real private owner whose checkout cannot fit a Unix socket")]
fn long_checkout_owner(world: &mut Telemetry) {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target")
        .canonicalize()
        .unwrap();
    let long = project.join("x".repeat(120));
    let root = Telemetry::owner_directory_in(&long);
    assert!(!root.path().starts_with(&long));
    assert!(
        root.path()
            .starts_with(std::path::Path::new("/tmp").canonicalize().unwrap())
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        root.path().metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
    let server =
        OwnerServer::bind(for_current_user(root.path().join("o")), Arc::new(Native)).unwrap();
    server.actions().observe(world.sink.clone());
    world.root = Some(root);
    world.server = Some(server);
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        if self.server.is_some() {
            self.control::<Power::Session::Protection::Quit>();
            self.sink.wait("owner Closed", |entries| {
                entries.iter().any(|entry| {
                    entry
                        .payload::<Power::Session::Protection::Closed>()
                        .unwrap()
                        .is_some()
                })
            });
            self.server.take();
        }
        self.collector.take();
        self.root.take();
    }
}

#[given("a real private owner without telemetry configuration")]
fn unconfigured(world: &mut Telemetry) {
    let root = Telemetry::owner_directory();
    let server =
        OwnerServer::bind(for_current_user(root.path().join("o")), Arc::new(Native)).unwrap();
    server.actions().observe(world.sink.clone());
    world.root = Some(root);
    world.server = Some(server);
}

#[when("the owner is queried through Actions")]
fn query(world: &mut Telemetry) {
    world.control::<Power::Session::Protection::Query>();
}

#[then("inactive forwarding has no attention fault or delivery claim and protection stays Off")]
fn inactive_unconfigured(world: &mut Telemetry) {
    let entries = world.sink.entries.lock().unwrap().clone();
    assert!(
        !entries.iter().any(|entry| entry
            .payload::<Error::Telemetry::Configuration::Occurred>()
            .unwrap()
            .is_some()),
        "intentional inactivity is not a configuration failure"
    );
    assert!(
        !entries.iter().any(|entry| entry
            .payload::<TelemetryActions::Delivery::Forwarding::Confirmed>()
            .unwrap()
            .is_some()),
        "inactive forwarding cannot claim delivery"
    );
    let settled = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let result = entries[settled]
        .payload::<Power::Session::Protection::Settled>()
        .unwrap()
        .unwrap()
        .result;
    assert_eq!(result.status.unwrap().is_on, Some(false));
    assert!(result.error.is_none());
}

fn configured(world: &mut Telemetry, status: u16) {
    let collector = Collector::new(status);
    let configuration = Configuration::Authorized(Destinations {
        analytics: Some(AnalyticsDestination {
            host: collector.origin(),
            project_key: "local-project".into(),
        }),
        bug_report_url: Some(format!("{}/bug-report", collector.origin())),
    });
    world.collector = Some(collector);
    world.start(configuration);
}

#[given("a real owner authorized only for an owned loopback collector")]
fn authorized(world: &mut Telemetry) {
    configured(world, 200);
}

#[given("an authorized collector returning retryable failures")]
fn rejected(world: &mut Telemetry) {
    configured(world, 503);
}

#[given("an authorized collector holding its response")]
fn held(world: &mut Telemetry) {
    configured(world, 200);
}

#[when("controls and private recoverable and unrecoverable faults are dispatched")]
fn private_actions(world: &mut Telemetry) {
    world.control::<Power::Session::Protection::TurnOn>();
    let bus = world.server.as_ref().unwrap().actions();
    for recoverable in [true, false] {
        let mut fault = Fault::new(
            "secret-key=/Users/private",
            "credential password=secret-key socket=/private/owner.sock",
            recoverable,
        );
        fault.failed_genus = Some("private-chain-token".into());
        if recoverable {
            bus.emit::<Error::Native::Corners::Occurred>(fault);
        } else {
            bus.emit::<Error::Native::Assertions::Occurred>(fault);
        }
    }
    bus.emit::<Log::Session::History::Query>(Empty {});
    world.control::<Power::Session::Protection::TurnOff>();
}

#[then("the real SDK forwards redacted analytics and only the unrecoverable bug report")]
fn redacted(world: &mut Telemetry) {
    let collector = world.collector.as_mut().unwrap();
    for _ in 0..5 {
        collector.receive(false);
    }
    assert_eq!(
        collector
            .requests
            .iter()
            .filter(|(path, _)| path == "/bug-report")
            .count(),
        1
    );
    let analytics: Vec<_> = collector
        .requests
        .iter()
        .filter(|(path, _)| path == "/batch/")
        .collect();
    assert_eq!(analytics.len(), 4);
    assert!(
        analytics
            .iter()
            .any(
                |(_, body)| body["batch"][0]["properties"]["action"] == "native_corners_failure"
                    && body["batch"][0]["properties"]["recoverable"] == true
            )
    );
    for (_, body) in analytics {
        assert_eq!(body["api_key"], "local-project");
        let event = &body["batch"][0];
        assert_eq!(event["event"], "screenwake_action");
        assert_eq!(event["properties"]["$process_person_profile"], false);
        assert_eq!(event["properties"]["$geoip_disable"], true);
        for key in event["properties"].as_object().unwrap().keys() {
            assert!(
                [
                    "action",
                    "recoverable",
                    "$process_person_profile",
                    "$geoip_disable"
                ]
                .contains(&key.as_str()),
                "SDK enrichment leaked: {key}"
            );
        }
    }
    let text = serde_json::to_string(&collector.requests).unwrap();
    for forbidden in [
        "secret-key",
        "password",
        "/Users",
        "owner.sock",
        "private-chain",
        "failed_genus",
        "$os",
        "$lib",
    ] {
        assert!(
            !text.contains(forbidden),
            "private content leaked: {forbidden}"
        );
    }
    let report = &collector
        .requests
        .iter()
        .find(|(path, _)| path == "/bug-report")
        .unwrap()
        .1;
    assert_eq!(report["recoverable"], false);
    assert_eq!(report.as_object().unwrap().len(), 2);
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
    );
}

#[then("delivery fails explicitly without false confirmation or recursive posts")]
fn transport_failed(world: &mut Telemetry) {
    world.collector.as_mut().unwrap().receive(false);
    world.sink.wait("analytics delivery failure", |entries| {
        entries.iter().any(|entry| {
            serde_json::to_value(entry).unwrap()["fault_code"] == "analytics_delivery_failed"
        })
    });
    assert_eq!(world.collector.as_ref().unwrap().requests.len(), 1);
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
    );
    assert!(world.last.as_ref().unwrap().error.is_none());
    assert!(!world.sink.entries.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<TelemetryActions::Delivery::Forwarding::Confirmed>()
            .unwrap()
            .is_some()
    }));
    world.assert_request_count(1, true, true);
}

#[when("80 owner queries overflow forwarding and Quit cleans up during a fresh held request")]
fn overflow(world: &mut Telemetry) {
    world.control::<Power::Session::Protection::TurnOn>();
    world.collector.as_mut().unwrap().receive(true);
    let bus = world.server.as_ref().unwrap().actions();
    let requests: Vec<_> = (0..80)
        .map(|_| {
            bus.prepare::<Power::Session::Protection::Query>(Empty {})
                .unwrap()
        })
        .collect();
    let ids: HashSet<_> = requests
        .iter()
        .map(|request| request.correlation())
        .collect();
    let (signal, finished) = UnixStream::pair().unwrap();
    *world.sink.forwarding.lock().unwrap() = Some(ForwardingBarrier {
        pending: ids.clone(),
        signal,
    });
    for request in requests {
        bus.emit_prepared(request);
    }
    world.sink.wait("forwarding queue overload", |entries| {
        entries.iter().any(|entry| {
            serde_json::to_value(entry).unwrap()["fault_code"] == "telemetry_queue_full"
        })
    });
    world.collector.as_mut().unwrap().held.take();
    world.collector.as_mut().unwrap().drain(&finished);
    world.sink.forwarding.lock().unwrap().take();
    world
        .sink
        .wait("all 80 correlated query settlements", |entries| {
            ids.iter().all(|id| {
                entries.iter().any(|entry| {
                    entry
                        .payload::<Power::Session::Protection::Settled>()
                        .unwrap()
                        .is_some_and(|settled| {
                            settled.correlation == *id
                                && settled.result.error.is_none()
                                && settled
                                    .result
                                    .status
                                    .is_some_and(|status| status.is_on == Some(true))
                        })
                })
            })
        });
    let held = world.control::<Power::Session::Protection::Query>();
    let accepted = world.collector.as_mut().unwrap().receive(true);
    assert!(
        !world
            .sink
            .times
            .lock()
            .unwrap()
            .contains_key(&(Phase::Terminal, held.clone()))
    );
    let started = Instant::now();
    let quit = world.control::<Power::Session::Protection::Quit>();
    world.sink.wait("overloaded owner Closed", |entries| {
        entries.iter().any(|entry| {
            entry
                .payload::<Power::Session::Protection::Closed>()
                .unwrap()
                .is_some()
        })
    });
    world.cleanup_elapsed = Some(started.elapsed());
    world.held_source = Some(held);
    world.quit_source = Some(quit);
    world.held_accepted = Some(accepted);
}

#[then("overflow and abandonment are explicit and the owned forwarding worker stops")]
fn bounded(world: &mut Telemetry) {
    assert!(world.sink.fault("telemetry_queue_full"));
    world.sink.wait("abandoned forwarding jobs", |entries| {
        entries.iter().any(|entry| {
            serde_json::to_value(entry).unwrap()["fault_code"] == "telemetry_abandoned"
        })
    });
    assert!(world.cleanup_elapsed.unwrap() < RuntimeOptions::default().operation_timeout);
    world.sink.wait("forwarding worker Stopped", |entries| {
        entries.iter().any(|entry| {
            entry
                .payload::<TelemetryActions::Session::Worker::Stopped>()
                .unwrap()
                .is_some()
        })
    });
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
    );
    assert!(world.last.as_ref().unwrap().error.is_none());
    let held = world.held_source.as_ref().unwrap();
    let quit = world.quit_source.as_ref().unwrap();
    let accepted = world.held_accepted.unwrap();
    let admitted = world.sink.at(Phase::Quit, quit);
    let zero = world.sink.at(Phase::NativeZero, quit);
    let closed = world.sink.at(Phase::Closed, quit);
    let terminal = world.sink.at(Phase::Terminal, held);
    let stopped = world.sink.at(Phase::Stopped, quit);
    assert!(accepted <= admitted && admitted < zero && zero < closed);
    assert!(closed < terminal && terminal <= stopped);
    assert!(world.collector.as_ref().unwrap().held.is_some());
    assert!(world.server.is_some());
    let entries = world.sink.entries.lock().unwrap();
    assert!(entries.iter().any(|entry| {
        entry.correlation().unwrap() == *held
            && entry
                .payload::<Error::Telemetry::Transport::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "analytics_delivery_failed")
    }));
    assert!(!entries.iter().any(|entry| {
        entry.correlation().unwrap() == *held
            && entry
                .payload::<TelemetryActions::Delivery::Forwarding::Confirmed>()
                .unwrap()
                .is_some()
    }));
    eprintln!(
        "held-request witness seconds after acceptance: Quit={:.6}, native-zero={:.6}, Closed={:.6}, HTTP-terminal={:.6}, Stopped={:.6}; Quit elapsed={:.6}",
        (admitted - accepted).as_secs_f64(),
        (zero - accepted).as_secs_f64(),
        (closed - accepted).as_secs_f64(),
        (terminal - accepted).as_secs_f64(),
        (stopped - accepted).as_secs_f64(),
        world.cleanup_elapsed.unwrap().as_secs_f64()
    );
}

#[given(regex = r#"^an owner with invalid authorized configuration "([^"]+)"$"#)]
fn invalid(world: &mut Telemetry, kind: String) {
    let collector = Collector::new(200);
    let mut destination = AnalyticsDestination {
        host: collector.origin(),
        project_key: "local-project".into(),
    };
    let mut report = format!("{}/bug-report", collector.origin());
    match kind.as_str() {
        "blank_key" => destination.project_key = " \t".into(),
        "missing_host" => destination.host.clear(),
        "embedded_credentials" => {
            destination.host = "https://private:secret@example.invalid".into()
        }
        "query" => destination.host.push_str("/?token=private"),
        "insecure_remote" => destination.host = "http://example.invalid".into(),
        "wrong_report_path" => report.push_str("/unexpected"),
        _ => panic!("unknown requirement example"),
    }
    world.collector = Some(collector);
    world.start(Configuration::Authorized(Destinations {
        analytics: Some(destination),
        bug_report_url: Some(report),
    }));
}

#[then("invalid forwarding is explicit without a post or changed protection")]
fn invalid_reported(world: &mut Telemetry) {
    assert!(world.sink.fault("telemetry_invalid_configuration"));
    assert!(world.collector.as_ref().unwrap().requests.is_empty());
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
    );
    assert!(world.last.as_ref().unwrap().error.is_none());
    world.assert_request_count(0, false, false);
}

fn report_only(world: &mut Telemetry, status: u16) {
    let collector = Collector::new(status);
    let configuration = Configuration::Authorized(Destinations {
        analytics: None,
        bug_report_url: Some(format!("{}/bug-report", collector.origin())),
    });
    world.collector = Some(collector);
    world.start(configuration);
}

#[given("only unrecoverable reports are authorized for the loopback collector")]
fn reports_authorized(world: &mut Telemetry) {
    report_only(world, 200);
}

#[given("the report-only collector rejects delivery")]
fn reports_rejected(world: &mut Telemetry) {
    report_only(world, 503);
}

#[given("forwarding is explicitly disabled beside an owned collector")]
fn disabled(world: &mut Telemetry) {
    world.collector = Some(Collector::new(200));
    world.start(Configuration::Disabled);
}

#[when("a private unrecoverable source error is dispatched")]
fn fatal(world: &mut Telemetry) {
    world
        .server
        .as_ref()
        .unwrap()
        .actions()
        .emit::<Error::Native::Activity::Occurred>(Fault::new(
            "private-key",
            "private-password /Users/private",
            false,
        ));
}

#[then("exactly the redacted fatal report is accepted and missing analytics stays explicit")]
fn report_accepted(world: &mut Telemetry) {
    world.collector.as_mut().unwrap().receive(false);
    world
        .sink
        .wait("fatal report delivery confirmation", |entries| {
            entries.iter().any(|entry| {
                entry
                    .payload::<TelemetryActions::Delivery::Forwarding::Confirmed>()
                    .unwrap()
                    .is_some()
            })
        });
    assert!(world.sink.fault("analytics_unconfigured"));
    let requests = &world.collector.as_ref().unwrap().requests;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, "/bug-report");
    assert_eq!(
        requests[0].1,
        serde_json::json!({"action":"native_activity_failure","recoverable":false})
    );
    world.assert_request_count(1, true, false);
}

#[then("the fatal report failure is explicit without false delivery or power disruption")]
fn report_failed(world: &mut Telemetry) {
    world.collector.as_mut().unwrap().receive(false);
    world.sink.wait("fatal report delivery failure", |entries| {
        entries.iter().any(|entry| {
            serde_json::to_value(entry).unwrap()["fault_code"] == "bug_report_delivery_failed"
        })
    });
    assert!(!world.sink.entries.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<TelemetryActions::Delivery::Forwarding::Confirmed>()
            .unwrap()
            .is_some()
    }));
    world.control::<Power::Session::Protection::Query>();
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
    );
    assert!(world.last.as_ref().unwrap().error.is_none());
    assert_eq!(world.collector.as_ref().unwrap().requests.len(), 1);
    world.assert_request_count(1, true, false);
}

#[then("disabled forwarding is explicit without a post or delivery confirmation")]
fn disabled_reported(world: &mut Telemetry) {
    inactive_unconfigured(world);
    assert!(world.collector.as_ref().unwrap().requests.is_empty());
    assert!(!world.sink.entries.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<TelemetryActions::Delivery::Forwarding::Confirmed>()
            .unwrap()
            .is_some()
    }));
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
    );
    world.assert_request_count(0, false, false);
}

fn main() {
    futures::executor::block_on(
        Telemetry::cucumber()
            .max_concurrent_scenarios(3)
            .fail_on_skipped()
            .run_and_exit(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/features/telemetry.feature"),
            ),
    );
}
// end telemetry.rs
