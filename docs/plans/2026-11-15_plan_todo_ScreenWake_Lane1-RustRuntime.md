<!-- 2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Lane 1 RustRuntime

Status: inProgress. AndrewC authorized implementation at 2026-11-15T21:20:33.703-08:00: "go ahead and start implementation". D2 now supplies the coherent Rust foundation below; D3 and real native/MCP delivery remain unfinished. No system changes or release are authorized by this increment. Historical planning receipts below predate that approval.

## Inputs, outputs, and ownership

* Inputs: [committed master](<2026-11-15_plan_todo_ScreenWake_Orch.md>), `b229642`, its Sources and license receipt and P1 through P6. D1 approval precedes every implementation task.
* Produces now: D2 workspace/contracts, a native-confirmed owner slice, executable foundation scenarios, measured Rust sanity tooling and the handoff below. D3 completes durable runtime/IPC/episode policy. The already committed license remains byte-for-byte unchanged.
* NotProduces: Swift/native OS implementations, MCP adapter/build driver, sibling documents, independent PRs, or unsupported capability claims.
* Actual D2 paths: root `Cargo.toml`/`Cargo.lock`, `.cargo/config.toml`, three `code/crates/ai2screenwake_{backend,client,native}/` packages and tooling-only `code/xtask/`. The private generic registry/identity code lives under backend `src/contract/`; client policy lives in `src/runtime.rs`, native interfaces in `src/contracts.rs`. No fourth runtime crate.
* D2 tests: client `tests/features/foundation.feature` with `tests/steps/foundation.rs`; future runtime/IPC/replay and platform cases remain D3/D4. `cargo xtask prepare` compiles the exact target separately; `cargo xtask sanity` runs only these scenarios.
* [Lane 2 MacOSMenuBar](<2026-11-15_plan_todo_ScreenWake_Lane2-MacOSMenuBar.md>) owns native OS implementations and Swift. [Lane 3 MCPDelivery](<2026-11-15_plan_todo_ScreenWake_Lane3-MCPDelivery.md>) owns `ai2screenwake_client/src/bin/ai2screenwake-mcp.rs`, MCP tests and `code/xtask/`. Agree exact disjoint files/manifests before parallel execution; serialize later shared-seam edits through Lane 1.

## D2 license, support, and contract freeze

* [x] Verified unchanged `LICENSE.txt` against the recorded upstream commit `28d3c6ab714344058356fccfee386480cad7650f`: `git hash-object LICENSE.txt` equals `90b2643a579327564ed214dee8ec849fbc1732f1`. No upstream refetch or license modification is claimed.
* [x] Froze tested host versus intended target in native README: macOS 27.0.1 arm64 compiled here; intended first native target macOS 13+ arm64, backward/runtime verification pending D4. Public headers document power assertions and genuine-input idle age, not screensaver efficacy.
* [x] Exactly three runtime packages, backend-owned `contract` feature and explicit contract-only client/native dependencies. Normal dependency trees and empty-default backend compilation exclude axum/tokio/sqlx; `xtask` is tooling only.
* [x] Froze `NativeSeam::{emit(KvEnvelope),observe(Arc<dyn ObservationSink>)}` and `ObservationSink::observed(KvEnvelope)`, all returning `()`. Rust callers use typed `Registry::emit<A>` and `observe_action<A>`. Generated UniFFI/Swift bindings remain D4/D5.
* [x] Froze all planned typed identities, guarded native payloads and private type-derived routes in backend contracts. Required native handlers fail startup explicitly. Queue append, full observer fan-out and handler invocation use one emission path; sibling continuations share one allocator.
* [x] Froze Apply/AssertionsObserved/Status in `src/contract.rs`; native observations additionally carry paired occurrence clocks. Entity IDs are distinct from framework-owned correlation counters. Only matching current-generation native success with both resources establishes On.
* [x] Implemented canonical On-only JSON field presence/omission, preserving nullable truth and explicit errors. Exactly three future MCP controls remain; no `idle_info` or MCP implementation is added.
* [x] Froze SuppressedEvent/CancellationObserved with UTC occurrence clocks and finite nonnegative `Minutes`. Actual hook validation, event-time offset calculation and retention are D3/D4 work; no cancellation event source or completed event processing is claimed.

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

* D1: Implementation approval is explicit above; historical pending-approval language is not a new approval request. Coordinator records canonical master status separately.
* D2: Keep one small in-process typed queue in backend's feature-isolated contract framework. Core projections are observed, never returned as control acknowledgements. In-memory log ownership stays with that registry; persistence remains D3, not an invented additional artifact.

## D2 foundation receipt and exact handoff

Task `task_99f75dabad82`, Dispatch `ctx_19284c6264fd`; branch `screenwake-rust-foundation`, exact root `/Users/acoven/dev/ai2sw/workspaces/main/screenwake-rust-foundation`, approved base `c6581b4fbcaa2268132ce660c3965b80d433a146`, Orca identity `wt2:local:68695ef4-188b-42c4-a04b-84be757e3571`. Baseline `mod:c75+s75='copilot:gpt-6-sol:1M:max'`; actual inherited process `gpt-6.1-sol`, `long_context`, `high`, verified from process argv; human-facing context capacity unverified, no model override/fallback.

First compiling/green source, tests and docs checkpoint: `fac2e4fb2ec72ce4b2bff487b9336ee20d22f841`, 14 scenarios/50 steps. A follow-on sibling-correlation scenario exposed duplicate nested Snapshot instances and was fixed with a shared chain-wide allocator. Final committed HEAD and rerun evidence are reported in the worker completion receipt and worktree-local timing artifact, not guessed from this earlier checkpoint.

