// runtime.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Small native-confirmed owner slice. Only handlers access policy state.
//! No OS resources, transport, replay restoration or telemetry endpoints live here.

use crate::contract::{
    ActionJournal, Apply, AssertionsObserved, CancellationObserved, ClockSample,
    CornerAuthorization, CornerConfigured, CornerLoss, CornerPreference, CornerRefresh,
    CornerRefreshed, CornerStatus, CorrelationId, Emitter, Empty, EntityId, Error, FailedApply,
    Fault, IdGeneration, IdleObserved, IdleRequest, IdleVetoFailure, InterruptionObserved,
    LifecycleAvailability, Outcome, OwnerClosed, Power, Registry, RegistryBuilder, RejectedApply,
    SleepPhase, StartupError, State, Status, SuppressionRequest, Ui,
};
use crate::projection::{Kind, Proofs, Requests};
use ai2screenwake_native::{NativePower, contracts};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const IDLE_ANCHOR_DRIFT_MS: u64 = 1;

struct InFlight {
    request: Apply,
}

struct PendingCornerConfiguration {
    correlation: CorrelationId,
    revision: u64,
    request: Apply,
    loss: Option<CornerLoss>,
}

struct Owner {
    session: EntityId,
    generation: u64,
    status: Status,
    in_flight: Option<InFlight>,
    confirmed_on: Option<(EntityId, ClockSample)>,
    requests: Requests,
    last_idle_clock: u64,
    toggle_tail: Option<(CorrelationId, bool)>,
    active: Option<Apply>,
    sleeping: bool,
    interruption: Option<InterruptionObserved>,
    protected_anchor: u64,
    closing: bool,
    proofs: Proofs,
    idle_fault: Option<Fault>,
    storage_fault: Option<Fault>,
    lifecycle: Option<LifecycleAvailability>,
    corner_loss: Option<CornerLoss>,
    corner_installation: Option<(Apply, EntityId)>,
    protection_fault: Option<Fault>,
    veto_loss: Option<IdleVetoFailure>,
    setup: crate::accessibility::Setup,
    corner_control: Ui::CornerControlPresentation,
    corner_query: Option<CorrelationId>,
    corner_configuration: Option<PendingCornerConfiguration>,
}

impl Owner {
    fn active_corner_request(&self) -> Option<Apply> {
        self.active.clone().filter(|request| {
            self.status.is_on == Some(true)
                && self.status.state == State::On
                && self.status.desired_on
                && !self.closing
                && self.in_flight.is_none()
                && self.storage_fault.is_none()
                && self.lifecycle_fault().is_none()
                && !self.sleeping
                && request.generation == self.generation
        })
    }

    fn advance_corner_revision(&mut self, events: &Emitter) -> bool {
        if let Some(revision) = self.corner_control.revision.checked_add(1) {
            self.corner_control.revision = revision;
            true
        } else {
            events.error::<Error::Native::Corners::Occurred>(Fault::new(
                "corner_control_exhausted",
                "Corner control identity is exhausted.",
                false,
            ));
            false
        }
    }

    fn observe_corner_trust(&mut self, trusted: bool, events: &Emitter) -> bool {
        if self.corner_control.trusted != Some(trusted) {
            if !self.advance_corner_revision(events) {
                return false;
            }
            self.corner_control.trusted = Some(trusted);
        }
        events.emit::<Ui::Accessibility::Control::Changed>(self.corner_control.clone());
        true
    }

    fn corner_preference(&self) -> CornerPreference {
        CornerPreference {
            revision: self.corner_control.revision,
            enabled: self.corner_control.enabled,
            authorization: CornerAuthorization {
                revision: self.status.revision,
                request: self.active_corner_request(),
            },
        }
    }

    fn query_corners(&mut self, events: &Emitter) {
        if self.closing {
            events.error::<Error::Native::Corners::Occurred>(Fault::new(
                "corner_control_closing",
                "Corner controls are unavailable during shutdown.",
                true,
            ));
            return;
        }
        self.corner_query = Some(events.correlation());
        events.emit::<Power::Corners::Access::Query>(Empty {});
    }

    fn configure_corner_preference(&mut self, events: &Emitter) {
        let preference = self.corner_preference();
        self.corner_configuration = preference
            .authorization
            .request
            .clone()
            .filter(|_| preference.enabled)
            .map(|request| PendingCornerConfiguration {
                correlation: events.correlation(),
                revision: preference.revision,
                request,
                loss: None,
            });
        events.emit::<Power::Corners::Guard::Configure>(preference);
    }

    fn corners_queried(&mut self, result: Ui::CornerTrust, events: &Emitter) {
        if self.closing || self.corner_query.as_ref() != Some(&events.correlation()) {
            return;
        }
        self.corner_query = None;
        if self.observe_corner_trust(result.trusted, events) {
            self.configure_corner_preference(events);
        }
    }

    fn activate_corners(&mut self, events: &Emitter) {
        if self.closing {
            events.error::<Error::Native::Corners::Occurred>(Fault::new(
                "corner_control_closing",
                "Corner controls are unavailable during shutdown.",
                true,
            ));
        } else if self.corner_control.trusted != Some(true) {
            events.emit::<Ui::Accessibility::Setup::Activated>(Empty {});
        } else if self.advance_corner_revision(events) {
            self.corner_control.enabled = !self.corner_control.enabled;
            self.setup.refresh = None;
            events.emit::<Ui::Accessibility::Control::Changed>(self.corner_control.clone());
            self.configure_corner_preference(events);
        }
    }

