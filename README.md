<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# ScreenWake (ai2screenwake)

ScreenWake is a macOS idle-protection utility under development. This checkout contains Rust/Swift host source, local MCP controls, original artwork, and native build tooling, not a released application. See the [project plan](<_orch.md>) for delivery gates.

## Controls and architecture

The menu-bar host starts Off. Primary click requests a toggle; only confirmed observations change the TV's closed-eyes/zzZ or wide-open-eyes state. Right-click or long press opens Status, Show Log, and Quit without toggling on release. Status includes truthful Pending/Error states, the auxiliary log is read-only, and normal Quit requires confirmed resource cleanup and owner disposal.

Three Rust runtime crates provide backend contracts, client/owner policy, and native OS access. Commands, observations, and errors use Actions; generated UniFFI connects minimal Swift/AppKit rendering. Native UI and MCP share one login-session owner, not separate power assertions. Bounded durable history rebuilds diagnostics after restart, never live On intent or assertions.

## Local build

Build from a clean, committed checkout on an arm64 Mac with Rust, Xcode's command-line tools/macOS SDK, `xcrun`, `codesign`, and `plutil`. MCP/bindgen dependencies require Rust 1.88 or later; the verified toolchain is Rust 1.97.1 and Swift 6.4.2 on macOS 27.0.1. The arm64/macOS 13 deployment target is not a macOS 13 runtime or Intel/universal compatibility claim.

```sh
cargo xtask build --profile release
```

The sole app output is `target/screenwake/ScreenWake.app`, containing the production host, MCP executable, native assets, and license; UniFFI bindings are generated during compilation. The builder checks source/artifact identity, ad-hoc signs and verifies the bundle, and performs discovery-only MCP smoke without launching the GUI or enabling protection. It does not install, notarize, or release the app. See [build and check details](<code/xtask/README.md>).

## Local MCP

Set your MCP client's stdio command to the absolute path of `target/screenwake/ScreenWake.app/Contents/MacOS/ai2screenwake-mcp` in the built checkout, with no arguments. The bundled executable implements `initialize`, `tools/list`, and `tools/call`; its only tools are `is_on`, `turn_on`, and `turn_off`, each accepting an empty object. Show Log and Quit are native UI only.

Discovery never launches or enables the owner. A control connects to the existing owner or requests launch of that exact bundled app Off; it never automatically turns protection on. An unbundled development binary returns a structured unavailable error, not false Off. Results follow observed settlement, including explicit pending/error and unavailable outcomes.

When confirmed On, status includes protected-idle duration and only proven suppressed events with `epoch_ms` and `minutes_into_protected_idle`. There is no verified native per-attempt cancellation source yet: an empty event list means none confirmed, not proof nothing was prevented. See the [MCP contract](<code/crates/ai2screenwake_client/src/mcp/README.md>).

## Validation and availability

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`; full validation runs locally with `cargo xtask check`. The most recent completed full validation, at `5067dd1`, passed once: 239 Cucumber scenarios/764 steps plus four AppKit groups, exit 0/`CHECK_OK`, in 124.96 seconds complete external wall time and 122.931975125 seconds mechanical runner time. Preparation and release build are included; the complete five-minute/300-second ceiling and every ten-second family guard remain unchanged. The earlier failed `561b3bb` working-directory attempt and older local/hosted receipts remain distinct historical evidence in the [master](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#planning-receipt-and-current-status>). No `verify-macos` command is implemented.

That `5067dd1` run produced a fresh worktree-local signed eleven-artifact app. Nineteen safe serial actual-app native/MCP stages passed in 2.841206541 seconds, covering packaged shared control, owner error/history, restart Off and ordinary Quit/resource cleanup; owned resources were cleaned up and signed artifact hashes stayed unchanged. These are nineteen stages, not Cucumber scenarios or full G1/G2 acceptance. Physical right-click/long-press/log-window, natural idle/screensaver efficacy, manual/security/forced-sleep behavior, macOS 13 runtime/support, a native cancellation stream and installation/release remain unverified; ScreenWake foreground consent remains unanswered.

CI is local-only by AndrewC's explicit instruction. The parent verified GitHub's Canonical check workflow `420435149` as `disabled_manually`, with no running or queued runs. Historical hosted failures are not required delivery/merge gates; local checks and real acceptance gates remain unchanged. Later commits require their own final local validation; the completed `5067dd1` record does not validate this documentation change.

The one implementation triad/fix completed at `e14a071`. Draft implementation PR #2 already exists; continue that PR, never open a duplicate. Its original CodeRabbit observation closed at T+10 without a substantive response at that deadline. Later substantive review `5925101981` has twenty mapped inline comments in the [complete late-source mapping](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#late-source-corrections>). The parent's latest snapshot has nineteen of twenty threads resolved; this documentation correction addresses the remaining README follow-up `4592436149`, not its GitHub thread resolution. No new code finding or merge approval is inferred; review, confirmation and the consumed local triad are not restarted.

Optional core-only telemetry and Rust UI error rendering are implemented with focused real-loopback/component evidence. The actual app bootstrap remains `Unconfigured`; no external endpoint/key/sending authorization, release feature selection or real external delivery is established.

AndrewC approved A1 validation recovery, A2 acceptance/documentation, then A3 one implementation review/fix/final-validation/delivery PR. All four completed source-leaf roots and branches are removed; only integration and bundle remain to finish their work and then be removed. Prior work must be completed or discarded, not archived; see the [master disposition ledger](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#prior-work-disposition>). No release is posted; delivery dates remain UNKNOWN.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
