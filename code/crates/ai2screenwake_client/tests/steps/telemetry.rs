// telemetry.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Owner-only forwarding requirements; HTTP witnesses use owned loopback listeners only.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, Emitter, Empty, IdleObserved, IdleRequest,
        KvEnvelope, ObservationSink, Outcome, Power,
    },
    ipc::{OwnerServer, for_current_user},
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
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

#[derive(Default, World)]
struct Telemetry {
    root: Option<tempfile::TempDir>,
    server: Option<OwnerServer>,
    sink: Arc<Sink>,
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry").finish_non_exhaustive()
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
    let bus = world.server.as_ref().unwrap().actions();
    bus.emit::<Power::Session::Query>(Empty {});
    let (entries, result) = world
        .sink
        .1
        .wait_timeout_while(
            world.sink.0.lock().unwrap(),
            Duration::from_secs(5),
            |entries| {
                !entries.iter().any(|entry| {
                    entry
                        .payload::<Power::Session::Settled>()
                        .unwrap()
                        .is_some()
                })
            },
        )
        .unwrap();
    assert!(
        !result.timed_out(),
        "the real owner did not settle: {} observations",
        entries.len()
    );
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
