// foundation.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Executable requirements at the canonical projection and Actions boundaries.

use ai2screenwake_client::{
    contract::{
        ActionJournal, Apply, AssertionsObserved, ClockSample, Emitter, Empty, Error, Fault,
        IdleObserved, IdleRequest, JournalHistory, KvEnvelope, Log, LogSnapshot, Minutes,
        ObservationSink, Outcome, Power, Registry, RegistryBuilder, State, Status,
    },
    runtime::start,
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use serde_json::Value;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};

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
    disposed_faults: Vec<bool>,
    malformed_roots: Vec<bool>,
}

impl std::fmt::Debug for Foundation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Foundation")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<KvEnvelope>>, Condvar);

impl ObservationSink for RecordingSink {
    fn observed(&self, envelope: KvEnvelope) {
        self.0.lock().unwrap().push(envelope);
        self.1.notify_all();
    }
}

#[derive(Default)]
struct NativeProbe {
    applies: Mutex<Vec<(Apply, Emitter)>>,
    samples: Mutex<Vec<(IdleRequest, Emitter)>>,
    synchronous: AtomicBool,
}

impl NativePower for NativeProbe {
    fn apply(&self, request: Apply, events: Emitter) {
        if self.synchronous.load(Ordering::SeqCst) {
            events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
                held_count: if request.enabled { 2 } else { 0 },
                request: request.clone(),
                outcome: Outcome::Applied,
                fault_codes: Vec::new(),
                clock: ClockSample {
                    epoch_ms: 1_800_000_000_000,
                    monotonic_ms: 1000,
                },
            });
        }
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
            .find_map(|e| e.payload::<Power::Session::Protection::Snapshot>().unwrap())
            .expect("an observed Snapshot is required")
    }

    fn complete(&self, held_count: u8, mismatch: bool) {
        let (mut request, events) = self.native.applies.lock().unwrap().last().unwrap().clone();
        if mismatch {
            request.generation += 1;
        }
        events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
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
    world
        .bus()
        .emit::<Power::Session::Protection::Query>(Empty {});
    assert_eq!(world.snapshot().is_on, Some(false));
}

#[when("protection is requested")]
fn turn_on(world: &mut Foundation) {
    world
        .bus()
        .emit::<Power::Session::Protection::TurnOn>(Empty {});
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
            e.payload::<Power::Session::Protection::Snapshot>()
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
            .starts_with("Power:Assertions:Resources:Observed:")
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
        .observe_action::<Power::Session::Protection::TurnOn>(|_, events| {
            events.emit::<Power::Session::Protection::Query>(Empty {});
        });
}

#[then("both observers see the same ordered entries exactly once")]
fn ordered(world: &mut Foundation) {
    let first = world.first.0.lock().unwrap();
    let second = world.second.0.lock().unwrap();
    let authorizations = first
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            entry
                .payload::<Power::Corners::Guard::Authorize>()
                .unwrap()
                .map(|authorization| (index, authorization))
        })
        .collect::<Vec<_>>();
    assert_eq!(authorizations.len(), 1);
    let (apply_index, request) = first
        .iter()
        .enumerate()
        .find_map(|(index, entry)| {
            entry
                .payload::<Power::Assertions::Resources::Apply>()
                .unwrap()
                .map(|request| (index, request))
        })
        .unwrap();
    assert_eq!(authorizations[0].1.request.as_ref(), Some(&request));
    assert!(authorizations[0].0 < apply_index);
    let limitations = first
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            entry
                .payload::<Power::Corners::Guard::Changed>()
                .unwrap()
                .map(|status| (index, status))
        })
        .collect::<Vec<_>>();
    assert_eq!(limitations.len(), 1);
    assert!(limitations[0].1.fault.is_none());
    assert!(authorizations[0].0 < limitations[0].0 && limitations[0].0 < apply_index);
    let first = first
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect::<Vec<_>>();
    let second = second
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(first, second);
    assert_eq!(first.len(), 9);
    assert!(
        first[0]["_genus"]
            .as_str()
            .unwrap()
            .starts_with("Power:Session:Protection:TurnOn:")
    );
    assert!(
        first[1]["_genus"]
            .as_str()
            .unwrap()
            .starts_with("Power:Session:Protection:Query:")
    );
    let unique: std::collections::HashSet<_> = first
        .iter()
        .map(|v| v["_genus"].as_str().unwrap())
        .collect();
    assert_eq!(unique.len(), first.len());
}