    fn corners_configured(&mut self, mut result: CornerConfigured, events: &Emitter) {
        if self.closing
            || result.preference.revision != self.corner_control.revision
            || result.preference.enabled != self.corner_control.enabled
            || result.preference.authorization.request != self.active_corner_request()
        {
            return;
        }
        let matching = self.corner_configuration.as_ref().is_some_and(|pending| {
            pending.correlation == events.correlation()
                && pending.revision == result.preference.revision
                && result.preference.authorization.request.as_ref() == Some(&pending.request)
        });
        if result.preference.enabled
            && result.preference.authorization.request.is_some()
            && !matching
        {
            return;
        }
        if matching {
            let pending = self
                .corner_configuration
                .take()
                .expect("matching pending configuration");
            if let Some(loss) = pending.loss.filter(|loss| {
                loss.installation_id.is_some() && loss.installation_id == result.installation_id
            }) {
                result.ready = false;
                result.fault = Some(loss.fault);
            }
        }
        if let Some(loss) = self.corner_loss.as_ref().filter(|loss| {
            loss.installation_id.is_some()
                && loss.installation_id == result.installation_id
                && result.preference.authorization.request.as_ref() == Some(&loss.request)
        }) {
            result.ready = false;
            result.fault = Some(loss.fault.clone());
        }
        self.corner_installation = result
            .preference
            .authorization
            .request
            .clone()
            .zip(result.installation_id.clone());
        if let Some(fault) = result.fault.clone() {
            if let Some(request) = result.preference.authorization.request.clone() {
                self.corner_loss = Some(CornerLoss {
                    installation_id: result.installation_id,
                    request,
                    fault,
                });
            }
        } else {
            self.corner_loss = None;
        }
        events.emit::<Power::Corners::Guard::Changed>(CornerStatus {
            fault: self.corner_loss.as_ref().map(|loss| loss.fault.clone()),
        });
        if self.setup.view.visible && self.setup.view.trusted == Some(true) {
            self.setup.view.guard_ready = result.ready;
            self.setup.ready = result
                .ready
                .then(|| result.preference.authorization.request.clone())
                .flatten();
            self.setup.update_message(result.fault.as_ref());
            self.setup.publish(events);
        }
    }

    fn setup_observed(&mut self, result: Ui::AccessibilityObservation, events: &Emitter) {
        let trusted = result.trusted;
        if !self.setup.expecting(result.session) {
            return;
        }
        if !self.setup.observed(result, events) {
            self.observe_corner_trust(false, events);
            return;
        }
        if !self.observe_corner_trust(trusted, events) || !self.corner_control.enabled {
            return;
        }
        let Some(request) = self.active_corner_request() else {
            return;
        };
        if self.setup.begin_refresh(&request) {
            events.emit::<Power::Corners::Guard::Recheck>(CornerRefresh {
                session: self.setup.view.session,
                authorization: CornerAuthorization {
                    revision: self.status.revision,
                    request: Some(request),
                },
            });
        }
    }

    fn setup_admits(&self, refresh: &CornerRefresh) -> bool {
        self.setup.current(refresh.session)
            && self.corner_control.enabled
            && self.setup.view.trusted == Some(true)
            && self.status.is_on == Some(true)
            && self.status.state == State::On
            && self.status.desired_on
            && !self.closing
            && self.in_flight.is_none()
            && self.storage_fault.is_none()
            && self.lifecycle_fault().is_none()
            && !self.sleeping
            && refresh
                .authorization
                .request
                .as_ref()
                .is_some_and(|request| {
                    self.active.as_ref() == Some(request)
                        && request.generation == self.generation
                        && self.setup.refresh.as_ref() == Some(request)
                })
    }

    fn setup_refreshed(&mut self, mut result: CornerRefreshed, events: &Emitter) {
        if !self.setup_admits(&result.refresh) {
            return;
        }
        if let Some(loss) = self.corner_loss.as_ref().filter(|loss| {
            loss.installation_id.is_some()
                && loss.installation_id == result.installation_id
                && result.refresh.authorization.request.as_ref() == Some(&loss.request)
        }) {
            result.ready = false;
            result.fault = Some(loss.fault.clone());
        }
        self.corner_installation = result.installation_id.map(|installation| {
            (
                result
                    .refresh
                    .authorization
                    .request
                    .clone()
                    .expect("admitted refresh"),
                installation,
            )
        });
        self.setup.refresh = None;
        self.setup.view.guard_ready = result.ready;
        self.setup.ready = result.ready.then(|| {
            result
                .refresh
                .authorization
                .request
                .clone()
                .expect("admitted refresh")
        });
        if let Some(fault) = result.fault {
            self.setup.update_message(Some(&fault));
        } else if result.ready {
            self.corner_loss = None;
            self.setup.update_message(None);
            events.emit::<Power::Corners::Guard::Changed>(CornerStatus { fault: None });
        } else {
            self.setup.update_message(None);
        }
        self.setup.publish(events);
    }

    fn lifecycle_fault(&self) -> Option<Fault> {
        self.lifecycle
            .as_ref()
            .filter(|source| !source.available)
            .map(|_| {
                Fault::new(
                    "lifecycle_unavailable",
                    "Sleep/wake observation is unavailable. Protection cannot be enabled.",
                    true,
                )
            })
    }

    fn availability(&mut self, source: LifecycleAvailability, events: &Emitter) {
        if self
            .lifecycle
            .as_ref()
            .is_some_and(|current| source.revision <= current.revision)
        {
            return;
        }
        self.lifecycle = Some(source);
        if let Some(fault) = self.lifecycle_fault() {
            self.confirmed_on = None;
            self.reset_interval(0);
            if self.status.desired_on {
                self.protection_fault.get_or_insert(fault.clone());
                events.error::<Error::Native::Activity::Occurred>(fault.clone());
                self.status.is_on = None;
                self.status.state = State::Error;
                self.requests
                    .finish_where(&self.status, Some(fault), |_| true);
            }
        }
        self.publish(events);
    }

    fn storage_failed(&mut self, fault: Fault, events: &Emitter) {
        if self.storage_fault.is_none() {
            self.storage_fault = Some(fault.clone());
            self.status.state = State::Error;
            self.status.protected_idle_duration_seconds = None;
            self.idle_fault = Some(fault.clone());
            self.requests
                .finish_where(&self.status, Some(fault), |_| true);
            self.publish(events);
        }
    }

    fn observe_storage(&mut self, events: &Emitter) {
        if let Some(fault) = events.persistence_error() {
            self.storage_failed(fault.clone(), events);
        }
    }

    fn closed(&self, events: &Emitter) {
        if self.closing && self.status.is_on == Some(false) && self.in_flight.is_none() {
            events.emit::<Power::Session::Protection::Closed>(OwnerClosed {
                session_id: self.session.clone(),
                generation: self.generation,
            });
        }
    }

    fn quit(&mut self, events: &Emitter) {
        self.setup.dismiss(events);
        self.observe_storage(events);
        self.closing = true;
        self.toggle_tail = None;
        events.emit::<Power::Session::Protection::TurnOff>(Empty {});
    }

    fn reset_interval(&mut self, anchor: u64) {
        self.protected_anchor = anchor;
        self.status.protected_idle_duration_seconds = None;
        self.status.protected_idle_suppressed_events.clear();
        self.status.suppressed_events_truncated = false;
        self.proofs.reset();
        self.idle_fault = None;
    }

