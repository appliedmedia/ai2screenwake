// power.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Native resource ownership behind the existing typed callback boundary.

use crate::{NativePower, activity};
use ai2screenwake_backend::contract::{
    Apply, AssertionsObserved, ClockSample, Emitter, Error, FailedApply, Fault, IdleRequest,
    Outcome, Power, WeakEmitter,
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
}

#[derive(Default)]
struct Resources {
    ids: [Option<u32>; 2],
    active: Option<(Apply, WeakEmitter)>,
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
        Self::with_api(Arc::new(SystemPower))
    }

    /// Inject an OS boundary; production must use the real macOS implementation.
    pub fn with_api(api: Arc<dyn PowerApi>) -> Self {
        Self {
            api,
            held: Mutex::new(Resources::default()),
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
                self.api.clock()
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

impl NativePower for MacPower {
    fn apply(&self, request: Apply, events: Emitter) {
        let mut terminal_fault = self.api.clock().err();
        if let Some(fault) = &terminal_fault {
            events.error::<Error::Native::Assertions::Occurred>(fault.clone());
        }
        let mut faults = Vec::new();
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
        if !request.enabled {
            held.active = None;
            self.release_all(&mut held.ids, &mut faults);
        } else if terminal_fault.is_none() {
            held.active = Some((request.clone(), events.downgrade()));
            for (slot, kind) in held
                .ids
                .iter_mut()
                .zip([AssertionKind::System, AssertionKind::Display])
            {
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
            if !faults.is_empty() {
                self.release_all(&mut held.ids, &mut faults);
            }
        }
        let held_count = held.ids.iter().filter(|id| id.is_some()).count() as u8;
        let clock = self.api.clock();
        drop(held);
        let fault_codes = faults
            .iter()
            .map(|fault| fault.fault_code.clone())
            .collect();
        let outcome = if faults.is_empty() {
            Outcome::Applied
        } else {
            Outcome::Failed
        };
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
            events.emit::<Power::Assertions::Failed>(FailedApply {
                request,
                held_count,
                fault,
            });
            return;
        }
        events.emit::<Power::Assertions::Observed>(AssertionsObserved {
            outcome,
            request,
            held_count,
            fault_codes,
            clock: clock.expect("a failed occurrence clock has a terminal failure"),
        });
    }

    fn sample(&self, request: IdleRequest, events: Emitter) {
        activity::sample(self.api.as_ref(), request, events);
    }
}

impl Drop for MacPower {
    fn drop(&mut self) {
        // Never keep a strong Emitter here: the registry's handler owns this object.
        // Logged/retryable teardown is apply(false); this is final bus-free cleanup.
        let held = match self.held.get_mut() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        };
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
    fn acquire(&self, kind: AssertionKind) -> Result<u32, Fault> {
        let kind = CFString::new(match kind {
            AssertionKind::System => "PreventUserIdleSystemSleep",
            AssertionKind::Display => "PreventUserIdleDisplaySleep",
        });
        let name = CFString::new("ScreenWake explicit idle protection");
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
