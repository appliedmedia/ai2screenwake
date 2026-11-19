// runtime.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Request and episode requirements at the real typed Actions boundary.

use ai2screenwake_client::{
    contract::{
        ActionJournal, Apply, AssertionsObserved, CancellationObserved, ClockSample, CornerLoss,
        CorrelationId, Emitter, Empty, FailedApply, Fault, IdGeneration, IdleObserved, IdleRequest,
        InterruptionObserved, JournalHistory, KvEnvelope, Log, LogSnapshot, ObservationSink,
        Outcome, Power, Registry, Settlement, SleepPhase, Status, SuppressionKind,
        SuppressionRequest, WeakEmitter,
    },
    runtime::{RuntimeOptions, start, start_with_journal, start_with_options},
};
use ai2screenwake_native::{
    NativePower,
    power::{AssertionKind, MacPower, PowerApi},
};
use cucumber::{World, given, then, when};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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

#[derive(Default)]
struct ClockCalls {
    clocks: usize,
    fail_at: usize,
    acquired: u32,
    held: Vec<u32>,
    released: Vec<u32>,
    fail_display: bool,
    fail_release: bool,
}

#[derive(Default)]
struct ClockApi(Mutex<ClockCalls>);

impl PowerApi for ClockApi {
    fn acquire(&self, kind: AssertionKind) -> Result<u32, Fault> {
        let mut calls = self.0.lock().unwrap();
        if kind == AssertionKind::Display && calls.fail_display {
            return Err(Fault::new(
                "fixture_display_acquisition",
                "The controlled display assertion could not be acquired.",
                true,
            ));
        }
        calls.acquired += 1;
        let id = calls.acquired;
        calls.held.push(id);
        Ok(id)
    }

    fn release(&self, id: u32) -> Result<(), Fault> {
        let mut calls = self.0.lock().unwrap();
        if calls.fail_release {
            return Err(Fault::new(
                "fixture_rollback",
                "The controlled rollback could not release its assertion.",
                true,
            ));
        }
        assert!(
            calls.held.contains(&id),
            "only retained IDs may be released"
        );
        calls.held.retain(|held| *held != id);
        calls.released.push(id);
        Ok(())
    }

    fn clock(&self) -> Result<ClockSample, Fault> {
        let mut calls = self.0.lock().unwrap();
        calls.clocks += 1;
        if calls.clocks == calls.fail_at {
            return Err(Fault::new(
                "test_apply_clock",
                "Controlled clock fault.",
                true,
            ));
        }
        Ok(ClockSample {
            epoch_ms: 1_800_000_000_000 + calls.clocks as u64,
            monotonic_ms: 1000 + calls.clocks as u64,
        })
    }

    fn idle_seconds(&self) -> Result<f64, Fault> {
        Ok(1.0)
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
    active: Option<Apply>,
    native_events: Option<WeakEmitter>,
    proof_guard: Option<IdleRequest>,
    proof_sequence: u64,
    count_before_reset: usize,
    untruncated_at_64: bool,
    expected_proof_error: Option<&'static str>,
    journal_failure: Option<FailureAt>,
    log_view: Option<LogSnapshot>,
    clock_api: Arc<ClockApi>,
    failed_apply: Option<Apply>,
}

#[derive(Clone, Copy)]
enum FailureAt {
    First,
    Apply,
    Settlement,
    LogView,
}

struct FailingJournal(FailureAt);

impl ActionJournal for FailingJournal {
    fn history(&self) -> Result<JournalHistory, Fault> {
        Ok(JournalHistory::default())
    }
    fn append(&self, _: u64, envelope: &KvEnvelope) -> Result<(), Fault> {
        let fail = match self.0 {
            FailureAt::First => true,
            FailureAt::Apply => envelope
                .payload::<Power::Assertions::Resources::Apply>()?
                .is_some_and(|request| request.enabled),
            FailureAt::Settlement => envelope
                .payload::<Power::Session::Protection::Settled>()?
                .is_some(),
            FailureAt::LogView => envelope
                .payload::<Log::Session::History::Snapshot>()?
                .is_some(),
        };
        if fail {
            Err(Fault::new(
                "injected_journal_failure",
                "Injected durable append failed.",
                false,
            ))
        } else {
            Ok(())
        }
    }
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
    fn wait_result(&self, correlation: &CorrelationId) -> Settlement {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = self
                .settlements()
                .into_iter()
                .find(|result| &result.correlation == correlation)
            {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "the owner did not report its durable append failure"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn proof_request(&mut self, at: u64, anchor: u64) -> SuppressionRequest {
        if self.proof_guard.is_none() {
            self.proof_guard = Some(self.sample(6000, 5000));
        }
        self.proof_sequence += 1;
        SuppressionRequest {
            request: self.proof_guard.as_ref().unwrap().clone(),
            native_request_id: IdGeneration::next_id::<Power::Session::Protection::Query>(
                &Empty {},
            )
            .unwrap(),
            sequence: self.proof_sequence,
            kind: SuppressionKind::IdleSleepRequest,
            clock: ClockSample {
                epoch_ms: 100_000 + at,
                monotonic_ms: at,
            },
            idle_age_ms: at - anchor,
        }
    }

    fn acknowledge(
        &self,
        request: &SuppressionRequest,
        at: u64,
        epoch: u64,
        anchor: u64,
        cancelled: bool,
    ) {
        self.native_events
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .emit::<Power::Suppression::Evidence::Recorded>(CancellationObserved {
            request: request.request.clone(),
            native_request_id: request.native_request_id.clone(),
            cancellation_id: IdGeneration::next_id::<Power::Session::Protection::Query>(&Empty {})
                .unwrap(),
            clock: ClockSample {
                epoch_ms: epoch,
                monotonic_ms: at,
            },
            idle_age_ms: at - anchor,
            cancelled,
        });
    }

    fn proof(&mut self, at: u64, epoch: u64, anchor: u64, requested: bool, cancelled: bool) {
        let request = self.proof_request(at - 1, anchor);
        if requested {
            self.native_events
                .as_ref()
                .unwrap()
                .upgrade()
                .unwrap()
                .emit::<Power::Suppression::Evidence::Requested>(request.clone());
        }
        self.acknowledge(&request, at, epoch, anchor, cancelled);
    }

    fn has_error(&self, code: &str) -> bool {
        self.sink.0.lock().unwrap().iter().any(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Error::Dispatch::Validation::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == code && fault.failed_genus.is_some())
        })
    }

    fn status(&self) -> Status {
        self.sink
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|entry| {
                entry
                    .payload::<Power::Session::Protection::Snapshot>()
                    .unwrap()
            })
            .unwrap()
    }

