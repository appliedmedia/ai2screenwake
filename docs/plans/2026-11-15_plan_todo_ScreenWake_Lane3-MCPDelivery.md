<!-- 2026-11-15_plan_todo_ScreenWake_Lane3-MCPDelivery.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# ScreenWake Lane 3 MCP delivery

Status: todo, awaiting application implementation approval. Plan/README updates and pinned root `LICENSE.txt` check-in are authorized; no application implementation, execution, or release.

## Inputs, produces, and ownership

* Inputs: [master](<2026-11-15_plan_todo_ScreenWake_Orch.md>), [Lane 1 RustRuntime](<2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md>), [Lane 2 MacOSMenuBar](<2026-11-15_plan_todo_ScreenWake_Lane2-MacOSMenuBar.md>). Master TRD 6 defines the canonical tool, Status, protected-idle/event, result, and authorization contracts; reuse contract types/schema generation, never hand-mirror variants.
* Produces: this plan now; after approval, MCP transport/adapter, executable integration scenarios, canonical packaging/check tooling, and delivery documentation proving G2:SharedControl.
* NotProduces: owner IPC/runtime/assertions (Lane 1), OS/LaunchServices/UI (Lane 2), fourth runtime artifact/tool, cloud daemon, product AI, login startup, preferences, license copying, or independent release.
* Planned exclusive modules: `code/crates/ai2screenwake_client/src/bin/ai2screenwake-mcp.rs`, `src/mcp/{mod.rs,protocol.rs,tools.rs,owner_connection.rs}` within that crate; `tests/features/{mcp_protocol.feature,mcp_owner.feature}` and `tests/steps/mcp_delivery.rs` there; `code/xtask/{Cargo.toml,src/main.rs,README.md}`.
* Future shared integration surfaces: `.github/workflows/check.yml`, client README, `code/README.md`, root workspace/client manifests. Lane 1 freezes manifests/module exports and approves dependency changes; coordinator serializes shared edits. L3 never edits L1 runtime modules or L2 native/UI files.

## TRD

