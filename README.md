<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake (ai2sw)

AI2ScreenWake is a macOS idle-protection utility under development, not a released application. PRs #2 through #4 are merged; this compact-TV/menu refinement starts from integrated main `ed74091`. See the [project plan](<_orch.md>).

## Controls and architecture

The menu-bar host starts Off. Its landscape TV has two left selector dots, a narrower inset screen and left rabbit ears: Off has closed eyes, a smile and ascending right-side Zzz; confirmed On has very wide-open eyes and straight gritted teeth; Error has X X eyes and a frown, never an exclamation badge or sleep marks. Pending explicitly reports `Status: Pending` and accessible Pending while retaining the last confirmed face, without title dots or optimistic power claims. A genuine attention fault immediately selects `Status: Error`; core-owned tooltip/accessibility help retains safe error detail and independently observed protection. MCP/resource truth stays unchanged. Intentionally unconfigured or disabled telemetry is normal inactivity, not an attention fault; no sending is enabled.

Primary click requests a toggle. Control-click, right-click or long press opens one compact native upward-pointing 172 x 224-point popover below the status icon, with a crisp matching 96-point preview, status, Show Log and Quit. There is no separate error row or reserved blank gap. Complete first-open sizing and the fixed 48-point status anchor remain stable; cached images stay centered and are assigned only when their identity changes. Secondary and long-press release never toggle. Normal Quit requires confirmed resource cleanup and owner disposal.

Show Log creates or refreshes one bounded private snapshot per owner run at `~/Library/Logs/AI2ScreenWake/yyyyMMdd{DD}HHmmss_ai2sw.log`. The timestamp is local 24-hour time; `DD` is lowercase `mo/tu/we/th/fr/sa/su`. The full actual absolute path is line one, and the read-only viewer uses the actual timestamped basename. The native seam resolves the current user's home and captures one instant; repeated opens refresh that same file only on demand. A collision with another run's filename is an explicit log error, never an overwrite or invented future time.

The snapshot contains at most 200 recent action labels/64 KiB, not protected payloads. Its directory/file are private 0700/0600; checked atomic replacement preserves previous bytes on failure and errors are bus-observed before the failed view. Only this derived text moves: the checksummed binary journal/archive, owner lock/socket and private login-session identity remain in Darwin's per-user temporary root and retain sole replay authority. No continuous tailing, automatic clipboard export, retention/deletion policy or external sending is added.

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

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`; full local validation is `cargo xtask check`. This batch's compiled behavioral red/green, actual native-cell geometry, rendered-asset inspection and committed-source normal validation belong to ignored `target/compact-tv/result.json`, not an earlier receipt. Canonical full validation includes generated production Swift, four nonpresenting AppKit groups, nineteen signed files and three-tool MCP discovery. The complete 300-second ceiling, ten-second development-sanity selection and actual asset/runner guards remain unchanged; there is no blanket ten-second cap on a whole IPC family. No `verify-macos` command is implemented.

Historical PR #4 final source `8d44f60` passed 257 scenarios/820 steps and four AppKit groups in 93.242592 external seconds; main `ed74091` produced the nineteen-file build in 25.799644 mechanical/27.585506 external seconds. Neither is validation of this later compact-menu source.

Historical PR #3 final source `1e103d2` passed 243 scenarios/776 steps plus four AppKit groups in 82.530111 external seconds. Its integrated-main `99ffe72` fifteen-file build took 28.437910 mechanical/30.767526 external seconds, receipt `target/ai2sw/build-evidence/1794902425563-14619-bundle.json`. Neither receipt validates these later faces or inactive-forwarding correction.

Historical `5067dd1` validation produced the old eleven-file bundle and passed nineteen safe actual-app native/MCP stages in 2.841206541 seconds, covering shared control, owner error/history, restart Off and ordinary Quit/resource cleanup. Those stages are not current-source physical acceptance. Physical context-click/long-press/log-window operation, natural idle/screensaver efficacy, manual/security/forced-sleep behavior, macOS 13 runtime/support, native cancellation evidence and installation/release remain unverified. Such operations are not selected by this approved delivery batch; no foreground/input consent or new approval is inferred or requested.

CI is local-only by AndrewC's explicit instruction; workflow `420435149` remains disabled. No workflow is changed, triggered or awaited. The coordinator owns this new batch's single pre-PR triad and consolidated fix after normal source validation. All correction commits precede final immutable-head validation at `validated_final_sha`, then one `coderabbit_optional` PR and authorized delivery. No prior PR's consumed reviewer token is reused or rerun. No release or physical acceptance is claimed.

Historical implementation PR #2 is merged, with its one triad/fix and twenty later review findings addressed in the [master mapping](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#late-source-corrections>). PR #3 and its delivery cleanup are also complete. This batch never controls, moves or copies the current user app; its normal build is worktree-local.

Optional core-only telemetry and Rust UI error rendering are implemented with focused real-loopback/component evidence. The actual app bootstrap remains `Unconfigured`; no external endpoint/key/sending authorization, release feature selection or real external delivery is established.

The previous implementation/refinement roots and branches, including `tv-screen-icon`, have been removed. The active `compact-tv-menu` batch and its receipts are recorded in the master; no release is posted and delivery dates remain UNKNOWN. Existing public-API, native-resource, binary-replay, security and consent limits remain unchanged. Finder's `.DS_Store` is ignored narrowly; actual source dirtiness still fails canonical builds.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
