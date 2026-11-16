<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Client owner foundation

`runtime::start(native: Option<Arc<dyn NativePower>>) -> Result<Registry, StartupError>` starts an isolated OFF owner and returns only its Actions port. Missing native collaborators fail explicitly before startup. Register observation before emitting controls; `Power::Session::Snapshot` carries state, never an emit return value.

The implemented slice admits typed controls, serializes native applies, shares repeated pending setters, rejects mismatched completions, reconciles newer intent only after older physical work completes, and reports unknown/error for partial or failed resource convergence. Native faults enter canonical error actions before the failed projection. A query while confirmed On requests fresh native idle age and calculates floor(min(OS idle age, elapsed confirmed-On age)/1000). Sampling faults preserve independently confirmed protection and expose null duration, never fabricated zero.

`tests/features/foundation.feature` and `tests/steps/foundation.rs` exercise the real typed bus, serialized contracts and an explicitly test-only controlled native boundary. No production native implementation pretends to acquire resources. `cargo test -p ai2screenwake_client --test foundation` is the focused runner; `cargo xtask sanity` will be its measured canonical entry.

D3 remains responsible for complete multi-request correlation, durable replay/history, bounded log retention, genuine-activity and forced-sleep/wake episode resets, cancellation-proof processing and latest-64 truncation, IPC ownership/authentication/deadlines, and shutdown completion. The present cancellation contract is reserved, not wired to a fabricated source. No native OS implementation, generated bindings, app, MCP server, telemetry destination or release is delivered here.

Future MCP leaves can own `src/bin/ai2screenwake-mcp.rs`, `src/mcp/`, `tests/features/mcp_*.feature` and `tests/steps/mcp_delivery.rs`. Changes to `src/lib.rs`, root/client manifests and all runtime/contract files must be serialized through Lane 1. Do not copy schemas or create a fourth runtime crate.

<!-- end README.md -->