    fn toggle(&mut self, events: &Emitter) {
        self.setup.dismiss(events);
        self.observe_storage(events);
        if self.closing {
            Requests::settled(
                events,
                &self.status,
                Some(Fault::new(
                    "owner_closing",
                    "This owner is closing and cannot enable protection.",
                    true,
                )),
                true,
            );
            return;
        }
        if let Some(fault) = &self.storage_fault {
            Requests::settled(events, &self.status, Some(fault.clone()), true);
            return;
        }
        let target = !self
            .toggle_tail
            .as_ref()
            .map_or(self.status.desired_on, |(_, target)| *target);
        self.toggle_tail = Some((events.correlation(), target));
        if target {
            events.emit::<Power::Session::Protection::TurnOn>(Empty {});
        } else {
            events.emit::<Power::Session::Protection::TurnOff>(Empty {});
        }
    }

    fn publish(&mut self, events: &Emitter) {
        self.status.revision += 1;
        if self.corner_configuration.as_ref().is_some_and(|pending| {
            !self.corner_control.enabled
                || pending.revision != self.corner_control.revision
                || self.active_corner_request().as_ref() != Some(&pending.request)
        }) {
            self.corner_configuration = None;
        }
        let request = if self.status.desired_on
            && !self.closing
            && self.storage_fault.is_none()
            && self.lifecycle_fault().is_none()
            && !self.sleeping
            && self.status.state != State::Error
        {
            self.in_flight
                .as_ref()
                .map(|pending| &pending.request)
                .or(self.active.as_ref())
                .filter(|request| request.enabled && request.generation == self.generation)
                .cloned()
        } else {
            None
        };
        if self.setup.ready.as_ref().is_some_and(|ready| {
            request.as_ref() != Some(ready)
                || self.status.is_on != Some(true)
                || self.status.state != State::On
        }) {
            self.setup.invalidate(events);
        }
        events.emit::<Power::Corners::Guard::Authorize>(CornerAuthorization {
            revision: self.status.revision,
            request,
        });
        events.emit::<Power::Corners::Guard::Changed>(CornerStatus {
            fault: self
                .corner_loss
                .as_ref()
                .filter(|loss| {
                    self.status.desired_on
                        && !self.closing
                        && loss.request.generation == self.generation
                        && (self.active.as_ref() == Some(&loss.request)
                            || self.in_flight.as_ref().map(|pending| &pending.request)
                                == Some(&loss.request))
                })
                .map(|loss| loss.fault.clone()),
        });
        events.emit::<Power::Session::Protection::Changed>(self.status.clone());
    }

    fn corner_lost(&mut self, loss: CornerLoss, events: &Emitter) {
        if !self.corner_control.enabled
            || !self.status.desired_on
            || self.closing
            || loss.request.generation != self.generation
            || (self.active.as_ref() != Some(&loss.request)
                && self.in_flight.as_ref().map(|pending| &pending.request) != Some(&loss.request))
        {
            return;
        }
        if loss.installation_id.is_some()
            && self
                .corner_installation
                .as_ref()
                .is_some_and(|(request, installation)| {
                    request == &loss.request && Some(installation) != loss.installation_id.as_ref()
                })
        {
            if let Some(pending) = self.corner_configuration.as_mut().filter(|pending| {
                pending.correlation == events.correlation()
                    && pending.revision == self.corner_control.revision
                    && pending.request == loss.request
            }) {
                pending.loss = Some(loss);
                return;
            }
            if self.setup.refresh.is_none() {
                return;
            }
        }
        self.corner_loss = Some(loss.clone());
        if self.setup.view.visible {
            if self.setup.refresh.is_none() || loss.installation_id.is_none() {
                self.setup.invalidate(events);
            } else {
                self.setup.ready = None;
            }
            self.setup.view.guard_ready = false;
            self.setup.view.message =
                crate::accessibility::failure_message(&loss.fault, self.setup.view.macos_major);
            self.setup.publish(events);
        }
        self.publish(events);
    }

    /// A lost current veto cannot restore protection through later wake or apply evidence.
    fn veto_failed(&mut self, loss: IdleVetoFailure, events: &Emitter) {
        if self.active.as_ref() != Some(&loss.request)
            && self.in_flight.as_ref().map(|pending| &pending.request) != Some(&loss.request)
        {
            return;
        }
        self.veto_loss = Some(loss.clone());
        self.confirmed_on = None;
        self.reset_interval(0);
        if self.status.desired_on && loss.request.generation == self.generation {
            self.status.is_on = None;
            self.status.state = State::Error;
            self.protection_fault.get_or_insert(loss.fault.clone());
            self.requests
                .finish_where(&self.status, Some(loss.fault), |_| true);
        }
        self.publish(events);
    }

    fn apply(&mut self, events: &Emitter) -> Result<(), Fault> {
        if self.status.desired_on {
            if let Some(fault) = self.lifecycle_fault() {
                self.status.state = State::Error;
                self.status.is_on = None;
                self.requests
                    .finish_where(&self.status, Some(fault), |_| true);
                self.publish(events);
                return Ok(());
            }
            if let Some(fault) = &self.storage_fault {
                self.status.state = State::Error;
                self.status.is_on = None;
                self.requests
                    .finish_where(&self.status, Some(fault.clone()), |_| true);
                self.publish(events);
                return Ok(());
            }
        }
        let operation_id =
            match IdGeneration::next_id::<Power::Session::Protection::Changed>(&self.status) {
                Ok(id) => id,
                Err(fault) => {
                    self.status.state = State::Error;
                    events.error::<Error::Dispatch::Validation::Occurred>(fault.clone());
                    self.publish(events);
                    self.requests
                        .finish_where(&self.status, Some(fault), |_| true);
                    return Ok(());
                }
            };
        let request = Apply {
            session_id: self.session.clone(),
            generation: self.generation,
            operation_id,
            enabled: self.status.desired_on,
        };
        self.status.operation_id = Some(request.operation_id.to_string());
        self.in_flight = Some(InFlight {
            request: request.clone(),
        });
        self.publish(events);
        events.emit::<Power::Assertions::Resources::Apply>(request);
        Ok(())
    }