    fn sample(&self, now: u64, idle: u64) -> IdleRequest {
        self.bus
            .as_ref()
            .unwrap()
            .emit::<Power::Session::Protection::Query>(Empty {});
        let (request, events) = self.native.samples.lock().unwrap().pop().unwrap();
        events.emit::<Power::Activity::System::Observed>(IdleObserved {
            request: request.clone(),
            clock: ClockSample {
                epoch_ms: 100_000 + now,
                monotonic_ms: now,
            },
            idle_age_ms: Some(idle),
            fault: None,
        });
        request
    }

    fn interruption(&self, phase: SleepPhase, now: u64, stale: bool) {
        let mut request = self.active.as_ref().unwrap().clone();
        if stale {
            request.generation += 1;
        }
        self.native_events
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .emit::<Power::Activity::System::Interrupted>(InterruptionObserved {
                request,
                clock: ClockSample {
                    epoch_ms: 100_000 + now,
                    monotonic_ms: now,
                },
                phase,
                held_count: 2,
            });
    }

    fn complete(&self, held: u8) {
        let (request, events) = self.native.applies.lock().unwrap().pop().unwrap();
        events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
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
            .filter_map(|entry| {
                entry
                    .payload::<Power::Session::Protection::Settled>()
                    .unwrap()
            })
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
                    .payload::<Power::Session::Protection::Snapshot>()
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

#[given("an owner whose native display acquisition and rollback both fail")]
fn first_native_fault_owner(world: &mut Runtime) {
    {
        let mut calls = world.clock_api.0.lock().unwrap();
        calls.fail_display = true;
        calls.fail_release = true;
    }
    let bus = start(Some(Arc::new(MacPower::with_api(world.clock_api.clone())))).unwrap();
    bus.observe(world.sink.clone());
    world.bus = Some(bus);
}

#[when("the actual native acquisition fails and its result is queried twice")]
fn first_native_fault(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    for query in [false, true, true] {
        if query {
            bus.emit::<Power::Session::Protection::Query>(Empty {});
        } else {
            bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
        }
    }
}

#[then("every result retains the first actual fault and explicit Off can recover")]
fn first_native_fault_retained(world: &mut Runtime) {
    let results = world.settlements();
    assert_eq!(results.len(), 3);
    for result in results {
        let fault = result.result.error.unwrap();
        assert_eq!(fault.fault_code, "fixture_display_acquisition");
        assert_eq!(
            fault.message,
            "The controlled display assertion could not be acquired."
        );
        assert!(fault.recoverable);
        assert_eq!(result.result.status.unwrap().is_on, None);
    }
    assert_eq!(world.clock_api.0.lock().unwrap().held.len(), 1);
    let entries = world.sink.0.lock().unwrap();
    let first = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Error::Native::Assertions::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "fixture_display_acquisition")
        })
        .unwrap();
    let rollback = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Error::Native::Assertions::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "fixture_rollback")
        })
        .unwrap();
    let settled = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(first < rollback && rollback < settled);
    drop(entries);
    world.clock_api.0.lock().unwrap().fail_release = false;
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Session::Protection::TurnOff>(Empty {});
    bus.emit::<Power::Session::Protection::Query>(Empty {});
    let recovered = world.settlements();
    assert_eq!(recovered.len(), 5);
    for result in recovered.into_iter().skip(3) {
        assert!(result.result.error.is_none());
        assert_eq!(result.result.status.unwrap().is_on, Some(false));
    }
    assert!(world.clock_api.0.lock().unwrap().held.is_empty());
}

#[when("two native acquisitions fail with different causes and the first failure is repeated")]
fn separate_native_faults(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let mut previous = None;
    for code in ["fixture_old_cause", "corner_pointer_inside"] {
        bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
        let (request, events) = world.native.applies.lock().unwrap().pop().unwrap();
        let failure = FailedApply {
            request,
            held_count: 0,
            fault: Fault::new(code, "The current controlled failure.", true),
        };
        events.error::<ai2screenwake_client::contract::Error::Native::Assertions::Occurred>(
            failure.fault.clone(),
        );
        events.emit::<Power::Assertions::Resources::Failed>(failure.clone());
        if let Some((old, old_events)) = previous.take() {
            let old_events: Emitter = old_events;
            old_events.emit::<Power::Assertions::Resources::Failed>(old);
        }
        previous = Some((failure, events));
    }
    bus.emit::<Power::Session::Protection::Query>(Empty {});
    bus.emit::<Power::Session::Protection::Query>(Empty {});
}

#[then("subsequent queries retain only the current operation failure")]
fn current_native_fault_retained(world: &mut Runtime) {
    let results = world.settlements();
    assert_eq!(results.len(), 4);
    for result in results.into_iter().skip(1) {
        assert_eq!(
            result.result.error.unwrap().fault_code,
            "corner_pointer_inside"
        );
        let status = result.result.status.unwrap();
        assert_eq!(status.is_on, None);
        assert_eq!(status.state, ai2screenwake_client::contract::State::Error);
    }
    assert!(world.has_error("stale_native_failure"));
}

#[given("a confirmed On shared owner")]
fn on(world: &mut Runtime) {
    let bus = start(Some(world.native.clone())).unwrap();
    bus.observe(world.sink.clone());
    bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
    let (request, events) = world.native.applies.lock().unwrap().pop().unwrap();
    world.active = Some(request.clone());
    world.native_events = Some(events.downgrade());
    events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
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

#[then("exact corner admission is logged while outward protection remains pending")]
fn corner_admission(world: &mut Runtime) {
    let (request, _) = world.native.applies.lock().unwrap().last().unwrap().clone();
    let entries = world.sink.0.lock().unwrap();
    let authorization = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Corners::Guard::Authorize>()
                .unwrap()
                .is_some_and(|authority| authority.request.as_ref() == Some(&request))
        })
        .unwrap();
    let apply = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Assertions::Resources::Apply>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(authorization < apply);
    assert!(!entries.iter().any(|entry| {
        entry
            .payload::<Power::Session::Protection::Snapshot>()
            .unwrap()
            .is_some_and(|status| status.is_on == Some(true))
    }));
    drop(entries);
    assert_eq!(world.status().is_on, None);
    world.complete(2);
    assert_eq!(world.status().is_on, Some(true));
}

fn emit_corner_loss(events: &Emitter, request: Apply) {
    let fault = Fault::new("test_corner_source", "Controlled corner source loss.", true);
    events.error::<ai2screenwake_client::contract::Error::Native::Assertions::Occurred>(
        fault.clone(),
    );
    events.emit::<Power::Corners::Guard::Lost>(CornerLoss { request, fault });
}

#[when("an old enabled native operation is offered after confirmed Off")]
fn obsolete_native_operation(world: &mut Runtime) {
    let old = world.active.clone().unwrap();
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Session::Protection::TurnOff>(Empty {});
    world.complete(0);
    assert_eq!(world.status().is_on, Some(false));
    bus.emit::<Power::Assertions::Resources::Apply>(old);
}

