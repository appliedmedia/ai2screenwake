// projection.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Bounded request notifications. No waiter owns the registry it is waiting on.

use crate::contract::{
    CancellationObserved, ControlResult, CorrelationId, Emitter, EntityId, Error, Fault,
    IdleRequest, Minutes, Power, Settlement, StartupError, Status, SuppressedEvent,
    SuppressionKind, SuppressionRequest, WeakEmitter,
};
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

pub(crate) const MAX_REQUESTS: usize = 128;

pub(crate) enum Kind {
    Setter { generation: u64 },
    Query(IdleRequest),
}

struct Waiting {
    kind: Kind,
    events: WeakEmitter,
}

#[derive(Default)]
struct TimerState {
    active: HashMap<CorrelationId, (Instant, WeakEmitter)>,
    stopping: bool,
}

#[derive(Default)]
struct Deadlines {
    state: Mutex<TimerState>,
    changed: Condvar,
}

/// One scheduler serves the bounded request set; deadlines emit actions, not state mutations.
fn deadlines(timers: Arc<Deadlines>) {
    let mut state = timers.state.lock().expect("deadline state poisoned");
    loop {
        if state.stopping {
            return;
        }
        let now = Instant::now();
        let due: Vec<_> = state
            .active
            .iter()
            .filter(|(_, (at, _))| *at <= now)
            .map(|(id, _)| id.clone())
            .collect();
        if !due.is_empty() {
            let due: Vec<_> = due
                .into_iter()
                .map(|id| {
                    let (_, events) = state.active.remove(&id).expect("selected above");
                    (id, events)
                })
                .collect();
            drop(state);
            for (id, events) in due {
                if let Some(events) = events.upgrade() {
                    events.emit::<Power::Session::Protection::Expired>(id);
                }
            }
            state = timers.state.lock().expect("deadline state poisoned");
            continue;
        }
        state = match state.active.values().map(|(at, _)| *at).min() {
            Some(at) => {
                timers
                    .changed
                    .wait_timeout(state, at.saturating_duration_since(Instant::now()))
                    .expect("deadline state poisoned")
                    .0
            }
            None => timers.changed.wait(state).expect("deadline state poisoned"),
        };
    }
}

pub(crate) struct Requests {
    waiting: HashMap<CorrelationId, Waiting>,
    timeout: Duration,
    timers: Arc<Deadlines>,
}

impl Drop for Requests {
    fn drop(&mut self) {
        let mut state = self.timers.state.lock().expect("deadline state poisoned");
        state.stopping = true;
        state.active.clear();
        self.timers.changed.notify_one();
    }
}

impl Requests {
    pub(crate) fn new(timeout: Duration) -> Result<Self, StartupError> {
        if timeout.is_zero() || timeout > Duration::from_secs(5) {
            return Err(StartupError(
                "operation timeout must be positive and at most five seconds".into(),
            ));
        }
        let timers = Arc::new(Deadlines::default());
        let worker = timers.clone();
        thread::Builder::new()
            .name("screenwake-deadlines".into())
            .spawn(move || deadlines(worker))
            .map_err(|error| StartupError(error.to_string()))?;
        Ok(Self {
            waiting: HashMap::new(),
            timeout,
            timers,
        })
    }

    pub(crate) fn add(&mut self, events: &Emitter, kind: Kind) -> Result<(), Fault> {
        let id = events.correlation();
        if self.waiting.contains_key(&id) {
            return Err(Fault::new(
                "duplicate_request",
                "This request is already pending.",
                true,
            ));
        }
        if self.waiting.len() >= MAX_REQUESTS {
            return Err(Fault::new(
                "request_capacity",
                "The owner has too many pending requests.",
                true,
            ));
        }
        self.timers
            .state
            .lock()
            .expect("deadline state poisoned")
            .active
            .insert(
                id.clone(),
                (Instant::now() + self.timeout, events.downgrade()),
            );
        self.timers.changed.notify_one();
        self.waiting.insert(
            id,
            Waiting {
                kind,
                events: events.downgrade(),
            },
        );
        Ok(())
    }

    pub(crate) fn query(&self, sample: &IdleRequest) -> Option<CorrelationId> {
        self.waiting
            .iter()
            .find_map(|(id, waiting)| match &waiting.kind {
                Kind::Query(expected) if expected == sample => Some(id.clone()),
                _ => None,
            })
    }

    pub(crate) fn has(&self, id: &CorrelationId) -> bool {
        self.waiting.contains_key(id)
    }

    pub(crate) fn finish(&mut self, id: &CorrelationId, status: &Status, error: Option<Fault>) {
        if let Some(waiting) = self.waiting.remove(id) {
            self.timers
                .state
                .lock()
                .expect("deadline state poisoned")
                .active
                .remove(id);
            self.timers.changed.notify_one();
            if let Some(events) = waiting.events.upgrade() {
                Self::settled(&events, status, error, false);
            }
        }
    }

