<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake (ai2sw)

AI2ScreenWake is a macOS idle-protection utility under development, not a released application. Implementation PR #2 and naming/usability PR #3 are merged; integrated main is `99ffe72`. The current batch refines three observed TV faces and corrects a false warning from intentionally inactive telemetry. See the [project plan](<_orch.md>).

## Controls and architecture

The menu-bar host starts Off. Its landscape TV has an inset screen and left rabbit ears: Off has closed eyes, a smile and ascending right-side Zzz; confirmed On has very wide-open eyes and straight gritted teeth; Error has X X eyes and a frown, never an exclamation badge or sleep marks. Pending retains dots. A genuine attention fault selects the Error face and exactly `Status: Error`, while accessibility retains independently observed protection and MCP/resource truth stays unchanged. Intentionally unconfigured or disabled telemetry is normal inactivity, not an attention fault; no sending is enabled.

Primary click requests a toggle. Control-click, right-click or long press opens one native upward-pointing popover below the status icon, with a crisp matching 96-point preview, status, Show Log and Quit. Its fixed layout is filled before the first open; secondary and long-press release never toggle. Normal Quit requires confirmed resource cleanup and owner disposal.

Show Log regenerates a bounded, private `ai2sw.log` snapshot beside the existing binary journal in macOS's per-user temporary root under `ai2screenwake-<login-session>/`. The first line and read-only viewer show the actual full absolute `.log` path. It contains at most 200 recent action labels, not protected payloads; it updates only on Show Log, not by continuous tailing. Atomic replacement preserves the previous snapshot on failure; errors are bus-observed. The checksummed binary journal remains the sole replay source.

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

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`; full local validation is `cargo xtask check`. This batch's focused behavioral red/green, rendered-asset inspection and committed-source normal validation belong to ignored `target/tv-screen-icon/result.json`, not an earlier receipt. Canonical full validation includes the generated production Swift, four nonpresenting AppKit groups, expanded release inventory, signatures and MCP discovery. The complete 300-second ceiling, ten-second development-sanity selection and actual asset/runner guards remain unchanged; there is no blanket ten-second cap on a whole IPC family. No `verify-macos` command is implemented.

Historical PR #3 final source `1e103d2` passed 243 scenarios/776 steps plus four AppKit groups in 82.530111 external seconds. Its integrated-main `99ffe72` fifteen-file build took 28.437910 mechanical/30.767526 external seconds, receipt `target/ai2sw/build-evidence/1794902425563-14619-bundle.json`. Neither receipt validates these later faces or inactive-forwarding correction.

Historical `5067dd1` validation produced the old eleven-file bundle and passed nineteen safe actual-app native/MCP stages in 2.841206541 seconds, covering shared control, owner error/history, restart Off and ordinary Quit/resource cleanup. Those stages are not current-source physical acceptance. Physical context-click/long-press/log-window operation, natural idle/screensaver efficacy, manual/security/forced-sleep behavior, macOS 13 runtime/support, native cancellation evidence and installation/release remain unverified. Such operations are not selected by this approved delivery batch; no foreground/input consent or new approval is inferred or requested.

CI is local-only by AndrewC's explicit instruction; workflow `420435149` remains disabled. No workflow is changed, triggered or awaited. This new batch requires its own one pre-PR triad and consolidated fix result after normal validation, then final immutable-head validation, one `coderabbit_optional` PR and authorized merge/readback/cleanup/main rebuild. PR #3's review tokens are historical and cannot be reused or rerun. No release or physical acceptance is claimed.

Historical implementation PR #2 is merged, with its one triad/fix and twenty later review findings addressed in the [master mapping](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#late-source-corrections>). PR #3 and its delivery cleanup are also complete. This batch never controls, moves or copies the current user app; its normal build is worktree-local.

Optional core-only telemetry and Rust UI error rendering are implemented with focused real-loopback/component evidence. The actual app bootstrap remains `Unconfigured`; no external endpoint/key/sending authorization, release feature selection or real external delivery is established.

The previous implementation/refinement roots and branches have been removed. The active `tv-screen-icon` batch and its receipts are recorded in the master; no release is posted and delivery dates remain UNKNOWN. Existing public-API, native-resource, binary-replay, security and consent limits remain unchanged. Finder's `.DS_Store` is ignored narrowly; actual source dirtiness still fails canonical builds.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
