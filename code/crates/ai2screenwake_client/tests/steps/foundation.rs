// foundation.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Executable requirements at the canonical projection and Actions boundaries.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, Emitter, Empty, Error, Fault, IdGeneration,
        IdleObserved, IdleRequest, KvEnvelope, Log, LogSnapshot, Minutes, ObservationSink, Outcome,
        Power, Registry, RegistryBuilder, State, Status,
    },
    runtime::start,
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Default, World)]
struct Foundation {
    status: Option<Status>,
    json: Option<Value>,
    bus: Option<Registry>,
    native: Arc<NativeProbe>,
    first: Arc<RecordingSink>,
    second: Arc<RecordingSink>,
    startup_failed: bool,
    invalid_minutes_rejected: bool,
}

impl std::fmt::Debug for Foundation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Foundation")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<KvEnvelope>>);

impl ObservationSink for RecordingSink {
    fn observed(&self, envelope: KvEnvelope) {
        self.0.lock().unwrap().push(envelope);
    }
}

#[derive(Default)]
struct NativeProbe {
    applies: Mutex<Vec<(Apply, Emitter)>>,
    samples: Mutex<Vec<(IdleRequest, Emitter)>>,
}

impl NativePower for NativeProbe {
    fn apply(&self, request: Apply, events: Emitter) {
        self.applies.lock().unwrap().push((request, events));
    }

    fn sample(&self, request: IdleRequest, events: Emitter) {
        self.samples.lock().unwrap().push((request, events));
    }
}

impl Foundation {
    fn bus(&self) -> &Registry {
        self.bus.as_ref().unwrap()
    }

    fn snapshot(&self) -> Status {
        self.first
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|e| e.payload::<Power::Session::Snapshot>().unwrap())
            .expect("an observed Snapshot is required")
    }

    fn complete(&self, held_count: u8, mismatch: bool) {
        let (mut request, events) = self.native.applies.lock().unwrap().last().unwrap().clone();
        if mismatch {
            request.generation += 1;
        }
        events.emit::<Power::Assertions::Observed>(AssertionsObserved {
            request,
            held_count,
            outcome: Outcome::Applied,
            fault_codes: Vec::new(),
            clock: ClockSample {
                epoch_ms: 1_800_000_000_000,
                monotonic_ms: 1000,
            },
        });
    }
}

#[given("a newly created off projection")]
fn off(world: &mut Foundation) {
    world.status = Some(Status::off());
}

#[when("the projection crosses the JSON boundary")]
fn serialize(world: &mut Foundation) {
    world.json = Some(serde_json::to_value(world.status.as_ref().unwrap()).unwrap());
}

#[then("protection is off without On-only fields")]
fn off_shape(world: &mut Foundation) {
    let json = world.json.as_ref().unwrap();
    assert_eq!(json["is_on"], false);
    for key in [
        "protected_idle_duration_seconds",
        "protected_idle_suppressed_events",
        "suppressed_events_truncated",
    ] {
        assert!(json.get(key).is_none(), "off status exposed {key}: {json}");
    }
}

#[given("an owner with an injected native boundary")]
fn owner(world: &mut Foundation) {
    let bus = start(Some(world.native.clone())).unwrap();
    bus.observe(world.first.clone());
    world.bus = Some(bus);
    world.bus().emit::<Power::Session::Query>(Empty {});
    assert_eq!(world.snapshot().is_on, Some(false));
}

#[when("protection is requested")]
fn turn_on(world: &mut Foundation) {
    world.bus().emit::<Power::Session::TurnOn>(Empty {});
}

#[when("protection is requested twice")]
fn turn_on_twice(world: &mut Foundation) {
    turn_on(world);
    turn_on(world);
}

#[then("one native apply is pending without claiming on")]
fn pending(world: &mut Foundation) {
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
    let status = world.snapshot();
    assert_eq!(status.is_on, None);
    assert!(status.desired_on);
    assert_eq!(status.state, State::Pending);
    assert!(status.operation_id.is_some());
    assert!(
        serde_json::to_value(status)
            .unwrap()
            .get("protected_idle_suppressed_events")
            .is_none()
    );
}

