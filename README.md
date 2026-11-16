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

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`. The completed-wave local/CI entry is `cargo xtask check`; its implementation and focused runner tests exist, but no actual full check or hosted CI run has occurred. Controlled macOS acceptance remains a separate pending gate; the CLI does not implement `verify-macos`.

A previous local bundle compiled, signed, and verified, but is stale and predates safe UI Closed consumption. Final UI/log integration, a fresh final-source build, actual GUI/MCP use, and installation/release remain unverified. Screensaver inhibition is unverified, not disproved; natural idle, manual/security/forced-sleep preservation, macOS 13 runtime, and authorized telemetry/bug-report configuration still require evidence.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