#[then("no obsolete physical work starts and a newer explicit On remains admissible")]
fn obsolete_native_rejected(world: &mut Runtime) {
    assert!(
        world.native.applies.lock().unwrap().is_empty(),
        "obsolete Apply reached the physical collaborator"
    );
    let entries = world.sink.0.lock().unwrap();
    let error = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Error::Native::Assertions::Occurred>()
                .unwrap()
                .is_some_and(|fault| fault.fault_code == "native_apply_superseded")
        })
        .unwrap();
    let rejected = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Assertions::Resources::Rejected>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(error < rejected);
    drop(entries);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOn>(Empty {});
    world.complete(2);
    assert_eq!(world.status().is_on, Some(true));
}

#[when("corner loss occurs between valid sleep and wake")]
fn corner_loss_sleep_wake(world: &mut Runtime) {
    world.interruption(SleepPhase::WillSleep, 3000, false);
    let events = world.native_events.as_ref().unwrap().upgrade().unwrap();
    emit_corner_loss(&events, world.active.clone().unwrap());
    world.interruption(SleepPhase::DidWake, 4000, false);
}

#[then("the lost request stays failed and a new explicit On receives current authorization")]
fn recovered_after_lost_wake(world: &mut Runtime) {
    assert_ne!(world.status().is_on, Some(true));
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOn>(Empty {});
    let request = world
        .native
        .applies
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .0
        .clone();
    assert!(
        world.sink.0.lock().unwrap().iter().any(|entry| entry
            .payload::<Power::Corners::Guard::Authorize>()
            .unwrap()
            .is_some_and(|authorization| authorization.request.as_ref() == Some(&request))),
        "valid wake left the new explicit acquisition permanently sleeping"
    );
    world.complete(2);
    assert_eq!(world.status().is_on, Some(true));
}

#[when("corner source loss precedes its acquisition acknowledgement")]
fn corner_loss_before_apply(world: &mut Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOn>(Empty {});
    let (request, events) = world.native.applies.lock().unwrap().last().unwrap().clone();
    emit_corner_loss(&events, request);
    world.complete(2);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Query>(Empty {});
}

#[when("current corner protection is lost before a completed wake")]
fn corner_loss_before_wake(world: &mut Runtime) {
    let events = world.native_events.as_ref().unwrap().upgrade().unwrap();
    emit_corner_loss(&events, world.active.clone().unwrap());
    world.interruption(SleepPhase::DidWake, 3000, false);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Query>(Empty {});
}

#[when("an old corner generation reports source loss")]
fn stale_corner_loss(world: &mut Runtime) {
    let events = world.native_events.as_ref().unwrap().upgrade().unwrap();
    let mut request = world.active.clone().unwrap();
    request.generation += 1;
    emit_corner_loss(&events, request);
}

#[then("stale corner loss leaves current protection and its result unchanged")]
fn stale_corner_ignored(world: &mut Runtime) {
    assert_eq!(world.status().is_on, Some(true));
    assert_eq!(
        world.status().state,
        ai2screenwake_client::contract::State::On
    );
    assert_eq!(world.settlements().len(), 1);
    assert!(world.settlements()[0].result.error.is_none());
}

#[when("corner protection is lost while explicit Off is pending")]
fn corner_loss_during_off(world: &mut Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOff>(Empty {});
    let events = world.native_events.as_ref().unwrap().upgrade().unwrap();
    emit_corner_loss(&events, world.active.clone().unwrap());
}

#[when("two independent queries are sampled in reverse order")]
fn queries(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Session::Protection::Query>(Empty {});
    bus.emit::<Power::Session::Protection::Query>(Empty {});
    let requests = std::mem::take(&mut *world.native.samples.lock().unwrap());
    assert_eq!(requests.len(), 2);
    for (index, (request, events)) in requests.into_iter().enumerate().rev() {
        let milliseconds = 6000 + u64::try_from(index).unwrap() * 1000;
        events.emit::<Power::Activity::System::Observed>(IdleObserved {
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
        let request = bus
            .prepare::<Power::Session::Protection::TurnOn>(Empty {})
            .unwrap();
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
        let request = bus
            .prepare::<Power::Session::Protection::TurnOn>(Empty {})
            .unwrap();
        world.correlations.push(request.correlation());
        bus.emit_prepared(request);
    }
    let request = bus
        .prepare::<Power::Session::Protection::TurnOff>(Empty {})
        .unwrap();
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
    let request = bus
        .prepare::<Power::Session::Protection::TurnOn>(Empty {})
        .unwrap();
    let correlation = request.correlation();
    bus.emit_prepared(request);
    bus.emit::<Power::Session::Protection::Expired>(correlation.clone());
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
                        .is_some_and(|s| s.starts_with("Power:Session:Protection:TurnOn:"))
            })
    }));
}

#[when("a scoped result observer is dropped before a query")]
fn unsubscribe(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let subscription = bus.subscribe(world.scoped.clone());
    drop(subscription);
    bus.emit::<Power::Session::Protection::Query>(Empty {});
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
        .emit::<Power::Session::Protection::TurnOn>(Empty {});
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
    bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
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
    let request = bus
        .prepare::<Power::Session::Protection::Quit>(Empty {})
        .unwrap();
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
    bus.emit::<Power::Session::Protection::Toggle>(Empty {});
    bus.emit::<Power::Session::Protection::Toggle>(Empty {});
}

#[then("typed On and Off intents are logged with only one physical apply pending")]
fn toggled_intent(world: &mut Runtime) {
    let entries = world.sink.0.lock().unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry
                .payload::<Power::Session::Protection::TurnOn>()
                .unwrap()
                .is_some())
            .count(),
        1
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry
                .payload::<Power::Session::Protection::TurnOff>()
                .unwrap()
                .is_some())
            .count(),
        1
    );
    let status = entries
        .iter()
        .rev()
        .find_map(|entry| {
            entry
                .payload::<Power::Session::Protection::Snapshot>()
                .unwrap()
        })
        .unwrap();
    assert!(!status.desired_on);
    assert_eq!(status.is_on, None);
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
}

#[when("native sleep at 5000 and wake at 10000 are observed")]
fn sleep_wake(world: &mut Runtime) {
    world.interruption(SleepPhase::WillSleep, 5000, false);
    world.interruption(SleepPhase::DidWake, 10000, false);
}

#[when(expr = "a fresh query samples idle age {int} at monotonic time {int}")]
fn fresh_idle(world: &mut Runtime, idle: u64, now: u64) {
    world.sample(now, idle);
}

#[then("protected-idle duration is exactly 1 second")]
fn sleep_duration(world: &mut Runtime) {
    assert_eq!(world.status().protected_idle_duration_seconds, Some(1));
}

