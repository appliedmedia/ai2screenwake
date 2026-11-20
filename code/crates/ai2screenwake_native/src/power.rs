// power.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Native resource ownership behind the existing typed callback boundary.

use crate::{
    NativePower, activity,
    corners::{CornerApi, CornerGuard},
    idle_veto::IdleVeto,
};
use ai2screenwake_backend::contract::{
    Apply, AssertionsObserved, ClockSample, CornerAuthorization, CornerConfigured,
    CornerPreference, CornerRefresh, CornerRefreshed, Emitter, EntityId, Error, FailedApply, Fault,
    IdleRequest, LifecycleAvailability, Outcome, Power, Ui, WeakEmitter,
};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
use core_foundation::{base::TCFType, string::CFString};

/// The two independently acquired idle-protection resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssertionKind {
    /// Prevent ordinary idle system sleep, not forced or policy sleep.
    System,
    /// Prevent ordinary idle display sleep, not manual/security locking.
    Display,
}

/// Injected OS calls, so resource-fault scenarios never need to alter the host.
pub trait PowerApi: Send + Sync {
    /// Running OS metadata for version-consistent copy, observed only through native Actions.
    fn macos_major_version(&self) -> Result<u32, Fault> {
        #[cfg(target_os = "macos")]
        {
            crate::identity::macos_major_version()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(Fault::new(
                "system_version_unavailable",
                "The system version could not be read.",
                true,
            ))
        }
    }
    /// Explicit injected seams default to assertion-only fixtures; production supplies a guard.
    fn corners(&self) -> Option<Arc<dyn CornerApi>> {
        None
    }
    /// Acquire one real resource or return a fault without transferring ownership.
    fn acquire(&self, kind: AssertionKind) -> Result<u32, Fault>;
    /// Release a resource; failure leaves its ID owned by the caller.
    fn release(&self, id: u32) -> Result<(), Fault>;
    /// Capture paired UTC and continuous-monotonic clocks.
    fn clock(&self) -> Result<ClockSample, Fault>;
    /// Read genuine session input age without logging input contents.
    fn idle_seconds(&self) -> Result<f64, Fault>;
}

/// Initially resource-free native owner; no state is returned to the caller.
pub struct MacPower {
    api: Arc<dyn PowerApi>,
    held: Mutex<Resources>,
    pub(crate) idle_veto: IdleVeto,
}

#[derive(Default)]
struct Resources {
    ids: [Option<u32>; 2],
    active: Option<(Apply, WeakEmitter)>,
    confirmed: Option<Apply>,
    sleeping: bool,
    source_required: bool,
    source_available: bool,
    source_generation: u64,
    source_revision: u64,
    source_events: Option<WeakEmitter>,
    corners: Option<CornerGuard>,
    corner_authorization: Option<CornerAuthorization>,
    corners_retired: bool,
    corner_disabled: bool,
    corner_revision: u64,
}

/// Native-only occurrence capture; neither a state getter nor a core policy decision.
pub(crate) struct LifecycleContext {
    pub request: Apply,
    pub events: WeakEmitter,
    pub held_count: u8,
    pub clock: Result<ClockSample, Fault>,
}

impl MacPower {
    /// Create an OFF owner using actual macOS APIs; no resource is acquired here.
    #[cfg(target_os = "macos")]
    pub fn new() -> Self {
        let power = Self::with_api(Arc::new(SystemPower));
        power.held.lock().expect("new native owner").source_required = true;
        power
    }

    /// Inject an OS boundary; production must use the real macOS implementation.
    pub fn with_api(api: Arc<dyn PowerApi>) -> Self {
        Self {
            api,
            held: Mutex::new(Resources::default()),
            idle_veto: IdleVeto::default(),
        }
    }

    pub(crate) fn begin_lifecycle(&self) -> Result<u64, Fault> {
        let mut held = self.held.lock().map_err(|_| {
            Fault::new(
                "native_owner_poisoned",
                "Native ownership is unavailable.",
                false,
            )
        })?;
        if held.source_required && held.source_available {
            return Err(Fault::new(
                "lifecycle_already_running",
                "A lifecycle source is already active.",
                true,
            ));
        }
        held.source_required = true;
        held.source_available = false;
        self.idle_veto.retire();
        held.source_generation = held.source_generation.checked_add(1).ok_or_else(|| {
            Fault::new(
                "lifecycle_exhausted",
                "Lifecycle identity is exhausted.",
                false,
            )
        })?;
        let generation = held.source_generation;
        drop(held);
        self.lifecycle_availability(generation, false, None);
        Ok(generation)
    }

