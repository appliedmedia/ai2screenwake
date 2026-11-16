<!-- 2026-11-15_plan_todo_ScreenWake_Orch.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# ScreenWake master orchestration

Status: todo, awaiting AndrewC approval. One proposed implementation wave; no implementation, license copying, system changes, or release authorized.

## Stated goals

* G1:IdleProtection: Explicitly on prevents ordinary idle system/display sleep and automatic screensaver. Proof: actual supported-macOS idle witness.
* G2:SharedControl: Menu bar and three MCP tools control one session truthfully. Proof: concurrent native/MCP interaction against the built app.

## PRD

* 1. Problem
  * 1.1. Idle sleep/screensaver interrupts work; disconnected controls misreport protection.
* 2. Personas
  * 2.1. Mac user: enables protection, retains manual locking/sleep, and keeps state private to their login session.
  * 2.2. Automation user: controls that session through authorized MCP; other users cannot read/change it.
* 3. Business opportunity
  * 3.1. Small native utility with equivalent human/automation control.
* 4. Recommended approach
  * 4.1. Menu bar app owns protection; bundled stdio MCP connects without auto-enabling.
* 5. Outcomes
  * 5.1. Given an approved idle session, when on, then idle sleep/screensaver stays inactive; off restores normal behavior.
  * 5.2. Given two MCP clients and menu, when controls interleave, then observed state agrees; failures never appear successful.
* 6. Primary measurement
  * 6.1. Protection-state mismatches per completed control attempt. Historic baseline UNKNOWN; measure before implementation. Redacted Actions records attempts/outcomes/mismatches, never protected payloads.

## TRD

* 1. Terminology
  * 1.1. Owner: app-embedded Rust runtime. Desired: admitted intent. Effective: native-confirmed resources. Generation orders changes; session identifies process lifetime.
* 2. Scope
  * 2.1. Menu: observed On/Off/Pending/Error, Turn on, Turn off, Quit; no Dock/main window. Start OFF on every new process.
  * 2.2. EXPLICITLY NOT DOING: login startup, timers/triggers, accounts, cloud backend, product AI, general preferences, WebUI, per-client leases, root/Accessibility/input-monitoring permission, synthetic input, preference writes, or caffeinate subprocesses.
  * 2.3. Manual Lock Screen, lid closure, manual/forced sleep, low-battery/thermal sleep, and managed/security locking remain effective. Never unlock or stop a manually started screensaver.
* 3. Environment
  * 3.1. Planned: root Cargo workspace; `code/crates/ai2screenwake_{backend,client,native}/`; `code/macos/ScreenWake/`; `code/xtask/`; worktree-local `target/screenwake/ScreenWake.app`. Freeze supported macOS/architectures in Lane 1.
* 4. Principles
  * 4.1. P1:SingleOwner: MUST route every control to one owner; NEVER create independent MCP assertions.
  * 4.2. P2:ObservedTruth: MUST derive state from logged native outcomes; NEVER equate requested-on with effective-on.
  * 4.3. P3:ActionsOnly: MUST dispatch every command/query/outcome/error through one typed registry, action-log append, and observer fan-out; NEVER expose route strings, method-on-state reads, or return-state RPC.
  * 4.4. P4:ThreeArtifacts: MUST have exactly three runtime crates; NEVER add core/UI/runtime helpers as a fourth artifact. `cargo xtask` is tooling only.
  * 4.5. P5:NativeBoundary: MUST keep resources/OS glue in the native seam and policy in Rust; NEVER duplicate core state in Swift.
  * 4.6. P6:SafeControl: MUST authenticate local peers and preserve security locking; NEVER bypass permissions or policy.
