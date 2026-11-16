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

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`. The completed-wave local/CI entry is `cargo xtask check`. Exact source `5cc3977` passed 185 Cucumber scenarios/605 steps plus four AppKit groups, exit 0 in 78.36 seconds (79.01 seconds wrapper wall), and produced a fresh signed eleven-artifact release bundle. A1 validation recovery is complete; the older `d8f3d63` failed attempt remains historical, not the current gate. No hosted CI run is established. Controlled macOS acceptance is separate, and the CLI does not implement `verify-macos`.

The historical source-227 bundle supplied actual two-packaged-client/native AX/IOKit shared-control, ordinary Quit, restart Off and cleanup evidence. Source-5cc compilation/signing, discovery smoke and non-presenting AppKit checks do not prove current visible-app acceptance or remaining physical right-click/long-press/log-window interactions. A2 safe actual-app acceptance is separately assigned; full G1/G2, natural idle/screensaver efficacy, manual/security/forced-sleep behavior, macOS 13 runtime and installation/release remain unverified.

Optional core-only telemetry and Rust UI error rendering are implemented with focused real-loopback/component evidence. The actual app bootstrap remains `Unconfigured`; no external endpoint/key/sending authorization, release feature selection or real external delivery is established.

AndrewC approved A1 validation recovery, A2 acceptance/documentation, then A3 one implementation review/fix/final-validation/delivery PR. All four completed source-leaf roots and branches are removed; only integration and bundle remain to finish their work and then be removed. Prior work must be completed or discarded, not archived; see the [master disposition ledger](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#prior-work-disposition>). No release is posted; delivery dates remain UNKNOWN.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
