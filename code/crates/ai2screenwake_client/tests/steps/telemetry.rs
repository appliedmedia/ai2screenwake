// telemetry.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Owner-only forwarding requirements; HTTP witnesses use owned loopback listeners only.

use ai2screenwake_client::{
    contract::{
        Action, Apply, AssertionsObserved, ClockSample, ControlResult, Emitter, Empty, Error,
        Fault, IdleObserved, IdleRequest, KvEnvelope, Log, ObservationSink, Outcome, Power,
        Telemetry as TelemetryActions,
    },
    ipc::{OwnerServer, for_current_user},
    telemetry::{AnalyticsDestination, Configuration, Destinations},
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::fd::AsRawFd,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

struct Native;
impl NativePower for Native {
    fn apply(&self, request: Apply, events: Emitter) {
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
                epoch_ms: 101_000,
                monotonic_ms: 2000,
            },
            idle_age_ms: Some(1000),
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
    fn wait(&self, predicate: impl Fn(&[KvEnvelope]) -> bool) {
        let (entries, _) = self
            .1
            .wait_timeout_while(self.0.lock().unwrap(), Duration::from_secs(5), |entries| {
                !predicate(entries)
            })
            .unwrap();
        assert!(
            predicate(&entries),
            "expected observation missing: {} actions",
            entries.len()
        );
    }

    fn fault(&self, code: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .iter()
            .any(|entry| serde_json::to_value(entry).unwrap()["fault_code"] == code)
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

    fn receive(&mut self, hold: bool) {
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
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry").finish_non_exhaustive()
    }
}

impl Telemetry {
    fn start(&mut self, configuration: Configuration) {
        let root = tempfile::Builder::new().prefix("sw-t-").tempdir().unwrap();
        let server = OwnerServer::bind_with_telemetry(
            for_current_user(root.path().join("owner")),
            Arc::new(Native),
            configuration,
        )
        .unwrap();
        server.actions().observe(self.sink.clone());
        self.root = Some(root);
        self.server = Some(server);
    }

    fn control<A: Action<Payload = Empty>>(&mut self) {
        let bus = self.server.as_ref().unwrap().actions();
        let request = bus.prepare::<A>(Empty {}).unwrap();
        let id = request.correlation();
        bus.emit_prepared(request);
        self.sink.wait(|entries| {
            entries.iter().any(|entry| {
                entry
                    .payload::<Power::Session::Settled>()
                    .unwrap()
                    .is_some_and(|result| result.correlation == id)
            })
        });
        self.last = self.sink.0.lock().unwrap().iter().find_map(|entry| {
            entry
                .payload::<Power::Session::Settled>()
                .unwrap()
                .filter(|result| result.correlation == id)
                .map(|result| result.result)
        });
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        if self.server.is_some() {
            self.control::<Power::Session::Quit>();
            self.sink.wait(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.payload::<Power::Session::Closed>().unwrap().is_some())
            });
            self.server.take();
        }
        self.collector.take();
        self.root.take();
    }
}

#[given("a real private owner without telemetry configuration")]
fn unconfigured(world: &mut Telemetry) {
    let root = tempfile::Builder::new().prefix("sw-t-").tempdir().unwrap();
    let server = OwnerServer::bind(
        for_current_user(root.path().join("owner")),
        Arc::new(Native),
    )
    .unwrap();
    server.actions().observe(world.sink.clone());
    world.root = Some(root);
    world.server = Some(server);
}

#[when("the owner is queried through Actions")]
fn query(world: &mut Telemetry) {
    world.control::<Power::Session::Query>();
}

#[then("unconfigured forwarding is reported before the unchanged Off settlement")]
fn explicit_unconfigured(world: &mut Telemetry) {
    let entries = world.sink.0.lock().unwrap();
    let error = entries
        .iter()
        .position(|entry| {
            serde_json::to_value(entry).unwrap()["fault_code"] == "telemetry_unconfigured"
        })
        .expect("unconfigured telemetry was silently ignored");
    let settled = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Settled>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(error < settled);
    let result = entries[settled]
        .payload::<Power::Session::Settled>()
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
    world.control::<Power::Session::TurnOn>();
    let bus = world.server.as_ref().unwrap().actions();
    for recoverable in [true, false] {
        let mut fault = Fault::new(
            "secret-key=/Users/private",
            "credential password=secret-key socket=/private/owner.sock",
            recoverable,
        );
        fault.failed_genus = Some("private-chain-token".into());
        bus.emit::<Error::Native::Assertions::Occurred>(fault);
    }
    bus.emit::<Log::Session::Query>(Empty {});
    world.control::<Power::Session::TurnOff>();
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
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
    );
}

#[then("delivery fails explicitly without false confirmation or recursive posts")]
fn transport_failed(world: &mut Telemetry) {
    world.collector.as_mut().unwrap().receive(false);
    world.sink.wait(|entries| {
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
    assert!(!world.sink.0.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<TelemetryActions::Delivery::Confirmed>()
            .unwrap()
            .is_some()
    }));
}

#[when("many owner queries overflow forwarding and Quit confirms cleanup")]
fn overflow(world: &mut Telemetry) {
    world.control::<Power::Session::TurnOn>();
    world.collector.as_mut().unwrap().receive(true);
    let bus = world.server.as_ref().unwrap().actions();
    for _ in 0..80 {
        bus.emit::<Power::Session::Query>(Empty {});
    }
    let started = Instant::now();
    world.control::<Power::Session::Quit>();
    world.sink.wait(|entries| {
        entries
            .iter()
            .any(|entry| entry.payload::<Power::Session::Closed>().unwrap().is_some())
    });
    world.cleanup_elapsed = Some(started.elapsed());
}

#[then("overflow and abandonment are explicit and the owned forwarding worker stops")]
fn bounded(world: &mut Telemetry) {
    assert!(world.sink.fault("telemetry_queue_full"));
    world.sink.wait(|entries| {
        entries.iter().any(|entry| {
            serde_json::to_value(entry).unwrap()["fault_code"] == "telemetry_abandoned"
        })
    });
    assert!(world.cleanup_elapsed.unwrap() < Duration::from_secs(5));
    world.sink.wait(|entries| {
        entries.iter().any(|entry| {
            entry
                .payload::<TelemetryActions::Session::Stopped>()
                .unwrap()
                .is_some()
        })
    });
    assert_eq!(
        world.last.as_ref().unwrap().status.as_ref().unwrap().is_on,
        Some(false)
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
