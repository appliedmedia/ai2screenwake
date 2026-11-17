<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake (ai2sw)

AI2ScreenWake is a macOS idle-protection utility under development, not a released application. The original implementation merged in PR #2 at `da17921`; this naming/usability refinement has its own pending validation and delivery. See the [project plan](<_orch.md>).

## Controls and architecture

The menu-bar host starts Off. Primary click requests a toggle; only confirmed observations change the simplified TV's large closed or wide-open eyes. Control-click, right-click or long press opens one native upward-pointing popover below the status icon, with a crisp large state preview, truthful On/Off/Pending/Error status, Show Log and Quit. Its fixed layout is filled before the first open; secondary and long-press release never toggle. Normal Quit requires confirmed resource cleanup and owner disposal.

Show Log regenerates a bounded, private `ai2sw.log` snapshot beside the existing binary journal in macOS's per-user temporary root under `ai2screenwake-<login-session>/`. The first line and read-only viewer show the actual full absolute `.log` path. It contains at most 200 recent action labels, not protected payloads; it updates only on Show Log, not by continuous tailing. Atomic replacement preserves the previous snapshot on failure; errors are bus-observed. The checksummed binary journal remains the sole replay source.

Three Rust runtime crates provide backend contracts, client/owner policy, and native OS access. Commands, observations, and errors use Actions; generated UniFFI connects minimal Swift/AppKit rendering. Native UI and MCP share one login-session owner, not separate power assertions. Bounded durable history rebuilds diagnostics after restart, never live On intent or assertions.

## Local build

Build from a clean, committed checkout on an arm64 Mac with Rust, Xcode's command-line tools/macOS SDK, `xcrun`, `codesign`, and `plutil`. MCP/bindgen dependencies require Rust 1.88 or later; the verified toolchain is Rust 1.97.1 and Swift 6.4.2 on macOS 27.0.1. The arm64/macOS 13 deployment target is not a macOS 13 runtime or Intel/universal compatibility claim.

```sh
cargo xtask build --profile release
```

The sole app output is `target/ai2sw/ai2sw.app`, containing executable `ai2sw`, the unchanged MCP executable, nine artwork resources and license. Its display name is `AI2ScreenWake`; the stable bundle identifier remains `com.appliedmedia.ScreenWake`, so renaming does not migrate private owner/history identity. Generated bindings, feature snapshots, assets and build evidence share `target/ai2sw/`. The builder verifies current-source provenance, ad-hoc signatures and discovery-only MCP without launching or enabling the app. It does not install, notarize or release it. See [build and check details](<code/xtask/README.md>).

## Local MCP

Set your MCP client's stdio command to the absolute path of `target/ai2sw/ai2sw.app/Contents/MacOS/ai2screenwake-mcp` in the built checkout, with no arguments. The bundled executable implements `initialize`, `tools/list`, and `tools/call`; its only tools are `is_on`, `turn_on`, and `turn_off`, each accepting an empty object. Show Log and Quit are native UI only.

Discovery never launches or enables the owner. A control connects to the existing owner or requests launch of that exact bundled app Off; it never automatically turns protection on. An unbundled development binary returns a structured unavailable error, not false Off. Results follow observed settlement, including explicit pending/error and unavailable outcomes.

When confirmed On, status includes protected-idle duration and only proven suppressed events with `epoch_ms` and `minutes_into_protected_idle`. There is no verified native per-attempt cancellation source yet: an empty event list means none confirmed, not proof nothing was prevented. See the [MCP contract](<code/crates/ai2screenwake_client/src/mcp/README.md>).

## Validation and availability

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`; full validation runs locally with `cargo xtask check`. The most recent completed full validation, at `5067dd1`, passed once: 239 Cucumber scenarios/764 steps plus four AppKit groups, exit 0/`CHECK_OK`, in 124.96 seconds complete external wall time and 122.931975125 seconds mechanical runner time. Preparation and release build are included; the complete five-minute/300-second ceiling and every ten-second family guard remain unchanged. The earlier failed `561b3bb` working-directory attempt and older local/hosted receipts remain distinct historical evidence in the [master](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#planning-receipt-and-current-status>). No `verify-macos` command is implemented.

That `5067dd1` run produced a fresh worktree-local signed eleven-artifact app. Nineteen safe serial actual-app native/MCP stages passed in 2.841206541 seconds, covering packaged shared control, owner error/history, restart Off and ordinary Quit/resource cleanup; owned resources were cleaned up and signed artifact hashes stayed unchanged. These are nineteen stages, not Cucumber scenarios or full G1/G2 acceptance. Physical right-click/long-press/log-window, natural idle/screensaver efficacy, manual/security/forced-sleep behavior, macOS 13 runtime/support, a native cancellation stream and installation/release remain unverified; ScreenWake foreground consent remains unanswered.

CI is local-only by AndrewC's explicit instruction. The parent verified GitHub's Canonical check workflow `420435149` as `disabled_manually`, with no running or queued runs. Historical hosted failures are not required delivery/merge gates; local checks and real acceptance gates remain unchanged. Later commits require their own final local validation; the completed `5067dd1` record does not validate this documentation change.

Historical implementation PR #2 is merged, with its one triad/fix and twenty later review findings addressed in the [master mapping](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#late-source-corrections>). This approved naming/artwork/popover/log change is a new batch, not a restart of that consumed review. The previous artifact is not moved, copied, launched or stopped; delivery rebuilds the new path from integrated main after this batch's review/PR.

Optional core-only telemetry and Rust UI error rendering are implemented with focused real-loopback/component evidence. The actual app bootstrap remains `Unconfigured`; no external endpoint/key/sending authorization, release feature selection or real external delivery is established.

The previous implementation roots/branches have been removed. The active approved refinement and its validation receipts are recorded in the master; no release is posted and delivery dates remain UNKNOWN. Existing public-API, native-resource, binary-replay, security and consent limits remain unchanged. Finder's `.DS_Store` is ignored narrowly so ordinary macOS browsing does not invalidate clean-source builds; actual source dirtiness still fails.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