    fn set(&mut self, enabled: bool, events: &Emitter) -> Result<(), Fault> {
        self.observe_storage(events);
        if self
            .toggle_tail
            .as_ref()
            .is_some_and(|(id, target)| *id == events.correlation() && *target == enabled)
        {
            self.toggle_tail = None;
        }
        if enabled && self.closing {
            Requests::settled(
                events,
                &self.status,
                Some(Fault::new(
                    "owner_closing",
                    "This owner is closing and cannot enable protection.",
                    true,
                )),
                true,
            );
            return Ok(());
        }
        if enabled {
            if let Some(fault) = self.lifecycle_fault() {
                Requests::settled(events, &self.status, Some(fault), true);
                return Ok(());
            }
        }
        if let Some(fault) = &self.storage_fault {
            if enabled || (self.status.is_on == Some(false) && self.in_flight.is_none()) {
                Requests::settled(events, &self.status, Some(fault.clone()), true);
                self.closed(events);
                return Ok(());
            }
        }
        if self.status.desired_on == enabled && self.status.state != State::Error {
            if self.in_flight.is_some() || self.status.is_on != Some(enabled) {
                if let Err(fault) = self.requests.add(
                    events,
                    Kind::Setter {
                        generation: self.generation,
                    },
                ) {
                    Requests::settled(events, &self.status, Some(fault), true);
                } else {
                    events.emit::<Power::Session::Protection::Snapshot>(self.status.clone());
                }
            } else {
                Requests::settled(events, &self.status, None, true);
                self.closed(events);
            }
            return Ok(());
        }
        if !enabled {
            self.requests.finish_where(
                &self.status,
                Some(Fault::new(
                    "superseded",
                    "Explicit cleanup superseded this pending request.",
                    true,
                )),
                |_| true,
            );
        }
        if let Err(fault) = self.requests.add(
            events,
            Kind::Setter {
                generation: self.generation + 1,
            },
        ) {
            Requests::settled(events, &self.status, Some(fault), true);
            return Ok(());
        }
        self.generation += 1;
        self.status.desired_on = enabled;
        self.status.is_on = None;
        self.status.state = State::Pending;
        self.protection_fault = None;
        self.reset_interval(0);
        self.confirmed_on = None;
        let generation = self.generation;
        self.requests.finish_where(
            &self.status,
            Some(Fault::new(
                "superseded",
                "A newer protection request superseded this request.",
                true,
            )),
            |kind| !matches!(kind, Kind::Setter { generation: current } if *current == generation),
        );
        if self.in_flight.is_some() {
            self.publish(events);
            Ok(())
        } else {
            self.apply(events)
        }
    }

