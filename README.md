<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake (ai2sw)

AI2ScreenWake is a macOS idle-protection utility under development, not a released application. PRs #2 through #5 are merged; this TV-artwork/native-menu refinement starts from integrated main `f2b14f5`. See the [project plan](<_orch.md>).

## Controls and architecture

The menu-bar host starts Off. Its landscape TV has two left selector dots, a narrower inset screen and smaller left rabbit ears, with equally weighted cabinet/screen outlines. Off has closed eyes, a smile and ascending right-side Zzz with a full clear pixel above the cabinet; confirmed On has larger wide-open eyeballs, distinct pupils and one straight horizontal mouth line; Error has X X eyes and a frown, never an exclamation badge or sleep marks. Pending explicitly reports `Status: Pending` and accessible Pending while retaining the last confirmed face, without title dots or optimistic power claims. A genuine attention fault immediately selects `Status: Error`; core-owned tooltip/accessibility help retains safe error detail and independently observed protection. MCP/resource truth stays unchanged. Intentionally unconfigured or disabled telemetry is normal inactivity, not an attention fault; no sending is enabled.

Primary short click requests one toggle. Control-click, right-click or a 0.45-second primary click-and-hold opens the same native upward-pointing 172 x 224-point popover, with a matching 96-point preview, status, Show Log and Quit. Pointer opening starts on neutral content instead of highlighting Show Log; native focus rings, key-view navigation, keyboard equivalents and accessibility actions remain. A system-sized square status item replaces the 48-point-wide highlight footprint without changing the landscape TV artwork. Complete first-open sizing stays stable; cached images remain centered and are assigned only when their identity changes. Recognized hold, drag/cancellation and late primary callbacks cannot toggle on release; only a fresh pointer-down admits another primary click. Normal Quit requires confirmed resource cleanup and owner disposal.

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

Focused foundation checks use `cargo xtask prepare`, then `cargo xtask sanity`; full local validation is literal `cargo xtask check`. The completed artwork increment's red/green and twelve image inspections remain immutable in ignored `target/artwork-polish/result.json`. Its `bfd650b` check, 264 scenarios/841 steps/four AppKit groups in 125.736209 external seconds, predates the added native changes. `target/artwork-polish/native-result.json` owns the native width/focus/release regressions and complete new committed-source normal validation. Canonical full validation includes generated production Swift, four nonpresenting AppKit groups, nineteen signed files and three-tool MCP discovery. The complete 300-second ceiling, ten-second development-sanity selection and actual asset/runner guards remain unchanged; there is no blanket ten-second cap on a whole IPC family. No `verify-macos` command is implemented.

The eight-request batch's one ARCH/QUALITY/PROCESS triad is complete at source `cf4f9bf`; all three reports contain no actionable findings. Its sole consolidated-fix task updates current review documentation only, commits every intended change, then runs the required literal final check at resulting HEAD. Exact final execution and signed-artifact identity belong to `target/artwork-review/fix-result.json` and sanitized `final-public-receipt.json`. The source's 264/841/four-group normal pass in 88.060219 external seconds is pre-triad history, not validation of the later documentation commit.

That local stage completed at `94996ae`. PR #6's initial CodeRabbit review then identified missing parent creation in a private native-test fixture and historical ownership mislabeled current in Lane 3. Both are corrected together; `target/artwork-review/provider-initial-result.json` and compact `provider-initial-public.json` record actual replacement-head validation. Older source/local receipts stay unchanged; the parent owns one non-triggering confirmation and delivery.

Historical PR #5 final source `f3c7b9` passed 261 scenarios/832 steps and four AppKit groups in 85.639821 mechanical/86.152488 external seconds; main `f2b14f5` produced its nineteen-file build in 26.712465 mechanical/27.343254 external seconds. These coordinator-supplied receipts are history, not validation of the current artwork.

Historical PR #4 final source `8d44f60` passed 257 scenarios/820 steps and four AppKit groups in 93.242592 external seconds; main `ed74091` produced the nineteen-file build in 25.799644 mechanical/27.585506 external seconds. Neither is validation of this later compact-menu source.

Historical PR #3 final source `1e103d2` passed 243 scenarios/776 steps plus four AppKit groups in 82.530111 external seconds. Its integrated-main `99ffe72` fifteen-file build took 28.437910 mechanical/30.767526 external seconds, receipt `target/ai2sw/build-evidence/1794902425563-14619-bundle.json`. Neither receipt validates these later faces or inactive-forwarding correction.

Historical `5067dd1` validation produced the old eleven-file bundle and passed nineteen safe actual-app native/MCP stages in 2.841206541 seconds, covering shared control, owner error/history, restart Off and ordinary Quit/resource cleanup. Those stages are not current-source physical acceptance. Physical context-click/long-press/log-window operation, natural idle/screensaver efficacy, manual/security/forced-sleep behavior, macOS 13 runtime/support, native cancellation evidence and installation/release remain unverified. Such operations are not selected by this approved delivery batch; no foreground/input consent or new approval is inferred or requested.

CI is local-only by AndrewC's explicit instruction; workflow `420435149` remains disabled. No workflow is changed, triggered or awaited. The completed single triad is not rerun; the sole fix result and all documentation commits precede actual final immutable-head validation at `validated_final_sha`, then one `coderabbit_optional` PR and authorized delivery. No prior PR's consumed reviewer token is reused. No release or physical acceptance is claimed.

Historical implementation PR #2 is merged, with its one triad/fix and twenty later review findings addressed in the [master mapping](<docs/plans/2026-11-15_plan_todo_ScreenWake_Orch.md#late-source-corrections>). PR #3 and its delivery cleanup are also complete. Source tests never control, move or copy the current user app; their build is worktree-local. AndrewC explicitly authorized the coordinator to complete review/delivery, build from integrated main, then normally quit/relaunch that new build and check responsiveness. This worker performs none of those user-app operations and requests no additional approval.

Optional core-only telemetry and Rust UI error rendering are implemented with focused real-loopback/component evidence. The actual app bootstrap remains `Unconfigured`; no external endpoint/key/sending authorization, release feature selection or real external delivery is established.

The previous implementation/refinement roots and branches, including `compact-tv-menu`, have been removed. The active `tv-artwork-polish` batch and its receipts are recorded in the master; no release is posted and delivery dates remain UNKNOWN. Existing public-API, native-resource, binary-replay, security and consent limits remain unchanged. Finder's `.DS_Store` is ignored narrowly; actual source dirtiness still fails canonical builds.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
