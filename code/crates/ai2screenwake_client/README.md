<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Client owner foundation

`runtime::start(native: Option<Arc<dyn NativePower>>) -> Result<Registry, StartupError>` starts an isolated OFF owner and returns only its Actions port. Missing native collaborators fail explicitly before startup. Register observation before emitting controls; `Power::Session::Snapshot` carries state, never an emit return value.

The implemented slice admits typed controls, serializes native applies, shares repeated pending setters, rejects mismatched completions, reconciles newer intent only after older physical work completes, and reports unknown/error for partial or failed resource convergence. Native faults enter canonical error actions before the failed projection. A query while confirmed On requests fresh native idle age and calculates floor(min(OS idle age, elapsed confirmed-On age)/1000). Sampling faults preserve independently confirmed protection and expose null duration, never fabricated zero.

`tests/features/foundation.feature` and `tests/steps/foundation.rs` exercise the real typed bus, serialized contracts and an explicitly test-only controlled native boundary. No production native implementation pretends to acquire resources. Run `cargo xtask prepare`, then `cargo xtask sanity` from the exact root; the latter verifies provenance and runs the focused scenarios with three isolated workers within 10 seconds, including setup. See [runner details](<../../xtask/README.md>).

D3 remains responsible for complete multi-request correlation, durable replay/history, bounded log retention, genuine-activity and forced-sleep/wake episode resets, cancellation-proof processing and latest-64 truncation, IPC ownership/authentication/deadlines, and shutdown completion. The present cancellation contract is reserved, not wired to a fabricated source. No native OS implementation, generated bindings, app, MCP server, telemetry destination or release is delivered here.

Future MCP leaves can own `src/bin/ai2screenwake-mcp.rs`, `src/mcp/`, `tests/features/mcp_*.feature` and `tests/steps/mcp_delivery.rs`. Changes to `src/lib.rs`, root/client manifests and all runtime/contract files must be serialized through Lane 1. Do not copy schemas or create a fourth runtime crate.

## D3 request work

The first D3 correction keeps concurrent native idle samples independently keyed by sample identity; a later-arriving older sample cannot rewind the shared observed projection. `tests/features/runtime.feature` and `tests/steps/runtime.rs` are the affected runtime target, separate from the D2 compatibility target. Per-request terminal settlement, bounded notification lifetime, episode policy, durable history and owner IPC remain in progress until the D3 completion receipt.

Backend `Registry::prepare<A>(payload)` reserves only opaque request metadata; `PreparedAction::correlation()` allows result interest to be registered before `emit_prepared(prepared)`. No core state is returned. `Registry::subscribe(sink)` provides a peer-lifetime subscription whose drop unregisters future callbacks; `Emitter::downgrade()` provides non-owning notification retention. Existing native emit/observe signatures remain unchanged.

<!-- end README.md -->