#[when("a query is pending as native sleep is observed")]
fn interrupted_query(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let query = bus
        .prepare::<Power::Session::Protection::Query>(Empty {})
        .unwrap();
    world.correlations.push(query.correlation());
    bus.emit_prepared(query);
    assert_eq!(world.native.samples.lock().unwrap().len(), 1);
    world.interruption(SleepPhase::WillSleep, 5000, false);
}

#[then("the pending query receives one explicit system-sleep error")]
fn interrupted_result(world: &mut Runtime) {
    let results = world.settlements();
    let query = world.correlations.last().unwrap();
    let results: Vec<_> = results
        .iter()
        .filter(|result| &result.correlation == query)
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].result.error.as_ref().unwrap().fault_code,
        "system_sleep"
    );
}

#[when("native sleep carries a different operation generation")]
fn stale_sleep(world: &mut Runtime) {
    world.interruption(SleepPhase::WillSleep, 5000, true);
}

#[then("an attributed stale-interruption error leaves protection On")]
fn stale_sleep_result(world: &mut Runtime) {
    assert_eq!(world.status().is_on, Some(true));
    assert!(world.sink.0.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<ai2screenwake_client::contract::Error::Dispatch::Validation::Occurred>()
            .unwrap()
            .is_some_and(|fault| {
                fault.fault_code == "stale_interruption" && fault.failed_genus.is_some()
            })
    }));
}

struct QuitBeforeSetter {
    bus: Registry,
    fired: AtomicBool,
}

impl ObservationSink for QuitBeforeSetter {
    fn observed(&self, envelope: KvEnvelope) {
        if envelope
            .payload::<Power::Session::Protection::Toggle>()
            .unwrap()
            .is_some()
            && !self.fired.swap(true, Ordering::SeqCst)
        {
            self.bus.emit::<Power::Session::Protection::Quit>(Empty {});
        }
    }
}

#[when("Quit is queued before a Toggle emits its derived setter")]
fn queued_toggle(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let subscription = bus.subscribe(Arc::new(QuitBeforeSetter {
        bus: bus.clone(),
        fired: AtomicBool::new(false),
    }));
    bus.emit::<Power::Session::Protection::Toggle>(Empty {});
    drop(subscription);
}

#[then("the derived enable is rejected and no native resource is acquired")]
fn queued_toggle_rejected(world: &mut Runtime) {
    assert!(world.native.applies.lock().unwrap().is_empty());
    assert_eq!(world.status().is_on, Some(false));
    assert!(world.settlements().iter().any(|settled| {
        settled
            .result
            .error
            .as_ref()
            .is_some_and(|fault| fault.fault_code == "owner_closing")
    }));
}

#[when(expr = "a matched native cancellation occurs at monotonic time {int} and epoch {int}")]
fn matched_proof(world: &mut Runtime, monotonic: u64, epoch: u64) {
    world.proof(monotonic, epoch, 1000, true, true);
}

#[then("one record preserves that occurrence epoch and episode-relative minutes")]
fn matched_record(world: &mut Runtime) {
    let status = world.status();
    assert_eq!(status.protected_idle_suppressed_events.len(), 1);
    let record = &status.protected_idle_suppressed_events[0];
    assert_eq!(record.epoch_ms, 900000);
    assert_eq!(record.kind, "idle_sleep_request_cancelled");
    assert!((f64::from(record.minutes_into_protected_idle) - 2000.0 / 60000.0).abs() < 1e-12);
}

#[when("a cancellation receipt has no matching eligible native request")]
fn no_request(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, false, true);
}

#[then("no suppression record is inferred and an explicit unmatched error is logged")]
fn no_inferred(world: &mut Runtime) {
    assert!(world.status().protected_idle_suppressed_events.is_empty());
    assert!(world.has_error("unmatched_cancellation"));
}

#[when("an eligible native request receives a failed cancellation acknowledgement")]
fn failed_ack(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, false);
}

#[then("the live suppression list stays empty with an explicit cancellation error")]
fn no_failed_record(world: &mut Runtime) {
    assert!(world.status().protected_idle_suppressed_events.is_empty());
    assert!(world.has_error("cancellation_unconfirmed"));
}

#[when("65 matched native cancellations are observed")]
fn sixty_five(world: &mut Runtime) {
    for index in 1..=65_u64 {
        world.proof(3000 + index * 2, 900000 + index, 1000, true, true);
        if index == 64 {
            let status = world.status();
            world.untruncated_at_64 = status.protected_idle_suppressed_events.len() == 64
                && !status.suppressed_events_truncated;
        }
    }
}

#[then("records two through 65 remain and truncation becomes true only beyond 64")]
fn bounded_records(world: &mut Runtime) {
    assert!(world.untruncated_at_64);
    let status = world.status();
    assert_eq!(status.protected_idle_suppressed_events.len(), 64);
    assert!(status.suppressed_events_truncated);
    assert_eq!(status.protected_idle_suppressed_events[0].epoch_ms, 900002);
    assert_eq!(status.protected_idle_suppressed_events[63].epoch_ms, 900065);
}

#[when("genuine activity follows a record and another pending cancellation")]
fn activity_after_proof(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, true);
    world.count_before_reset = world.status().protected_idle_suppressed_events.len();
    let pending = world.proof_request(3500, 1000);
    world
        .native_events
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .emit::<Power::Suppression::Evidence::Requested>(pending.clone());
    world.sample(8000, 1000);
    world.sample(9000, 2000);
    world.acknowledge(&pending, 4000, 900001, 1000, true);
}

#[then("the new idle interval is empty and the late receipt cannot restore old records")]
fn input_cleared(world: &mut Runtime) {
    assert_eq!(world.count_before_reset, 1);
    let status = world.status();
    assert!(status.protected_idle_suppressed_events.is_empty());
    assert!(!status.suppressed_events_truncated);
    assert_eq!(status.protected_idle_duration_seconds, Some(2));
    assert!(world.has_error("stale_cancellation"));
}

#[when("a verified record is followed by repeated queries and On requests")]
fn stable_interval(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, true);
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOn>(Empty {});
    world.sample(8000, 7000);
    world.sample(9000, 8000);
}

#[then("the record and anchor survive while duration advances truthfully")]
fn unchanged_interval(world: &mut Runtime) {
    let status = world.status();
    assert_eq!(status.protected_idle_suppressed_events.len(), 1);
    assert_eq!(status.protected_idle_duration_seconds, Some(8));
    assert!(world.native.applies.lock().unwrap().is_empty());
    assert!(
        (f64::from(status.protected_idle_suppressed_events[0].minutes_into_protected_idle)
            - 2000.0 / 60000.0)
            .abs()
            < 1e-12
    );
}

#[when("cancellation epochs jump backward while monotonic time advances")]
fn jumping_epoch(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, true);
    world.proof(5000, 100000, 1000, true, true);
    world.sample(9000, 8000);
}

