<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Native macOS boundary

The user-facing assertion reason is `AI2ScreenWake explicit idle protection`. This display correction changes neither IOKit resource types/ownership nor bundle ID `com.appliedmedia.ScreenWake`, native LaunchServices validation or the `ai2screenwake-<login-session>` private directory. Launch resolves the actual selected `ai2sw.app` by URL, not a hardcoded basename.

`src/contracts.rs` remains the frozen injected boundary:

```rust
pub trait NativePower: Send + Sync {
    fn watch_lifecycle(&self, events: Emitter) {}
    fn apply(&self, request: Apply, events: Emitter);
    fn sample(&self, request: IdleRequest, events: Emitter);
}
```

`power::MacPower::new() -> MacPower` creates a resource-free OFF owner backed by actual public macOS APIs. Inject `Arc::new(MacPower::new())` into `contracts::register(&mut RegistryBuilder, Arc<dyn NativePower>)`, which wires both required handlers. `MacPower::with_api(Arc<dyn PowerApi>)` is the explicit low-level injection seam for deterministic fault tests, not an automatic production fallback.

`src/power.rs` owns both IOKit IDs: `PreventUserIdleSystemSleep` and `PreventUserIdleDisplaySleep`, acquired using `IOPMAssertionCreateWithName`. Partial acquisition rolls back; a failed release retains its ID for a later explicit Off retry. Repeated enable does not duplicate held IDs. Errors dispatch first, then `Power::Assertions::Observed` carries the exact request, actual retained count, outcome and paired clocks. Mutexes are released before any observer is called, including synchronous cleanup requests.

Quit must emit and observe bounded explicit Off cleanup while the bus is live. Final `Drop` only attempts best-effort OS release; it cannot report success or retry indefinitely. No strong Emitter is cached: Registry owns its native handler, so retaining one would create a cycle and prevent destruction. A destructor release failure has no live callback path; process exit remains the final OS cleanup boundary. This lifetime constraint was clarified by coordinator message `msg_20aa512637b2`.

`NativeSeam::attach(Registry)` exposes `emit(KvEnvelope) -> ()` and `observe(Arc<dyn ObservationSink>) -> ()`; the sink receives `observed(KvEnvelope) -> ()`. The integrated client FFI host and generated UniFFI/Swift attachment use this same owner. Delayed native completions use the supplied typed `Emitter`, not public raw ingress.

`src/activity.rs` samples `CGEventSourceSecondsSinceLastEventType(kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType)`. It validates finite, nonnegative, representable age before converting seconds to milliseconds; source faults dispatch an attributed error before an explicit null-age observation. Every observation preserves owner/generation/episode/sample identity. Clock capture pairs real UTC with a bracketed `mach_continuous_time` sample and checked timebase conversion; that monotonic clock includes sleep. Client policy, not native sampling, resets/excludes suspended episodes. A sampling clock failure emits its explicit timing error without inventing a sample. An apply clock failure instead emits error-first `Power::Assertions::Failed(FailedApply { request, held_count, fault })`, retiring that exact completed physical operation without a clock or success claim. Preliminary timing failure prevents acquisition, never an Off release attempt. Post-acquisition failure retains both IDs; failed releases retain their actual IDs. Explicit Off/Quit can retry, while caller expiration alone never retires unfinished physical work.

Native sampling has no event tap, synthetic input, activity pulse, privileged/input-monitoring permission, preference write, or input-content logging. OS-reported input age is not a per-attempt cancellation stream. This increment emits no suppression records and does not infer them from assertions or elapsed time. Actual forced-sleep/wake product behavior, screensaver efficacy and controlled idle behavior remain unverified integration gates; display assertion documentation is not screensaver proof.

## Explicit sleep/wake monitor

`Power::Activity::Watch` binds the source-owning native implementation's weak observation provenance; `Availability { revision, available }` uses that same typed logged bus. Production `MacPower::new` requires successful registration before acquisition. A failed poll atomically latches unavailable under the resource mutex before emitting its fault/availability; new enables fail without acquiring, while Off always attempts release. Core invalidates duration/proof intervals and rejects On until a newer registration succeeds, without auto-enabling. Source-generation guards prevent a failed monitor's late stop/callback from invalidating its replacement. Low-level injected `with_api` seams have no background-source requirement until an explicit monitor starts.

Queued interruption fan-out rechecks both source generation and acquisition identity, so work captured by an older monitor cannot escape its replacement guard.

