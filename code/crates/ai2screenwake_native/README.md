<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Native boundary contracts

`src/contracts.rs` defines the injected boundary, not a fake OS implementation:

```rust
pub trait NativePower: Send + Sync {
    fn apply(&self, request: Apply, events: Emitter);
    fn sample(&self, request: IdleRequest, events: Emitter);
}
```

`contracts::register(&mut RegistryBuilder, Arc<dyn NativePower>)` wires both required handlers. Implementations retain their real resource IDs, acquire both assertions all-or-nothing, retain failed-release IDs and emit typed `Power::Assertions::Observed` with the exact request, resource count, outcome, fault codes and paired occurrence clocks. Never return state or manufacture success.

`NativeSeam::attach(Registry)` exposes `emit(KvEnvelope) -> ()` and `observe(Arc<dyn ObservationSink>) -> ()`; the sink receives `observed(KvEnvelope) -> ()`. These are Rust callback contracts, not generated UniFFI/Swift bindings yet. Delayed native completions use the supplied typed `Emitter`, not public raw ingress.

Idle samples carry owner/session generation, confirmed-On episode and sample IDs. The native implementation captures actual OS idle age and paired UTC/continuous-monotonic milliseconds atomically. Core policy owns calculations and resets. A cancellation receipt additionally carries real matched request/acknowledgement identities and an occurrence-time monotonic anchor; no supported cancellation stream is asserted to exist.

Host compilation is on macOS 27.0.1 arm64 with Rust 1.97.1. Intended first native target is macOS 13+ arm64, not a verified backward-compatibility claim; other architectures are untested. Public Xcode SDK evidence inspected in this task: `IOPMLib.h` documents both idle assertions and `IOPMAssertionCreateWithName` availability from 10.6; `CGEventSource.h:136-143` documents elapsed genuine-input time and availability from 10.4. No API was executed. Native idle accuracy/permissions, sleep/wake clock handling and screensaver efficacy require D4's actual supported-system witness. Display assertion documentation alone does not prove screensaver protection.

Future native workers may add `src/power.rs`, `src/activity.rs`, `src/identity.rs`, `src/launch.rs` and `code/macos/ScreenWake/`. They implement these frozen traits without editing `src/contracts.rs` or the client runtime. Module exports/manifests and binding generation are shared integration edits owned by Lane 1. No resources, settings, app, lock/sleep or long idle test are run by this foundation.

<!-- end README.md -->