    fn observed(&mut self, result: AssertionsObserved, events: &Emitter) -> Result<(), Fault> {
        self.observe_storage(events);
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
        if !result.request.enabled
            && result.outcome == Outcome::Applied
            && result.held_count == 0
            && result.fault_codes.is_empty()
        {
            self.active = None;
            self.sleeping = false;
            self.interruption = None;
        }
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
            self.active = None;
            let fault = self
                .protection_fault
                .get_or_insert_with(|| {
                    Fault::new(
                        result
                            .fault_codes
                            .first()
                            .map_or("native_apply_failed", String::as_str),
                        "Native protection did not converge.",
                        true,
                    )
                })
                .clone();
            events.error::<Error::Native::Assertions::Occurred>(fault);
        } else {
            if result.request.enabled && self.active.as_ref() != Some(&result.request) {
                self.proofs = Proofs::default();
            }
            self.active = result.request.enabled.then_some(result.request.clone());
            let newer_failed_wake = self.interruption.as_ref().is_some_and(|interruption| {
                interruption.request == result.request
                    && interruption.phase == SleepPhase::DidWake
                    && interruption.held_count != 2
                    && interruption.clock.monotonic_ms > result.clock.monotonic_ms
            });
            let source_unavailable = self.lifecycle_fault().is_some()
                || self
                    .veto_loss
                    .as_ref()
                    .is_some_and(|loss| loss.request == result.request);
            if result.request.enabled && (self.sleeping || newer_failed_wake || source_unavailable)
            {
                self.status.is_on = None;
                self.status.state = if newer_failed_wake || source_unavailable {
                    State::Error
                } else {
                    State::Pending
                };
                self.confirmed_on = None;
            } else {
                let clock = self
                    .interruption
                    .as_ref()
                    .filter(|interruption| {
                        interruption.request == result.request
                            && interruption.phase == SleepPhase::DidWake
                            && interruption.clock.monotonic_ms > result.clock.monotonic_ms
                    })
                    .map_or(result.clock, |interruption| interruption.clock);
                self.status.is_on = Some(result.request.enabled);
                self.status.state = if result.request.enabled {
                    State::On
                } else {
                    State::Off
                };
                self.confirmed_on = result
                    .request
                    .enabled
                    .then_some((result.request.operation_id, clock));
                self.reset_interval(clock.monotonic_ms);
                self.last_idle_clock = clock.monotonic_ms;
                self.protection_fault = None;
            }
        }
        let native_error = (self.status.state == State::Error).then(|| {
            self.protection_fault.clone().unwrap_or_else(|| {
                Fault::new(
                    "native_apply_failed",
                    "Native protection did not converge.",
                    true,
                )
            })
        });
        if self.storage_fault.is_some() {
            self.status.state = State::Error;
            self.status.protected_idle_duration_seconds = None;
        }
        self.publish(events);
        let error = self.storage_fault.clone().or(native_error);
        let generation = self.generation;
        if self.status.is_on == Some(self.status.desired_on) || error.is_some() {
            self.requests.finish_where(&self.status, error,
                |kind| matches!(kind, Kind::Setter { generation: current } if *current == generation));
        }
        self.closed(events);
        Ok(())
    }

    fn query(&mut self, events: &Emitter) -> Result<(), Fault> {
        self.observe_storage(events);
        if let Some(fault) = &self.storage_fault {
            Requests::settled(events, &self.status, Some(fault.clone()), true);
            return Ok(());
        }
        if let Some((episode, _)) = &self.confirmed_on {
            let sample_id =
                match IdGeneration::next_id::<Power::Session::Protection::Query>(&Empty {}) {
                    Ok(id) => id,
                    Err(fault) => {
                        Requests::settled(events, &self.status, Some(fault), true);
                        return Ok(());
                    }
                };
            let request = IdleRequest {
                session_id: self.session.clone(),
                generation: self.generation,
                episode_id: episode.clone(),
                sample_id,
            };
            if let Err(fault) = self.requests.add(events, Kind::Query(request.clone())) {
                Requests::settled(events, &self.status, Some(fault), true);
            } else {
                events.emit::<Power::Activity::System::Sample>(request);
            }
        } else {
            let fault = (self.status.state == State::Error).then(|| {
                self.protection_fault.clone().unwrap_or_else(|| {
                    Fault::new(
                        "protection_unresolved",
                        "Protection has not converged.",
                        true,
                    )
                })
            });
            Requests::settled(events, &self.status, fault, true);
        }
        Ok(())
    }

    fn idle(&mut self, result: IdleObserved, events: &Emitter) -> Result<(), Fault> {
        self.observe_storage(events);
        if self.storage_fault.is_some() {
            return Ok(());
        }
        let correlation = self.requests.query(&result.request);
        if correlation.is_none() || self.confirmed_on.is_none() {
            return Err(Fault::new(
                "stale_idle_observation",
                "Idle observation belongs to an old request or interval.",
                true,
            ));
        }
        let on = self.confirmed_on.as_ref().expect("checked above").1;
        let correlation = correlation.expect("checked above");
        if result.clock.monotonic_ms < on.monotonic_ms
            || result.clock.monotonic_ms < self.protected_anchor
        {
            let fault = Fault::new(
                "stale_idle_observation",
                "Idle sampling predates the current protected-idle interval.",
                true,
            );
            let mut status = self.status.clone();
            status.protected_idle_duration_seconds = None;
            self.requests.finish(&correlation, &status, Some(fault));
            return Ok(());
        }
        if result.clock.monotonic_ms < self.last_idle_clock {
            events.emit::<Power::Session::Protection::Snapshot>(self.status.clone());
            let fault = self.idle_fault.clone().or_else(|| {
                self.status
                    .protected_idle_duration_seconds
                    .is_none()
                    .then(|| {
                        Fault::new(
                            "stale_idle_observation",
                            "A newer native observation has no measured duration.",
                            true,
                        )
                    })
            });
            self.requests.finish(&correlation, &self.status, fault);
            return Ok(());
        }
        self.last_idle_clock = result.clock.monotonic_ms;
        self.status.protected_idle_duration_seconds = None;
        let mut failure = None;
        match (
            result.idle_age_ms,
            result.fault,
            result.clock.monotonic_ms.checked_sub(on.monotonic_ms),
        ) {
            (Some(age), None, Some(elapsed)) => {
                let anchor = on
                    .monotonic_ms
                    .max(result.clock.monotonic_ms.saturating_sub(age));
                if anchor.saturating_add(IDLE_ANCHOR_DRIFT_MS) < self.protected_anchor {
                    let fault = Fault::new(
                        "stale_idle_observation",
                        "Native idle sampling contradicts observed activity.",
                        true,
                    );
                    events.error::<Error::Native::Activity::Occurred>(fault.clone());
                    failure = Some(fault);
                } else {
                    if anchor > self.protected_anchor.saturating_add(IDLE_ANCHOR_DRIFT_MS) {
                        self.reset_interval(anchor);
                    }
                    self.status.protected_idle_duration_seconds = Some(age.min(elapsed) / 1000);
                }
            }
            (_, fault, _) => {
                let fault = fault.unwrap_or_else(|| {
                    Fault::new(
                        "invalid_idle_sample",
                        "Native idle observation is unavailable or inconsistent.",
                        true,
                    )
                });
                events.error::<Error::Native::Activity::Occurred>(fault.clone());
                failure = Some(fault);
            }
        }
        self.idle_fault = failure.clone();
        self.publish(events);
        self.requests.finish(&correlation, &self.status, failure);
        Ok(())
    }

    fn interrupted(&mut self, result: InterruptionObserved, events: &Emitter) -> Result<(), Fault> {
        self.observe_storage(events);
        if let Some(fault) = self.lifecycle_fault() {
            return Err(fault);
        }
        let expected = self
            .in_flight
            .as_ref()
            .map(|pending| &pending.request)
            .or(self.active.as_ref());
        if !result.request.enabled
            || expected != Some(&result.request)
            || result.request.generation != self.generation
            || self.interruption.as_ref().is_some_and(|previous| {
                previous.request == result.request
                    && previous.clock.monotonic_ms >= result.clock.monotonic_ms
            })
        {
            return Err(Fault::new(
                "stale_interruption",
                "The native interruption is no longer current.",
                true,
            ));
        }
        self.interruption = Some(result.clone());
        self.sleeping = result.phase == SleepPhase::WillSleep;
        if let Some(loss) = &self.veto_loss {
            if loss.request == result.request {
                return Err(loss.fault.clone());
            }
        }
        self.reset_interval(result.clock.monotonic_ms);
        self.last_idle_clock = result.clock.monotonic_ms;
        if result.phase == SleepPhase::WillSleep {
            self.sleeping = true;
            self.confirmed_on = None;
            self.status.is_on = None;
            self.status.state = State::Pending;
            self.requests.finish_where(
                &self.status,
                Some(Fault::new(
                    "system_sleep",
                    "The system interrupted the protected-idle interval.",
                    true,
                )),
                |_| true,
            );
        } else {
            self.sleeping = false;
            if result.held_count != 2 {
                self.confirmed_on = None;
                self.status.is_on = None;
                self.status.state = State::Error;
                let fault = Fault::new(
                    "wake_resources_unresolved",
                    "Native protection did not survive wake.",
                    true,
                );
                self.protection_fault.get_or_insert(fault.clone());
                events.error::<Error::Native::Assertions::Occurred>(fault.clone());
                self.requests
                    .finish_where(&self.status, Some(fault), |_| true);
            } else if self.in_flight.is_none() {
                self.confirmed_on = Some((result.request.operation_id, result.clock));
                self.status.is_on = Some(true);
                self.status.state = State::On;
                let generation = self.generation;
                self.requests.finish_where(&self.status, None,
                    |kind| matches!(kind, Kind::Setter { generation: current } if *current == generation));
            }
        }
        self.publish(events);
        Ok(())
    }

    fn proof_anchor(
        &self,
        request: &IdleRequest,
        clock: ClockSample,
        age: u64,
        code: &str,
    ) -> Result<u64, Fault> {
        let Some((episode, on)) = &self.confirmed_on else {
            return Err(Fault::new(
                code,
                "Native evidence has no current protected acquisition.",
                true,
            ));
        };
        if self.status.is_on != Some(true)
            || request.session_id != self.session
            || request.generation != self.generation
            || &request.episode_id != episode
            || clock.monotonic_ms < on.monotonic_ms
        {
            return Err(Fault::new(
                code,
                "Native evidence belongs to an old owner or acquisition.",
                true,
            ));
        }
        let anchor = on.monotonic_ms.max(clock.monotonic_ms.saturating_sub(age));
        if anchor.saturating_add(IDLE_ANCHOR_DRIFT_MS) < self.protected_anchor {
            return Err(Fault::new(
                code,
                "Native evidence belongs to an earlier protected-idle interval.",
                true,
            ));
        }
        Ok(
            if anchor.abs_diff(self.protected_anchor) <= IDLE_ANCHOR_DRIFT_MS {
                self.protected_anchor
            } else {
                anchor
            },
        )
    }

    fn suppression_requested(
        &mut self,
        request: SuppressionRequest,
        events: &Emitter,
    ) -> Result<(), Fault> {
        self.observe_storage(events);
        if let Some(fault) = &self.storage_fault {
            return Err(fault.clone());
        }
        self.proofs.validate_request(&request)?;
        let anchor = self.proof_anchor(
            &request.request,
            request.clock,
            request.idle_age_ms,
            "stale_suppression",
        )?;
        let changed_interval = anchor > self.protected_anchor;
        if changed_interval {
            self.reset_interval(anchor);
        }
        if self.proofs.expire(request.clock.monotonic_ms) > 0 {
            events.error::<Error::Native::Activity::Occurred>(Fault::new(
                "cancellation_proof_expired",
                "Unacknowledged native cancellation evidence expired.",
                true,
            ));
        }
        let clock = request.clock;
        let age = request.idle_age_ms;
        self.proofs.requested(request, anchor)?;
        let fresh_observation = clock.monotonic_ms >= self.last_idle_clock;
        if fresh_observation {
            let on = self
                .confirmed_on
                .as_ref()
                .expect("proof guard checked above")
                .1;
            self.status.protected_idle_duration_seconds =
                Some(age.min(clock.monotonic_ms - on.monotonic_ms) / 1000);
            self.last_idle_clock = clock.monotonic_ms;
            self.idle_fault = None;
        }
        if changed_interval || fresh_observation {
            self.publish(events);
        }
        Ok(())
    }

    fn suppression_recorded(
        &mut self,
        receipt: CancellationObserved,
        events: &Emitter,
    ) -> Result<(), Fault> {
        self.observe_storage(events);
        if let Some(fault) = &self.storage_fault {
            return Err(fault.clone());
        }
        let anchor = self.proof_anchor(
            &receipt.request,
            receipt.clock,
            receipt.idle_age_ms,
            "stale_cancellation",
        )?;
        self.proofs.validate_receipt(&receipt)?;
        if anchor > self.protected_anchor {
            self.reset_interval(anchor);
            events.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                "stale_cancellation",
                "Activity changed before this cancellation completed.",
                true,
            ));
            self.publish(events);
            return Ok(());
        }
        self.proofs.recorded(&receipt, anchor)?;
        self.proofs.project(&mut self.status);
        if receipt.clock.monotonic_ms >= self.last_idle_clock {
            let (_, on) = self
                .confirmed_on
                .as_ref()
                .expect("proof guard checked above");
            self.status.protected_idle_duration_seconds = Some(
                receipt
                    .idle_age_ms
                    .min(receipt.clock.monotonic_ms - on.monotonic_ms)
                    / 1000,
            );
            self.last_idle_clock = receipt.clock.monotonic_ms;
            self.idle_fault = None;
        }
        self.publish(events);
        Ok(())
    }

    fn failed(&mut self, result: FailedApply, events: &Emitter) -> Result<(), Fault> {
        self.observe_storage(events);
        if self.in_flight.as_ref().map(|pending| &pending.request) != Some(&result.request)
            || result.held_count > 2
        {
            return Err(Fault::new(
                "stale_native_failure",
                "Failed operation identity or resource count is invalid.",
                true,
            ));
        }
        self.in_flight = None;
        self.active = None;
        self.confirmed_on = None;
        self.reset_interval(0);
        if result.request.generation != self.generation {
            return self.apply(events);
        }
        self.status.operation_id = None;
        self.status.state = State::Error;
        self.status.is_on = None;
        self.protection_fault.get_or_insert(result.fault.clone());
        self.requests
            .finish_where(&self.status, Some(result.fault), |_| true);
        self.publish(events);
        Ok(())
    }

    fn rejected(&mut self, result: RejectedApply, events: &Emitter) -> Result<(), Fault> {
        self.observe_storage(events);
        if self.in_flight.as_ref().map(|pending| &pending.request) != Some(&result.request) {
            return Err(Fault::new(
                "stale_native_rejection",
                "Rejected operation is no longer pending.",
                true,
            ));
        }
        self.in_flight = None;
        if result.request.generation != self.generation {
            return self.apply(events);
        }
        self.status.state = State::Error;
        self.status.is_on = None;
        self.protection_fault.get_or_insert(result.fault.clone());
        self.requests
            .finish_where(&self.status, Some(result.fault), |_| true);
        self.publish(events);
        Ok(())
    }

    fn expired(&mut self, correlation: CorrelationId, events: &Emitter) {
        self.observe_storage(events);
        if self.requests.has(&correlation) {
            self.status.protected_idle_duration_seconds = None;
            if self.in_flight.is_some() {
                self.status.state = State::Error;
                self.status.is_on = None;
            }
            self.requests.finish(
                &correlation,
                &self.status,
                Some(Fault::new(
                    "operation_timed_out",
                    "The owner operation did not finish in time.",
                    true,
                )),
            );
            self.publish(events);
        }
    }
}

