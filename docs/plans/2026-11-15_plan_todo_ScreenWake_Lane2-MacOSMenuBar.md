<!-- 2026-11-15_plan_todo_ScreenWake_Lane2-MacOSMenuBar.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Lane 2 MacOSMenuBar

Status: todo, awaiting AndrewC approval. This document authorizes no implementation, build, UI launch, settings change, or idle test.

## Inputs

* [Master orchestration](<2026-11-15_plan_todo_ScreenWake_Orch.md>): authoritative proposed G1:IdleProtection, G2:SharedControl, P1:SingleOwner through P6:SafeControl; TRD 5 through 8 define the sole seam, payloads, lifecycle, authorization, and verification contracts.
* [Lane 1 RustRuntime](<2026-11-15_plan_todo_ScreenWake_Lane1-RustRuntime.md>): D2 freezes supported macOS/architectures, generated interfaces, native ownership, ordering invariants, and meaningful behavioral red.
* [Lane 3 MCPDelivery](<2026-11-15_plan_todo_ScreenWake_Lane3-MCPDelivery.md>): shared-owner integration and canonical build/check runners.

## Produces and NotProduces

Produces now: this approval-only lane document. After approval: minimal menu/native integration, affected executable scenarios and colocated documentation, delivered into the shared wave.

NotProduces: independent core state, schemas, IPC policy, telemetry SDK, fourth engine/UI crate, license copying, timers, login startup, accounts, preferences, synthetic input, security suppression, or a separate PR. Reference patterns cited by the master are source-only; never copy Adobe implementation or DIMO automatic-busy/account policy.

## Prerequisites and touches

D1 approval and D2 contract freeze precede execution. Lane 2 may then run alongside Lane 3 only with disjoint file ownership recorded before writes. Unsettled contracts return to Lane 1's reasoning owner.

Proposed touches:

* `code/macos/ScreenWake/`: thin app/menu rendering, menu input capture, and generated-seam attachment; no idle-duration, event, SDK, or timer policy in Swift.
* `code/crates/ai2screenwake_native/`: assigned OS-resource, genuine idle-age sampling, conditional proven-cancellation callbacks, login-session identity, and LaunchServices glue only; D2 names exact files. Rust client retains all desired/effective/pending/error state, generation/episode policy, duration/event computation, and localized copy.
* Native/menu Cucumber scenarios and narrow actual-renderer/OS witness bindings, with exact paths frozen at D2. Lane 3 owns runner/bundle tooling; Lane 1 owns core transition scenarios.

## Implementation checklist

