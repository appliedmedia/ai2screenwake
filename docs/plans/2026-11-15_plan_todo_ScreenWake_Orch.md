<!-- 2026-11-15_plan_todo_ScreenWake_Orch.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# ScreenWake master orchestration

Status: inProgress. At 2026-11-15T21:20:33.703-08:00, AndrewC said "go ahead and start implementation", authorizing the existing G1:IdleProtection/G2:SharedControl implementation wave under P1:SingleOwner through P6:SafeControl. This supersedes historical planning-only authorization; D2 support, native-API evidence, and contract freeze remain unfinished. Implementation approval is not delivery evidence or release authorization.

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
  * 2.2. EXPLICITLY NOT DOING: login startup, timers/triggers, accounts, cloud backend, product AI, general preferences, WebUI, per-client leases, root/Accessibility/input-monitoring permission, synthetic input, preference writes, or caffeinate subprocesses. On-demand native idle observation is not product timer/trigger automation.
  * 2.3. Manual Lock Screen, lid closure, manual/forced sleep, low-battery/thermal sleep, and managed/security locking remain effective. Never unlock or stop a manually started screensaver.
  * 2.4. SVG is the artwork source, exported to appropriate native macOS assets, not assumed runtime SVG support. Detailed app icon and promotional artwork show a friendly old-school television with antenna, smile, closed eyes, and rising zzZ. Its simplified monochrome menu-bar counterpart keeps the TV/smile: OFF has closed eyes and zzZ; ON has really wide-open eyes and no zzZ. Primary click emits typed TurnOn/TurnOff intent through the existing Actions seam; visible toggle state changes only from confirmed observed protection. Pending/error remain truthful and accessible, with existing menu controls including Quit still reachable; no new window, timer, or security feature. Artwork does not exist yet.
* 3. Environment
  * 3.1. Planned: root Cargo workspace; `code/crates/ai2screenwake_{backend,client,native}/`; `code/macos/ScreenWake/`; `code/xtask/`; worktree-local `target/screenwake/ScreenWake.app`. Freeze supported macOS/architectures in Lane 1.
  * 3.2. Documentation/license PR #1 merged at `c6581b4`, supplying the README explainer and exact pinned root `LICENSE.txt`. Code Transparency v1 is source-available, not open source or a permissive grant; viewing, study, audit, contributions, reuse, derivatives, redistribution, and commercial use remain governed by that license and any separately required permission. Do not borrow unverified zero-network/zero-telemetry claims.
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
  * 5.3. `ai2screenwake_client` owns registry, fold, ordering, IPC endpoint, projections, and MCP adapter. `ai2screenwake_native` owns UniFFI host interfaces, OS resources, LaunchServices glue, genuine OS idle-age sampling, and paired UTC/monotonic cancellation evidence. Rust owns duration/episode/event policy; Swift and MCP render projections without clock/state reads or policy calculations.
  * 5.4. Planned seam signatures: `emit(envelope: KvEnvelope) -> ()`, `observe(sink: Arc<dyn ObservationSink>) -> ()`, `ObservationSink::observed(envelope: KvEnvelope) -> ()`; native work and completions use that same seam, never a second state channel.
  * 5.5. Wire: KV map, framework-owned `_genus`, typed payloads. Identities: `Power::Session::{TurnOn,TurnOff,Query,Quit,Changed,Snapshot}`, `Power::Assertions::{Apply,Observed}`, `Power::Activity::{Sample,Observed}`, `Power::Suppression::Recorded`. Framework derives at most four static prefix segments; callers use typed capabilities, not genus strings. `IdGeneration` mints entities; correlation tails are not stored identities.
  * 5.6. Apply: `{session_id:string,generation:u64,operation_id:string,enabled:bool}`. Observed continues its correlation and adds `{held_count:u8,outcome:"applied"|"failed",fault_codes:string[]}`. Snapshot uses Status. Inject registry/log/native/IPC/analytics collaborators; reject missing registrations at startup.
  * 5.7. Serialize one native apply at a time. Changed desired intent advances generation; repeated setters share its pending operation. Account for older completions without overwriting newer intent, then reconcile the latest generation. A superseded setter receives an explicit superseded error, not success.
  * 5.8. Acquire PreventUserIdleSystemSleep and PreventUserIdleDisplaySleep all-or-nothing; roll back partial acquisition. Retain failed-release IDs until confirmed cleanup; report partial/unknown state. Quit drains bounded cleanup; process exit releases app-owned resources. Replay rebuilds diagnostic history, never restores assertions or on intent into a new session.
  * 5.9. Query registers interest and dispatches a fresh authorized native idle-age sample through the same typed Actions bus, never UI/MCP polling or direct state/clock reads. Validate source quality/API on supported macOS; do not invent last activity from synthetic input/caffeinate or log key/mouse content. Sampling failure dispatches an explicit recoverable error and returns null duration, not fabricated zero or a successful query; report current protection truth independently.
  * 5.10. While confirmed On, `protected_idle_duration_seconds = floor(min(observed_idle_age_ms, monotonic_elapsed_since_effective_on_ms) / 1000)`. The protected-idle anchor is the latest genuine OS-reported user activity, bounded below by the current native-confirmed On interval. Repeated TurnOn/queries never restart it; genuine activity, actual forced-sleep/wake interruption, off/re-enable, and process restart reset the applicable interval so no pre-protection or through-sleep time counts.
  * 5.11. Native evidence carries the same sampling/episode identity and generation/correlation checks through named logged events. Capture confirmed cancellations with paired actual UTC/monotonic timestamps; record event minutes against that occurrence's protected-idle anchor, never recalculate against a later episode or use file mtimes. Snapshot clock sources and episode updates atomically; stale samples/receipts cannot overwrite newer state or enter a new interval. Activity/new On/restart reset the live event list; durable history retains real records, but replay never recreates live assertions/events.