The generated host keeps its concrete `Arc<MacPower>`, registers it through the unchanged native contracts, then calls `lifecycle::LifecycleMonitor::start(power.clone()) -> Result<LifecycleMonitor, Fault>` and retains the returned guard. Construction of MacPower alone never registers notifications or starts a thread. Startup has a one-second handshake deadline and reports registration/thread failures explicitly. Startup/shutdown faults return to the host for dispatch through its existing error path; notification allowance/clock/delivery faults dispatch `Error::Native::Activity::Occurred` through the live weak continuation.

The source registers `IORegisterForSystemPower`, attaches its owned notification-port source to a dedicated thread's CFRunLoop, and pumps at most 25ms per iteration. `CanSystemSleep` and `SystemWillSleep` always call `IOAllowPowerChange` with the actual token before any owner mutex, clock, queue or observer. `CanSystemSleep` never produces an interruption. Only `SystemWillSleep` and `SystemHasPoweredOn` produce `Power::Activity::Interrupted`, with the exact active Apply, actual retained count and paired occurrence clocks; wake is not acknowledged. There is no cancellation/veto call, new assertion, activity pulse or suppression entry.

Native stores only the active acquisition's `WeakEmitter` alongside the retained IDs. Off clears that context; clock failures never fabricate an occurrence; expired registries cannot be resurrected. The callback captures native accounting under its mutex, releases it, and queues delivery on a separate worker so slow/reentrant bus observers cannot delay OS acknowledgements. Delivery checks that the acquisition is still current; the core's existing request/generation guards remain authoritative for a concurrent Off/new-generation race. The bounded 32-entry queue reports overflow explicitly before its newest observation instead of silently pretending an uninterrupted history.

The host calls `monitor.stop(&mut self) -> Result<(), Fault>` during teardown, separately from bounded, bus-observed explicit Off cleanup. Stop signals both workers, removes the CF source, deregisters the notifier, closes the IOKit connection and destroys the port on the registering thread before freeing callback context. It waits at most 500ms for completion and never joins an unfinished/self thread. A stop called by its own bus observer lets that delivery thread exit after the observer returns. A long-running external observer produces an explicit timeout; retain the guard, release that observer and retry stop. Drop signals best-effort cleanup with the same bound; even on timeout, owned thread context remains alive until the thread exits, so no callback points to freed memory. A monitor does not hold a strong registry or keep MacPower alive.

`LifecycleMonitor::with_source(Arc<MacPower>, Arc<dyn NotificationSource>)` is the explicit native notification injection seam used by executable lifecycle scenarios, not a production no-op fallback. Its thread-confined `NotificationRegistration` must implement bounded `poll` and same-thread `stop`; the actual source is IOKit/CF only. SDK evidence: IOPMLib.h:127-208 requires prompt allowance for non-abortable WillSleep/idle permission and forbids acknowledgements for completed wake; IOMessage.h:131-175 supplies the corresponding messages; IOKitLib.h:159-182 defines source/port ownership.

## Local host helpers

* `identity::runtime_directory() -> Result<PathBuf, Fault>` resolves Darwin's `_CS_DARWIN_USER_TEMP_DIR` and `SessionGetInfo(callerSecuritySession)` to a canonical parent plus `ai2screenwake-<security-session-id>`. It ignores `TMPDIR` and process `getsid`, requires the current graphical login security session and unchanged real/effective UID, and creates nothing. App/MCP callers in different security sessions intentionally do not share an owner. Client IPC owns final path-length/symlink/UID/mode validation, 0700 directory creation, exclusive locks, 0600 sockets and peer authorization.
* `identity::readable_log_path()` resolves the real-UID account home with `getpwuid_r`, never HOME or shell commands, and captures one real `SystemTime` instant. `localtime_r` produces `Library/Logs/AI2ScreenWake/yyyyMMdd{DD}HHmmss_ai2sw.log` with local 24-hour time and weekday `mo/tu/we/th/fr/sa/su`. This creates no directory/file and is independent of binary/IPC identity. The host retains any failure for an explicit log query, not failed power startup.
* Rust-only `readable_log_path_in(home)` and `readable_log_path_at(home, epoch_seconds)` pair the same resolver/formatter with owned fixture roots and injected instants; no generated Swift setting or ambient override exists. Executable native calendar cases cover local midnight/date/weekday rollover, all seven weekday tokens, 24-hour noon and no file creation. Client snapshot cases cover actual isolated output/title/permissions, repeated refresh, collision preservation and deferred error-first behavior.
* `launch::launch_owner(bundle_path: &Path) -> Result<(), Fault>` accepts an absolute existing `.app` directory selected by the authenticated owner connector. It uses that exact canonical CFURL in `LSOpenFromURLSpec` with Async/DontSwitch/DontAddToRecents, without new-instance or OS error-display flags. Success means admission, not readiness or enabled protection. The caller dispatches returned startup/launch faults through its existing error path and waits for an authenticated owner; this leaf never launches an app in tests.
* `launch::OwnerLauncher: Send + Sync` exposes `fn launch(&self) -> Result<(), Fault>`. `launch::BundleLauncher::new(bundle_path: PathBuf) -> BundleLauncher` implements it by invoking the exact-bundle helper; a transport worker, not a blocking UI/core call, invokes this bootstrap port.