#[when("protection is turned off before native completion")]
fn opposite(world: &mut Foundation) {
    world
        .bus()
        .emit::<Power::Session::Protection::TurnOff>(Empty {});
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
    world
        .bus()
        .emit::<Power::Session::Protection::Query>(Empty {});
    let (request, events) = world.native.samples.lock().unwrap().last().unwrap().clone();
    events.emit::<Power::Activity::System::Observed>(IdleObserved {
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
    world
        .bus()
        .emit::<Power::Session::Protection::Query>(Empty {});
    let (request, events) = world.native.samples.lock().unwrap().last().unwrap().clone();
    events.emit::<Power::Activity::System::Observed>(IdleObserved {
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
    world.bus().emit::<Log::Session::History::Query>(Empty {});
}

#[then("the log view enumerates every prior dispatch exactly once")]
fn log_snapshot(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let log: LogSnapshot = entries
        .last()
        .unwrap()
        .payload::<Log::Session::History::Snapshot>()
        .unwrap()
        .unwrap();
    let startup_entries = log
        .entries
        .len()
        .checked_sub(entries.len() - 1)
        .expect("every subscribed dispatch must be represented");
    for (index, record) in log.entries.iter().enumerate() {
        assert_eq!(record.sequence, u64::try_from(index).unwrap());
    }
    for (record, observed) in log
        .entries
        .iter()
        .skip(startup_entries)
        .zip(&entries[..entries.len() - 1])
    {
        let actual = serde_json::to_value(observed).unwrap();
        let genus = actual["_genus"].as_str().unwrap();
        let prefix = genus.rsplitn(4, ':').last().unwrap();
        assert!(record.display.starts_with(&format!("{prefix}  [registry ")));
        assert!(!record.display.contains(genus.rsplit(':').nth(2).unwrap()));
    }
}

#[when("an owner is started without native collaborators")]
fn missing_native(world: &mut Foundation) {
    world.startup_failed = start(None).is_err();
}

#[when("a registry is built without its required handler")]
fn missing_handler(world: &mut Foundation) {
    let mut builder = RegistryBuilder::default();
    builder.require::<Power::Assertions::Resources::Apply>();
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
}

#[then("each nonfinite or negative value is rejected")]
fn minutes_rejected(world: &mut Foundation) {
    assert!(world.invalid_minutes_rejected);
}

#[given("a registry with sibling and nested snapshot emissions")]
fn siblings(world: &mut Foundation) {
    let mut builder = RegistryBuilder::default();
    builder
        .on::<Power::Session::Protection::Query>(|_, events| {
            events.emit::<Power::Session::Protection::Changed>(Status::off());
            events.emit::<Power::Session::Protection::Snapshot>(Status::off());
            Ok(())
        })
        .unwrap();
    builder
        .on::<Power::Session::Protection::Changed>(|status, events| {
            events.emit::<Power::Session::Protection::Snapshot>(status);
            Ok(())
        })
        .unwrap();
    let registry = builder.build().unwrap();
    registry.observe(world.first.clone());
    world.bus = Some(registry);
}

#[when("the root query is emitted")]
fn root_query(world: &mut Foundation) {
    world
        .bus()
        .emit::<Power::Session::Protection::Query>(Empty {});
}

#[then("both snapshot continuations have distinct correlation instances")]
fn unique_continuations(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let snapshots: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .payload::<Power::Session::Protection::Snapshot>()
                .unwrap()
                .is_some()
        })
        .map(|entry| {
            serde_json::to_value(entry).unwrap()["_genus"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(snapshots.len(), 2);
    assert_ne!(
        snapshots[0], snapshots[1],
        "sibling branches reused correlation"
    );
}

#[when("repeated and differently typed roots are emitted")]
fn distinct_roots(world: &mut Foundation) {
    root_query(world);
    root_query(world);
    world.bus().emit::<Log::Session::History::Query>(Empty {});
}

#[then("one lifetime identity and distinct root ordinals preserve every continuation")]
fn lifetime_roots(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let queries: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry
                .payload::<Power::Session::Protection::Query>()
                .unwrap()
                .is_some()
                || entry
                    .payload::<Log::Session::History::Query>()
                    .unwrap()
                    .is_some()
        })
        .collect();
    assert_eq!(queries.len(), 3);
    let values: Vec<_> = queries
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    let tails: Vec<Vec<_>> = values
        .iter()
        .map(|v| v["_genus"].as_str().unwrap().rsplitn(4, ':').collect())
        .collect();
    assert!(
        tails.iter().all(|tail| tail[2] == tails[0][2]),
        "ordinary requests must not mint another entity ID: {values:?}"
    );
    assert_eq!(
        tails.iter().map(|tail| tail[1]).collect::<Vec<_>>(),
        ["20001", "20002", "20001"]
    );
    let ids: std::collections::HashSet<_> =
        queries.iter().map(|e| e.correlation().unwrap()).collect();
    assert_eq!(
        ids.len(),
        3,
        "different root types must not share a request"
    );
    let mut legacy = serde_json::to_value(queries[0].correlation().unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("root");
    assert_ne!(
        serde_json::from_value::<ai2screenwake_client::contract::CorrelationId>(legacy).unwrap(),
        queries[0].correlation().unwrap(),
        "absent legacy ordinals must never match upgraded roots"
    );
    let roots: std::collections::HashSet<_> = values
        .iter()
        .map(|v| v["_root"].as_u64().expect("positive root ordinal"))
        .collect();
    assert_eq!(roots.len(), 3);
    assert!(!roots.contains(&0));
    for query in &queries[..2] {
        let id = query.correlation().unwrap();
        let continuations: Vec<_> = entries
            .iter()
            .filter(|e| e.correlation().unwrap() == id)
            .collect();
        assert_eq!(continuations.len(), 4);
        assert!(
            continuations
                .iter()
                .all(|e| serde_json::to_value(e).unwrap()["_root"]
                    == serde_json::to_value(query).unwrap()["_root"])
        );
        let instances: std::collections::HashSet<_> = continuations
            .iter()
            .map(|e| {
                serde_json::to_value(e).unwrap()["_genus"]
                    .as_str()
                    .unwrap()
                    .rsplit(':')
                    .next()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(instances.len(), 4);
    }
}

#[when("two independent registries emit their first query")]
fn independent_registries(world: &mut Foundation) {
    for _ in 0..2 {
        let mut builder = RegistryBuilder::default();
        builder
            .on::<Power::Session::Protection::Query>(|_, _| Ok(()))
            .unwrap();
        let bus = builder.build().unwrap();
        bus.observe(world.first.clone());
        bus.emit::<Power::Session::Protection::Query>(Empty {});
    }
}

#[then("their root correlations and lifetime identities differ")]
fn isolated_registries(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    assert_eq!(entries.len(), 2);
    assert_ne!(
        entries[0].correlation().unwrap(),
        entries[1].correlation().unwrap()
    );
    let values: Vec<_> = entries
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    assert_ne!(
        values[0]["_genus"].as_str().unwrap().rsplit(':').nth(2),
        values[1]["_genus"].as_str().unwrap().rsplit(':').nth(2)
    );
    assert!(
        values
            .iter()
            .all(|v| v["_root"].as_u64().is_some_and(|root| root > 0))
    );
}

#[then("readable entries have four semantic fields and separate compact correlations")]
fn readable_semantics(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let log = entries
        .iter()
        .find_map(|e| e.payload::<Log::Session::History::Snapshot>().unwrap())
        .unwrap();
    assert!(!log.entries.is_empty());
    for record in &log.entries {
        let (semantic, correlation) = record
            .display
            .split_once("  [")
            .expect("correlation must be separate from genus");
        assert_eq!(semantic.split(':').count(), 4, "{semantic}");
        assert!(correlation.starts_with("registry "));
        assert!(
            !record.display.contains('-'),
            "opaque entity IDs do not belong in readable rows"
        );
    }
    assert!(
        log.entries
            .iter()
            .any(|e| e.display.starts_with("Power:Session:Protection:Query  ["))
    );
    assert!(
        log.entries
            .iter()
            .any(|e| e.display.starts_with("Log:Session:History:Query  ["))
    );
}

#[when("malformed root ordinals enter raw ingress")]
fn invalid_roots(world: &mut Foundation) {
    root_query(world);
    let envelope = world.first.0.lock().unwrap()[0].clone();
    for invalid in [
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(null),
        serde_json::json!("2"),
        serde_json::json!(1.5),
    ] {
        let mut value = serde_json::to_value(&envelope).unwrap();
        value["_root"] = invalid;
        let envelope: KvEnvelope = serde_json::from_value(value).unwrap();
        world
            .malformed_roots
            .push(envelope.validate_history().is_err());
        world.bus().emit_envelope(envelope);
    }
    world.bus().emit::<Log::Session::History::Query>(Empty {});
}

#[then("every malformed ordinal is rejected before a later normal log response")]
fn invalid_roots_rejected(world: &mut Foundation) {
    assert_eq!(world.malformed_roots, [true; 5]);
    let entries = world.first.0.lock().unwrap();
    assert_eq!(
        entries
            .iter()
            .filter_map(|e| e
                .payload::<Error::Dispatch::Validation::Occurred>()
                .unwrap())
            .filter(|e| e.fault_code == "invalid_envelope")
            .count(),
        5
    );
    assert!(
        entries
            .last()
            .unwrap()
            .payload::<Log::Session::History::Snapshot>()
            .unwrap()
            .is_some()
    );
}

#[given("an owner with a synchronous native completion")]
fn synchronous(world: &mut Foundation) {
    world.native.synchronous.store(true, Ordering::SeqCst);
    owner(world);
}

#[then("pending is observed before native-confirmed On")]
fn synchronous_order(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let states = entries
        .iter()
        .filter_map(|entry| {
            entry
                .payload::<Power::Session::Protection::Snapshot>()
                .unwrap()
        })
        .map(|status| status.state)
        .collect::<Vec<_>>();
    assert_eq!(states, vec![State::Off, State::Pending, State::On]);
}

#[when("the native result is replayed through raw UI ingress")]
fn forge(world: &mut Foundation) {
    let result = world
        .first
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|entry| {
            entry
                .payload::<Power::Assertions::Resources::Observed>()
                .unwrap()
                .is_some()
        })
        .unwrap()
        .clone();
    world.bus().emit_envelope(result);
}

#[then("the ingress rejects it without another resource operation")]
fn rejected_native(world: &mut Foundation) {
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
    assert_eq!(world.snapshot().is_on, Some(true));
    assert!(world.first.0.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some_and(|fault| {
                fault.fault_code == "inadmissible_action" && fault.failed_genus.is_some()
            })
    }));
}

#[when("a query with unexpected parameters enters raw UI ingress")]
fn malformed_query(world: &mut Foundation) {
    let query = world
        .first
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|entry| {
            entry
                .payload::<Power::Session::Protection::Query>()
                .unwrap()
                .is_some()
        })
        .unwrap()
        .clone();
    let mut value = serde_json::to_value(query).unwrap();
    value["unexpected"] = Value::Bool(true);
    world
        .bus()
        .emit_envelope(serde_json::from_value(value).unwrap());
}

#[then("an attributed invalid-payload error is observed without native work")]
fn invalid_payload(world: &mut Foundation) {
    assert!(world.native.applies.lock().unwrap().is_empty());
    assert!(world.native.samples.lock().unwrap().is_empty());
    assert_eq!(world.snapshot().is_on, Some(false));
    let entries = world.first.0.lock().unwrap();
    let error = entries
        .iter()
        .find_map(|entry| {
            entry
                .payload::<Error::Dispatch::Validation::Occurred>()
                .unwrap()
        })
        .unwrap();
    assert_eq!(error.fault_code, "invalid_payload");
    assert!(
        error.failed_genus.is_some(),
        "triggered error lost the failed action identity"
    );
}

#[when("an exhausted log-query continuation enters the owner")]
fn exhausted_continuation(world: &mut Foundation) {
    world.bus().emit::<Log::Session::History::Query>(Empty {});
    let entries = world.first.0.lock().unwrap();
    let (entries, _) = world
        .first
        .1
        .wait_timeout_while(entries, std::time::Duration::from_secs(2), |entries| {
            !entries.iter().any(|entry| {
                entry
                    .payload::<Log::Session::History::Query>()
                    .unwrap()
                    .is_some()
            })
        })
        .unwrap();
    let envelope = entries
        .iter()
        .find(|entry| {
            entry
                .payload::<Log::Session::History::Query>()
                .unwrap()
                .is_some()
        })
        .unwrap()
        .clone();
    drop(entries);
    let mut value = serde_json::to_value(envelope).unwrap();
    let genus = value["_genus"].as_str().unwrap();
    let (prefix, _) = genus.rsplit_once(':').unwrap();
    let prefix = prefix.to_owned();
    for instance in [u64::MAX, u64::MAX - 1] {
        value["_genus"] = Value::String(format!("{prefix}:{instance}"));
        world
            .bus()
            .emit_envelope(serde_json::from_value(value.clone()).unwrap());
    }
    world.bus().emit::<Log::Session::History::Query>(Empty {});
}

#[then("a validation error and a later normal log response are observed")]
fn exhausted_rejected(world: &mut Foundation) {
    let entries = world.first.0.lock().unwrap();
    let (entries, _) = world
        .first
        .1
        .wait_timeout_while(entries, std::time::Duration::from_secs(2), |entries| {
            entries
                .iter()
                .filter(|entry| {
                    entry
                        .payload::<Log::Session::History::Snapshot>()
                        .unwrap()
                        .is_some()
                })
                .count()
                < 2
        })
        .unwrap();
    assert!(entries.iter().any(|entry| {
        entry
            .payload::<Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some_and(|fault| fault.fault_code == "invalid_envelope")
    }));
    assert!(entries.iter().any(|entry| {
        entry
            .payload::<Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some_and(|fault| fault.fault_code == "instance_exhausted")
    }));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry
                .payload::<Log::Session::History::Snapshot>()
                .unwrap()
                .is_some())
            .count(),
        2
    );
}

