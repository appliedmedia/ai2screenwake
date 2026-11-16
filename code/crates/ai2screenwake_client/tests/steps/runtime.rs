// runtime.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Request and episode requirements at the real typed Actions boundary.

use ai2screenwake_client::{
    contract::{
        ActionJournal, Apply, AssertionsObserved, CancellationObserved, ClockSample, CorrelationId,
        Emitter, Empty, Fault, IdGeneration, IdleObserved, IdleRequest, InterruptionObserved,
        JournalHistory, KvEnvelope, Log, LogSnapshot, ObservationSink, Outcome, Power, Registry,
        Settlement, SleepPhase, Status, SuppressionKind, SuppressionRequest, WeakEmitter,
    },
    runtime::{RuntimeOptions, start, start_with_journal, start_with_options},
};
use ai2screenwake_native::NativePower;
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
                .payload::<Power::Assertions::Apply>()?
                .is_some_and(|request| request.enabled),
            FailureAt::Settlement => envelope.payload::<Power::Session::Settled>()?.is_some(),
            FailureAt::LogView => envelope.payload::<Log::Session::Snapshot>()?.is_some(),
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
            native_request_id: IdGeneration::next_id::<Power::Session::Query>(&Empty {}).unwrap(),
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
            .emit::<Power::Suppression::Recorded>(CancellationObserved {
                request: request.request.clone(),
                native_request_id: request.native_request_id.clone(),
                cancellation_id: IdGeneration::next_id::<Power::Session::Query>(&Empty {}).unwrap(),
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
                .emit::<Power::Suppression::Requested>(request.clone());
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
            .find_map(|entry| entry.payload::<Power::Session::Snapshot>().unwrap())
            .unwrap()
    }

    fn sample(&self, now: u64, idle: u64) -> IdleRequest {
        self.bus
            .as_ref()
            .unwrap()
            .emit::<Power::Session::Query>(Empty {});
        let (request, events) = self.native.samples.lock().unwrap().pop().unwrap();
        events.emit::<Power::Activity::Observed>(IdleObserved {
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
            .emit::<Power::Activity::Interrupted>(InterruptionObserved {
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
    world.active = Some(request.clone());
    world.native_events = Some(events.downgrade());
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
    let query = bus.prepare::<Power::Session::Query>(Empty {}).unwrap();
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
            .payload::<Power::Session::Toggle>()
            .unwrap()
            .is_some()
            && !self.fired.swap(true, Ordering::SeqCst)
        {
            self.bus.emit::<Power::Session::Quit>(Empty {});
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
    bus.emit::<Power::Session::Toggle>(Empty {});
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
        .emit::<Power::Suppression::Requested>(pending.clone());
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
        .emit::<Power::Session::TurnOn>(Empty {});
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
        .emit::<Power::Suppression::Requested>(stale);
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
        let query = bus.prepare::<Power::Session::Query>(Empty {}).unwrap();
        world.correlations.push(query.correlation());
        bus.emit_prepared(query);
    }
    let mut samples = std::mem::take(&mut *world.native.samples.lock().unwrap());
    let (request, events) = samples.pop().unwrap();
    events.emit::<Power::Activity::Observed>(IdleObserved {
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
    events.emit::<Power::Activity::Observed>(IdleObserved {
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
    let query = bus.prepare::<Power::Session::Query>(Empty {}).unwrap();
    world.correlations.push(query.correlation());
    bus.emit_prepared(query);
    let (request, events) = world.native.samples.lock().unwrap().pop().unwrap();
    events.emit::<Power::Activity::Observed>(IdleObserved {
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
    let request = bus.prepare::<Power::Session::TurnOn>(Empty {}).unwrap();
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
    let quit = bus.prepare::<Power::Session::Quit>(Empty {}).unwrap();
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
    while !world
        .sink
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|entry| entry.payload::<Power::Session::Closed>().unwrap().is_some())
        && Instant::now() < deadline
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
                .payload::<Power::Assertions::Observed>()
                .unwrap()
                .is_some_and(|value| !value.request.enabled && value.held_count == 0)
        })
        .unwrap();
    let closed = entries
        .iter()
        .position(|entry| entry.payload::<Power::Session::Closed>().unwrap().is_some())
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
                .payload::<Power::Session::Settled>()
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
        .emit::<Log::Session::Query>(Empty {});
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let view = world
            .sink
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|entry| entry.payload::<Log::Session::Snapshot>().unwrap());
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

fn main() {
    futures::executor::block_on(Runtime::cucumber().fail_on_skipped().run_and_exit(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/features/runtime.feature"
    )));
}
// end runtime.rs
