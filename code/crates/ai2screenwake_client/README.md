<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Client owner foundation

`runtime::start(native: Option<Arc<dyn NativePower>>) -> Result<Registry, StartupError>` starts an isolated OFF owner and returns only its Actions port. Missing native collaborators fail explicitly before startup. Register observation before emitting controls; `Power::Session::Snapshot` carries state, never an emit return value.

The implemented slice admits typed controls, serializes native applies, shares repeated pending setters, rejects mismatched completions, reconciles newer intent only after older physical work completes, and reports unknown/error for partial or failed resource convergence. Native faults enter canonical error actions before the failed projection. A query while confirmed On requests fresh native idle age and calculates floor(min(OS idle age, elapsed confirmed-On age)/1000). Sampling faults preserve independently confirmed protection and expose null duration, never fabricated zero.

`tests/features/foundation.feature` and `tests/steps/foundation.rs` exercise the real typed bus, serialized contracts and an explicitly test-only controlled native boundary. No production native implementation pretends to acquire resources. Run `cargo xtask prepare`, then `cargo xtask sanity` from the exact root; the latter verifies provenance and runs the focused scenarios with three isolated workers within 10 seconds, including setup. See [runner details](<../../xtask/README.md>).

D3 remains responsible for completing durable replay/history, bounded log retention, genuine-activity and forced-sleep/wake episode resets, cancellation-proof processing and latest-64 truncation, and IPC ownership/authentication/deadlines. Native OS implementations are owned by the separate native package; this client increment adds no OS power execution, generated bindings, app, MCP server, telemetry destination or release.

Future MCP leaves can own `src/bin/ai2screenwake-mcp.rs`, `src/mcp/`, `tests/features/mcp_*.feature` and `tests/steps/mcp_delivery.rs`. Changes to `src/lib.rs`, root/client manifests and all runtime/contract files must be serialized through Lane 1. Do not copy schemas or create a fourth runtime crate.

## D3 request work

Concurrent native idle samples are independently keyed; a later-arriving older sample cannot rewind the shared observed projection. Every admitted control has one eventual `Power::Session::Settled` result, separate from intermediate observed Snapshots: repeated setters wait for their shared native convergence, every superseded setter gets its own error, and Quit only settles Off after explicit zero-resource confirmation. Timed-out or abandoned callers release notifications without canceling physical intent; late native results cannot complete an expired caller twice.

`src/projection.rs` caps retained requests/deadlines at 128 and uses one condition-variable scheduler, not per-request blocking threads or an unbounded timer queue. Waiters and deadlines hold only weak continuations. Production deadline is five seconds; `runtime::start_with_options` accepts a positive shorter test/host deadline, never a longer timeout or product preference. Expiration itself is a named logged action. `tests/features/runtime.feature` and `tests/steps/runtime.rs` are the affected target, separate from D2 compatibility; real socket correlation, episode policy and durable history remain in progress until the D3 completion receipt.

Backend `Registry::prepare<A>(payload)` reserves only opaque request metadata; `PreparedAction::correlation()` allows result interest to be registered before `emit_prepared(prepared)`. No core state is returned. `Registry::subscribe(sink)` provides a peer-lifetime subscription whose drop unregisters future callbacks; `Emitter::downgrade()` provides non-owning notification retention. Existing native emit/observe signatures remain unchanged.

## Local owner consumer API

`src/ipc.rs` now supplies the actual private Unix-socket transport:

* `for_current_user(directory: PathBuf) -> OwnerLocation` binds the real UID to the directory supplied by native `identity::runtime_directory()`, without guessing a temp location.
* `OwnerServer::bind(location: OwnerLocation, native: Arc<dyn NativePower>) -> Result<OwnerServer, StartupError>` acquires the exclusive private owner and starts OFF; `actions() -> Registry` supplies the exact emit/observe port for `NativeSeam::attach`.
* `OwnerConnection::connect(location: OwnerLocation, launcher: Arc<dyn ai2screenwake_native::launch::OwnerLauncher>) -> Result<OwnerConnection, StartupError>` starts a lazy connector, not protection. The native `BundleLauncher` owns exact app selection; actual readiness is bounded separately.
* Call `connection.observe(Arc<dyn ObservationSink>)` once, reserve `connection.prepare::<Power::Session::Query>()` or either setter, register the opaque `PreparedAction::correlation()` in the consumer's request map, then `connection.emit(prepared)`. Emit returns `()`; only `Power::Session::Settled` carries eventual `Settlement { correlation, result: ControlResult { status, error } }`. Exactly Query/TurnOn/TurnOff implement `OwnerControl`; Quit and Show Log are native-only.

Native primary-click uses typed `Power::Session::Toggle(Empty)`, admitted only on the native seam. Its core handler reads the latest admitted/pending owner intent and emits typed TurnOn or TurnOff; Swift owns no toggle policy or mirrored enabled flag. A small queued-target fence keeps already queued native toggles from reading the same older intent before their derived setter messages run. Toggle does not implement `OwnerControl` and is not an IPC/MCP tool.

The transport uses the same KV envelope with a four-byte big-endian prefix, a 64 KiB body limit and 32 unsettled requests per peer. It verifies actual peer UID, private 0700 owned non-symlink directories, 0600 sockets and lock files, nonblocking exclusive `flock`, bounded socket paths and stale socket identity before removal. There are at most 16 peer workers; queues are bounded, scoped subscriptions are released on disconnect, and disconnect never reverses admitted intent. Launch/connect is limited to ten seconds and operations to five seconds; failures enter error actions before terminal results. The native launch helper is never an automatic TurnOn.

For normal host shutdown, emit native Quit and observe its confirmed-Off settlement before disposing the server. `Drop` closes transport and reconciles only its own unchanged socket inode; it does not pretend native cleanup succeeded. The current transport checkpoint is real IPC with in-memory owner history; durable history and remaining episode policy are still D3 work, not claimed complete.

`tests/features/ipc.feature` / `tests/steps/ipc.rs` exercise real private sockets, two clients plus native control, actual peer UID/mismatch, the exclusive owner lock, launch-without-enabling, malformed and oversized input, forged native-outcome rejection, disconnect without canceling intent, the 32-request limit, stale owned sockets, symlink preservation, fragmented frames and unsafe-directory rejection. Native resources are deterministic test-only collaborators, not production substitutes; no different-UID process or OS power witness is claimed. Durable restart coverage remains in the active D3 increment. Run affected `cargo test -p ai2screenwake_client --test runtime --test ipc -- --concurrency 3` after separately measured `--no-run` preparation; the parallel tooling owner will add these targets to the canonical xtask selection.

<!-- end README.md -->