* 5. Architecture
  * 5.1. Swift menu and `ai2screenwake_client/src/bin/ai2screenwake-mcp.rs` emit typed intents; owner dispatches/logs/projects. MCP JSON-RPC correlation stays in transport.
  * 5.2. `ai2screenwake_backend` defines KvEnvelope/registry/payloads behind `contract`; consumers use `default-features=false, features=["contract"]`. Isolate optional hosting; no server-only dependencies or mirrored schemas.
  * 5.3. `ai2screenwake_client` owns registry, fold, ordering, IPC endpoint, projections, and MCP adapter. `ai2screenwake_native` owns UniFFI host interfaces, OS resources, and LaunchServices glue. Swift renders projections and captures input.
  * 5.4. Planned seam signatures: `emit(envelope: KvEnvelope) -> ()`, `observe(sink: Arc<dyn ObservationSink>) -> ()`, `ObservationSink::observed(envelope: KvEnvelope) -> ()`; native work and completions use that same seam, never a second state channel.
  * 5.5. Wire: KV map, framework-owned `_genus`, typed payloads. Identities: `Power::Session::{TurnOn,TurnOff,Query,Quit,Changed,Snapshot}`, `Power::Assertions::{Apply,Observed}`. Framework derives at most four static prefix segments; callers use typed capabilities, not genus strings. `IdGeneration` mints entities; correlation tails are not stored identities.
  * 5.6. Apply: `{session_id:string,generation:u64,operation_id:string,enabled:bool}`. Observed continues its correlation and adds `{held_count:u8,outcome:"applied"|"failed",fault_codes:string[]}`. Snapshot uses Status. Inject registry/log/native/IPC/analytics collaborators; reject missing registrations at startup.
  * 5.7. Serialize one native apply at a time. Changed desired intent advances generation; repeated setters share its pending operation. Account for older completions without overwriting newer intent, then reconcile the latest generation. A superseded setter receives an explicit superseded error, not success.
  * 5.8. Acquire PreventUserIdleSystemSleep and PreventUserIdleDisplaySleep all-or-nothing; roll back partial acquisition. Retain failed-release IDs until confirmed cleanup; report partial/unknown state. Quit drains bounded cleanup; process exit releases app-owned resources. Replay rebuilds diagnostic history, never restores assertions or on intent into a new session.
* 6. MCP and local authorization
  * 6.1. Only `is_on`, `turn_on`, `turn_off`: "Am I on?", "Turn on", "Turn off". Input schema `{"type":"object","properties":{},"additionalProperties":false}`; idempotent setters.
  * 6.2. `Status={is_on:bool|null,desired_on:bool,state:"off"|"on"|"pending"|"error",revision:u64,operation_id:string|null}`. True requires current-generation convergence with both resources; false requires confirmed zero resources and convergence. Otherwise null avoids false success/off. Queries return an ordered snapshot, including pending/error.
  * 6.3. Tool result: `structuredContent={status:Status|null,error:null|{fault_code:string,message:string,recoverable:bool}}`, matching one advertised output schema; `content` contains its JSON text and `isError` equals error presence. Setters resolve only from observed convergence; timeout, supersession, acquisition/release failure, and unavailable owner return structured errors with last observed status when available.
  * 6.4. Native seam supplies canonical login-session runtime directory and OS identity checks. Rust requires non-symlink UID ownership, directory 0700, exclusive owner lock, `owner.sock` 0600, valid path length, and matching `getpeereid` UID. Reject unsafe peers/paths; same UID is the trust boundary.
  * 6.5. Launch-or-connect uses LaunchServices for the exact bundled app, never a shell daemon. Under the owner lock only, remove a stale owned socket after proving no listener; verify inode/type/ownership before unlink. Simultaneous launches converge; an existing owner remains authoritative.
  * 6.6. IPC: four-byte big-endian length, UTF-8 KvEnvelope JSON, maximum 64 KiB, 32 outstanding requests/peer. Admit only the three control types, never native outcomes. Deadlines: launch/connect 10 seconds, operation 5 seconds. Register before emitting; timeout/disconnect never cancels intent. Malformed frames dispatch errors; stdout is MCP-only.
* 7. Cross-cutting constraints
  * 7.1. Dispatch `Error:<Module>:<Fault>:Occurred` first, with `_genus`, `fault_code`, `message`, `recoverable`, and `failed_genus` when triggered. Observers provide nonblocking short in-menu error copy and diagnostics; no raw chains or OS alerts.
  * 7.2. Core-only PostHog observer forwards redacted Actions metadata; no Swift SDK. Analytics and unrecoverable `/bug-report` destination/configuration are unverified release design gates: finalize authorized endpoints, bounded buffering, and failure dispatch without inventing credentials or a cloud deployment.
  * 7.3. Accessibility includes named menu items, keyboard operation, truthful enabled state, and non-color status. Rust owns localized strings; Swift renders them. Privacy tests prohibit secrets, user payloads, and socket credentials in telemetry/logs.