struct Journal;
impl ActionJournal for Journal {
    fn history(&self) -> Result<JournalHistory, Fault> {
        Ok(JournalHistory::default())
    }
    fn append(&self, _: u64, _: &KvEnvelope) -> Result<(), Fault> {
        Ok(())
    }
}

#[given("a journal-dispatched Actions registry")]
fn persistent_registry(world: &mut Foundation) {
    let mut builder = RegistryBuilder::default();
    builder.journal(Arc::new(Journal));
    let bus = builder.build().unwrap();
    bus.observe(world.first.clone());
    world.bus = Some(bus);
}

#[when("the last ingress enqueues cleanup errors and immediately drops")]
fn last_ingress_fault(world: &mut Foundation) {
    struct Disposal(std::sync::mpsc::SyncSender<bool>, AtomicBool);
    impl ObservationSink for Disposal {
        fn observed(&self, envelope: KvEnvelope) {
            if envelope
                .payload::<Error::Ipc::Protocol::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "socket_cleanup")
            {
                self.1.store(true, Ordering::SeqCst);
            }
        }
    }
    impl Drop for Disposal {
        fn drop(&mut self) {
            self.0.send(self.1.load(Ordering::SeqCst)).unwrap();
        }
    }
    for _ in 0..32 {
        let mut builder = RegistryBuilder::default();
        builder.journal(Arc::new(Journal));
        let bus = builder.build().unwrap();
        let (sender, disposed) = std::sync::mpsc::sync_channel(1);
        bus.observe(Arc::new(Disposal(sender, AtomicBool::new(false))));
        bus.emit::<Error::Ipc::Protocol::Occurred>(Fault::new(
            "socket_cleanup",
            "Owned endpoint cleanup failed.",
            true,
        ));
        drop(bus);
        world.disposed_faults.push(
            disposed
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
        );
    }
}

#[then("every journal worker observes its final fault before acknowledging disposal")]
fn final_fault_observed(world: &mut Foundation) {
    assert!(
        world.disposed_faults.iter().all(|observed| *observed),
        "queued cleanup errors were lost before actual disposal: {:?}",
        world.disposed_faults
    );
}

fn main() {
    futures::executor::block_on(
        Foundation::cucumber()
            .fail_on_skipped()
            .run_and_exit("tests/features/foundation.feature"),
    );
}
// end foundation.rs