#[then("event order timestamps and offsets remain tied to each occurrence")]
fn event_time_offsets(world: &mut Runtime) {
    let status = world.status();
    let records = status.protected_idle_suppressed_events;
    assert_eq!(records.len(), 2);
    assert_eq!([records[0].epoch_ms, records[1].epoch_ms], [900000, 100000]);
    for (record, elapsed) in records.iter().zip([2000.0, 4000.0]) {
        assert!((f64::from(record.minutes_into_protected_idle) - elapsed / 60000.0).abs() < 1e-12);
    }
}

#[when("a verified record is followed by real sleep wake and a new cancellation")]
fn wake_proof(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, true);
    world.count_before_reset = world.status().protected_idle_suppressed_events.len();
    world.interruption(SleepPhase::WillSleep, 5000, false);
    world.interruption(SleepPhase::DidWake, 10000, false);
    world.proof(12000, 100000, 10000, true, true);
}

#[then("only the post-wake record remains with a post-wake offset")]
fn wake_record(world: &mut Runtime) {
    assert_eq!(world.count_before_reset, 1);
    let status = world.status();
    assert_eq!(status.protected_idle_suppressed_events.len(), 1);
    assert_eq!(status.protected_idle_suppressed_events[0].epoch_ms, 100000);
    assert!(
        (f64::from(status.protected_idle_suppressed_events[0].minutes_into_protected_idle)
            - 2000.0 / 60000.0)
            .abs()
            < 1e-12
    );
}

#[when("replayed request evidence claims a different idle anchor")]
fn stale_proof_anchor(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, true);
    let mut stale = world.proof_request(8000, 7000);
    stale.sequence = 1;
    world
        .native_events
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .emit::<Power::Suppression::Evidence::Requested>(stale);
    world.sample(9000, 8000);
    world.expected_proof_error = Some("stale_suppression");
}

#[when("an unmatched cancellation claims a different idle anchor")]
fn unmatched_anchor(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, true);
    world.proof(8000, 900001, 7000, false, true);
    world.sample(9000, 8000);
    world.expected_proof_error = Some("unmatched_cancellation");
}

#[then("the verified record and anchor survive that rejected evidence")]
fn rejected_proof_preserved(world: &mut Runtime) {
    let status = world.status();
    assert_eq!(status.protected_idle_suppressed_events.len(), 1);
    assert_eq!(status.protected_idle_suppressed_events[0].epoch_ms, 900000);
    assert_eq!(status.protected_idle_duration_seconds, Some(8));
    assert!(world.has_error(world.expected_proof_error.unwrap()));
}

#[when("the newer of two samples fails before the older one succeeds")]
fn newer_sample_fails(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    for _ in 0..2 {
        let query = bus
            .prepare::<Power::Session::Protection::Query>(Empty {})
            .unwrap();
        world.correlations.push(query.correlation());
        bus.emit_prepared(query);
    }
    let mut samples = std::mem::take(&mut *world.native.samples.lock().unwrap());
    let (request, events) = samples.pop().unwrap();
    events.emit::<Power::Activity::System::Observed>(IdleObserved {
        request,
        clock: ClockSample {
            epoch_ms: 107000,
            monotonic_ms: 7000,
        },
        idle_age_ms: None,
        fault: Some(ai2screenwake_client::contract::Fault::new(
            "idle_source_failed",
            "Native idle sampling failed.",
            true,
        )),
    });
    let (request, events) = samples.pop().unwrap();
    events.emit::<Power::Activity::System::Observed>(IdleObserved {
        request,
        clock: ClockSample {
            epoch_ms: 106000,
            monotonic_ms: 6000,
        },
        idle_age_ms: Some(5000),
        fault: None,
    });
}

#[then("both requests retain the explicit latest sampling failure without false Off")]
fn ordered_sampling_errors(world: &mut Runtime) {
    let settled = world.settlements();
    for id in &world.correlations {
        let result = &settled
            .iter()
            .find(|settled| &settled.correlation == id)
            .unwrap()
            .result;
        assert_eq!(
            result.error.as_ref().map(|fault| fault.fault_code.as_str()),
            Some("idle_source_failed")
        );
        let status = result.status.as_ref().unwrap();
        assert_eq!(status.is_on, Some(true));
        assert_eq!(status.protected_idle_duration_seconds, None);
    }
}

#[when("a newer sample contradicts the known activity anchor")]
fn contradictory_idle(world: &mut Runtime) {
    world.sample(8000, 1000);
    let bus = world.bus.as_ref().unwrap();
    let query = bus
        .prepare::<Power::Session::Protection::Query>(Empty {})
        .unwrap();
    world.correlations.push(query.correlation());
    bus.emit_prepared(query);
    let (request, events) = world.native.samples.lock().unwrap().pop().unwrap();
    events.emit::<Power::Activity::System::Observed>(IdleObserved {
        request,
        clock: ClockSample {
            epoch_ms: 109000,
            monotonic_ms: 9000,
        },
        idle_age_ms: Some(8000),
        fault: None,
    });
}

#[then("its query has an explicit stale-sample error and null duration while On stays true")]
fn contradictory_error(world: &mut Runtime) {
    let settled = world.settlements();
    let result = &settled
        .iter()
        .find(|settled| Some(&settled.correlation) == world.correlations.last())
        .unwrap()
        .result;
    assert_eq!(
        result.error.as_ref().map(|fault| fault.fault_code.as_str()),
        Some("stale_idle_observation")
    );
    assert_eq!(result.status.as_ref().unwrap().is_on, Some(true));
    assert_eq!(
        result
            .status
            .as_ref()
            .unwrap()
            .protected_idle_duration_seconds,
        None
    );
}

fn failing_journal(world: &mut Runtime, at: FailureAt) {
    let bus = start_with_journal(
        Some(world.native.clone()),
        RuntimeOptions::default(),
        Some(Arc::new(FailingJournal(at))),
    )
    .unwrap();
    bus.observe(world.sink.clone());
    world.bus = Some(bus);
    world.journal_failure = Some(at);
}

#[given("an owner whose first durable append fails")]
fn first_append_failure(world: &mut Runtime) {
    failing_journal(world, FailureAt::First);
}

#[given("an owner whose enabling-apply append fails")]
fn apply_append_failure(world: &mut Runtime) {
    failing_journal(world, FailureAt::Apply);
}

#[given("an owner whose success-result append fails")]
fn result_append_failure(world: &mut Runtime) {
    failing_journal(world, FailureAt::Settlement);
}

#[when("On is requested through the failing journal")]
fn on_with_failed_journal(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let request = bus
        .prepare::<Power::Session::Protection::TurnOn>(Empty {})
        .unwrap();
    let id = request.correlation();
    bus.emit_prepared(request);
    if matches!(world.journal_failure, Some(FailureAt::Settlement)) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while world.native.applies.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(world.native.applies.lock().unwrap().len(), 1);
        world.complete(2);
    }
    world.wait_result(&id);
    world.correlations.push(id);
}