* 6. MCP and local authorization
  * 6.1. Only `is_on`, `turn_on`, `turn_off`: "Am I on?", "Turn on", "Turn off". Input schema `{"type":"object","properties":{},"additionalProperties":false}`; idempotent setters.
  * 6.2. `Status={is_on:bool|null,desired_on:bool,state:"off"|"on"|"pending"|"error",revision:u64,operation_id:string|null}`. True requires current-generation convergence with both resources; false requires confirmed zero resources and convergence. Otherwise null avoids false success/off. Queries return an ordered snapshot, including pending/error.
  * 6.3. Tool result: `structuredContent={status:Status|null,error:null|{fault_code:string,message:string,recoverable:bool}}`, matching one advertised output schema; `content` contains its JSON text and `isError` equals error presence. Setters resolve only from observed convergence; timeout, supersession, acquisition/release failure, and unavailable owner return structured errors with last observed status when available.
  * 6.4. Native seam supplies canonical login-session runtime directory and OS identity checks. Rust requires non-symlink UID ownership, directory 0700, exclusive owner lock, `owner.sock` 0600, valid path length, and matching `getpeereid` UID. Reject unsafe peers/paths; same UID is the trust boundary.
  * 6.5. Launch-or-connect uses LaunchServices for the exact bundled app, never a shell daemon; connect to the existing owner or launch it OFF without automatically enabling protection. Under the owner lock only, remove a stale owned socket after proving no listener; verify inode/type/ownership before unlink. Simultaneous launches converge; an existing owner remains authoritative.
  * 6.6. IPC: four-byte big-endian length, UTF-8 KvEnvelope JSON, maximum 64 KiB, 32 outstanding requests/peer. Admit only the three control types, never native outcomes. Deadlines: launch/connect 10 seconds, operation 5 seconds. Register before emitting; timeout/disconnect never cancels intent. Malformed frames dispatch errors; stdout is MCP-only.
  * 6.7. Extend the existing Status schema only when `is_on=true` with `protected_idle_duration_seconds:u64|null`, `protected_idle_suppressed_events:SuppressedEvent[]`, and `suppressed_events_truncated:bool`; omit all three when definitely off or unresolved. `is_on` remains a JSON status result with existing metadata/error semantics, never a fourth `idle_info` tool. Idle sampling failure preserves independently truthful protection status, supplies null duration and the explicit recoverable tool error from 5.9.
  * 6.8. `SuppressedEvent={kind:string,epoch_ms:u64,minutes_into_protected_idle:number}`. `epoch_ms` is actual Unix UTC milliseconds at confirmed cancellation; minutes are nonnegative finite monotonic elapsed milliseconds from that event's protected-idle anchor divided by 60000, recorded at occurrence. Wall-clock/NTP changes affect epoch timestamps but never corrupt durations/offsets. Return only the latest 64 verified records in the current protected-idle interval; truncation is true only when earlier current-interval records were omitted, never silently imply completeness.
  * 6.9. Entries require an actual eligible idle request plus matching successful app cancellation acknowledgment/evidence. Candidate `idle_sleep_request_cancelled` is conditional on a verified supported OS hook, not promised by IOKit assertions; ordinary assertions expose no reliable per-attempt suppression stream. Without verified cancellation return `[]`, meaning no confirmed entries, not proof nothing was prevented. Never list on/off, generic idle intervals, inferred display/screensaver deadlines, active assertions, observed sleep/wake, lid close, manual sleep, or security locking as suppressed; never fabricate logging or change behavior to obtain entries.
  * 6.10. Future packaging includes the local Rust stdio server at `ScreenWake.app/Contents/MacOS/ai2screenwake-mcp`. README documents its executable/client configuration and local launch-or-connect behavior; `initialize`, `tools/list`, and `tools/call` make exactly the three tools in 6.1 discoverable and callable. No additional MCP tool or remote service.
