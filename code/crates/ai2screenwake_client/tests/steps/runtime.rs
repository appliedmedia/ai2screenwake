// runtime.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Request and episode requirements at the real typed Actions boundary.

use ai2screenwake_client::{
    contract::{
        Apply, AssertionsObserved, ClockSample, Emitter, Empty, IdleObserved, IdleRequest,
        KvEnvelope, ObservationSink, Outcome, Power, Registry,
    },
    runtime::start,
};
use ai2screenwake_native::NativePower;
use cucumber::{World, given, then, when};
use std::sync::{Arc, Mutex};

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
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

impl Runtime {
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

fn main() {
    futures::executor::block_on(
        Runtime::cucumber()
            .fail_on_skipped()
            .run_and_exit("tests/features/runtime.feature"),
    );
}
// end runtime.rs
