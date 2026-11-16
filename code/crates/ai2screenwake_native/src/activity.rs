// activity.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Genuine input observations and paired clocks, never protected-idle policy.

use crate::power::PowerApi;
#[cfg(target_os = "macos")]
use ai2screenwake_backend::contract::ClockSample;
use ai2screenwake_backend::contract::{Emitter, Error, Fault, IdleObserved, IdleRequest, Power};

/// Validate before converting; float-to-integer saturation must not fabricate an age.
fn milliseconds(seconds: f64) -> Result<u64, Fault> {
    let ms = seconds * 1000.0;
    if !seconds.is_finite() || seconds < 0.0 || !ms.is_finite() || ms >= u64::MAX as f64 {
        return Err(Fault::new(
            "idle_age_invalid",
            "macOS did not provide a valid input-idle age.",
            true,
        ));
    }
    Ok(ms.floor() as u64)
}

/// Observe one genuine sample with its exact request; errors precede null observations.
pub(crate) fn sample(api: &dyn PowerApi, request: IdleRequest, events: Emitter) {
    let age = api.idle_seconds().and_then(milliseconds);
    let clock = match api.clock() {
        Ok(clock) => clock,
        Err(fault) => {
            events.error::<Error::Native::Activity::Occurred>(fault);
            return;
        }
    };
    let (idle_age_ms, fault) = match age {
        Ok(age) => (Some(age), None),
        Err(fault) => {
            events.error::<Error::Native::Activity::Occurred>(fault.clone());
            (None, Some(fault))
        }
    };
    events.emit::<Power::Activity::Observed>(IdleObserved {
        request,
        clock,
        idle_age_ms,
        fault,
    });
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mach_timebase_info(info: *mut Timebase) -> i32;
    fn mach_continuous_time() -> u64;
}

/// Read combined-session input age; does not tap events or request monitoring access.
#[cfg(target_os = "macos")]
pub(crate) fn idle_seconds() -> f64 {
    // SDK: CombinedSessionState=0; kCGAnyInputEventType=~0u.
    unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) }
}

/// Pair UTC with a bracketed continuous clock, which includes suspended time.
///
/// The client, not this clock, excludes sleep from protected-idle episodes.
#[cfg(target_os = "macos")]
pub(crate) fn clock() -> Result<ClockSample, Fault> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let fault = || Fault::new("native_clock_failed", "Native timing is unavailable.", true);
    let mut timebase = Timebase { numer: 0, denom: 0 };
    // The layout matches mach_timebase_info_data_t and the output is writable.
    if unsafe { mach_timebase_info(&mut timebase) } != 0
        || timebase.denom == 0
        || timebase.numer == 0
    {
        return Err(fault());
    }
    // Both reads are side-effect-free, public continuous-clock snapshots.
    let before = unsafe { mach_continuous_time() };
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| fault())?;
    let after = unsafe { mach_continuous_time() };
    let midpoint =
        u128::from(before) + u128::from(after.checked_sub(before).ok_or_else(fault)? / 2);
    let millis = midpoint * u128::from(timebase.numer) / u128::from(timebase.denom) / 1_000_000;
    Ok(ClockSample {
        epoch_ms: u64::try_from(epoch.as_millis()).map_err(|_| fault())?,
        monotonic_ms: u64::try_from(millis).map_err(|_| fault())?,
    })
}
// end activity.rs