These are native-seam primitives, not new UI/core methods or public MCP tools. The integrated generated Swift/FFI host uses `MacPower` and the `contracts.rs` emit/observe port. The client owns policy, correlation, IPC, shutdown deadlines and episode resets; Swift only renders and forwards input.

## Verification

From this checkout's workspace root or native package directory, separately compile the affected target, then run its deterministic scenarios. The runner resolves `tests/features` from its compile-time `CARGO_MANIFEST_DIR`, independent of the working directory Cargo selects:

```sh
cargo test --locked -p ai2screenwake_native --test native --no-run
cargo test --locked -p ai2screenwake_native --test native -- --tags 'not @real_host' --concurrency 3
```

The real-host leaf is a named serial exception because its assertions affect the current login's idle resources. It acquires only its own two resources, queries their actual IOKit properties, samples input/clocks, releases both and verifies absence. It also checks two-process runtime-path agreement despite a TMPDIR override and rejection of invalid launch paths without launching anything:

```sh
cargo test --locked -p ai2screenwake_native --test native -- --tags '@real_host' --concurrency 1
cargo fmt -p ai2screenwake_native -- --check
cargo clippy --locked -p ai2screenwake_native --all-targets -- -D warnings
```

Changed lifecycle-only selectors are `--tags '@lifecycle and not @real_host' --concurrency 3` and serial `--tags '@lifecycle and @real_host' --concurrency 1` on that same cargo target, after separate compilation. The host smoke registers/unregisters twice without triggering sleep, wake, a screen lock, settings or permissions. It verifies registration ownership only, not actual forced-sleep delivery or product episode policy. All maintained power/activity/lifecycle scenarios remain in the general selectors above.

Run-time sanity excludes separately measured compilation and must remain <=10 seconds. Worktree-local evidence retains command output and timing; it is not checked in. The completed-wave `cargo xtask check` already includes both deterministic and serial real-host native/UI selections; `cargo xtask sanity` remains foundation-only. `cargo xtask build --profile release` generates and compiles the worktree-local production attachment and signed bundle. Controlled `MacPower::with_api` plus real-owner clock/retry regressions live in the client's `runtime` target under `@apply_clock`; they create no OS assertions.

Intended first native target: macOS 13+ arm64; actual host: macOS 27.0.1 arm64, Rust 1.97.1. SDK evidence: IOPMLib.h documents both assertion types and create/release/property APIs; CGEventSource.h:136-143 documents elapsed input age since 10.4; mach_time.h documents continuous time since 10.12; AuthSession.h documents login-session inheritance; LSOpen.h specifies the exact URL launch and flags. The master records exact source-5cc packaged launch/shared-control/error/replay/ordinary-Quit subcriteria, not full G1/G2. Deployment-target compilation is not a macOS 13 runtime witness; Intel/universal, physical menu/log, long idle/screensaver/security-preservation behavior and the support matrix remain unverified.

Historical native Task `task_77088cf104e7`, Dispatch `ctx_781f12d83f4e`, branch `screenwake-native-power`, exact root `/Users/acoven/dev/ai2sw/workspaces/main/screenwake-native-power`, D2 base `74e9486`. Baseline `mod:c75+s85='copilot:gpt-6-sol:1M:max'`; actual process argv: `gpt-6.1-sol`, `long_context`, `high`, no override/fallback; human-facing context capacity unverified.

Lifecycle increment: Task `task_a57c8e4f644a`, Dispatch `ctx_80bd4a091794`, same exact native branch/root, actual metadata base `7c077b9`. Baseline `mod:c80+s80='copilot:gpt-6-sol:1M:max'`; inherited actual profile unchanged, no override/fallback. Core/IPC/FFI/Swift/artwork/xtask/contracts and license are outside this leaf.

<!-- end README.md -->