#[then("one explicit storage error result is observed and native On was never called")]
fn no_unlogged_enable(world: &mut Runtime) {
    let result = world.wait_result(&world.correlations[0]);
    assert_eq!(
        result.result.error.as_ref().unwrap().fault_code,
        "injected_journal_failure"
    );
    assert!(world.native.applies.lock().unwrap().is_empty());
}

#[when("explicit Quit cleanup follows the storage failure")]
fn cleanup_after_storage(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    let quit = bus
        .prepare::<Power::Session::Protection::Quit>(Empty {})
        .unwrap();
    let id = quit.correlation();
    bus.emit_prepared(quit);
    let deadline = Instant::now() + Duration::from_secs(1);
    while world.native.applies.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
    assert!(!world.native.applies.lock().unwrap()[0].0.enabled);
    world.complete(0);
    world.wait_result(&id);
    world.correlations.push(id);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !world.sink.0.lock().unwrap().iter().any(|entry| {
        entry
            .payload::<Power::Session::Protection::Closed>()
            .unwrap()
            .is_some()
    }) && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[then("native Off is observed before Closed without a false durable-success result")]
fn safe_storage_cleanup(world: &mut Runtime) {
    let settled = world.wait_result(world.correlations.last().unwrap());
    assert_eq!(settled.result.status.as_ref().unwrap().is_on, Some(false));
    assert_eq!(
        settled.result.error.as_ref().unwrap().fault_code,
        "injected_journal_failure"
    );
    let entries = world.sink.0.lock().unwrap();
    let observed = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Assertions::Resources::Observed>()
                .unwrap()
                .is_some_and(|value| !value.request.enabled && value.held_count == 0)
        })
        .unwrap();
    let closed = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Protection::Closed>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(observed < closed);
}

#[then("the caller observes exactly one failed terminal result after its storage error")]
fn no_false_storage_success(world: &mut Runtime) {
    let results: Vec<_> = world
        .settlements()
        .into_iter()
        .filter(|result| result.correlation == world.correlations[0])
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].result.error.as_ref().unwrap().fault_code,
        "injected_journal_failure"
    );
    let entries = world.sink.0.lock().unwrap();
    let fault = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Error::Log::Storage::Occurred>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    let result = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(fault < result);
}

#[given("an owner whose log-view append fails")]
fn log_append_failure(world: &mut Runtime) {
    failing_journal(world, FailureAt::LogView);
}

#[when("the native log view is requested through the failing journal")]
fn log_with_failed_journal(world: &mut Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Log::Session::History::Query>(Empty {});
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let view = world
            .sink
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|entry| entry.payload::<Log::Session::History::Snapshot>().unwrap());
        if let Some(view) = view {
            world.log_view = Some(view);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "failed log append produced no explicit view"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[then("its observed log projection explicitly includes the storage failure")]
fn log_view_failed(world: &mut Runtime) {
    assert_eq!(
        world
            .log_view
            .as_ref()
            .unwrap()
            .storage_error
            .as_ref()
            .map(|fault| fault.fault_code.as_str()),
        Some("injected_journal_failure")
    );
    assert!(world.native.applies.lock().unwrap().is_empty());
}

#[when("a verified record is followed by explicit Off and a newly confirmed On")]
fn new_on_interval(world: &mut Runtime) {
    world.proof(3000, 900000, 1000, true, true);
    world.count_before_reset = world.status().protected_idle_suppressed_events.len();
    let bus = world.bus.as_ref().unwrap();
    for (enabled, now) in [(false, 7000), (true, 10000)] {
        if enabled {
            bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
        } else {
            bus.emit::<Power::Session::Protection::TurnOff>(Empty {});
        }
        let (request, events) = world.native.applies.lock().unwrap().pop().unwrap();
        events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
            request,
            held_count: if enabled { 2 } else { 0 },
            outcome: Outcome::Applied,
            fault_codes: Vec::new(),
            clock: ClockSample {
                epoch_ms: 100_000 + now,
                monotonic_ms: now,
            },
        });
    }
    world.sample(11000, 10000);
}

#[then("the new On has no old records or pre-enable idle time")]
fn fresh_on_interval(world: &mut Runtime) {
    assert_eq!(world.count_before_reset, 1);
    let status = world.status();
    assert_eq!(status.is_on, Some(true));
    assert_eq!(status.protected_idle_duration_seconds, Some(1));
    assert!(status.protected_idle_suppressed_events.is_empty());
    assert!(!status.suppressed_events_truncated);
}

#[given(expr = "a real native owner whose apply clock {int} fails once")]
fn clock_owner(world: &mut Runtime, call: usize) {
    world.clock_api.0.lock().unwrap().fail_at = call;
    let bus = start_with_options(
        Some(Arc::new(MacPower::with_api(world.clock_api.clone()))),
        RuntimeOptions {
            operation_timeout: Duration::from_millis(200),
        },
    )
    .unwrap();
    bus.observe(world.sink.clone());
    world.bus = Some(bus);
}

fn clock_control<A: ai2screenwake_client::contract::Action<Payload = Empty>>(
    world: &mut Runtime,
) -> Settlement {
    let bus = world.bus.as_ref().unwrap();
    let prepared = bus.prepare::<A>(Empty {}).unwrap();
    let id = prepared.correlation();
    bus.emit_prepared(prepared);
    world.correlations.push(id.clone());
    world.wait_result(&id)
}

#[when("On encounters that clock fault")]
fn clock_on(world: &mut Runtime) {
    let result = clock_control::<Power::Session::Protection::TurnOn>(world);
    assert!(result.result.error.is_some());
    assert_ne!(
        result.result.error.unwrap().fault_code,
        "operation_timed_out",
        "a completed native clock failure must settle, not expire"
    );
    world.failed_apply = world.sink.0.lock().unwrap().iter().find_map(|entry| {
        entry
            .payload::<Power::Assertions::Resources::Apply>()
            .unwrap()
    });
    let entries = world.sink.0.lock().unwrap();
    let failures: Vec<_> = entries
        .iter()
        .filter_map(|entry| {
            entry
                .payload::<Power::Assertions::Resources::Failed>()
                .unwrap()
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].request, *world.failed_apply.as_ref().unwrap());
    assert_eq!(
        usize::from(failures[0].held_count),
        world.clock_api.0.lock().unwrap().held.len()
    );
    let status = result.result.status.unwrap();
    assert_eq!(status.is_on, None);
    assert!(status.operation_id.is_none());
}

