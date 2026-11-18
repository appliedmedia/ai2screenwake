<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake (ai2sw)

AI2ScreenWake is a macOS idle-protection utility under development, not a released application. The genus/mode-label correction starts from delivered PR #7/main `1e0fd01`. See the [project plan](<_orch.md>).

## Controls and architecture

The host starts Off. Its landscape TV retains two selector dots, an inset screen and equally weighted cabinet/screen outlines; the shorter, lower left antenna has two rays and no horizontal base. Off has closed eyes, a smile and right-side Zzz with a clear pixel above the cabinet. On has large eyes, distinct pupils and one straight mouth; Error has X X eyes and a frown. Visible values are exactly `Mode: Can Sleep`, `Mode: No Sleep` and `Off: Error`. The last is literal display copy, not a claim that independently confirmed protection became Off. Pending retains the last confirmed face/value, with truthful Pending detail in Rust tooltip/accessibility and unchanged nullable core/MCP status. Genuine faults immediately take precedence over intentional previews.

Short primary input toggles once on release, never on down. Control-click, right-click or a 0.45-second held press opens the same native anchored popover without toggling on release. A transparent, accessibility-hidden public pointer view receives status-button hits and owns event-tracking-mode timing, avoiding competing native-cell/recognizer loops; the system square button keeps its native drawing, keyboard and accessibility activation. Drag, outside release, Escape and disabled/closing rendering cannot enable protection.

The large 96-point TV is a keyboard-accessible cycle control: On to Off to Error to On, with real power targets On, Off and Off respectively. Error preview first requires its exact successful confirmed-Off settlement, clearly identifies itself in tooltip/accessibility and never creates a core fault or MCP error. Rapid input waits for observed power; newer ordinary admitted controls retire preview transitions, while queries preserve the preview. The popover stays open during cycling and begins with neutral focus. Show Log and Quit share one bottom row: native 95/50-point title fitting plus normal spacing requires 176 x 188 points here, rather than clipping at 172 or retaining the old 224-point height.

`No Sleep` means ordinary idle system/display protection, not prevention of manual screensavers, forced sleep or security locking. The investigated manual Start Screen Saver corner has no supported public global veto or reliable start capture in the reviewed SDK; it remains unsupported and unimplemented. Automatic screensaver efficacy remains unverified. No guessed trigger, fabricated suppression record, synthetic activity, OS change or permanent Error is added for this limitation.

Show Log creates or refreshes one bounded private snapshot per owner run at `~/Library/Logs/AI2ScreenWake/yyyyMMdd{DD}HHmmss_ai2sw.log`. The timestamp is local 24-hour time; `DD` is lowercase `mo/tu/we/th/fr/sa/su`. The full actual absolute path is line one, and the read-only viewer uses the actual timestamped basename. The native seam resolves the current user's home and captures one instant; repeated opens refresh that same file only on demand. A collision with another run's filename is an explicit log error, never an overwrite or invented future time.

Below that path, the newest actual entry comes first and older entries follow; omission/storage notices come after entries. Export and viewer share the same bounded 200-label/64-KiB metadata-only formatter. Identical background renders preserve selection/scroll; explicit opening or changed refresh reveals the top. Private 0700/0600 ownership, atomic publication and errors-first behavior stay unchanged. Binary journal/archive append/replay/checksums and login-session lock/socket identity do not reverse. No tailing, automatic clipboard, retention/deletion or external sending is added.

Current readable genus labels have four static typed fields, ending in the verb, such as `Power:Session:Protection:Query` or `Ui:Menu:Presentation:Render`. Compact registry/root/action/instance diagnostics follow separately; no opaque ID is printed per row. One genuine registry-lifetime identity plus checked root ordinals replaces IDs minted for every ordinary root. Real native operation/session/sample identities remain. Old journal rows retain their actual legacy path, explicitly marked `legacy`, without changing stored bytes or replaying protection. IPC negotiates its updated envelope format before controls, refusing older peers explicitly instead of risking mismatched settlement.

Three Rust runtime crates provide backend contracts, client/owner policy, and native OS access. Commands, observations, and errors use Actions; generated UniFFI connects minimal Swift/AppKit rendering. Native UI and MCP share one login-session owner, not separate power assertions. Bounded durable history rebuilds diagnostics after restart, never live On intent or assertions.

