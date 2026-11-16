// power.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Native resource ownership behind the existing typed callback boundary.

use crate::{NativePower, activity};
use ai2screenwake_backend::contract::{
    Apply, AssertionsObserved, ClockSample, Emitter, Error, Fault, IdleRequest, Outcome, Power,
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
    held: Mutex<[Option<u32>; 2]>,
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
            held: Mutex::new([None, None]),
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
        if let Err(fault) = self.api.clock() {
            events.error::<Error::Native::Assertions::Occurred>(fault);
            return;
        }
        let mut held = match self.held.lock() {
            Ok(held) => held,
            Err(_) => {
                events.error::<Error::Native::Assertions::Occurred>(Fault::new(
                    "native_owner_poisoned",
                    "Native ownership is unavailable; restart the owner.",
                    false,
                ));
                return;
            }
        };
        let mut faults = Vec::new();
        if request.enabled {
            for (slot, kind) in held
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
                self.release_all(&mut held, &mut faults);
            }
        } else {
            self.release_all(&mut held, &mut faults);
        }
        let held_count = held.iter().filter(|id| id.is_some()).count() as u8;
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
            Ok(clock) => clock,
            Err(fault) => {
                events.error::<Error::Native::Assertions::Occurred>(fault);
                return;
            }
        };
        events.emit::<Power::Assertions::Observed>(AssertionsObserved {
            outcome,
            request,
            held_count,
            fault_codes,
            clock,
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
        for slot in held.iter_mut().rev() {
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