#[when(expr = "native confirms {int} resources")]
fn confirms(world: &mut Foundation, count: u8) {
    world.complete(count, false);
}

#[then("protection is on with empty unmeasured On-only fields")]
fn confirmed_on(world: &mut Foundation) {
    let status = serde_json::to_value(world.snapshot()).unwrap();
    assert_eq!(status["is_on"], true);
    assert_eq!(status["state"], "on");
    assert!(
        status
            .get("protected_idle_duration_seconds")
            .unwrap()
            .is_null()
    );
    assert_eq!(
        status["protected_idle_suppressed_events"],
        serde_json::json!([])
    );
    assert_eq!(status["suppressed_events_truncated"], false);
}

#[then("an attributed error precedes the unknown failed projection")]
fn partial_error(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let error = entries
        .iter()
        .position(|e| {
            e.payload::<Error::Native::Assertions::Occurred>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let failed = entries
        .iter()
        .position(|e| {
            e.payload::<Power::Session::Snapshot>()
                .unwrap()
                .is_some_and(|s| s.state == State::Error)
        })
        .unwrap();
    assert!(error < failed);
    let fault = entries[error]
        .payload::<Error::Native::Assertions::Occurred>()
        .unwrap()
        .unwrap();
    assert!(
        fault
            .failed_genus
            .unwrap()
            .starts_with("Power:Assertions:Observed:")
    );
    drop(entries);
    let status = world.snapshot();
    assert_eq!(status.is_on, None);
    assert!(
        serde_json::to_value(status)
            .unwrap()
            .get("protected_idle_duration_seconds")
            .is_none()
    );
}

#[when("a mismatched native confirmation arrives")]
fn stale(world: &mut Foundation) {
    world.complete(2, true);
}

#[then("protection remains pending and a stale observation error is logged")]
fn stale_error(world: &mut Foundation) {
    assert_eq!(world.snapshot().state, State::Pending);
    assert_eq!(world.snapshot().is_on, None);
    assert!(world.first.0.lock().unwrap().iter().any(|e| {
        e.payload::<Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some_and(|f| f.fault_code == "stale_native_observation")
    }));
}

#[given("two observers with a reentrant query observer")]
fn reentrant(world: &mut Foundation) {
    world.first.0.lock().unwrap().clear();
    world.bus().observe(world.second.clone());
    world
        .bus()
        .observe_action::<Power::Session::TurnOn>(|_, events| {
            events.emit::<Power::Session::Query>(Empty {});
        });
}

#[then("both observers see the same ordered entries exactly once")]
fn ordered(world: &mut Foundation) {
    let first = world.first.0.lock().unwrap();
    let second = world.second.0.lock().unwrap();
    let first = first
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect::<Vec<_>>();
    let second = second
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(first, second);
    assert_eq!(first.len(), 6);
    assert!(
        first[0]["_genus"]
            .as_str()
            .unwrap()
            .starts_with("Power:Session:TurnOn:")
    );
    assert!(
        first[1]["_genus"]
            .as_str()
            .unwrap()
            .starts_with("Power:Session:Query:")
    );
    let unique: std::collections::HashSet<_> = first
        .iter()
        .map(|v| v["_genus"].as_str().unwrap())
        .collect();
    assert_eq!(unique.len(), first.len());
}

#[when("protection is turned off before native completion")]
fn opposite(world: &mut Foundation) {
    world.bus().emit::<Power::Session::TurnOff>(Empty {});
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
}

#[then("the next native apply releases resources without claiming off")]
fn reconcile(world: &mut Foundation) {
    let requests = world.native.applies.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(!requests[1].0.enabled);
    assert!(requests[1].0.generation > requests[0].0.generation);
    assert_eq!(world.snapshot().is_on, None);
    assert!(!world.snapshot().desired_on);
}

#[when(expr = "a query observes {int} milliseconds of OS idle at monotonic time {int}")]
fn idle_sample(world: &mut Foundation, age: u64, now: u64) {
    world.bus().emit::<Power::Session::Query>(Empty {});
    let (request, events) = world.native.samples.lock().unwrap().last().unwrap().clone();
    events.emit::<Power::Activity::Observed>(IdleObserved {
        request,
        clock: ClockSample {
            epoch_ms: 1_800_000_005_500,
            monotonic_ms: now,
        },
        idle_age_ms: Some(age),
        fault: None,
    });
}

#[then(expr = "protected-idle duration is {int} seconds")]
fn duration(world: &mut Foundation, seconds: u64) {
    assert_eq!(
        world.snapshot().protected_idle_duration_seconds,
        Some(seconds)
    );
}

#[when("native idle sampling fails for a query")]
fn sample_failure(world: &mut Foundation) {
    world.bus().emit::<Power::Session::Query>(Empty {});
    let (request, events) = world.native.samples.lock().unwrap().last().unwrap().clone();
    events.emit::<Power::Activity::Observed>(IdleObserved {
        request,
        clock: ClockSample {
            epoch_ms: 1_800_000_005_500,
            monotonic_ms: 6500,
        },
        idle_age_ms: None,
        fault: Some(Fault::new(
            "idle_source_failed",
            "Native idle sampling failed.",
            true,
        )),
    });
}

#[then("protection stays on with null duration and an attributed sampling error")]
fn sample_error(world: &mut Foundation) {
    assert_eq!(world.snapshot().is_on, Some(true));
    assert_eq!(world.snapshot().protected_idle_duration_seconds, None);
    assert!(world.first.0.lock().unwrap().iter().any(|e| {
        e.payload::<Error::Native::Activity::Occurred>()
            .unwrap()
            .is_some_and(|f| f.fault_code == "idle_source_failed" && f.failed_genus.is_some())
    }));
}

#[when("the native UI requests the log view")]
fn log_request(world: &mut Foundation) {
    world.bus().emit::<Log::Session::Query>(Empty {});
}

#[then("the log view enumerates every prior dispatch exactly once")]
fn log_snapshot(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let log: LogSnapshot = entries
        .last()
        .unwrap()
        .payload::<Log::Session::Snapshot>()
        .unwrap()
        .unwrap();
    assert_eq!(log.entries.len(), entries.len() - 1);
    for (index, record) in log.entries.iter().enumerate() {
        assert_eq!(record.sequence, u64::try_from(index).unwrap());
        let actual = serde_json::to_value(&entries[index]).unwrap();
        assert_eq!(record.display, actual["_genus"]);
    }
}

#[when("an owner is started without native collaborators")]
fn missing_native(world: &mut Foundation) {
    world.startup_failed = start(None).is_err();
}

#[when("a registry is built without its required handler")]
fn missing_handler(world: &mut Foundation) {
    let mut builder = RegistryBuilder::default();
    builder.require::<Power::Assertions::Apply>();
    world.startup_failed = builder.build().is_err();
}

#[then("startup explicitly fails")]
fn startup_failed(world: &mut Foundation) {
    assert!(world.startup_failed);
}

#[when("invalid event minutes cross the JSON boundary")]
fn invalid_minutes(world: &mut Foundation) {
    world.invalid_minutes_rejected = [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY]
        .into_iter()
        .all(|v| Minutes::try_from(v).is_err());
    assert!(serde_json::from_str::<Minutes>("-1").is_err());
    assert!(IdGeneration::next_id::<Power::Session::Query>(&Empty {}).is_ok());
}

#[then("each nonfinite or negative value is rejected")]
fn minutes_rejected(world: &mut Foundation) {
    assert!(world.invalid_minutes_rejected);
}

fn main() {
    futures::executor::block_on(
        Foundation::cucumber()
            .fail_on_skipped()
            .run_and_exit("tests/features/foundation.feature"),
    );
}
// end foundation.rs