## Local build

Build from a clean, committed checkout on an arm64 Mac with Rust, Xcode's command-line tools/macOS SDK, `xcrun`, `codesign`, and `plutil`. MCP/bindgen dependencies require Rust 1.88 or later; the verified toolchain is Rust 1.97.1 and Swift 6.4.2 on macOS 27.0.1. The arm64/macOS 13 deployment target is not a macOS 13 runtime or Intel/universal compatibility claim.

```sh
cargo xtask build --profile release
```

The sole app output is `target/ai2sw/ai2sw.app`, containing executable `ai2sw`, the unchanged MCP executable, thirteen artwork resources and license, nineteen signed files in all. Four SVG masters produce twenty-three encoded assets; each of the three menu masters independently supplies 24/48-pixel menu images and 96/192-pixel previews. The detailed app icon is unchanged. The stable bundle identifier remains `com.appliedmedia.ScreenWake`; no private owner/history identity migrates. Generated bindings, feature snapshots, assets and build evidence share `target/ai2sw/`. The builder verifies current-source provenance, ad-hoc signatures and discovery-only MCP without launching or enabling the app. It does not install, notarize or release it. See [build and check details](<code/xtask/README.md>).

## Local MCP

Set your MCP client's stdio command to the absolute path of `target/ai2sw/ai2sw.app/Contents/MacOS/ai2screenwake-mcp` in the built checkout, with no arguments. The bundled executable implements `initialize`, `tools/list`, and `tools/call`; its only tools are `is_on`, `turn_on`, and `turn_off`, each accepting an empty object. Show Log and Quit are native UI only.

Discovery never launches or enables the owner. A control connects to the existing owner or requests launch of that exact bundled app Off; it never automatically turns protection on. An unbundled development binary returns a structured unavailable error, not false Off. Results follow observed settlement, including explicit pending/error and unavailable outcomes.

When confirmed On, status includes protected-idle duration and only proven suppressed events with `epoch_ms` and `minutes_into_protected_idle`. There is no verified native per-attempt cancellation source yet: an empty event list means none confirmed, not proof nothing was prevented. See the [MCP contract](<code/crates/ai2screenwake_client/src/mcp/README.md>).

## Validation and availability

Current source Task `task_c8c052a18e1d`/Dispatch `ctx_3d2f81451899` completed semantic genus/lifetime-root identity and literal mode labels at `4938d56`. Its immutable `target/genus-mode/source-result.json` records compiled reds, focused sanity (28 scenarios/87 steps/four native groups in 9.511442 seconds) and exact-head nineteen-file native build without app launch. Normal/final fields in that source-stage receipt stay null.

The parent has now assembled the feasible batch and authorized one normal literal `cargo xtask check` after all intended documentation is committed. Actual normal execution belongs only to `target/genus-mode/assembled-result.json` and compact `assembled-public.json`; final PR validation and this batch's one new triad/combined handling remain separate parent-owned stages. Sanitized app-exit diagnosis shows orderly Quit/Off/Closed/terminal-render completion, not an observed crash; the initiating input/actor remains unknown. Requested manual-corner prevention remains unfinished and unimplemented. No speculative tap, automatic relaunch, origin tracking, permissions/settings change, or user-app/live-Logs/input operation is added.

PR #7 is completed history: merge `1e0fd01` at `2026-11-18T11:39:19Z`, exact sleep-menu root/local/remote removal and a fresh nineteen-file main build. Opened parent receipts also retain its then-live Off response, not current app-state proof. Its source `749b459` passed 285 scenarios/914 steps/four AppKit groups in 96.200056 external seconds before delivery.

