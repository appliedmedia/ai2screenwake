// runtime.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Request and episode requirements at the real typed Actions boundary.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, CorrelationId, Emitter, Empty, IdleObserved,
        IdleRequest, KvEnvelope, ObservationSink, Outcome, Power, Registry, Settlement,
    },
    runtime::{RuntimeOptions, start, start_with_options},
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Native {
    applies: Mutex<Vec<(Apply, Emitter)>>,
    samples: Mutex<Vec<(IdleRequest, Emitter)>>,
}

impl NativePower for Native {
    fn apply(&self, request: Apply, events: Emitter) {
        self.applies.lock().unwrap().push((request, events));
    }

    fn sample(&self, request: IdleRequest, events: Emitter) {
        self.samples.lock().unwrap().push((request, events));
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
struct Runtime {
    bus: Option<Registry>,
    native: Arc<Native>,
    sink: Arc<Sink>,
    prior_snapshots: usize,
    correlations: Vec<CorrelationId>,
    scoped: Arc<Sink>,
    disposed: bool,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.native.applies.lock().unwrap().clear();
        self.native.samples.lock().unwrap().clear();
        self.bus.take();
    }
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

impl Runtime {
    fn complete(&self, held: u8) {
        let (request, events) = self.native.applies.lock().unwrap().pop().unwrap();
        events.emit::<Power::Assertions::Observed>(AssertionsObserved {
            request,
            held_count: held,
            outcome: Outcome::Applied,
            fault_codes: Vec::new(),
            clock: ClockSample {
                epoch_ms: 100_000,
                monotonic_ms: 1000,
            },
        });
    }

    fn settlements(&self) -> Vec<Settlement> {
        self.sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|entry| entry.payload::<Power::Session::Settled>().unwrap())
            .collect()
    }

    fn snapshots(&self) -> usize {
        self.sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| {
                entry
                    .payload::<Power::Session::Snapshot>()
                    .unwrap()
                    .is_some()
            })
            .count()
    }
}

#[given("a new shared owner")]
fn new_owner(world: &mut Runtime) {
    let bus = start(Some(world.native.clone())).unwrap();
    bus.observe(world.sink.clone());
    world.bus = Some(bus);
}

#[given("a confirmed On shared owner")]
fn on(world: &mut Runtime) {
    let bus = start(Some(world.native.clone())).unwrap();
    bus.observe(world.sink.clone());
    bus.emit::<Power::Session::TurnOn>(Empty {});
    let (request, events) = world.native.applies.lock().unwrap().pop().unwrap();
    events.emit::<Power::Assertions::Observed>(AssertionsObserved {
        request,
        held_count: 2,
        outcome: Outcome::Applied,
        fault_codes: Vec::new(),
        clock: ClockSample {
            epoch_ms: 100_000,
            monotonic_ms: 1000,
        },
    });
    world.bus = Some(bus);
    world.prior_snapshots = world.snapshots();
}

#[when("two independent queries are sampled in reverse order")]
fn queries(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Session::Query>(Empty {});
    bus.emit::<Power::Session::Query>(Empty {});
    let requests = std::mem::take(&mut *world.native.samples.lock().unwrap());
    assert_eq!(requests.len(), 2);
    for (index, (request, events)) in requests.into_iter().enumerate().rev() {
        let milliseconds = 6000 + u64::try_from(index).unwrap() * 1000;
        events.emit::<Power::Activity::Observed>(IdleObserved {
            request,
            clock: ClockSample {
                epoch_ms: 100_000 + milliseconds,
                monotonic_ms: milliseconds,
            },
            idle_age_ms: Some(milliseconds - 1000),
            fault: None,
        });
    }
}

#[then("both queries publish an ordered observed snapshot")]
fn both(world: &mut Runtime) {
    assert_eq!(
        world.snapshots() - world.prior_snapshots,
        2,
        "one native reply must not replace another query's observation"
    );
}

#[when("two setters share one native completion")]
fn repeated(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    for _ in 0..2 {
        let request = bus.prepare::<Power::Session::TurnOn>(Empty {}).unwrap();
        world.correlations.push(request.correlation());
        bus.emit_prepared(request);
    }
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
    assert!(
        world.settlements().is_empty(),
        "a pending setter fabricated completion"
    );
    world.complete(2);
}

#[then("both distinct setter requests settle successfully")]
fn repeated_settled(world: &mut Runtime) {
    let settled = world.settlements();
    assert_eq!(settled.len(), 2);
    assert_ne!(world.correlations[0], world.correlations[1]);
    for correlation in &world.correlations {
        let result = &settled
            .iter()
            .find(|s| &s.correlation == correlation)
            .unwrap()
            .result;
        assert!(result.error.is_none());
        assert_eq!(result.status.as_ref().unwrap().is_on, Some(true));
    }
}

#[when("two pending On setters are superseded by Off")]
fn superseded(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    for _ in 0..2 {
        let request = bus.prepare::<Power::Session::TurnOn>(Empty {}).unwrap();
        world.correlations.push(request.correlation());
        bus.emit_prepared(request);
    }
    let request = bus.prepare::<Power::Session::TurnOff>(Empty {}).unwrap();
    world.correlations.push(request.correlation());
    bus.emit_prepared(request);
}