#[then("a recovered explicit On succeeds and Quit releases its actual IDs")]
fn recover_clock_on(world: &mut Runtime) {
    let result = clock_control::<Power::Session::Protection::TurnOn>(world);
    assert!(result.result.error.is_none());
    assert_eq!(result.result.status.unwrap().is_on, Some(true));
    assert_eq!(world.clock_api.0.lock().unwrap().held.len(), 2);
    clock_cleanup(world, "Quit".into());
    clock_terminal(world);
}

#[then(expr = "the failed acquisition retains both IDs until explicit {word} cleanup")]
fn clock_cleanup(world: &mut Runtime, control: String) {
    if world.failed_apply.is_some() && world.clock_api.0.lock().unwrap().acquired > 0 {
        assert_eq!(world.clock_api.0.lock().unwrap().held.len(), 2);
    }
    let result = match control.as_str() {
        "Off" => clock_control::<Power::Session::Protection::TurnOff>(world),
        "Quit" => clock_control::<Power::Session::Protection::Quit>(world),
        _ => unreachable!(),
    };
    assert!(result.result.error.is_none());
    assert_eq!(result.result.status.unwrap().is_on, Some(false));
    assert!(world.clock_api.0.lock().unwrap().held.is_empty());
    assert_eq!(world.clock_api.0.lock().unwrap().released.len(), 2);
    if control == "Off" {
        assert!(
            clock_control::<Power::Session::Protection::Quit>(world)
                .result
                .error
                .is_none()
        );
    }
}

#[when(expr = "confirmed On is followed by an Off clock {int} fault")]
fn off_clock_fault(world: &mut Runtime, relative: usize) {
    let on = clock_control::<Power::Session::Protection::TurnOn>(world);
    assert!(on.result.error.is_none());
    {
        let mut calls = world.clock_api.0.lock().unwrap();
        calls.fail_at = calls.clocks + relative;
    }
    let off = clock_control::<Power::Session::Protection::TurnOff>(world);
    assert!(
        off.result.error.is_some(),
        "unavailable clocks must not become success"
    );
    assert_ne!(off.result.error.unwrap().fault_code, "operation_timed_out");
    world.failed_apply = world.sink.0.lock().unwrap().iter().rev().find_map(|entry| {
        entry
            .payload::<Power::Assertions::Resources::Apply>()
            .unwrap()
    });
    assert!(world.clock_api.0.lock().unwrap().held.is_empty());
    assert_eq!(world.clock_api.0.lock().unwrap().released.len(), 2);
    assert!(
        clock_control::<Power::Session::Protection::Quit>(world)
            .result
            .error
            .is_none()
    );
}

#[then("all native failures are error-first without fake clocks or duplicate results")]
fn clock_terminal(world: &mut Runtime) {
    let settlements = world.settlements();
    let entries = world.sink.0.lock().unwrap();
    assert!(entries.iter().any(|entry| {
        entry
            .payload::<Power::Session::Protection::Closed>()
            .unwrap()
            .is_some()
    }));
    for correlation in &world.correlations {
        let results: Vec<_> = settlements
            .iter()
            .filter(|result| &result.correlation == correlation)
            .collect();
        assert_eq!(results.len(), 1);
    }
    let error = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Error::Native::Assertions::Occurred>()
                .unwrap()
                .is_some_and(|fault| {
                    fault.fault_code == "test_apply_clock" && fault.failed_genus.is_some()
                })
        })
        .unwrap();
    let failed = entries
        .iter()
        .position(|entry| {
            entry
                .payload::<Power::Session::Protection::Settled>()
                .unwrap()
                .is_some_and(|result| result.result.error.is_some())
        })
        .unwrap();
    assert!(error < failed);
    if let Some(request) = &world.failed_apply {
        assert!(
            !entries.iter().any(|entry| {
                entry
                    .payload::<Power::Assertions::Resources::Observed>()
                    .unwrap()
                    .is_some_and(|result| &result.request == request)
            }),
            "a clockless failed operation must not invent an occurrence clock"
        );
    }
}

#[when("an old terminal native failure is repeated while explicit Off is pending")]
fn stale_clock_failure(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap().clone();
    bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
    let (request, events) = world.native.applies.lock().unwrap().last().unwrap().clone();
    let failure = FailedApply {
        request,
        held_count: 2,
        fault: Fault::new("test_apply_clock", "Controlled clock fault.", true),
    };
    events.error::<ai2screenwake_client::contract::Error::Native::Assertions::Occurred>(
        failure.fault.clone(),
    );
    events.emit::<Power::Assertions::Resources::Failed>(failure.clone());
    let off = bus
        .prepare::<Power::Session::Protection::TurnOff>(Empty {})
        .unwrap();
    world.correlations.push(off.correlation());
    bus.emit_prepared(off);
    events.emit::<Power::Assertions::Resources::Failed>(failure);
    assert!(
        world
            .settlements()
            .iter()
            .all(|result| result.correlation != world.correlations[0])
    );
    world.complete(0);
}

#[then("only the current native completion settles Off successfully")]
fn stale_failure_guard(world: &mut Runtime) {
    let result = world.wait_result(&world.correlations[0]);
    assert!(result.result.error.is_none());
    assert_eq!(result.result.status.unwrap().is_on, Some(false));
    assert!(world.has_error("stale_native_failure"));
    assert_eq!(
        world
            .settlements()
            .iter()
            .filter(|result| result.correlation == world.correlations[0])
            .count(),
        1
    );
}

fn main() {
    futures::executor::block_on(Runtime::cucumber().fail_on_skipped().run_and_exit(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/features/runtime.feature"
    )));
}
#[when(expr = "128 native idle queries are pending before explicit {word}")]
fn saturated_cleanup(world: &mut Runtime, control: String) {
    let bus = world.bus.as_ref().unwrap();
    for _ in 0..128 {
        bus.emit::<Power::Session::Protection::Query>(Empty {});
    }
    assert_eq!(world.native.samples.lock().unwrap().len(), 128);
    if control == "Quit" {
        bus.emit::<Power::Session::Protection::Quit>(Empty {});
    } else {
        bus.emit::<Power::Session::Protection::TurnOff>(Empty {});
    }
}

#[then("native release proceeds and all saturated requests settle before confirmed Off")]
fn saturated_released(world: &mut Runtime) {
    assert_eq!(
        world.native.applies.lock().unwrap().len(),
        1,
        "cleanup must not depend on query capacity"
    );
    assert_eq!(world.settlements().len(), 129);
    world.complete(0);
    assert_eq!(world.settlements().len(), 130);
    let settled = world.settlements().pop().unwrap().result;
    assert!(settled.error.is_none());
    assert_eq!(settled.status.unwrap().is_on, Some(false));
}