/// Physical work is admitted against current policy at handler invocation,
/// not an older authorization waiting in the same Actions queue.
struct AdmittedNative {
    owner: Arc<Mutex<Owner>>,
    native: Arc<dyn NativePower>,
}

impl NativePower for AdmittedNative {
    fn query_accessibility(&self, events: Emitter) {
        let admitted = {
            let owner = self.owner.lock().expect("owner poisoned");
            !owner.closing && owner.corner_query.as_ref() == Some(&events.correlation())
        };
        if admitted {
            self.native.query_accessibility(events);
        } else {
            let fault = Fault::new(
                "corner_access_query_retired",
                "The corner query was retired.",
                true,
            );
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
            events.emit::<Power::Corners::Access::Queried>(Ui::CornerTrust {
                trusted: false,
                fault: Some(fault),
                macos_major: None,
            });
        }
    }

    fn configure_corners(&self, preference: CornerPreference, events: Emitter) {
        let persistence_fault = events.persistence_error().cloned();
        let admitted = {
            let owner = self.owner.lock().expect("owner poisoned");
            !owner.closing
                && preference.revision == owner.corner_control.revision
                && preference.enabled == owner.corner_control.enabled
        };
        if persistence_fault.is_none() && admitted {
            self.native.configure_corners(preference, events);
        } else {
            let fault = persistence_fault.unwrap_or_else(|| {
                Fault::new(
                    "corner_preference_retired",
                    "The corner preference was retired.",
                    true,
                )
            });
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
            events.emit::<Power::Corners::Guard::Configured>(CornerConfigured {
                preference,
                installation_id: None,
                ready: false,
                fault: Some(fault),
            });
        }
    }