    pub(crate) fn finish_where(
        &mut self,
        status: &Status,
        error: Option<Fault>,
        predicate: impl Fn(&Kind) -> bool,
    ) {
        let ids: Vec<_> = self
            .waiting
            .iter()
            .filter(|(_, waiting)| predicate(&waiting.kind))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.finish(&id, status, error.clone());
        }
    }

    pub(crate) fn abandon(&mut self, ids: &[CorrelationId]) -> Result<(), Fault> {
        if ids.len() > 32 {
            return Err(Fault::new(
                "invalid_abandon",
                "A peer cannot abandon more than 32 requests.",
                true,
            ));
        }
        for id in ids {
            if self.waiting.remove(id).is_some() {
                self.timers
                    .state
                    .lock()
                    .expect("deadline state poisoned")
                    .active
                    .remove(id);
                self.timers.changed.notify_one();
            }
        }
        Ok(())
    }

    pub(crate) fn settled(events: &Emitter, status: &Status, error: Option<Fault>, snapshot: bool) {
        if let Some(error) = &error {
            events.error::<Error::Dispatch::Validation::Occurred>(error.clone());
        }
        if snapshot {
            events.emit::<Power::Session::Protection::Snapshot>(status.clone());
        }
        events.emit::<Power::Session::Protection::Settled>(Settlement {
            correlation: events.correlation(),
            result: ControlResult {
                status: Some(status.clone()),
                error,
            },
        });
    }
}

const MAX_PROOFS: usize = 64;
const MAX_RECORDS: usize = 64;
const PROOF_WINDOW_MS: u64 = 5000;

struct NativeProof {
    request: SuppressionRequest,
    anchor: u64,
}

/// Only current-interval evidence lives here; durable history is a separate log observer.
#[derive(Default)]
pub(crate) struct Proofs {
    pending: HashMap<EntityId, NativeProof>,
    records: Vec<(u64, u64, SuppressedEvent)>,
    sequence: u64,
    truncated: bool,
}

impl Proofs {
    pub(crate) fn reset(&mut self) {
        self.pending.clear();
        self.records.clear();
        self.truncated = false;
    }

    pub(crate) fn expire(&mut self, now: u64) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, proof| {
            now.saturating_sub(proof.request.clock.monotonic_ms) <= PROOF_WINDOW_MS
        });
        before - self.pending.len()
    }

    pub(crate) fn validate_request(&self, request: &SuppressionRequest) -> Result<(), Fault> {
        if request.sequence <= self.sequence
            || self.pending.contains_key(&request.native_request_id)
        {
            return Err(Fault::new(
                "stale_suppression",
                "Native request evidence was already observed.",
                true,
            ));
        }
        Ok(())
    }

    pub(crate) fn requested(
        &mut self,
        request: SuppressionRequest,
        anchor: u64,
    ) -> Result<(), Fault> {
        self.validate_request(&request)?;
        if self.pending.len() >= MAX_PROOFS {
            return Err(Fault::new(
                "suppression_capacity",
                "Too many native requests lack cancellation receipts.",
                true,
            ));
        }
        self.sequence = request.sequence;
        self.pending.insert(
            request.native_request_id.clone(),
            NativeProof { request, anchor },
        );
        Ok(())
    }

    pub(crate) fn validate_receipt(&self, receipt: &CancellationObserved) -> Result<u64, Fault> {
        let proof = self
            .pending
            .get(&receipt.native_request_id)
            .ok_or_else(|| {
                Fault::new(
                    "unmatched_cancellation",
                    "Cancellation has no matching eligible native request.",
                    true,
                )
            })?;
        if proof.request.request != receipt.request
            || receipt.clock.monotonic_ms < proof.request.clock.monotonic_ms
            || receipt.clock.monotonic_ms - proof.request.clock.monotonic_ms > PROOF_WINDOW_MS
        {
            return Err(Fault::new(
                "stale_cancellation",
                "Cancellation evidence belongs to another request or interval.",
                true,
            ));
        }
        Ok(proof.anchor)
    }

    pub(crate) fn recorded(
        &mut self,
        receipt: &CancellationObserved,
        anchor: u64,
    ) -> Result<(), Fault> {
        if self.validate_receipt(receipt)? != anchor {
            return Err(Fault::new(
                "stale_cancellation",
                "Cancellation belongs to another protected-idle anchor.",
                true,
            ));
        }
        let proof = self
            .pending
            .remove(&receipt.native_request_id)
            .expect("matched above");
        if !receipt.cancelled {
            return Err(Fault::new(
                "cancellation_unconfirmed",
                "Native cancellation did not succeed.",
                true,
            ));
        }
        let elapsed = receipt
            .clock
            .monotonic_ms
            .checked_sub(anchor)
            .ok_or_else(|| {
                Fault::new(
                    "invalid_event_offset",
                    "Native event predates its protected-idle anchor.",
                    true,
                )
            })?;
        let minutes = Minutes::try_from(elapsed as f64 / 60000.0).map_err(|_| {
            Fault::new(
                "invalid_event_offset",
                "Native event offset is not finite.",
                true,
            )
        })?;
        let kind = match proof.request.kind {
            SuppressionKind::IdleSleepRequest => "idle_sleep_request_cancelled",
        };
        let at = self.records.partition_point(|(clock, sequence, _)| {
            (*clock, *sequence) <= (receipt.clock.monotonic_ms, proof.request.sequence)
        });
        self.records.insert(
            at,
            (
                receipt.clock.monotonic_ms,
                proof.request.sequence,
                SuppressedEvent {
                    kind: kind.into(),
                    epoch_ms: receipt.clock.epoch_ms,
                    minutes_into_protected_idle: minutes,
                },
            ),
        );
        if self.records.len() > MAX_RECORDS {
            self.records.remove(0);
            self.truncated = true;
        }
        Ok(())
    }

    pub(crate) fn project(&self, status: &mut Status) {
        status.protected_idle_suppressed_events = self
            .records
            .iter()
            .map(|(_, _, event)| event.clone())
            .collect();
        status.suppressed_events_truncated = self.truncated;
    }
}
// end projection.rs