* [ ] Confirm D1/D2 evidence and ownership; attach the generated emit/observe seam and register every assigned native handler. Reference master contracts without mirrored schemas, route strings, direct state methods, or blocking return-state RPC.
* [ ] Render observed On/Off/Pending/Error and Turn on, Turn off, Quit; no Dock/main window. Start OFF each process, including MCP-triggered launch. Swift renders Rust-localized projections without optimistic mutations; verify menu labels, keyboard operation, focus, enabled state, and non-color status.
* [ ] Implement native acquisition of both IOKit assertions all-or-nothing with partial rollback. Retain failed-release resource IDs until confirmed cleanup; report outcomes through the same bus. Rust owns generation guards, supersession, and reconciliation of opposite commands.
* [ ] Supply fresh genuine native idle age for queries through typed `Power::Activity::{Sample,Observed}` on the existing emit/log/observe seam. Verify the source/API and accuracy on supported macOS before claiming duration. Sampling failure dispatches an explicit canonical error and yields null duration, never a fabricated zero. No raw input/key/mouse logging, invented activity, synthetic input, or new permissions.
* [ ] Match the shared `is_on` contract: while confirmed On, add `protected_idle_duration_seconds:u64|null`, `protected_idle_suppressed_events:SuppressedEvent[]`, and `suppressed_events_truncated:bool`; omit these fields off/unresolved. Rust computes duration as `floor(min(genuine native idle age, monotonic current confirmed-On age))`. Repeated queries/TurnOn do not restart it; genuine activity, forced sleep/wake, off/new On, and restart reset the episode, excluding forced-sleep time.
* [ ] Capture only eligible native-reported idle requests with matching successful cancellation proof, using a conditional `idle_sleep_request_cancelled` hook only if verified available. Native callbacks capture actual Unix UTC `epoch_ms:u64` alongside monotonic cancellation time; typed `Power::Suppression::Recorded` uses the same bus. Rust produces `SuppressedEvent={kind:string,epoch_ms:u64,minutes_into_protected_idle:number}`, computing finite nonnegative minutes from that event's monotonic episode anchor as elapsed milliseconds / 60000, never wall-clock subtraction. Current-episode output retains only the latest 64 proven records with truthful truncation; durable historical records remain in the action log, never carried into a new episode.
* [ ] Return an empty event array when no cancellation is confirmed; active IOKit assertions commonly expose no per-attempt stream. Exclude inferred screensaver/display-timer suppression, assertion-active/on-off, generic activity/sleep-wake, and lid/manual/security-sleep claims. Never introduce power behavior merely to manufacture an event; callbacks remain OS glue, not independent state or policy.
* [ ] Integrate same-user login-session identity and exact-app LaunchServices hooks with the client-owned lock/IPC lifecycle. Preserve existing-owner authority; quit drains bounded cleanup, crash releases process resources, and restart/replay never restore on intent. Resource faults dispatch canonical error events before observers render short in-menu errors; no OS alerts or Swift telemetry.
* [ ] Before production edits, run requirement-based executable Cucumber Given/When/Then red for changed behavior, then green. Own actual menu/native assertions for off/on/pending/error, keyboard control, opposite commands, quit/crash/restart, replayed history, and induced faults; reuse Lane 1 core cases without duplication. Compiler errors or zero selection are not behavioral red.
* [ ] Use planned `cargo xtask build --profile release` to generate worktree-local UniFFI and `target/screenwake/ScreenWake.app`; assert target/configuration/SHA before dependent witnesses. Never copy artifacts across checkouts or commit generated bindings unless documented as source-of-truth. No Claiyr iOS scheme.
* [ ] Run affected `cargo xtask sanity` within 10 seconds, recording count, duration, command, exit status, and artifact identity; coordinate shared checks with Lane 3. These commands are planned, not existing tools.
* [ ] After approval, run `cargo xtask verify-macos` on the frozen support matrix in an isolated authorized session: fresh state, replayed history, induced errors, and concurrent native/MCP control. Record actual idle/screensaver duration separately, serial resource ownership, budget, and stage/total timing. Preserve manual/forced sleep, lid/low-battery/thermal behavior, security locking, and manually started screensaver.
* [ ] Extend executable native witness scenarios to verify fresh idle-age accuracy, explicit sampling-failure error/null duration, query/TurnOn continuity, and activity/sleep-wake/off-newOn/restart resets without counting forced sleep. If a cancellation hook is available, correlate each eligible request and successful cancellation with actual epoch/monotonic timestamps and its event-time episode offset. Verify empty/no-inferred/excluded events, stale-evidence isolation, latest-64 truncation, and wall-clock changes without negative/nonfinite offsets; coordinate deterministic core cases with Lane 1. Without a verified cancellation stream, prove empty output rather than claiming suppression evidence. Keep exactly `is_on`, `turn_on`, `turn_off`; no `idle_info`.

Display-assertion documentation does not prove screensaver inhibition. A failed real witness leaves G1 deliverability unverified; investigate public APIs and obtain explicit design approval, never add user-activity heartbeats, input simulation, preference writes, or security bypasses.

## Handoff and receipt

Return exact source/test paths, commit and generated artifact identities, scenario counts, timing/exit evidence, and unresolved G1 findings to the master. No per-lane PRs: open exactly one PR against shared wave branch. Batch review/final validation remain master-owned.

Document baseline: `mod:c60+s85='copilot:gpt-5.6-terra:1M:xhigh'`. Future implementation baseline: `mod:c65+s75='copilot:gpt-5.6-terra:1M:xhigh'`; actual inherited model/context/effort unverified. Task `task_70ff1f901c98`, Dispatch `ctx_296fd906370c`, worker `term_f7fadc89-9ede-4dfb-b5f9-67c6ac7638d9`, worktree `/Users/acoven/dev/ai2sw/workspaces/main/sponge`, branch `boot-ai-context-report`, master commit `b229642`. Coordinator serializes staging/commit after disjoint document returns.

2026-11-15 planning amendment: `mod:c55+s85='copilot:gpt-5.6-terra:1M:xhigh'`; Task `task_b9219f257edf`, Dispatch `ctx_f064860adc6f`, worker `term_40357ee0-9673-4ecf-a828-315bb388d543`, same assigned worktree/branch. Approved JSON requirements inform this proposal only; all implementation tasks remain unstarted. Native idle-source/cancellation-hook availability remains unverified; no implementation, system tests, staging, or commit authorized.

Operator actions surface: [master Operator next actions](<2026-11-15_plan_todo_ScreenWake_Orch.md#operator-next-actions>), record/reference only.

<!-- end 2026-11-15_plan_todo_ScreenWake_Lane2-MacOSMenuBar.md -->