    fn check_accessibility(&self, check: Ui::SetupSession, events: Emitter) {
        if self
            .owner
            .lock()
            .expect("owner poisoned")
            .setup
            .current(check.session)
        {
            self.native.check_accessibility(check, events);
        }
    }

    fn refresh_corners(&self, refresh: CornerRefresh, events: Emitter) {
        let persistence_fault = events.persistence_error().cloned();
        if persistence_fault.is_none()
            && self
                .owner
                .lock()
                .expect("owner poisoned")
                .setup_admits(&refresh)
        {
            self.native.refresh_corners(refresh, events);
        } else {
            let fault = persistence_fault.unwrap_or_else(|| {
                Fault::new(
                    "corner_refresh_retired",
                    "The setup or native acquisition has been retired.",
                    true,
                )
            });
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
            events.emit::<Power::Corners::Guard::Refreshed>(CornerRefreshed {
                installation_id: None,
                refresh,
                ready: false,
                fault: Some(fault),
            });
        }
    }
    fn authorize_corners(&self, authorization: CornerAuthorization) {
        self.native.authorize_corners(authorization);
    }

    fn retire_corners(&self) {
        self.native.retire_corners();
    }

    fn watch_lifecycle(&self, events: Emitter) {
        self.native.watch_lifecycle(events);
    }

    fn apply(&self, request: Apply, events: Emitter) {
        let admitted = {
            let owner = self.owner.lock().expect("owner poisoned");
            !request.enabled
                || (owner.in_flight.as_ref().map(|pending| &pending.request) == Some(&request)
                    && request.generation == owner.generation
                    && owner.status.desired_on
                    && owner.status.state != State::Error
                    && !owner.closing
                    && owner.storage_fault.is_none()
                    && owner.lifecycle_fault().is_none()
                    && !owner.sleeping)
        };
        if !admitted {
            let fault = Fault::new(
                "native_apply_superseded",
                "This native acquisition is no longer admitted by the current owner.",
                true,
            );
            events.error::<Error::Native::Assertions::Occurred>(fault.clone());
            events.emit::<Power::Assertions::Resources::Rejected>(RejectedApply { request, fault });
            return;
        }
        self.native.apply(request, events);
    }

    fn sample(&self, request: IdleRequest, events: Emitter) {
        self.native.sample(request, events);
    }
}

/// Host/test deadline configuration; production defaults to the fixed five-second contract.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeOptions {
    /// Must be positive and no greater than five seconds.
    pub operation_timeout: Duration,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            operation_timeout: Duration::from_secs(5),
        }
    }
}

/// Start an isolated OFF owner and return only its Actions port.
///
/// Native collaboration is mandatory. Register observers before emitting Query or
/// TurnOn; no startup snapshot can race their registration. Production hosts must
/// supply a real initially resource-free native implementation.
pub fn start(native: Option<Arc<dyn NativePower>>) -> Result<Registry, StartupError> {
    start_with_options(native, RuntimeOptions::default())
}

/// Start with a bounded injected deadline; this does not add a product preference.
pub fn start_with_options(
    native: Option<Arc<dyn NativePower>>,
    options: RuntimeOptions,
) -> Result<Registry, StartupError> {
    start_with_journal(native, options, None)
}

/// Inject the canonical first append observer; production owners use the private file journal.
pub fn start_with_journal(
    native: Option<Arc<dyn NativePower>>,
    options: RuntimeOptions,
    journal: Option<Arc<dyn ActionJournal>>,
) -> Result<Registry, StartupError> {
    let actions = build_owner(native, options, journal)?;
    crate::telemetry::install(&actions, crate::telemetry::Configuration::Unconfigured);
    Ok(actions)
}

