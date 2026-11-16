// runtime.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Small native-confirmed owner slice. Only handlers access policy state.
//! No OS resources, transport, replay restoration or telemetry endpoints live here.

use crate::contract::{
    Apply, AssertionsObserved, ClockSample, Emitter, Empty, EntityId, Error, Fault, IdGeneration,
    IdleObserved, IdleRequest, Outcome, Power, Registry, RegistryBuilder, StartupError, State,
    Status,
};
use ai2screenwake_native::{NativePower, contracts};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

struct InFlight {
    request: Apply,
    events: Emitter,
}

struct Owner {
    session: EntityId,
    generation: u64,
    status: Status,
    in_flight: Option<InFlight>,
    confirmed_on: Option<(EntityId, ClockSample)>,
    samples: HashMap<EntityId, IdleRequest>,
    last_idle_clock: u64,
}

impl Owner {
    fn publish(&mut self, events: &Emitter) {
        self.status.revision += 1;
        events.emit::<Power::Session::Changed>(self.status.clone());
    }

    fn apply(&mut self, events: &Emitter) -> Result<(), Fault> {
        let operation_id = IdGeneration::next_id::<Power::Session::Changed>(&self.status)?;
        let request = Apply {
            session_id: self.session.clone(),
            generation: self.generation,
            operation_id,
            enabled: self.status.desired_on,
        };
        self.status.operation_id = Some(request.operation_id.to_string());
        self.in_flight = Some(InFlight {
            request: request.clone(),
            events: events.clone(),
        });
        self.publish(events);
        events.emit::<Power::Assertions::Apply>(request);
        Ok(())
    }

    fn set(&mut self, enabled: bool, events: &Emitter) -> Result<(), Fault> {
        if self.status.desired_on == enabled && self.status.state != State::Error {
            events.emit::<Power::Session::Snapshot>(self.status.clone());
            return Ok(());
        }
        self.generation += 1;
        self.status.desired_on = enabled;
        self.status.is_on = None;
        self.status.state = State::Pending;
        self.status.protected_idle_duration_seconds = None;
        self.status.protected_idle_suppressed_events.clear();
        self.status.suppressed_events_truncated = false;
        self.confirmed_on = None;
        self.samples.clear();
        if let Some(previous) = &self.in_flight {
            previous
                .events
                .error::<Error::Native::Assertions::Occurred>(Fault::new(
                    "superseded",
                    "A newer protection request superseded this operation.",
                    true,
                ));
            self.publish(events);
            Ok(())
        } else {
            self.apply(events)
        }
    }

    fn observed(&mut self, result: AssertionsObserved, events: &Emitter) -> Result<(), Fault> {
        let Some(pending) = &self.in_flight else {
            return Err(Fault::new(
                "stale_native_observation",
                "Native operation is no longer pending.",
                true,
            ));
        };
        if pending.request != result.request {
            return Err(Fault::new(
                "stale_native_observation",
                "Native operation identity does not match.",
                true,
            ));
        }
        self.in_flight = None;
        if result.request.generation != self.generation {
            // Account for the older physical operation before reconciling the latest intent.
            if result.outcome == Outcome::Failed
                || !result.fault_codes.is_empty()
                || result.held_count > 2
            {
                events.error::<Error::Native::Assertions::Occurred>(Fault::new(
                    "superseded_native_failure",
                    "A superseded native operation failed.",
                    true,
                ));
            }
            return self.apply(events);
        }
        let expected = if result.request.enabled { 2 } else { 0 };
        self.status.operation_id = None;
        if result.outcome != Outcome::Applied
            || result.held_count != expected
            || !result.fault_codes.is_empty()
        {
            self.status.is_on = None;
            self.status.state = State::Error;
            events.error::<Error::Native::Assertions::Occurred>(Fault::new(
                "native_apply_failed",
                "Native protection did not converge.",
                true,
            ));
        } else {
            self.status.is_on = Some(result.request.enabled);
            self.status.state = if result.request.enabled {
                State::On
            } else {
                State::Off
            };
            self.confirmed_on = result
                .request
                .enabled
                .then_some((result.request.operation_id, result.clock));
            self.last_idle_clock = result.clock.monotonic_ms;
        }
        self.publish(events);
        Ok(())
    }