    pub(crate) fn lifecycle_generation_is_current(&self, generation: u64) -> bool {
        self.held
            .lock()
            .is_ok_and(|held| held.source_generation == generation)
    }

    pub(crate) fn lifecycle_availability(
        &self,
        generation: u64,
        available: bool,
        failure: Option<Fault>,
    ) {
        let (events, state) = {
            let mut held = match self.held.lock() {
                Ok(held) => held,
                Err(poisoned) => poisoned.into_inner(),
            };
            if held.source_generation != generation {
                return;
            }
            held.source_available = available;
            if !available {
                self.idle_veto.retire();
                if let Some(corners) = &held.corners {
                    corners.revoke();
                }
            }
            held.source_revision += 1;
            (
                held.source_events.clone(),
                LifecycleAvailability {
                    revision: held.source_revision,
                    available,
                },
            )
        };
        if let Some(events) = events.and_then(|events| events.upgrade()) {
            if let Some(failure) = failure {
                events.error::<Error::Native::Activity::Occurred>(failure);
            }
            events.emit::<Power::Activity::System::Availability>(state);
        }
    }

    pub(crate) fn lifecycle_context(&self) -> Option<LifecycleContext> {
        let (held, poisoned) = match self.held.lock() {
            Ok(held) => (held, false),
            Err(error) => (error.into_inner(), true),
        };
        let (request, events) = held.active.as_ref()?;
        if !request.enabled || held.ids.iter().all(Option::is_none) {
            return None;
        }
        Some(LifecycleContext {
            request: request.clone(),
            events: events.clone(),
            held_count: held.ids.iter().filter(|id| id.is_some()).count() as u8,
            clock: if poisoned {
                Err(Fault::new(
                    "native_owner_poisoned",
                    "Native ownership is unavailable.",
                    false,
                ))
            } else {
                self.idle_veto.check().and_then(|()| self.api.clock())
            },
        })
    }

    pub(crate) fn lifecycle_is_current(&self, request: &Apply) -> bool {
        match self.held.lock() {
            Ok(held) => held
                .active
                .as_ref()
                .is_some_and(|(active, _)| active == request),
            Err(_) => false,
        }
    }

    /// Capture reply completion clocks without taking the native resource/owner lock.
    pub(crate) fn reply_clock(&self) -> Result<ClockSample, Fault> {
        self.idle_veto.check().and_then(|()| self.api.clock())
    }

    /// Apply physical phase facts only after the OS reply and for the current source/acquisition.
    pub(crate) fn lifecycle_phase(&self, source: u64, request: &Apply, sleeping: bool) -> bool {
        let mut held = self.held.lock().unwrap_or_else(|error| error.into_inner());
        if held.source_generation != source
            || held
                .active
                .as_ref()
                .is_none_or(|(current, _)| current != request)
        {
            return false;
        }
        held.sleeping = sleeping;
        self.idle_veto.sleeping(sleeping);
        true
    }

