<!-- 2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Lane 1 RustRuntime

Status: todo, awaiting AndrewC implementation approval. Planning only, with separately authorized pinned license check-in; support matrix/screensaver efficacy remain unverified. No implementation, system changes, or release authorized.

## Inputs, outputs, and ownership

* Inputs: [committed master](<2026-11-15_plan_todo_ScreenWake_Orch.md>), `b229642`, its Sources and license receipt and P1 through P6. D1 approval precedes every implementation task.
* Produces now: this lane document; a separate worker owns the authorized planning-stage `LICENSE.txt` addition. Future D2/D3 produce frozen contracts, license verification, runtime, paired executable scenarios/docs, and handoff evidence.
* NotProduces: Swift/native OS implementations, MCP adapter/build driver, sibling documents, independent PRs, or unsupported capability claims.
* Future touches: root `Cargo.toml`/`LICENSE.txt`; `code/crates/ai2screenwake_backend/{Cargo.toml,src/lib.rs,src/contract.rs}`; `code/crates/ai2screenwake_client/{Cargo.toml,src/lib.rs,src/registry.rs,src/runtime.rs,src/projection.rs,src/action_log.rs,src/ipc.rs,src/ffi.rs,src/analytics.rs}`; `code/crates/ai2screenwake_native/{Cargo.toml,src/lib.rs,src/contracts.rs}`; associated rustdoc/folder READMEs.
* Tests owned: client `tests/features/{runtime,ipc,replay}.feature` with `tests/steps/{runtime,ipc,replay}.rs`; backend `tests/features/contract_isolation.feature` and `tests/steps/contract_isolation.rs`. Freeze shared runner wiring with Lane 3.
* [Lane 2 MacOSMenuBar](<2026-11-15_plan_todo_ScreenWake_Lane2-MacOSMenuBar.md>) owns native OS implementations and Swift. [Lane 3 MCPDelivery](<2026-11-15_plan_todo_ScreenWake_Lane3-MCPDelivery.md>) owns `ai2screenwake_client/src/bin/ai2screenwake-mcp.rs`, MCP tests and `code/xtask/`. Agree exact disjoint files/manifests before parallel execution; serialize later shared-seam edits through Lane 1.

## D2 license, support, and contract freeze