pub(crate) fn build_owner(
    native: Option<Arc<dyn NativePower>>,
    options: RuntimeOptions,
    journal: Option<Arc<dyn ActionJournal>>,
) -> Result<Registry, StartupError> {
    let native =
        native.ok_or_else(|| StartupError("native power collaborator is required".into()))?;
    let session = IdGeneration::next_id::<Power::Session::Protection::Query>(&Empty {})
        .map_err(|fault| StartupError(fault.message))?;
    let owner = Arc::new(Mutex::new(Owner {
        session,
        generation: 0,
        status: Status::off(),
        in_flight: None,
        confirmed_on: None,
        requests: Requests::new(options.operation_timeout)?,
        last_idle_clock: 0,
        toggle_tail: None,
        active: None,
        sleeping: false,
        interruption: None,
        protected_anchor: 0,
        closing: false,
        proofs: Proofs::default(),
        idle_fault: None,
        storage_fault: None,
        lifecycle: None,
        corner_loss: None,
        corner_installation: None,
        protection_fault: None,
        veto_loss: None,
        setup: crate::accessibility::Setup::default(),
        corner_control: Ui::CornerControlPresentation::default(),
        corner_query: None,
        corner_configuration: None,
    }));
    let mut builder = RegistryBuilder::default();
    if let Some(journal) = journal {
        builder.journal(journal);
    }
    let native: Arc<dyn NativePower> = Arc::new(AdmittedNative {
        owner: owner.clone(),
        native,
    });
    contracts::register(&mut builder, native.clone())?;
    let corners = owner.clone();
    builder.on::<Ui::Accessibility::Control::Query>(move |_, events| {
        corners
            .lock()
            .expect("owner poisoned")
            .query_corners(events);
        Ok(())
    })?;
    let corners = owner.clone();
    builder.on::<Ui::Accessibility::Control::Activated>(move |_, events| {
        corners
            .lock()
            .expect("owner poisoned")
            .activate_corners(events);
        Ok(())
    })?;
    let corners = owner.clone();
    builder.on::<Power::Corners::Access::Queried>(move |result, events| {
        corners
            .lock()
            .expect("owner poisoned")
            .corners_queried(result, events);
        Ok(())
    })?;
    let corners = owner.clone();
    builder.on::<Power::Corners::Guard::Configured>(move |result, events| {
        corners
            .lock()
            .expect("owner poisoned")
            .corners_configured(result, events);
        Ok(())
    })?;
    let setup = owner.clone();
    builder.on::<Ui::Accessibility::Setup::Activated>(move |_, events| {
        let mut owner = setup.lock().expect("owner poisoned");
        if owner.closing {
            events.error::<Error::Native::Corners::Occurred>(Fault::new(
                "accessibility_setup_closing",
                "Setup is unavailable during shutdown.",
                true,
            ));
        } else {
            owner.setup.activate(events);
        }
        Ok(())
    })?;
    let setup = owner.clone();
    builder.on::<Ui::Accessibility::Setup::Completed>(move |result, events| {
        setup
            .lock()
            .expect("owner poisoned")
            .setup
            .completed(result, events);
        Ok(())
    })?;
    let setup = owner.clone();
    builder.on::<Ui::Accessibility::Setup::Check>(move |check, events| {
        setup
            .lock()
            .expect("owner poisoned")
            .setup
            .check(check.session, events);
        Ok(())
    })?;
    let setup = owner.clone();
    builder.on::<Ui::Accessibility::Setup::Tick>(move |tick, events| {
        setup
            .lock()
            .expect("owner poisoned")
            .setup
            .check(tick.session, events);
        Ok(())
    })?;
    let setup = owner.clone();
    builder.on::<Ui::Accessibility::Setup::Dismissed>(move |window, events| {
        let mut owner = setup.lock().expect("owner poisoned");
        if owner.setup.current(window.session) {
            owner.setup.dismiss(events);
        }
        Ok(())
    })?;
    let setup = owner.clone();
    builder.on::<Power::Corners::Access::Observed>(move |result, events| {
        setup
            .lock()
            .expect("owner poisoned")
            .setup_observed(result, events);
        Ok(())
    })?;
    let setup = owner.clone();
    builder.on::<Power::Corners::Guard::Refreshed>(move |result, events| {
        setup
            .lock()
            .expect("owner poisoned")
            .setup_refreshed(result, events);
        Ok(())
    })?;
    let corners = owner.clone();
    builder.on::<Power::Corners::Guard::Lost>(move |loss, events| {
        corners
            .lock()
            .expect("owner poisoned")
            .corner_lost(loss, events);
        Ok(())
    })?;
    let availability = owner.clone();
    let veto = owner.clone();
    builder.on::<Power::Activity::IdleSleep::Failed>(move |loss, events| {
        veto.lock()
            .expect("owner poisoned")
            .veto_failed(loss, events);
        Ok(())
    })?;
    builder.on::<Power::Activity::System::Availability>(move |source, events| {
        availability
            .lock()
            .expect("owner poisoned")
            .availability(source, events);
        Ok(())
    })?;
    builder.require::<Power::Session::Protection::TurnOn>();
    builder.require::<Power::Session::Protection::TurnOff>();
    builder.require::<Power::Session::Protection::Query>();
    builder.require::<Power::Session::Protection::Quit>();
    builder.require::<Power::Session::Protection::Toggle>();
    let toggle = owner.clone();
    builder.on::<Power::Session::Protection::Toggle>(move |_, events| {
        toggle.lock().expect("owner poisoned").toggle(events);
        Ok(())
    })?;
    let on = owner.clone();
    builder.on::<Power::Session::Protection::TurnOn>(move |_, events| {
        on.lock().expect("owner poisoned").set(true, events)
    })?;
    let off = owner.clone();
    builder.on::<Power::Session::Protection::TurnOff>(move |_, events| {
        let mut owner = off.lock().expect("owner poisoned");
        owner.setup.dismiss(events);
        owner.set(false, events)
    })?;
    let query = owner.clone();
    builder.on::<Power::Session::Protection::Query>(move |_, events| {
        query.lock().expect("owner poisoned").query(events)
    })?;
    let quit = owner.clone();
    builder.on::<Power::Session::Protection::Quit>(move |_, events| {
        quit.lock().expect("owner poisoned").quit(events);
        Ok(())
    })?;
    let observed = owner.clone();
    builder.on::<Power::Assertions::Resources::Observed>(move |result, events| {
        observed
            .lock()
            .expect("owner poisoned")
            .observed(result, events)
    })?;
    let rejected = owner.clone();
    builder.on::<Power::Assertions::Resources::Rejected>(move |result, events| {
        rejected
            .lock()
            .expect("owner poisoned")
            .rejected(result, events)
    })?;
    let failed = owner.clone();
    builder.on::<Power::Assertions::Resources::Failed>(move |result, events| {
        failed
            .lock()
            .expect("owner poisoned")
            .failed(result, events)
    })?;
    let storage = owner.clone();
    builder.on::<Error::Log::Storage::Occurred>(move |_, events| {
        // A derived export fault does not invalidate the canonical append journal.
        if let Some(fault) = events.persistence_error() {
            storage
                .lock()
                .expect("owner poisoned")
                .storage_failed(fault.clone(), events);
        }
        Ok(())
    })?;
    let idle = owner.clone();
    builder.on::<Power::Activity::System::Observed>(move |result, events| {
        idle.lock().expect("owner poisoned").idle(result, events)
    })?;
    let interrupted = owner.clone();
    builder.on::<Power::Activity::System::Interrupted>(move |result, events| {
        interrupted
            .lock()
            .expect("owner poisoned")
            .interrupted(result, events)
    })?;
    let requested = owner.clone();
    builder.on::<Power::Suppression::Evidence::Requested>(move |request, events| {
        requested
            .lock()
            .expect("owner poisoned")
            .suppression_requested(request, events)
    })?;
    let recorded = owner.clone();
    builder.on::<Power::Suppression::Evidence::Recorded>(move |receipt, events| {
        recorded
            .lock()
            .expect("owner poisoned")
            .suppression_recorded(receipt, events)
    })?;
    let expired = owner.clone();
    builder.on::<Power::Session::Protection::Expired>(move |correlation, events| {
        expired
            .lock()
            .expect("owner poisoned")
            .expired(correlation, events);
        Ok(())
    })?;
    builder.on::<Power::Session::Protection::Abandoned>(move |abandoned, _| {
        owner
            .lock()
            .expect("owner poisoned")
            .requests
            .abandon(&abandoned.correlations)
    })?;
    builder.on::<Power::Session::Protection::Changed>(|status, events| {
        events.emit::<Power::Session::Protection::Snapshot>(status);
        Ok(())
    })?;
    let bus = builder.build()?;
    contracts::observe_controls(&bus, &native);
    bus.emit::<Power::Activity::System::Watch>(Empty {});
    Ok(bus)
}
// end runtime.rs