    fn release_all(&self, held: &mut [Option<u32>; 2], faults: &mut Vec<Fault>) {
        for slot in held.iter_mut().rev() {
            if let Some(id) = *slot {
                match self.api.release(id) {
                    Ok(()) => *slot = None,
                    Err(fault) => faults.push(fault),
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
impl Default for MacPower {
    fn default() -> Self {
        Self::new()
    }
}

impl MacPower {
    fn accessibility_permission(&self) -> Result<(), Fault> {
        self.api
            .corners()
            .ok_or_else(|| {
                Fault::new(
                    "accessibility_unavailable",
                    "Accessibility observation is unavailable.",
                    true,
                )
            })
            .and_then(|api| api.permission(false).into_result())
    }

    fn reconcile_corners(
        &self,
        authorization: &CornerAuthorization,
        events: &Emitter,
    ) -> (Option<EntityId>, Result<bool, Fault>) {
        let authorization_revision = authorization.revision;
        let mut installation_id = None;
        let result = (|| {
            let mut held = self.held.lock().map_err(|_| {
                Fault::new(
                    "native_owner_poisoned",
                    "Native ownership is unavailable.",
                    false,
                )
            })?;
            let request = authorization
                .request
                .as_ref()
                .filter(|request| {
                    held.confirmed.as_ref() == Some(*request)
                        && held
                            .active
                            .as_ref()
                            .is_some_and(|(active, _)| active == *request)
                        && held.ids.iter().all(Option::is_some)
                        && held
                            .corner_authorization
                            .as_ref()
                            .is_some_and(|authorization| {
                                authorization.request.as_ref() == Some(*request)
                                    && authorization.revision >= authorization_revision
                            })
                        && !held.corners_retired
                        && !held.corner_disabled
                        && !held.sleeping
                        && (!held.source_required || held.source_available)
                })
                .ok_or_else(|| {
                    Fault::new(
                        "corner_refresh_retired",
                        "The native acquisition is no longer admitted.",
                        true,
                    )
                })?
                .clone();
            if let Err(fault) = self.accessibility_permission() {
                if let Some(corners) = &held.corners {
                    corners.revoke();
                }
                return Err(fault);
            }
            if held.corners.as_ref().is_some_and(CornerGuard::filtering) {
                installation_id = held.corners.as_ref().map(CornerGuard::installation_id);
                return Ok(true);
            }
            if let Some(corners) = &mut held.corners {
                corners.revoke();
                corners.stop()?;
            }
            held.corners = None;
            let api = self.api.corners().ok_or_else(|| {
                Fault::new(
                    "corner_refresh_unavailable",
                    "Optional corner setup is unavailable.",
                    true,
                )
            })?;
            let mut corners = CornerGuard::start(api, request, events)?;
            installation_id = Some(corners.installation_id());
            if let Err(fault) = corners.activate(authorization) {
                corners.revoke();
                if let Err(cleanup) = corners.stop() {
                    events.error::<Error::Native::Corners::Occurred>(cleanup);
                }
                held.corners = Some(corners);
                return Err(fault);
            }
            let ready = corners.filtering();
            held.corners = Some(corners);
            Ok(ready)
        })();
        (installation_id, result)
    }
}

impl NativePower for MacPower {
    fn query_accessibility(&self, events: Emitter) {
        let result = self.accessibility_permission();
        let version = self.api.macos_major_version();
        if let Err(fault) = &result {
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
        }
        if let Err(fault) = &version {
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
        }
        events.emit::<Power::Corners::Access::Queried>(Ui::CornerTrust {
            trusted: result.is_ok(),
            fault: version.as_ref().err().cloned().or_else(|| result.err()),
            macos_major: version.ok(),
        });
    }

    fn check_accessibility(&self, check: Ui::SetupSession, events: Emitter) {
        let result = self.accessibility_permission();
        let version = self.api.macos_major_version();
        if let Err(fault) = &result {
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
        }
        if let Err(fault) = &version {
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
        }
        events.emit::<Power::Corners::Access::Observed>(Ui::AccessibilityObservation {
            session: check.session,
            trusted: result.is_ok(),
            fault: version.as_ref().err().cloned().or_else(|| result.err()),
            macos_major: version.ok(),
        });
    }

    fn configure_corners(&self, preference: CornerPreference, events: Emitter) {
        let mut installation_id = None;
        let result = (|| {
            let mut held = self.held.lock().map_err(|_| {
                Fault::new(
                    "native_owner_poisoned",
                    "Native ownership is unavailable.",
                    false,
                )
            })?;
            if preference.revision < held.corner_revision {
                return Err(Fault::new(
                    "corner_preference_retired",
                    "The corner preference was retired.",
                    true,
                ));
            }
            held.corner_revision = preference.revision;
            held.corner_disabled = !preference.enabled;
            if held.corner_disabled {
                if let Some(corners) = &mut held.corners {
                    installation_id = Some(corners.installation_id());
                    corners.revoke();
                    corners.stop()?;
                }
                held.corners = None;
                installation_id = None;
                return Ok(false);
            }
            drop(held);
            if preference.authorization.request.is_none() {
                return Ok(false);
            }
            let (identity, result) = self.reconcile_corners(&preference.authorization, &events);
            installation_id = identity;
            result
        })();
        if let Err(fault) = &result {
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
        }
        events.emit::<Power::Corners::Guard::Configured>(CornerConfigured {
            preference,
            installation_id,
            ready: result.as_ref().is_ok_and(|ready| *ready),
            fault: result.err(),
        });
    }

    fn refresh_corners(&self, refresh: CornerRefresh, events: Emitter) {
        let (installation_id, result) = self.reconcile_corners(&refresh.authorization, &events);
        if let Err(fault) = &result {
            events.error::<Error::Native::Corners::Occurred>(fault.clone());
        }
        events.emit::<Power::Corners::Guard::Refreshed>(CornerRefreshed {
            installation_id,
            refresh,
            ready: result.as_ref().is_ok_and(|ready| *ready),
            fault: result.err(),
        });
    }
    fn authorize_corners(&self, authorization: CornerAuthorization) {
        let mut veto_failure = None;
        let mut held = self.held.lock().expect("native ownership poisoned");
        if held
            .corner_authorization
            .as_ref()
            .is_some_and(|current| current.revision >= authorization.revision)
        {
            return;
        }
        if let Some(corners) = &held.corners
            && !held.corner_disabled
        {
            corners.authorize(&authorization);
        }
        if held
            .corner_authorization
            .as_ref()
            .and_then(|current| current.request.as_ref())
            != authorization.request.as_ref()
        {
            self.idle_veto.authorize(authorization.request.clone());
            if let Some(request) = authorization.request.as_ref().filter(|request| {
                held.confirmed.as_ref() == Some(*request)
                    && held.ids.iter().all(Option::is_some)
                    && (!held.source_required || held.source_available)
                    && !held.corners_retired
                    && !held.sleeping
            }) {
                let events = held
                    .active
                    .as_ref()
                    .expect("confirmed native acquisition")
                    .1
                    .clone();
                match self.idle_veto.epoch(request) {
                    Ok(epoch) => {
                        self.idle_veto
                            .ready(epoch, request.clone(), events, held.source_generation)
                    }
                    Err(fault) => veto_failure = Some((request.clone(), events, fault)),
                }
            }
        }
        held.corner_authorization = Some(authorization);
        drop(held);
        if let Some((request, events, fault)) = veto_failure
            && let Some(events) = events.upgrade()
        {
            events.error::<Error::Native::Activity::Occurred>(fault.clone());
            events.emit::<Power::Activity::IdleSleep::Failed>(
                ai2screenwake_backend::contract::IdleVetoFailure { request, fault },
            );
        }
    }

    fn retire_corners(&self) {
        self.idle_veto.retire();
        let mut held = self.held.lock().expect("native ownership poisoned");
        held.corners_retired = held.corners.is_some()
            || held
                .active
                .as_ref()
                .is_some_and(|(request, _)| request.enabled)
            || held.ids.iter().any(Option::is_some);
        if let Some(corners) = &held.corners {
            corners.revoke();
        }
    }

    fn watch_lifecycle(&self, events: Emitter) {
        let state = {
            let mut held = self.held.lock().expect("native ownership poisoned");
            held.source_events = Some(events.downgrade());
            LifecycleAvailability {
                revision: held.source_revision,
                available: !held.source_required || held.source_available,
            }
        };
        events.emit::<Power::Activity::System::Availability>(state);
    }

    fn apply(&self, request: Apply, events: Emitter) {
        let (veto_epoch, veto_fault) = match self.idle_veto.epoch(&request) {
            Ok(epoch) => (epoch, None),
            Err(fault) => (None, Some(fault)),
        };
        if !request.enabled {
            self.idle_veto.retire();
        }
        let mut terminal_fault = self.api.clock().err();
        if let Some(fault) = &terminal_fault {
            events.error::<Error::Native::Assertions::Occurred>(fault.clone());
        }
        let mut faults: Vec<_> = veto_fault.into_iter().collect();
        let mut corner_faults = Vec::new();
        let corner_api = self.api.corners();
        let mut held = match self.held.lock() {
            Ok(held) => held,
            Err(poisoned) => {
                let fault = Fault::new(
                    "native_owner_poisoned",
                    "Native ownership is unavailable.",
                    false,
                );
                terminal_fault.get_or_insert(fault.clone());
                faults.push(fault);
                poisoned.into_inner()
            }
        };
        if held.source_events.is_none() {
            held.source_events = Some(events.downgrade());
        }
        held.confirmed = None;
        if request.enabled && held.source_required && !held.source_available {
            let fault = Fault::new(
                "lifecycle_unavailable",
                "Sleep/wake observation is unavailable. Protection cannot be enabled.",
                true,
            );
            terminal_fault.get_or_insert(fault.clone());
            faults.push(fault);
        }
        if request.enabled
            && (held.source_required || corner_api.is_some())
            && (veto_epoch.is_none()
                || held
                    .corner_authorization
                    .as_ref()
                    .and_then(|current| current.request.as_ref())
                    != Some(&request))
        {
            let fault = Fault::new(
                "corner_owner_not_admitted",
                "The current owner has not admitted this native acquisition.",
                true,
            );
            terminal_fault.get_or_insert(fault.clone());
            faults.push(fault);
        }
        if !request.enabled {
            held.active = None;
            if let Some(corners) = &mut held.corners {
                corners.revoke();
                if let Err(failure) = corners.stop() {
                    faults.push(failure);
                }
            }
            if faults.is_empty() {
                held.corners = None;
            }
            self.release_all(&mut held.ids, &mut faults);
            if faults.is_empty() {
                held.corners_retired = false;
                held.sleeping = false;
                self.idle_veto.sleeping(false);
            }
        } else if terminal_fault.is_none() {
            held.active = Some((request.clone(), events.downgrade()));
            if let Some(api) = corner_api.filter(|_| !held.corner_disabled) {
                if let Some(corners) = &mut held.corners {
                    corners.revoke();
                    if let Err(failure) = corners.stop() {
                        corner_faults.push(failure);
                    }
                }
                if corner_faults.is_empty() {
                    held.corners = None;
                }
                if held.corners_retired {
                    corner_faults.push(Fault::new(
                        "corner_cleanup_required",
                        "Turn protection off before enabling the retired corner guard again.",
                        true,
                    ));
                } else if corner_faults.is_empty() {
                    match CornerGuard::start(api, request.clone(), &events) {
                        Ok(corners) => held.corners = Some(corners),
                        Err(failure) => corner_faults.push(failure),
                    }
                }
            }
            for (slot, kind) in held
                .ids
                .iter_mut()
                .zip([AssertionKind::System, AssertionKind::Display])
            {
                if !faults.is_empty() {
                    break;
                }
                if slot.is_none() {
                    match self.api.acquire(kind) {
                        Ok(id) => *slot = Some(id),
                        Err(fault) => {
                            faults.push(fault);
                            break;
                        }
                    }
                }
            }
            if let (true, Some(corners)) = (
                faults.is_empty() && corner_faults.is_empty() && !held.corner_disabled,
                &held.corners,
            ) {
                match &held.corner_authorization {
                    Some(authorization) if authorization.request.as_ref() == Some(&request) => {
                        if let Err(failure) = corners.activate(authorization) {
                            corner_faults.push(failure);
                        }
                    }
                    _ => corner_faults.push(Fault::new(
                        "corner_owner_not_admitted",
                        "The current owner has not authorized corner protection.",
                        true,
                    )),
                }
            }
            if !faults.is_empty() || !corner_faults.is_empty() {
                if let Some(corners) = &mut held.corners {
                    corners.revoke();
                    if let Err(failure) = corners.stop() {
                        corner_faults.push(failure);
                    }
                }
                if !faults.is_empty() {
                    self.release_all(&mut held.ids, &mut faults);
                }
            }
        }
        let held_count = held.ids.iter().filter(|id| id.is_some()).count() as u8;
        if let Some(fault) = faults.first() {
            terminal_fault.get_or_insert(fault.clone());
        }
        let clock = self.api.clock();
        if clock.is_err() || terminal_fault.is_some() {
            self.idle_veto.retire_request(&request);
            if let Some(corners) = &held.corners {
                corners.revoke();
            }
        } else if request.enabled
            && held_count == 2
            && faults.is_empty()
            && (!held.source_required || held.source_available)
        {
            held.confirmed = Some(request.clone());
            if !held.sleeping {
                self.idle_veto.ready(
                    veto_epoch,
                    request.clone(),
                    events.downgrade(),
                    held.source_generation,
                );
            }
        }
        let installation_id = held.corners.as_ref().map(CornerGuard::installation_id);
        drop(held);
        if let Some(fault) = corner_faults.first().cloned() {
            for failure in corner_faults {
                events.error::<Error::Native::Corners::Occurred>(failure);
            }
            events.emit::<Power::Corners::Guard::Lost>(
                ai2screenwake_backend::contract::CornerLoss {
                    installation_id,
                    request: request.clone(),
                    fault,
                },
            );
        }
        for fault in faults {
            events.error::<Error::Native::Assertions::Occurred>(fault);
        }
        let clock = match clock {
            Ok(clock) => Some(clock),
            Err(fault) => {
                events.error::<Error::Native::Assertions::Occurred>(fault.clone());
                terminal_fault.get_or_insert(fault);
                None
            }
        };
        if let Some(fault) = terminal_fault {
            events.emit::<Power::Assertions::Resources::Failed>(FailedApply {
                request,
                held_count,
                fault,
            });
            return;
        }
        events.emit::<Power::Assertions::Resources::Observed>(AssertionsObserved {
            outcome: Outcome::Applied,
            request,
            held_count,
            fault_codes: Vec::new(),
            clock: clock.expect("a failed occurrence clock has a terminal failure"),
        });
    }

    fn sample(&self, request: IdleRequest, events: Emitter) {
        activity::sample(self.api.as_ref(), request, events);
    }
}

impl Drop for MacPower {
    fn drop(&mut self) {
        self.idle_veto.retire();
        // Never keep a strong Emitter here: the registry's handler owns this object.
        // Logged/retryable teardown is apply(false); this is final bus-free cleanup.
        let held = match self.held.get_mut() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(corners) = &mut held.corners {
            corners.revoke();
            let _ = corners.stop();
        }
        for slot in held.ids.iter_mut().rev() {
            if let Some(id) = *slot {
                if self.api.release(id).is_ok() {
                    *slot = None;
                }
            }
        }
    }
}

/// Real public macOS power/input APIs; never a fake production fallback.
#[cfg(target_os = "macos")]
pub struct SystemPower;

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        kind: core_foundation::string::CFStringRef,
        level: u32,
        name: core_foundation::string::CFStringRef,
        id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
}

#[cfg(target_os = "macos")]
impl PowerApi for SystemPower {
    fn corners(&self) -> Option<Arc<dyn CornerApi>> {
        Some(Arc::new(crate::corners::system::SystemCorners))
    }
    fn acquire(&self, kind: AssertionKind) -> Result<u32, Fault> {
        let kind = CFString::new(match kind {
            AssertionKind::System => "PreventUserIdleSystemSleep",
            AssertionKind::Display => "PreventUserIdleDisplaySleep",
        });
        let name = CFString::new("AI2ScreenWake explicit idle protection");
        let mut id = 0;
        // The CF strings remain alive and id is a writable IOPMAssertionID.
        let status = unsafe {
            IOPMAssertionCreateWithName(
                kind.as_concrete_TypeRef(),
                255,
                name.as_concrete_TypeRef(),
                &mut id,
            )
        };
        if status != 0 {
            return Err(Fault::new(
                "assertion_acquire_failed",
                &format!("macOS refused an idle assertion ({status})."),
                true,
            ));
        }
        Ok(id)
    }

    fn release(&self, id: u32) -> Result<(), Fault> {
        // Only IDs transferred by successful IOKit acquisition reach this method.
        let status = unsafe { IOPMAssertionRelease(id) };
        if status != 0 {
            return Err(Fault::new(
                "assertion_release_failed",
                &format!("macOS could not release an idle assertion ({status})."),
                true,
            ));
        }
        Ok(())
    }

    fn clock(&self) -> Result<ClockSample, Fault> {
        activity::clock()
    }

    fn idle_seconds(&self) -> Result<f64, Fault> {
        Ok(activity::idle_seconds())
    }
}
// end power.rs
