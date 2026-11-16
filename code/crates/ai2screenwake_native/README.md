<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Native macOS boundary

`src/contracts.rs` remains the frozen injected boundary:

```rust
pub trait NativePower: Send + Sync {
    fn apply(&self, request: Apply, events: Emitter);
    fn sample(&self, request: IdleRequest, events: Emitter);
}
```

`power::MacPower::new() -> MacPower` creates a resource-free OFF owner backed by actual public macOS APIs. Inject `Arc::new(MacPower::new())` into `contracts::register(&mut RegistryBuilder, Arc<dyn NativePower>)`, which wires both required handlers. `MacPower::with_api(Arc<dyn PowerApi>)` is the explicit low-level injection seam for deterministic fault tests, not an automatic production fallback.

`src/power.rs` owns both IOKit IDs: `PreventUserIdleSystemSleep` and `PreventUserIdleDisplaySleep`, acquired using `IOPMAssertionCreateWithName`. Partial acquisition rolls back; a failed release retains its ID for a later explicit Off retry. Repeated enable does not duplicate held IDs. Errors dispatch first, then `Power::Assertions::Observed` carries the exact request, actual retained count, outcome and paired clocks. Mutexes are released before any observer is called, including synchronous cleanup requests.

Quit must emit and observe bounded explicit Off cleanup while the bus is live. Final `Drop` only attempts best-effort OS release; it cannot report success or retry indefinitely. No strong Emitter is cached: Registry owns its native handler, so retaining one would create a cycle and prevent destruction. A destructor release failure has no live callback path; process exit remains the final OS cleanup boundary. This lifetime constraint was clarified by coordinator message `msg_20aa512637b2`.

`NativeSeam::attach(Registry)` exposes `emit(KvEnvelope) -> ()` and `observe(Arc<dyn ObservationSink>) -> ()`; the sink receives `observed(KvEnvelope) -> ()`. These are Rust callback contracts, not generated UniFFI/Swift bindings yet. Delayed native completions use the supplied typed `Emitter`, not public raw ingress.

`src/activity.rs` samples `CGEventSourceSecondsSinceLastEventType(kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType)`. It validates finite, nonnegative, representable age before converting seconds to milliseconds; source faults dispatch an attributed error before an explicit null-age observation. Every observation preserves owner/generation/episode/sample identity. Clock capture pairs real UTC with a bracketed `mach_continuous_time` sample and checked timebase conversion; that monotonic clock includes sleep. Client policy, not native sampling, must reset/exclude suspended episodes. If clocks themselves fail, only the explicit timing error is emitted: the frozen nonnullable ClockSample cannot truthfully describe an outcome without a valid clock, so callers must retain pending/error and use their deadline rather than invent a timestamp.

Native sampling has no event tap, synthetic input, activity pulse, privileged/input-monitoring permission, preference write, or input-content logging. OS-reported input age is not a per-attempt cancellation stream. This increment emits no suppression records and does not infer them from assertions or elapsed time. Screensaver efficacy, forced-sleep/wake listener wiring and controlled idle behavior remain unverified integration gates; display assertion documentation is not screensaver proof.

## Local host helpers

* `identity::runtime_directory() -> Result<PathBuf, Fault>` resolves Darwin's `_CS_DARWIN_USER_TEMP_DIR` and `SessionGetInfo(callerSecuritySession)` to a canonical parent plus `ai2screenwake-<security-session-id>`. It ignores `TMPDIR` and process `getsid`, requires the current graphical login security session and unchanged real/effective UID, and creates nothing. App/MCP callers in different security sessions intentionally do not share an owner. Client IPC owns final path-length/symlink/UID/mode validation, 0700 directory creation, exclusive locks, 0600 sockets and peer authorization.
* `launch::launch_owner(bundle_path: &Path) -> Result<(), Fault>` accepts an absolute existing `.app` directory selected by the authenticated owner connector. It uses that exact canonical CFURL in `LSOpenFromURLSpec` with Async/DontSwitch/DontAddToRecents, without new-instance or OS error-display flags. Success means admission, not readiness or enabled protection. The caller dispatches returned startup/launch faults through its existing error path and waits for an authenticated owner; this leaf never launches an app in tests.
* `launch::OwnerLauncher: Send + Sync` exposes `fn launch(&self) -> Result<(), Fault>`. `launch::BundleLauncher::new(bundle_path: PathBuf) -> BundleLauncher` implements it by invoking the exact-bundle helper; a transport worker, not a blocking UI/core call, invokes this bootstrap port.

These are native-seam primitives, not new UI/core methods or public MCP tools. Future generated Swift/FFI host attachment uses `MacPower` plus the unchanged `contracts.rs` emit/observe port. The client owns policy, correlation, IPC, shutdown deadlines and episode resets; Swift, MCP, xtask, contract files and license remain outside this leaf.

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

Run-time sanity excludes separately measured compilation and must remain <=10 seconds. Worktree-local `target/native-evidence/` retains command output and stage/total rows with SHA, environment, worker count, start/end, elapsed time and exit status; it is not checked in. Existing xtask remains foundation-only, so the build owner must integrate these exact native selectors rather than treating them as already present in the shared runner.

Intended first native target: macOS 13+ arm64; actual host: macOS 27.0.1 arm64, Rust 1.97.1. SDK evidence: IOPMLib.h documents both assertion types and create/release/property APIs; CGEventSource.h:136-143 documents elapsed input age since 10.4; mach_time.h documents continuous time since 10.12; AuthSession.h documents login-session inheritance; LSOpen.h specifies the exact URL launch and flags. Deployment-target compilation is not a macOS 13 runtime witness; Intel/universal, actual bundle launch, long idle/screensaver behavior, and permission behavior across the support matrix remain unverified.

Native Task `task_77088cf104e7`, Dispatch `ctx_781f12d83f4e`, branch `screenwake-native-power`, exact root `/Users/acoven/dev/ai2sw/workspaces/main/screenwake-native-power`, D2 base `74e9486`. Baseline `mod:c75+s85='copilot:gpt-6-sol:1M:max'`; actual process argv: `gpt-6.1-sol`, `long_context`, `high`, no override/fallback; human-facing context capacity unverified.

<!-- end README.md -->