* 8. Verification
  * 8.1. Executable requirement-based Cucumber Given/When/Then red precedes production edits, then green; compile failure/zero selection is not red. Tests/docs share code commits.
  * 8.2. Host scenarios cover idempotence, concurrent clients, rapid opposite commands, stale/duplicate outcomes, partial acquisition/rollback, retained release failures, disconnect, launch races, unsafe sockets, quit/crash/restart/replay, and exact MCP schemas/tool count.
  * 8.3. Planned identical local/CI entries: `cargo xtask build --profile release`, `cargo xtask sanity`, `cargo xtask check`, `cargo xtask verify-macos`. Build generates UniFFI/bundle; verify fresh artifact/SHA/target/configuration, counts, exit codes, success markers.
  * 8.4. Sanity <=10 seconds; completed-wave full checks <10 minutes; >=3 isolated host workers where safe, named serial exceptions. Persist stage/total started_at/finished_at/elapsed_seconds/command/environment/git_sha/worker_count/exit_status. Check covers fmt/clippy/Cucumber, contract isolation, Swift/bundle, license, docs.
  * 8.5. Separate controlled real-macOS gate measures idle sleep/screensaver plus native menu/MCP behavior on an isolated approved test session. Fresh-state, replayed-history, and induced-error passes prove actual resources and rendering. Document necessary idle duration separately from sanity; no settings changes or wake tests during planning.
* 9. ModelPicker
  * 9.1. Lane 1 baseline: `mod:c75+s75='copilot:gpt-6-sol:1M:max'`; cross-process ordering/native contract needs a strong reasoning owner. Lane 2: `mod:c65+s75='copilot:gpt-5.6-terra:1M:xhigh'`. Lane 3: `mod:c70+s75='copilot:gpt-6-sol:1M:max'`. Freeze uncertain invariants before assigning settled leaves; no Luna-safe assumption.
  * 9.2. Record actual model/context/effort separately from baseline/congestion history. This inherited dispatch profile is UNVERIFIED: no concrete receipt supplied.
* 10. Decisions and review gate
  * 10.1. Architecture/schemas await approval. Screensaver inhibition remains unverified: display-dimming documentation is insufficient. Failed witness requires public-API investigation and explicit design/scope approval, never user-activity heartbeats, preference edits, or security bypass.
  * 10.2. After approval and completed integration/normal validation, run one embedded triad below, one consolidated fix-worker result even with no findings, then commit all changes before final validation. Record `validated_final_sha`; assert local/remote/PR equality before and after push/open. Later mutations rerun validation only, never the triad.

## Task DAG and linked lanes

Unstarted implementation DAG; coordinator supplies linked plans before approval handoff.

* [ ] D1: Obtain plan approval and settle scope/design gates; prerequisite for all execution.
* [ ] D2: [Lane 1 RustRuntime](<2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md>): after D1, verify/copy pinned license and freeze three-crate contracts, ordering invariants, and executable behavioral red.
* [ ] D3: Finish Lane 1 runtime and focused green tests after D2; satisfies P1 through P6 core boundaries.
* [ ] D4: [Lane 2 MacOSMenuBar](<2026-11-15_plan_todo_ScreenWake_Lane2-MacOSMenuBar.md>): after D2, implement disjoint native/UI leaves and G1 witness.
* [ ] D5: [Lane 3 MCPDelivery](<2026-11-15_plan_todo_ScreenWake_Lane3-MCPDelivery.md>): after D2, implement disjoint MCP/build/delivery leaves and G2 integration.
* [ ] D6: Integrate D3/D4/D5 into one wave branch; run maintained acceptance and close real-platform/telemetry design gates.
* [ ] D7: After D6 normal validation, collect one ARCH/QUALITY/PROCESS pass each and one consolidated fix result.
* [ ] D8: Final-validate committed HEAD, open one PR; catalog `coderabbit_optional`, exact trigger/bounded confirmation per ai.yaml, no human-reviewer request.
* [ ] D9: After review/CI/head gates, merge commit, server MERGED readback, canonical-main reconciliation and exact delivered branch/worktree cleanup.
* [ ] D10: Audit goals/constraints, retro/INT sweep, status refresh and separately gated plan closeout; never mark unsupported screensaver behavior delivered.

## Embedded implementation-batch reviewer briefs

Common: record branch/base/triad-input SHA, Orca worktree/Task/Dispatch, rehydration, T+5 proof. Produces findings only, not implementation/PR/re-review. Operator actions surface: this document, record/reference only. No per-lane PRs: open exactly one PR against shared wave branch.