### Historical PR #6 and earlier evidence

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`; full local validation is literal `cargo xtask check`. The completed artwork increment's red/green and twelve image inspections remain immutable in ignored `target/artwork-polish/result.json`. Its `bfd650b` check, 264 scenarios/841 steps/four AppKit groups in 125.736209 external seconds, predates the added native changes. `target/artwork-polish/native-result.json` owns the native width/focus/release regressions and complete new committed-source normal validation. Canonical full validation includes generated production Swift, four nonpresenting AppKit groups, nineteen signed files and three-tool MCP discovery. The complete 300-second ceiling, ten-second development-sanity selection and actual asset/runner guards remain unchanged; there is no blanket ten-second cap on a whole IPC family. No `verify-macos` command is implemented.

Historical PR #6's eight-request batch completed one clean ARCH/QUALITY/PROCESS pass at `cf4f9bf` and sole no-finding handling at `94996ae`. Its exact final execution remains in `target/artwork-review/fix-result.json` and sanitized `final-public-receipt.json`. The older source's 264/841/four-group normal pass in 88.060219 external seconds is pre-triad history, not validation of sleep-menu or its later corrections.

Both historical PR #6 initial CodeRabbit points were fixed at `d5b2132`; its confirmation identified only the regression fixture path overhead. Final `a9cde25` preserves ordinary 94/103-byte socket budgets and its original provider receipts. Opened parent receipts confirm merge as `bd804c0`, exact `tv-artwork-polish` branch/worktree removal, a fresh nineteen-file main build in 25.286109 mechanical/26.094999 external seconds and normal launch/read-only Off response. No prior app was running, so no Quit was needed. This is completed historical delivery, not current app state, physical-input acceptance or a review token for sleep-menu.

Historical PR #5 final source `f3c7b9` passed 261 scenarios/832 steps and four AppKit groups in 85.639821 mechanical/86.152488 external seconds; main `f2b14f5` produced its nineteen-file build in 26.712465 mechanical/27.343254 external seconds. These coordinator-supplied receipts are history, not validation of the current artwork.

Historical PR #4 final source `8d44f60` passed 257 scenarios/820 steps and four AppKit groups in 93.242592 external seconds; main `ed74091` produced the nineteen-file build in 25.799644 mechanical/27.585506 external seconds. Neither is validation of this later compact-menu source.

Historical PR #3 final source `1e103d2` passed 243 scenarios/776 steps plus four AppKit groups in 82.530111 external seconds. Its integrated-main `99ffe72` fifteen-file build took 28.437910 mechanical/30.767526 external seconds, receipt `target/ai2sw/build-evidence/1794902425563-14619-bundle.json`. Neither receipt validates these later faces or inactive-forwarding correction.

Historical `5067dd1` validation produced the old eleven-file bundle and passed nineteen safe actual-app native/MCP stages in 2.841206541 seconds, covering shared control, owner error/history, restart Off and ordinary Quit/resource cleanup. Those stages are not current-source physical acceptance. Physical context-click/long-press/log-window operation, natural idle/screensaver efficacy, manual/security/forced-sleep behavior, macOS 13 runtime/support, native cancellation evidence and installation/release remain unverified. Such operations are not selected by this approved delivery batch; no foreground/input consent or new approval is inferred or requested.

CI is local-only by AndrewC's explicit instruction; workflow `420435149` remains disabled. No workflow is changed, triggered or awaited. This batch receives its own one triad after assembled normal validation, then sole handling and final immutable-head validation before its one `coderabbit_optional` PR. No prior PR's consumed reviewer token is reused. No release or physical acceptance is claimed.

Historical implementation PR #2 is merged, with its one triad/fix and twenty later review findings addressed in the [master mapping](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#late-source-corrections>). PR #3 and its delivery cleanup are also complete. Source tests never control, move or copy the current user app; their build is worktree-local. AndrewC explicitly authorized the coordinator to complete review/delivery, build from integrated main, then normally quit/relaunch that new build and check responsiveness. This worker performs none of those user-app operations and requests no additional approval.

Optional core-only telemetry and Rust UI error rendering are implemented with focused real-loopback/component evidence. The actual app bootstrap remains `Unconfigured`; no external endpoint/key/sending authorization, release feature selection or real external delivery is established.

The delivered prior roots/branches, including `sleep-menu`, are historical. The active batch is [genus/mode-label correction](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#active-genus-and-mode-label-correction>). No release is posted and delivery dates remain UNKNOWN. Public-API, native-resource, binary-replay, security and consent limits stay unchanged; actual source dirtiness still fails canonical builds.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