* 7. Cross-cutting constraints
  * 7.1. Dispatch `Error:<Module>:<Fault>:Occurred` first, with `_genus`, `fault_code`, `message`, `recoverable`, and `failed_genus` when triggered. Observers provide nonblocking short in-menu error copy and diagnostics; no raw chains or OS alerts.
  * 7.2. Core-only PostHog observer forwards redacted Actions metadata; no Swift SDK. Analytics and unrecoverable `/bug-report` destination/configuration are unverified release design gates: finalize authorized endpoints, bounded buffering, and failure dispatch without inventing credentials or a cloud deployment.
  * 7.3. Accessibility includes named menu items, keyboard operation, truthful enabled state, and non-color status. Rust owns localized strings; Swift renders them. Privacy tests prohibit secrets, user payloads, and socket credentials in telemetry/logs.
* 8. Verification
  * 8.1. Executable requirement-based Cucumber Given/When/Then red precedes production edits, then green; compile failure/zero selection is not red. Tests/docs share code commits.
  * 8.2. Host scenarios cover idempotence, concurrent clients, rapid opposite commands, stale/duplicate outcomes, partial acquisition/rollback, retained release failures, disconnect, launch races, unsafe sockets, quit/crash/restart/replay, and exact MCP schemas/tool count.
  * 8.3. Planned identical local/CI entries: `cargo xtask build --profile release`, `cargo xtask sanity`, `cargo xtask check`, `cargo xtask verify-macos`. Build generates UniFFI/bundle; verify fresh artifact/SHA/target/configuration, counts, exit codes, success markers.
  * 8.4. Sanity <=10 seconds; completed-wave full checks <10 minutes; >=3 isolated host workers where safe, named serial exceptions. Persist stage/total started_at/finished_at/elapsed_seconds/command/environment/git_sha/worker_count/exit_status. Check covers fmt/clippy/Cucumber, contract isolation, Swift/bundle, license, docs.
  * 8.5. Separate controlled real-macOS gate measures idle sleep/screensaver plus native menu/MCP behavior on an isolated approved test session. Fresh-state, replayed-history, and induced-error passes prove actual resources and rendering. Document necessary idle duration separately from sanity; no settings changes or security bypass.
  * 8.6. Add separate executable Cucumber acceptance cases for native idle age shorter/longer than confirmed-On age, repeated TurnOn/query stability, genuine activity/off/re-enable/restart/forced-sleep-wake resets, query sampling failure with null duration/recoverable error and independent protection truth, stale observations, and omission of On-only fields when off/unresolved.
  * 8.7. Add atomic executable cases for actual Unix UTC millisecond timestamps, occurrence-recorded monotonic minutes across wall-clock jumps/new intervals, empty lists without inferred entries, request-plus-successful-cancellation evidence only, exclusion of nonsuppressible events, latest-64 explicit truncation, atomic episode snapshots, stale receipt rejection, and historical replay without live events. D2 verifies idle-age source/candidate cancellation hook against supported macOS before promising source proof.
  * 8.8. Requirement-based executable acceptance for 2.4 asserts icon state and primary-click intent across confirmed Off/On, pending, and failure, including accessible access to menu controls and Quit. Artwork and application tests are not yet integrated on main.