* [ ] After D1, verify root `LICENSE.txt` supplied by the authorized planning batch remains unchanged from `appliedmedia/print2paper4vscode` upstream `LICENSE` at commit `28d3c6ab714344058356fccfee386480cad7650f`; `git hash-object LICENSE.txt` must equal blob `90b2643a579327564ed214dee8ec849fbc1732f1`. Preserve bytes/header/dates/escaped ending. Source: master receipt; future verification only, no duplicate copy or completed license proof.
* [ ] Freeze supported macOS/architectures and requirement scenarios; do not infer screensaver protection from display assertions.
* [ ] Exactly backend/client/native runtime crates. Backend owns KvEnvelope/registry contract/payloads behind `contract`; consumers use `default-features=false, features=["contract"]`. Verify server dependency isolation; no mirrored schemas/fourth engine artifact.
* [ ] Freeze `emit(envelope: KvEnvelope) -> ()`, `observe(sink: Arc<dyn ObservationSink>) -> ()`, `ObservationSink::observed(envelope: KvEnvelope) -> ()`.
* [ ] Typed identities: `Power::Session::{TurnOn,TurnOff,Query,Quit,Changed,Snapshot}`, `Power::Assertions::{Apply,Observed}`, `Power::Activity::{Sample,Observed}`. Freeze native idle samples/cancellation receipts with session/episode/generation guards through the existing emit/observe seam. Private framework genus derives <=4 static segments; KV `_genus`, IdGeneration entities and correlation tails remain distinct. Inject collaborators; register handlers before startup; every dispatch appends/fans out.
* [ ] Apply: `{session_id:string,generation:u64,operation_id:string,enabled:bool}`. Observed continues correlation and adds `{held_count:u8,outcome:"applied"|"failed",fault_codes:string[]}`. Snapshot: `Status={is_on:bool|null,desired_on:bool,state:"off"|"on"|"pending"|"error",revision:u64,operation_id:string|null}`.
* [ ] Freeze the [canonical master TRD](<2026-11-15_plan_todo_ScreenWake_Orch.md#trd>) On JSON extension without weakening Status/resource-convergence/error safety: while `is_on=true`, include `protected_idle_duration_seconds:u64|null`, `protected_idle_suppressed_events:SuppressedEvent[]`, `suppressed_events_truncated:bool`; omit all three when off/unresolved. Keep exactly `is_on`, `turn_on`, `turn_off`, never `idle_info`.
* [ ] `SuppressedEvent={kind:string,epoch_ms:u64,minutes_into_protected_idle:number}`: capture Unix UTC milliseconds and monotonic offset at actual matching cancellation. Minutes are elapsed monotonic milliseconds since that protected-idle episode's anchor divided by 60000, finite/nonnegative; wall-clock jumps cannot change the offset.

## D3 runtime and focused green

* [ ] Single owner serializes native apply. Changed intent advances generation; identical setters share pending operation. Account for stale/duplicate completions, reconcile latest intent, and explicitly error superseded callers. True requires current-generation convergence holding both resources; false requires converged zero; otherwise null.
* [ ] Native contract acquires PreventUserIdleSystemSleep/PreventUserIdleDisplaySleep all-or-nothing, rolls back partial acquisition, retains failed-release IDs until cleanup, and reports partial/unknown outcomes. Quit drains bounded cleanup; process exit releases resources. Replay rebuilds history but every new session starts OFF without resurrecting intent/assertions.
* [ ] Rust core folds logged `Power::Activity::{Sample,Observed}` outcomes and calculates `floor(min(genuine OS idle age, monotonic elapsed since native-confirmed On))` in seconds; Swift/MCP own no idle timers or state calculations. Repeated status/TurnOn never reset the anchor. Genuine input, sleep/wake, new On, and restart reset the applicable episode; reject stale session/episode/generation samples and cancellation receipts. Native idle observation failure dispatches an explicit error and yields null duration, never fabricated zero or a false off/success.
* [ ] Record only a native-reported eligible idle transition with successful matching cancellation proof. `idle_sleep_request_cancelled` is a conditional candidate, not a guaranteed API alongside assertions. With no verifiable events, `[]` means none confirmed. Never list assertion active, on/off, inferred deadlines, observed sleep/wake, lid-close/manual sleep/security locking, or assumed screensaver cancellation.
* [ ] Scope the projected array to the current protected-idle interval, retaining its latest 64 real records and setting `suppressed_events_truncated` truthfully when earlier records were omitted. Preserve full durable history separately; replay never restores live effects or imports prior-episode events into the current array.
* [ ] Native supplies login-session directory/identity; Rust enforces non-symlink UID ownership, 0700 directory, exclusive owner lock, 0600 `owner.sock`, path length and `getpeereid` UID. Stale socket removal requires lock, no listener and inode/type/ownership checks. LaunchServices launches exact bundle; never shell daemon.
* [ ] IPC: four-byte big-endian length, UTF-8 KvEnvelope JSON, 64 KiB maximum, 32 outstanding requests/peer. Admit only three MCP control types, never native completions. Register before emitting; launch/connect 10s, operations 5s; timeout/disconnect never cancels intent.
* [ ] Dispatch `Error:<Module>:<Fault>:Occurred` first with `_genus`, `fault_code`, `message`, `recoverable`, and triggered `failed_genus`. Observer-owned diagnostics/localized projections/redacted analytics preserve privacy; telemetry endpoints remain master design gates.

## Acceptance and handoff

* [ ] Before production edits, run requirement-based Cucumber Given/When/Then scenarios at actual boundaries: idempotence/opposites/supersession, stale outcomes/rollback/release failures, peers/launch races/disconnect and quit/restart/replay. Behavioral red requires compiling, selected scenarios; then green. Tests/docs accompany source commits.
* [ ] Add atomic executable idle-duration scenarios: OS idle age shorter/longer than native-confirmed On age, floor-to-seconds, repeated status/TurnOn preserving anchors, genuine input and sleep/wake resets, new On/restart/replay isolation, stale guarded samples, and observation failure producing explicit error/null rather than zero. Assert On-only field presence and unchanged bool|null/error safety for off/pending/error snapshots.
* [ ] Add atomic event scenarios: actual eligible transition plus matching successful cancellation produces one record; missing/failed/mismatched proof produces none. Verify empty confirmed-event lists and every excluded non-event, stale receipt rejection across episodes/generations, and latest-64 retention with truthful truncation while durable history survives.
* [ ] Verify `epoch_ms` uses Unix UTC milliseconds captured at cancellation, `minutes_into_protected_idle` uses elapsed monotonic milliseconds since that episode's anchor divided by 60000 and remains finite/nonnegative through forward/backward wall-clock jumps; later queries never recompute receipt timestamps/offsets from query time.
* [ ] Future commands: `cargo xtask sanity` after meaningful increments, <=10s; affected-target compilation measured separately. No full suite internally. Completed-wave `cargo xtask check` <10min; >=3 isolated host workers where safe, explicit serial exceptions, stage/total timing/count/SHA/exit evidence. Lane 3 supplies drivers; real-macOS G1/G2 evidence remains integration work.
* [ ] Handoff frozen D2 schemas/ownership/invariants to both lanes before D4/D5; D3 green joins D6. No per-lane PRs: open exactly one delivery PR from the shared wave integration branch (head) into canonical `main` (base).

## Model and documentation receipt

Documentation baseline: `mod:c65+s85='copilot:gpt-5.6-terra:1M:xhigh'`. Future source baseline: `mod:c75+s75='copilot:gpt-6-sol:1M:max'`; strong reasoning owns ordering/seam design. Actual inherited context/effort profile UNVERIFIED; no fallback inferred.

Task `task_67820087d728`, Dispatch `ctx_8ab91ffa7935`; worktree `/Users/acoven/dev/ai2sw/workspaces/main/sponge`, branch `boot-ai-context-report`. Coordinator integrates the preserved unstaged document; no worker staging/commit.

## 2026-11-15 protected-idle amendment

Records AndrewC's approved `is_on` reporting requirements: On-only protected-idle duration and proven suppressed events with epoch time and episode-relative minutes. Implementation approval is unchanged; all execution boxes remain unchecked.

Amendment baseline: `mod:c55+s85='copilot:gpt-5.6-terra:1M:xhigh'`; actual session model `gpt-6.1-sol`, inherited context/effort UNVERIFIED. Task `task_bb5c0a5c7cff`, Dispatch `ctx_52ce3834bb26`; only this lane document amended, no staging/commit. Coordinator integrates after parallel document writers finish.

## Decision log

Empty pending approval. [Operator actions surface](<2026-11-15_plan_todo_ScreenWake_Orch.md#operator-next-actions>): record/reference only.

<!-- end 2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md -->