    fn query(&mut self, events: &Emitter) -> Result<(), Fault> {
        if let Some((episode, _)) = &self.confirmed_on {
            let request = IdleRequest {
                session_id: self.session.clone(),
                generation: self.generation,
                episode_id: episode.clone(),
                sample_id: IdGeneration::next_id::<Power::Session::Query>(&Empty {})?,
            };
            self.samples
                .insert(request.sample_id.clone(), request.clone());
            events.emit::<Power::Activity::Sample>(request);
        } else {
            events.emit::<Power::Session::Snapshot>(self.status.clone());
        }
        Ok(())
    }

    fn idle(&mut self, result: IdleObserved, events: &Emitter) -> Result<(), Fault> {
        if self.samples.get(&result.request.sample_id) != Some(&result.request)
            || self.confirmed_on.is_none()
        {
            return Err(Fault::new(
                "stale_idle_observation",
                "Idle observation belongs to an old request or interval.",
                true,
            ));
        }
        let (_, anchor) = self.confirmed_on.as_ref().expect("checked above");
        self.samples.remove(&result.request.sample_id);
        if result.clock.monotonic_ms < self.last_idle_clock {
            events.emit::<Power::Session::Snapshot>(self.status.clone());
            return Ok(());
        }
        self.last_idle_clock = result.clock.monotonic_ms;
        self.status.protected_idle_duration_seconds = None;
        match (
            result.idle_age_ms,
            result.fault,
            result.clock.monotonic_ms.checked_sub(anchor.monotonic_ms),
        ) {
            (Some(age), None, Some(elapsed)) => {
                self.status.protected_idle_duration_seconds = Some(age.min(elapsed) / 1000);
            }
            (_, fault, _) => {
                events.error::<Error::Native::Activity::Occurred>(fault.unwrap_or_else(|| {
                    Fault::new(
                        "invalid_idle_sample",
                        "Native idle observation is unavailable or inconsistent.",
                        true,
                    )
                }));
            }
        }
        self.publish(events);
        Ok(())
    }
}

/// Start an isolated OFF owner and return only its Actions port.
///
/// Native collaboration is mandatory. Register observers before emitting Query or
/// TurnOn; no startup snapshot can race their registration. Production hosts must
/// supply a real initially resource-free native implementation.
pub fn start(native: Option<Arc<dyn NativePower>>) -> Result<Registry, StartupError> {
    let native =
        native.ok_or_else(|| StartupError("native power collaborator is required".into()))?;
    let session = IdGeneration::next_id::<Power::Session::Query>(&Empty {})
        .map_err(|fault| StartupError(fault.message))?;
    let owner = Arc::new(Mutex::new(Owner {
        session,
        generation: 0,
        status: Status::off(),
        in_flight: None,
        confirmed_on: None,
        samples: HashMap::new(),
        last_idle_clock: 0,
    }));
    let mut builder = RegistryBuilder::default();
    contracts::register(&mut builder, native)?;
    builder.require::<Power::Session::TurnOn>();
    builder.require::<Power::Session::TurnOff>();
    builder.require::<Power::Session::Query>();
    builder.require::<Power::Session::Quit>();
    let on = owner.clone();
    builder.on::<Power::Session::TurnOn>(move |_, events| {
        on.lock().expect("owner poisoned").set(true, events)
    })?;
    let off = owner.clone();
    builder.on::<Power::Session::TurnOff>(move |_, events| {
        off.lock().expect("owner poisoned").set(false, events)
    })?;
    let query = owner.clone();
    builder.on::<Power::Session::Query>(move |_, events| {
        query.lock().expect("owner poisoned").query(events)
    })?;
    builder.on::<Power::Session::Quit>(|_, events| {
        events.emit::<Power::Session::TurnOff>(Empty {});
        Ok(())
    })?;
    let observed = owner.clone();
    builder.on::<Power::Assertions::Observed>(move |result, events| {
        observed
            .lock()
            .expect("owner poisoned")
            .observed(result, events)
    })?;
    builder.on::<Power::Activity::Observed>(move |result, events| {
        owner.lock().expect("owner poisoned").idle(result, events)
    })?;
    builder.on::<Power::Session::Changed>(|status, events| {
        events.emit::<Power::Session::Snapshot>(status);
        Ok(())
    })?;
    builder.build()
}
// end runtime.rs