* 9. ModelPicker
  * 9.1. Lane 1 baseline: `mod:c75+s75='copilot:gpt-6-sol:1M:max'`; cross-process ordering/native contract needs a strong reasoning owner. Lane 2: `mod:c65+s75='copilot:gpt-5.6-terra:1M:xhigh'`. Lane 3: `mod:c70+s75='copilot:gpt-6-sol:1M:max'`. Freeze uncertain invariants before assigning settled leaves; no Luna-safe assumption.
  * 9.2. Record actual model/context/effort separately from baseline/congestion history. Current task receipts are recorded below; inherited context/effort remain UNVERIFIED.
* 10. Decisions and review gate
  * 10.1. Implementation approval is obtained; D2 owns supported macOS/architectures, contract/schema freeze, and native-API evidence. Screensaver inhibition remains unverified: display-dimming documentation is insufficient. A failed witness requires public-API investigation; any necessary design/scope change requires explicit approval, never user-activity heartbeats, preference edits, or security bypass. Telemetry destinations remain unresolved technical release gates, not a renewed implementation-approval request.
  * 10.2. After completed integration/normal validation, run one embedded implementation-batch triad below, one consolidated fix-worker result even with no findings, then commit all changes before final validation. This is a new implementation batch, not a rerun of the consumed documentation triad. Record `validated_final_sha`; assert local/remote/PR equality before and after push/open. Later mutations rerun validation only, never the triad.

## Task DAG and linked lanes

Implementation approval is obtained; D2 foundation work is inProgress. D3 follows the D2 freeze; native/MCP workers receive frozen exact files/APIs before starting, with parallel work confined to this implementation wave.

* [x] D1: Obtain implementation approval: AndrewC's exact instruction/timestamp is recorded above. Technical support/API/design gates remain in D2/D6, not completed by this approval.
* [ ] D2: [Lane 1 RustRuntime](<2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md>): inProgress after D1; verify pinned `LICENSE.txt`, establish supported macOS/architectures and native-API evidence, and freeze exact file ownership, three-crate contracts, ordering invariants, and executable behavioral red.
* [ ] D3: Finish Lane 1 runtime and focused green tests after D2; satisfies P1 through P6 core boundaries.
* [ ] D4: [Lane 2 MacOSMenuBar](<2026-11-15_plan_todo_ScreenWake_Lane2-MacOSMenuBar.md>): after D2, implement disjoint native/UI leaves and G1 witness.
* [ ] D5: [Lane 3 MCPDelivery](<2026-11-15_plan_todo_ScreenWake_Lane3-MCPDelivery.md>): after D2, implement disjoint MCP/build/delivery leaves and G2 integration.
* [ ] D6: Integrate D3/D4/D5 into one wave branch; run maintained acceptance and close real-platform/telemetry design gates.
* [ ] D7: After D6 normal validation, collect one ARCH/QUALITY/PROCESS pass each and one consolidated fix result.
* [ ] D8: Final-validate committed HEAD, open one delivery PR from the shared wave integration branch (head) into canonical `main` (base); catalog `coderabbit_optional`, exact trigger/bounded confirmation per ai.yaml, no human-reviewer request.
* [ ] D9: After review/CI/head gates, foreground PR owner merges that PR with a server-side merge commit into `origin/main` and verifies server MERGED/merge SHA. After its completion notification, cleanup owner pulls canonical `main` with `git pull --ff-only` to that merged state, then removes exact delivered branch/worktree residue.
* [ ] D10: Audit goals/constraints, retro/INT sweep, status refresh and separately gated plan closeout; never mark unsupported screensaver behavior delivered.