#[when("a failed wake precedes delivery of its older successful apply")]
fn wake_before_apply(world: &mut Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOn>(Empty {});
    let (request, events) = world.native.applies.lock().unwrap().pop().unwrap();
    events.emit::<Power::Activity::System::Interrupted>(InterruptionObserved {
        request: request.clone(),
        clock: ClockSample {
            epoch_ms: 102_000,
            monotonic_ms: 3000,
        },
        phase: SleepPhase::DidWake,
        held_count: 1,
    });
    events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
        request,
        held_count: 2,
        outcome: Outcome::Applied,
        fault_codes: Vec::new(),
        clock: ClockSample {
            epoch_ms: 100_000,
            monotonic_ms: 1000,
        },
    });
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Query>(Empty {});
}

#[then("protection remains unresolved with the newer wake failure")]
fn wake_failure_preserved(world: &mut Runtime) {
    let result = world.settlements().pop().unwrap().result;
    let status = result.status.unwrap();
    assert_eq!(status.is_on, None);
    assert_eq!(status.state, ai2screenwake_client::contract::State::Error);
    assert!(result.error.is_some());
    assert!(world.native.samples.lock().unwrap().is_empty());
}

#[when("a verified cancellation is followed by one millisecond anchor jitter")]
fn anchor_jitter(world: &mut Runtime) {
    world.proof_guard = Some(world.sample(6000, 4000));
    world.proof(7000, 107_000, 2000, true, true);
    world.sample(8000, 5999);
    world.sample(9000, 7001);
    let request = world.proof_request(10_000, 1999);
    world
        .native_events
        .as_ref()
        .unwrap()
        .upgrade()
        .unwrap()
        .emit::<Power::Suppression::Evidence::Requested>(request.clone());
    world.acknowledge(&request, 10_001, 110_001, 2001, true);
}

#[then("queries and cancellation proofs retain the same interval without stale errors")]
fn jitter_preserved(world: &mut Runtime) {
    let status = world.status();
    assert_eq!(status.is_on, Some(true));
    assert_eq!(status.protected_idle_suppressed_events.len(), 2);
    assert_eq!(status.protected_idle_suppressed_events[0].epoch_ms, 107_000);
    assert!(
        world
            .settlements()
            .iter()
            .all(|entry| entry.result.error.is_none())
    );
}
#[when("the native source reports unavailable before Off and another On request")]
fn source_unavailable(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Activity::System::Availability>(
        ai2screenwake_client::contract::LifecycleAvailability {
            revision: 1,
            available: false,
        },
    );
    bus.emit::<Power::Session::Protection::TurnOff>(Empty {});
    world.complete(0);
    bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
}

#[then("only Off reaches native until a newer availability event permits explicit On")]
fn source_recovered(world: &mut Runtime) {
    assert!(
        world.native.applies.lock().unwrap().is_empty(),
        "On reached native without lifecycle observation"
    );
    assert_eq!(
        world
            .settlements()
            .last()
            .unwrap()
            .result
            .error
            .as_ref()
            .unwrap()
            .fault_code,
        "lifecycle_unavailable"
    );
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Activity::System::Availability>(
        ai2screenwake_client::contract::LifecycleAvailability {
            revision: 0,
            available: true,
        },
    );
    bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
    assert!(
        world.native.applies.lock().unwrap().is_empty(),
        "stale availability restored eligibility"
    );
    bus.emit::<Power::Activity::System::Availability>(
        ai2screenwake_client::contract::LifecycleAvailability {
            revision: 2,
            available: true,
        },
    );
    assert!(
        world.native.applies.lock().unwrap().is_empty(),
        "source recovery must not auto-enable"
    );
    bus.emit::<Power::Session::Protection::TurnOn>(Empty {});
    assert_eq!(world.native.applies.lock().unwrap().len(), 1);
    world.complete(2);
}
fn source_lost(world: &Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Activity::System::Availability>(
            ai2screenwake_client::contract::LifecycleAvailability {
                revision: 1,
                available: false,
            },
        );
}

#[when("lifecycle observation is lost while explicit Off is pending")]
fn source_during_off(world: &mut Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::TurnOff>(Empty {});
    source_lost(world);
}

#[then("Off still awaits native zero resources and settles successfully")]
fn source_safe_off(world: &mut Runtime) {
    let before = world.settlements().len();
    world.complete(0);
    assert_eq!(
        before, 1,
        "source failure must not prematurely settle pending cleanup"
    );
    let result = world.settlements().pop().unwrap().result;
    assert!(result.error.is_none());
    assert_eq!(result.status.unwrap().is_on, Some(false));
}

#[when("lifecycle observation stops after confirmed Quit cleanup")]
fn source_after_quit(world: &mut Runtime) {
    world
        .bus
        .as_ref()
        .unwrap()
        .emit::<Power::Session::Protection::Quit>(Empty {});
    world.complete(0);
    world.prior_snapshots = world.sink.0.lock().unwrap().len();
    source_lost(world);
}

#[then("normal teardown adds no failure to the confirmed Off outcome")]
fn source_normal_stop(world: &mut Runtime) {
    let entries = world.sink.0.lock().unwrap();
    assert!(
        !entries[world.prior_snapshots..].iter().any(|entry| {
            entry
                .payload::<ai2screenwake_client::contract::Error::Native::Activity::Occurred>()
                .unwrap()
                .is_some()
        }),
        "normal observer teardown must not report a protection fault"
    );
    drop(entries);
    assert_eq!(
        world
            .settlements()
            .last()
            .unwrap()
            .result
            .status
            .as_ref()
            .unwrap()
            .is_on,
        Some(false)
    );
}
#[when("native Toggle is rejected during source loss and retried after recovery")]
fn source_toggle_retry(world: &mut Runtime) {
    let bus = world.bus.as_ref().unwrap();
    bus.emit::<Power::Activity::System::Availability>(
        ai2screenwake_client::contract::LifecycleAvailability {
            revision: 1,
            available: false,
        },
    );
    bus.emit::<Power::Session::Protection::Toggle>(Empty {});
    assert!(world.native.applies.lock().unwrap().is_empty());
    bus.emit::<Power::Activity::System::Availability>(
        ai2screenwake_client::contract::LifecycleAvailability {
            revision: 2,
            available: true,
        },
    );
    bus.emit::<Power::Session::Protection::Toggle>(Empty {});
}

#[then("one recovered Toggle reaches native On without requiring an extra click")]
fn source_toggle_recovered(world: &mut Runtime) {
    let requests = world.native.applies.lock().unwrap();
    let count = requests.len();
    let enabled = requests.first().is_some_and(|(request, _)| request.enabled);
    drop(requests);
    assert_eq!(
        count, 1,
        "a rejected On must not consume the next real toggle"
    );
    assert!(enabled);
    world.complete(2);
    let result = world.settlements().last().unwrap().result.clone();
    assert!(result.error.is_none());
    assert_eq!(result.status.unwrap().is_on, Some(true));
}
// end runtime.rs