* 1. Protocol
  * 1.1. At execution verify/pin a compatible Rust MCP SDK and supported protocol negotiation. Implement `initialize`, `tools/list`, `tools/call`; reject unsupported versions, unknown methods/tools, malformed calls, and invalid parameters with protocol-correct structured JSON-RPC errors, not success-shaped tool results.
  * 1.2. Advertise only `is_on` ("Am I on?"), `turn_on` ("Turn on"), `turn_off` ("Turn off"); each input is `{"type":"object","properties":{},"additionalProperties":false}`. Advertise exactly the master's output schema: JSON text `content` matches `structuredContent`, and `isError` equals error presence.
  * 1.3. Queries return ordered snapshots. Setters are idempotent, registering correlation/interest before typed dispatch and completing only from observed convergence. Superseded setters return explicit errors; stale outcomes cannot overwrite newer intent. Timeout/disconnect never cancels intent or fabricates off/success; include last observed Status when available.
  * 1.4. Preserve every canonical Status/error field. Only while `status.is_on=true`, serialize the master's `protected_idle_duration_seconds` (`u64|null`), `protected_idle_suppressed_events` (`SuppressedEvent[]`), and `suppressed_events_truncated` (`bool`); omit all three when off or unresolved. `is_on` returns this JSON, never an extra `idle_info` tool.
  * 1.5. Duration is floor(min(genuine OS idle age, monotonic elapsed since native-confirmed On)), in seconds, not enable age alone. Repeated queries/TurnOn never reset it; real input, sleep/wake, off followed by new On, and restart reset the episode. Native sampling errors use the canonical explicit error path and null duration, never fake zero/off. Samples/observations traverse the same Actions bus; MCP only serializes the owner's snapshot, with no independent clock/state.
  * 1.6. Reuse canonical SuppressedEvent: `kind`, `epoch_ms` (UTC Unix milliseconds, `u64`), and `minutes_into_protected_idle` (finite nonnegative monotonic milliseconds since that event's current protected-idle episode anchor divided by 60000). Record at occurrence; wall-clock jumps never change offsets. Require matching OS-reported eligible request/cancellation proof; `idle_sleep_request_cancelled` is conditional, not a promised available signal. Never invent per-attempt display/screensaver records or classify active assertions, observed sleep/wake, lid, manual, or security events as suppression.
  * 1.7. `[]` means no confirmed records, not inferred attempts. Return only the latest 64 proven records in the current interval, with truthful truncation; exclude stale/history events from the snapshot and retain history separately in the action log. No extra tool, polling/timer automation, or release change.
* 2. Owner connection
  * 2.1. Stdio connects to one same-user, login-session app owner; it holds no assertions or independent state and never automatically enables protection. Native-seam LaunchServices starts only the exact bundled app; simultaneous clients/launches converge on the singleton.
  * 2.2. Consume L1/L2's canonical session directory and authenticated connector: non-symlink UID ownership, directory 0700, socket 0600, valid path length, exclusive owner lock, matching `getpeereid` UID. Reject unsafe paths/peers; never unlink sockets or bypass owner-lock cleanup.
  * 2.3. Enforce four-byte big-endian UTF-8 KvEnvelope framing, 64 KiB/frame, 32 outstanding requests/peer, bounded buffers, 10-second launch/connect and 5-second operations. Bound stdio parsing/backpressure too. Dispatch malformed-frame/errors first; observers own stderr diagnostics, stdout exclusively MCP. JSON-RPC correlation is transport-only; core/UI remain nonblocking emit/observe, never return-state RPC.

## Implementation DAG and acceptance

All checkboxes remain unstarted.

* [ ] L3.1: After D1 approval and D2 frozen typed contracts/ordering invariants, agree disjoint ownership and SDK/manifest changes with L1. Write executable Cucumber Given/When/Then scenarios; prove behavioral red before production edits. Compiler errors/zero selections are not red.
* [ ] L3.2: After L3.1, implement protocol and owner adapter in parallel with L2; support tests do not replace scenarios. Exercise actual stdio plus real owner: initialization/schema/exactly-three-tools/invalid-params/unknown-tool; native and MCP on/query/off; repeated setters/two clients/opposite intents/stale outcomes; disconnect/unavailable/restarting owner/unsafe socket/frame/deadline failures. Assert canonical On-only fields and off/unresolved omission; native-idle versus enable-age limiting, floor seconds, repeated-query/TurnOn continuity, every episode reset, and explicit sampling-error/null duration. Require exact eligible OS request/cancellation proof for records; test empty lists and reject fabricated/non-event inputs. Verify UTC epoch milliseconds and occurrence-time monotonic minutes mapping despite clock jumps, prior-episode exclusion, and latest-64 retention with false truncation at 64 and true beyond 64. MCP must not compute duration/events independently.
* [ ] L3.3: After frozen generation contracts, implement shared local/CI entries `cargo xtask build --profile release`, `cargo xtask sanity`, `cargo xtask check`, `cargo xtask verify-macos`. Build generates UniFFI and worktree-local `target/screenwake/ScreenWake.app`, including `Contents/MacOS/ai2screenwake-mcp`; no iOS schemes. Installed configuration example names `/Applications/ScreenWake.app/Contents/MacOS/ai2screenwake-mcp`, explicitly a future installed path.
* [ ] L3.4: After L1/L2 integration, require fmt/clippy/Cucumber, contract-only cargo-tree isolation, Swift/UniFFI/bundle identity, docs lockstep, exactly-three-artifacts/tools audit, and unchanged pinned license: `git hash-object LICENSE.txt` equals master blob `90b2643a579327564ed214dee8ec849fbc1732f1`. Verify privacy/redaction, localization/accessibility metadata, and authorized core-only PostHog/bug-report configuration without invented endpoints/credentials.
* [ ] L3.5: Sanity <=10 seconds; completed-wave full checks <10 minutes with >=3 isolated independent host workers and named serial exceptions. Persist stage/total start/finish/elapsed/command/environment/git_sha/worker_count/exit_status rows and scenario counts. Real witnesses require the immediately built target/configuration/commit, exit 0 and success markers; L2 separately owns actual macOS idle/screensaver proof, never unit-only equivalence.
* [ ] L3.6: Coordinator integrates one wave, then one pre-PR ARCH/QUALITY/PROCESS triad and one consolidated fix result. Commit before final validation; preserve immutable validated SHA through push/open. One `coderabbit_optional` PR uses exact trigger/bounded confirmation, no assigned human reviewer or repeated triad. Require merge/readback/cleanup/audit; distinguish local bundle from release/posted proof, with shareable date UNKNOWN without evidence. No per-lane PRs: open exactly one delivery PR from the shared wave integration branch (head) into canonical `main` (base).

## ModelPicker, receipt, and decisions

Documentation baseline: `mod:c65+s85='copilot:gpt-5.6-terra:1M:xhigh'`. Future implementation baseline: `mod:c70+s75='copilot:gpt-6-sol:1M:max'`; strong reasoning owns correlation/security invariants. Actual inherited context/effort: UNVERIFIED; record actual dispatch profile separately from baseline/fallback before execution.

Task `task_d871f138c14c`, Dispatch `ctx_dc38413e7cf5`, worker `term_9a021fcc-0941-4f38-a4c7-083efeccba37`. Worktree `/Users/acoven/dev/ai2sw/workspaces/main/sponge`, branch `boot-ai-context-report`; canonical main `/Users/acoven/dev/ai2sw/main`. Coordinator integrates the preserved documentation diff; this worker does not stage/commit.

2026-11-15 requirement amendment: canonical On-only protected-idle duration and proof-only suppressed-event JSON, including units, resets/errors, clock jumps, and latest-64 truncation. `mod:c55+s85='copilot:gpt-5.6-terra:1M:xhigh'`; Task `task_b3ce884d9ffd`, Dispatch `ctx_1f7dc8e0bc55`, worker `term_bccb44cf-1918-4e7d-b03c-2d3ac87eead0`, same assigned worktree/branch. Documentation only; implementation remains unapproved, all tasks unstarted, no staging/commit.

Pending decision log: EMPTY; inherited approval/design gates remain in the master.

Operator actions surface: [master](<2026-11-15_plan_todo_ScreenWake_Orch.md#operator-next-actions>), record/reference only.

<!-- end 2026-11-15_plan_todo_ScreenWake_Lane3-MCPDelivery.md -->