## Embedded implementation-batch reviewer briefs

Common: record branch/base/triad-input SHA, Orca worktree/Task/Dispatch, rehydration, T+5 proof. Produces findings only, not implementation/PR/re-review. Operator actions surface: this document, record/reference only. No per-lane PRs: open exactly one delivery PR from the shared wave integration branch (head) into canonical `main` (base).

* ARCH: `mod:c75+s50='copilot:gpt-6-sol:1M:max'`. Inspect divergence, unwired seams, side channels, abstractions: three manifests/contract isolation, emit/observe, single owner, IPC authentication, native accounting, generation/replay, G1 witness.
* QUALITY: `mod:c65+s40='copilot:gpt-5.6-terra:1M:max'`. Inspect weak tests/errors/dead code/stale docs: Cucumber bindings, concurrency/rollback/release failures, MCP schemas, Swift/bundle identity, license blob, budgets, real idle witness, privacy/localization/accessibility.
* PROCESS: `mod:c65+s40='copilot:gpt-5.6-terra:1M:max'`. Inspect approval/worktree violations, fabricated evidence, unexecuted tasks, skipped retro: D1, receipts/branches, one PR/triad/fix result, final SHA, UNKNOWN metrics, provider deadlines, merge cleanup.

## Sources and license

Historical source receipts, not reverified in this correction. Patterns only, never copy Adobe implementation. Recorded reference HEADs: Claiyr `977d1a1`; DIMO `abf187a` / `9cdfb08`.