* ARCH: `mod:c75+s50='copilot:gpt-6-sol:1M:max'`. Inspect divergence, unwired seams, side channels, abstractions: three manifests/contract isolation, emit/observe, single owner, IPC authentication, native accounting, generation/replay, G1 witness.
* QUALITY: `mod:c65+s40='copilot:gpt-5.6-terra:1M:max'`. Inspect weak tests/errors/dead code/stale docs: Cucumber bindings, concurrency/rollback/release failures, MCP schemas, Swift/bundle identity, license blob, budgets, real idle witness, privacy/localization/accessibility.
* PROCESS: `mod:c65+s40='copilot:gpt-5.6-terra:1M:max'`. Inspect approval/worktree violations, fabricated evidence, unexecuted tasks, skipped retro: D1, receipts/branches, one PR/triad/fix result, final SHA, UNKNOWN metrics, provider deadlines, merge cleanup.

## Sources and license

Patterns only, never copy Adobe implementation. Verified reference HEADs: Claiyr `977d1a1`; DIMO `abf187a` / `9cdfb08`.

* Claiyr root `/Users/acoven/dev/claiyr/main/code/engines`: `claiyr_backend/Cargo.toml:17-75`, `claiyr_client/Cargo.toml:11-130`, `claiyr_client/src/ffi.rs:9050-9142`: contract/UniFFI/emit patterns, never legacy submit/await.
* DIMO integration root `/Users/acoven/dev/r/worktrees/macosx-integrate/code/macosx`: `crates/dimo_backend/Cargo.toml:1-20`, `crates/dimo_client/Cargo.toml:1-42`, `swift/Sources/DimoReviews/UniffiSeamAdapter.swift:1-39`, `swift/Sources/DimoReviews/DimoReviewsApp.swift:1-20`: serialized ingress/observation, not WebUI/fourth artifacts.
* DIMO awake root `/Users/acoven/dev/r/worktrees/main/dimo-awake-finish`: `code/macosx/crates/dimo_client/src/keep_awake.rs:1-162`, `code/macosx/swift/Sources/DimoReviews/PowerAssertions.swift:1-61`, `code/macosx/crates/dimo_client/tests/features/keep_awake.feature:1-20`: generations/resource retention; exclude accounts/automatic-busy policy.
* Apple [QA1340](<https://developer.apple.com/library/archive/qa/qa1340/_index.html>)/[display assertion](<https://developer.apple.com/documentation/iokit/kiopmassertiontypepreventuseridledisplaysleep>): fetched this session, not screensaver proof.
* Coordinator's direct-GitHub [LICENSE receipt](<https://github.com/appliedmedia/print2paper4vscode/blob/main/LICENSE>): commit `28d3c6ab714344058356fccfee386480cad7650f`, blob `90b2643a579327564ed214dee8ec849fbc1732f1`, `# License: Code Transparency v1`. D2 copies pinned bytes; `git hash-object LICENSE` must match. Preserve copyright/dates/escaped marker and restrictive maintainer/redistribution terms; never relabel MIT. No copy now.

## Planning receipt and current status

Master baseline: `mod:c75+s75='copilot:gpt-6-sol:1M:max'`; actual profile unverified. Task `task_162aefb68c01`, Dispatch `ctx_fb08820f253c`, worker `term_27359d29-6baa-4488-94f1-2b4b90834b6a`, coordinator `term_f29f11e1-6a27-485d-982c-6327ebffdb9a`. Root `/Users/acoven/dev/ai2sw/workspaces/main/sponge`, branch `boot-ai-context-report`, base `9630adb5a32c306fd282210f8c512951f69b33e1`; canonical main `/Users/acoven/dev/ai2sw/main`. Sole deliverable: master; completion receipt records commit/validation.

No implementation/build/release. Project-wide inventory, measured average duration, ETA, shareable-build date: UNKNOWN. No fabricated progress percentage. BLOCKERS: EMPTY; approval is a decision gate. `code/README.md`/status markers/pointers absent; coordinator creates authorized planning pointers only.

## Operator next actions

* AndrewC approves/changes master and lanes before execution; record approval here, never infer from silence.
* Resolve macOS/architectures/screensaver API evidence before G1 delivery; explicitly authorize new scope/telemetry endpoints.

<!-- end 2026-11-15_plan_todo_ScreenWake_Orch.md -->