#[then("both old setters receive superseded errors and Off is still pending")]
fn superseded_settled(world: &mut Runtime) {
    let results = world.settlements();
    assert_eq!(results.len(), 2);
    for result in results {
        assert!(world.correlations[..2].contains(&result.correlation));
        assert_eq!(
            result.result.error.as_ref().unwrap().fault_code,
            "superseded"
        );
    }
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
}

#[when("a setter expires before its native reply")]
fn expires(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let request = bus.prepare::<Power::Session::TurnOn>(Empty {}).unwrap();
    let correlation = request.correlation();
    bus.emit_prepared(request);
    bus.emit::<Power::Session::Expired>(correlation.clone());
    world.correlations.push(correlation);
    world.complete(2);
}

#[then("that setter has exactly one attributed timeout result")]
fn expired_once(world: &mut Runtime) {
    let results = world.settlements();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].correlation, world.correlations[0]);
    assert_eq!(
        results[0].result.error.as_ref().unwrap().fault_code,
        "operation_timed_out"
    );
    assert!(world.sink.0.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<ai2screenwake_client::contract::Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some_and(|fault| {
                fault.fault_code == "operation_timed_out"
                    && fault
                        .failed_genus
                        .as_ref()
                        .is_some_and(|s| s.starts_with("Power:Session:TurnOn:"))
            })
    }));
}

#[when("a scoped result observer is dropped before a query")]
fn unsubscribe(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let subscription = bus.subscribe(world.scoped.clone());
    drop(subscription);
    bus.emit::<Power::Session::Query>(Empty {});
}

#[then("it receives no later query entries")]
fn unsubscribed(world: &mut Runtime) {
    assert!(world.scoped.0.lock().unwrap().is_empty());
}

#[given("an owner with a 20 millisecond operation deadline")]
fn short_deadline(world: &mut Runtime) {
    let bus = start_with_options(
        Some(world.native.clone()),
        RuntimeOptions {
            operation_timeout: Duration::from_millis(20),
        },
    )
    .unwrap();
    bus.observe(world.sink.clone());
    world.bus = Some(bus);
}

#[when("protection is requested without a native reply")]
fn no_reply(world: &mut Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::TurnOn>(Empty {});
}

#[then("the scheduler emits one timeout result within one second")]
fn real_expiry(world: &mut Runtime) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while world.settlements().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let settled = world.settlements();
    assert_eq!(settled.len(), 1);
    assert_eq!(
        settled[0].result.error.as_ref().unwrap().fault_code,
        "operation_timed_out"
    );
}

#[when("its native callback and last owner port are dropped while pending")]
fn dispose(world: &mut Runtime) {
    let bus = world.bus.take().unwrap();
    bus.emit::<Power::Session::TurnOn>(Empty {});
    let (_, events) = world.native.applies.lock().unwrap().pop().unwrap();
    let notification = events.downgrade();
    drop(events);
    drop(bus);
    world.disposed = notification.upgrade().is_none();
}

#[then("the notification continuation cannot resurrect that owner")]
fn disposed(world: &mut Runtime) {
    assert!(world.disposed);
}

#[when("Quit is requested and native cleanup confirms zero resources")]
fn quit(world: &mut Runtime) {
    world.sink.0.lock().unwrap().clear();
    let bus = world.bus.as_ref().unwrap();
    let request = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
    world.correlations.push(request.correlation());
    bus.emit_prepared(request);
    assert!(world.settlements().is_empty());
    assert!(
        !world
            .native
            .applies
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .0
            .enabled
    );
    world.complete(0);
}

#[then("Quit has one confirmed Off result")]
fn quit_off(world: &mut Runtime) {
    let settled = world.settlements();
    assert_eq!(settled.len(), 1);
    assert_eq!(settled[0].correlation, world.correlations[0]);
    assert!(settled[0].result.error.is_none());
    assert_eq!(
        settled[0].result.status.as_ref().unwrap().is_on,
        Some(false)
    );
}

#[when("the native toggle is emitted twice before native completion")]
fn toggles(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Session::Toggle>(Empty {});
    bus.emit::<Power::Session::Toggle>(Empty {});
}

#[then("typed On and Off intents are logged with only one physical apply pending")]
fn toggled_intent(world: &mut Runtime) {
    let entries = world.sink.0.lock().unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.payload::<Power::Session::TurnOn>().unwrap().is_some())
            .count(),
        1
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry
                .payload::<Power::Session::TurnOff>()
                .unwrap()
                .is_some())
            .count(),
        1
    );
    let status = entries
        .iter()
        .rev()
        .find_map(|entry| entry.payload::<Power::Session::Snapshot>().unwrap())
        .unwrap();
    assert!(!status.desired_on);
    assert_eq!(status.is_on, None);
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
}

fn main() {
    futures::executor::block_on(Runtime::cucumber().fail_on_skipped().run_and_exit(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/features/runtime.feature"
    )));
}
// end runtime.rs