* Claiyr root `/Users/acoven/dev/claiyr/main/code/engines`: `claiyr_backend/Cargo.toml:17-75`, `claiyr_client/Cargo.toml:11-130`, `claiyr_client/src/ffi.rs:9050-9142`: contract/UniFFI/emit patterns, never legacy submit/await.
* DIMO integration root `/Users/acoven/dev/r/worktrees/macosx-integrate/code/macosx`: `crates/dimo_backend/Cargo.toml:1-20`, `crates/dimo_client/Cargo.toml:1-42`, `swift/Sources/DimoReviews/UniffiSeamAdapter.swift:1-39`, `swift/Sources/DimoReviews/DimoReviewsApp.swift:1-20`: serialized ingress/observation, not WebUI/fourth artifacts.
* DIMO awake root `/Users/acoven/dev/r/worktrees/main/dimo-awake-finish`: `code/macosx/crates/dimo_client/src/keep_awake.rs:1-162`, `code/macosx/swift/Sources/DimoReviews/PowerAssertions.swift:1-61`, `code/macosx/crates/dimo_client/tests/features/keep_awake.feature:1-20`: generations/resource retention; exclude accounts/automatic-busy policy.
* Apple [QA1340](<https://developer.apple.com/library/archive/qa/qa1340/_index.html>)/[display assertion](<https://developer.apple.com/documentation/iokit/kiopmassertiontypepreventuseridledisplaysleep>): fetched this session, not screensaver proof.
* Coordinator's direct-GitHub [LICENSE receipt](<https://github.com/appliedmedia/print2paper4vscode/blob/main/LICENSE>): upstream filename `LICENSE`, commit `28d3c6ab714344058356fccfee386480cad7650f`, blob `90b2643a579327564ed214dee8ec849fbc1732f1`, `# License: Code Transparency v1`. The authorized planning batch copies exact bytes to root `LICENSE.txt`; `git hash-object LICENSE.txt` must match that blob. Preserve copyright/dates/escaped marker and restrictive maintainer/redistribution terms; never relabel MIT. This master amendment does not itself copy or verify the destination file.

## Planning receipt and current status

Historical master-planning receipt: baseline `mod:c75+s75='copilot:gpt-6-sol:1M:max'`; actual profile unverified. Task `task_162aefb68c01`, Dispatch `ctx_fb08820f253c`, worker `term_27359d29-6baa-4488-94f1-2b4b90834b6a`, coordinator `term_f29f11e1-6a27-485d-982c-6327ebffdb9a`. Root `/Users/acoven/dev/ai2sw/workspaces/main/sponge`, branch `boot-ai-context-report`, base `9630adb5a32c306fd282210f8c512951f69b33e1`; canonical main `/Users/acoven/dev/ai2sw/main`. Sole deliverable: master; completion receipt records commit/validation.

Current baseline: documentation/license PR #1 merged into main at `c6581b4fbcaa2268132ce660c3965b80d433a146`; plans, README, `_goal.md`, `_orch.md`, and `LICENSE.txt` are present. Root pointers still resolve to this unchanged dated master filename. The assigned worktree was clean at that base; `git hash-object LICENSE.txt` returned `90b2643a579327564ed214dee8ec849fbc1732f1`. Application source, build, and artwork assets are not yet integrated on main; source work in flight is not a built artifact. `code/README.md`/status markers are absent, so no dashboard is created. Project-wide inventory, measured average duration, ETA, and shareable-build date: UNKNOWN. BLOCKERS: EMPTY; unresolved support/screensaver/telemetry checks remain visible technical work, not a request to repeat D1 approval.

### Implementation-start receipt

Verified integrated base: `c6581b4fbcaa2268132ce660c3965b80d433a146`. Canonical main `/Users/acoven/dev/ai2sw/main` is not a worker write target. Both tasks belong to the same implementation wave; neither has authority to open a separate PR.

* Approval/start amendment: Task `task_89b74ecd04ab`, Dispatch `ctx_0d7ddb8bfc9d`, worker `term_2fe30f6e-b0e7-46f5-b241-ab95a2ac4560`; Orca receipt `wt2:local:475d7d39-890d-4c9f-adf2-11bffc8676d7`, worktree `/Users/acoven/dev/ai2sw/workspaces/main/screenwake-implementation-plan`, branch `screenwake-implementation-plan`. Sole deliverable: this committed master amendment. Baseline `mod:c45+s90='copilot:gpt-6-luna:400K:medium'`; actual model `gpt-6.1-sol`, inherited context/effort UNVERIFIED, no override or congestion fallback. Settled documentation scope only; no architecture redesign.
* D2 source foundation: Task `task_99f75dabad82`, Dispatch `ctx_19284c6264fd`; Orca-assigned worktree `/Users/acoven/dev/ai2sw/workspaces/main/screenwake-rust-foundation`, assigned branch UNVERIFIED until its receipt is supplied. Status: inProgress, with no source/test/build completion claimed here. Baseline `mod:c75+s75='copilot:gpt-6-sol:1M:max'`; actual inherited model/context/effort UNVERIFIED. Strong reasoning owns novel state/order/contracts; no automatic Luna downgrade. Freeze exact files/APIs and technical evidence before native/MCP worker dispatch.

### Historical documentation-batch review receipt

Batch branch `boot-ai-context-report`, comparison base `origin/main=9630adb5a32c306fd282210f8c512951f69b33e1`, triad-input SHA `33970e0e1349ebd1584594d8a51dd56b7b48641e`; assigned worktree `/Users/acoven/dev/ai2sw/workspaces/main/sponge`. Exactly one completed independent pass per role was collected:

* ARCH: Task `task_fe950f4f5180`, Dispatch `ctx_36c670f42f28`; baseline `mod:c75+s50='copilot:gpt-6-sol:1M:max'`; actual `gpt-6.1-sol`, inherited context/effort UNVERIFIED.
* QUALITY: Task `task_f50aae97b599`, Dispatch `ctx_76b9b547c26d`; baseline `mod:c65+s40='copilot:gpt-5.6-terra:1M:max'`; actual `copilot:gpt-6.1-sol:long_context:high`, human-facing capacity UNVERIFIED.
* PROCESS: Task `task_15b362fce328`, Dispatch `ctx_7304d0708229`; baseline `mod:c65+s40='copilot:gpt-5.6-terra:1M:max'`; actual `gpt-6.1-sol`, inherited context/effort UNVERIFIED.

Canonical non-license finding: all three passes identified stale master line 128 at the input SHA, incorrectly declaring root pointers absent; PROCESS also requested current delivery and documentation-only PR authorization. ARCH rated it P3, QUALITY/PROCESS P2. The current-status paragraph above corrects this duplicate finding once; no other actionable non-license findings were reported.

One logical consolidated fix-worker slot: Task `task_a6a60d6e2fb5`, Dispatch `ctx_a5618a8bba4d`; baseline `mod:c35+s95='copilot:gpt-6-luna:400K:medium'`; actual `gpt-6.1-sol`, inherited context/effort UNVERIFIED, no override. Sole deliverable: this committed master-only status/review amendment; license and all other files remain unchanged. Coordinator consumes the collected-once triad token after successful `worker_done` and final-validates the committed head; the foreground PR owner holds final-SHA evidence. No local triad or reviewer is rerun.

Historical AndrewC documentation/license PR authorization on 2026-11-15: "Go ahead and push this to a pr and ask coderabbit for a review of it all. We are NOT changing the LICENSE so if it has problems with it just let me know but defer them. Otherwise whatever else it might recommend is fair game." At that time, this authorized only the documentation/license PR and CodeRabbit review, not D1 implementation. The later implementation instruction above supersedes that execution limit; license concerns remain deferred.

### Operator-deferred license concerns

Exact `LICENSE.txt` bytes remain frozen under the quoted instruction. These reported concerns are deferred, do not reinterpret the terms, and add no merge prerequisites:

* ARCH: `LICENSE.txt:18` permits pull requests while `:22` forbids modified versions without an explicit contribution exception, a potential wording ambiguity.
* PROCESS: upstream `LICENSE.txt:1,3,46` places the title before copyright and uses an escaped ending, differing from the generic first-line copyright/matching-end convention.

## Historical 2026-11-15 user-requirement amendment

At this amendment, AndrewC approved these plan requirements only, without changing D1 or implementation authorization. Sources:

* "is_on is great by me to return a json blob of current status on/off, and if on then protected-idle duration, protected-idle-suppressed-events and only list the events we circumvented."
* "Make sure each event is with an epoch time so you can show how many minutes into idle forcing on we were when that event occurred."

Historical protected-idle amendment receipt: Task `task_9cef8a9374c2`, Dispatch `ctx_d1c95eed9646`, worker `term_a3acefae-17f2-4f0b-a0b5-86b6a99e771b`; baseline `mod:c65+s85='copilot:gpt-5.6-terra:1M:xhigh'`. Actual runtime model `gpt-6.1-sol`; context/effort UNVERIFIED, no model override. Sole deliverable: this master amendment; native API/cancellation evidence remains unverified and no source, tests, license, or system behavior changed.

At the subsequent design amendment, TV iconography and local MCP/transparency delivery were approved design/documentation requirements only, without changing D1 or application implementation authorization. The earlier "Looks perfect" was not permission to build. Design sources:

* "I'm thinking SVG for all this, but how about a very simple tv-looking thing with a smiley mouth but 'closed eyes' and zzZ coming from the top of it's 'head' and a big detailed version of that is the app icon and when you click it in the menubar the Zzz goes away and the eyes open REALLY WIDE and that's the toggle state?"
* "Have it be an old-school looking tv with antenna for the app icon and app propaganda."

Historical master-only amendment receipt: Task `task_a8572aee9e76`, Dispatch `ctx_ed6bbf85f970`, worker `term_cda7d73c-7b10-430a-baf5-31363bc53f85`; baseline `mod:c45+s95='copilot:gpt-6-luna:400K:medium'`, actual model `gpt-6.1-sol`, context/effort UNVERIFIED, no override. Assigned worktree `/Users/acoven/dev/ai2sw/workspaces/main/sponge`, branch `boot-ai-context-report`, starting HEAD `7907321cc4e3fca9247264c21f7fa3b2ddcff6aa`. Separate workers own README/license delivery and coupled lane path updates; no artwork, application source/tests, security-policy change, PR, or release produced by this amendment.

## Operator next actions

* Implementation approval is recorded above; no repeat approval is requested.
* D2 owner resolves macOS/architectures/screensaver API evidence before G1 delivery. Telemetry configuration remains a technical release gate; any new scope or external endpoint needs its own authorization, not an assumed current operator dependency.

<!-- end 2026-11-15_plan_todo_ScreenWake_Orch.md -->