* Backend `src/contract.rs`: canonical `KvEnvelope`, Status, Fault, Apply, AssertionsObserved, ClockSample, IdleRequest, IdleObserved, CancellationObserved, finite Minutes, typed Power/Error/Log hierarchies. `src/contract/registry.rs`: `RegistryBuilder::{require,on,build}`, `Registry::emit<A>(payload)`, typed/raw observation, opaque `Emitter::{emit,error}`. No caller-readable route-key API.
* Native `src/contracts.rs`: `NativePower::apply(&self, Apply, Emitter)` and `sample(&self, IdleRequest, Emitter)`, both `()`. `contracts::register` supplies required handlers. `NativeSeam::attach(Registry)` exposes one envelope emit/observe seam. Real implementations start resource-free, retain resource IDs and emit outcomes through their captured continuation; they never report fake success.
* Ordering invariants: append once before all observers; invoke the handler only after fan-out; callbacks hold no queue lock; reentrant/synchronous emissions wait in that same queue; sibling and nested continuations never reuse an instance. Core serializes physical applies, preserves newer desired generation, and never treats partial/stale completion as On.
* Native guard contract: every apply carries session/generation/operation; every idle sample carries session/generation/episode/sample identity and paired UTC/continuous-monotonic clocks. Current matched samples calculate floor(min(OS age, confirmed-On age)/1000). No evidence means no suppression entries. Sampling failure dispatches its attributed error before null-duration projection without falsely reporting Off.
* Show Log uses native-only typed `Log::Session::{Query,Snapshot}`. Its observer receives ordered sequence/display metadata from the canonical log, never a Swift file/state getter. A real log window, durable storage/export and bounded retention are unfinished; no new MCP tool or remote upload is authorized.
* Disjoint native leaves may add `ai2screenwake_native/src/{power,activity,identity,launch}.rs` and `code/macos/ScreenWake/`. MCP leaves may add client `src/bin/ai2screenwake-mcp.rs`, `src/mcp/`, `tests/features/mcp_*.feature` and `tests/steps/mcp_delivery.rs`. Lane 1 serializes all changes to existing runtime/contracts, manifests, module exports and generated interface registration. Lane 3 may extend `code/xtask/` after integration; no parallel edits to the same runner.

Verification evidence retained under `target/foundation-evidence/`:

* Expected behavioral red: compiling off-projection scenario failed because On-only fields appeared while Off; runner exit 101, one selected scenario, approximately one second, after separately measured 12-second cold preparation. `red.log` and `timing.jsonl` preserve the evidence.
* Separate correlation red: `--name 'Sibling emissions' --concurrency 3` selected one scenario and exited 101 on duplicate snapshot correlation instances before the correction; `correlation-red.log` preserves exact wire evidence. No elapsed time was captured for that individual red attempt, so none is inferred.
* Invalid-control red: `cargo test -p ai2screenwake_client --test foundation -- --name 'Invalid control parameters' --concurrency 3` selected one scenario and exited 101 because the explicit error lost `failed_genus`; 2.03 seconds including compilation, preserved in `invalid-control-red.log`. The correction attaches the full validated triggering action before error emission.
* The complete pre-handoff inventory passed 18 scenarios/64 steps with three independent workers: preparation 1.660 seconds and complete sanity stage 1.156 seconds, whole `cargo xtask sanity` command 1.52 seconds. Final checks rerun on the committed final HEAD, with exact command/exit/count/duration/environment/artifact identity in attempt-specific logs and stage/total rows. The stale-source gate separately rejected an outdated prepared runner with exit 1 and recorded the failed attempt instead of running or claiming success.
* `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, normal client/native dependency trees, `cargo check --locked -p ai2screenwake_backend --no-default-features`, scoped whitespace and exact license-hash checks passed. This is targeted foundation validation, not a full suite or native/app witness.
* Two compiler failures and one stopped test-harness attempt preceded green. The latter was a test-owned mutex guard reacquired by the same step; releasing it fixed the harness without weakening assertions. Initial ad-hoc `green-preparation` timing has a missing start field and is not admissible duration evidence; the canonical Rust runner now preserves every subsequent attempt with complete metadata.
* Narrow generated-file exception, coordinator reply `msg_8a26ec40e58a`: keep `Cargo.lock` exactly as Cargo produces it. Actual Cargo runs prepend their generated preamble and remove a manual trailing marker; copyright remains immediately after the generated preamble where naturally preserved. Do not repair those comments after validation. Authored source/config/docs retain first-line copyright and final markers; `LICENSE.txt` remains exact.

Remaining named D3/D4/D5 gates are not D2-delivered behavior: multi-request settlement/timeouts and complete supersession correlation; durable replay/history and retention; genuine-activity/forced-sleep/wake episode resets; validated cancellation hook, offsets and latest-64 processing; same-user authenticated IPC/launch ownership; bounded Quit completion; real resource rollback/retained-release IDs; generated native/Swift host, menu/secondary gesture/log window/artwork; MCP adapter/schema packaging; telemetry/bug-report authorization, localization/accessibility, CI/full checks and actual macOS/screensaver witnesses. No release, installed-system change, PR, push, helper agent or license edit was performed.

[Operator actions surface](<2026-11-15_plan_todo_ScreenWake_Orch.md#operator-next-actions>): record/reference only.

<!-- end 2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md -->
