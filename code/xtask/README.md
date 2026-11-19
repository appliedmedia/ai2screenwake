<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Worktree-local tooling

## Completed-wave regression check

```sh
cargo xtask check
```

Run this full check only after the active wave's implementation and affected checks are complete, not at every source checkpoint. The workflow retains the same entry/flags, but current delivery is LOCAL ONLY: workflow `420435149` remains disabled and is never enabled, triggered or awaited. The check requires a clean committed arm64 macOS checkout, actual login-session/native resources, and native build tools. Its complete budget is 300 seconds including preparation, feature/lint checks, families and release build. Development sanity selection is at most ten seconds including setup; existing actual asset/build/check harness guards remain unchanged, not a blanket ten-second full-IPC-family requirement. Missing resources, failed stages, zero selection, stale executables or deadline expiry fail explicitly; no gate is skipped.

The runner discovers every integration-test target and its required features using Cargo metadata, including newly added history/replay families. It requires exactly the backend/client/native runtime packages plus tooling-only xtask, empty backend/client defaults, and the maintained foundation/runtime/IPC/MCP/UI/native/assets/build/check families. Default, `ffi`, `mcp`, and tooling-only `bindgen` profiles stay separate. Formatting, deliberate-profile clippy, backend empty-default compilation, and normal-dependency graph checks reject MCP/server/schema SDK or bindgen leakage into default/FFI consumers. The frozen license blob is verified unchanged.

Preparation compiles each package/feature group's targets once with Cargo's JSON artifact output. Every test executable and supporting binary is retained with source SHA, feature profile, absolute target path, hash, and mode in `target/check-evidence/<attempt>-prepared.json`. Both check/build executors preserve an explicit caller-selected working directory and default to the worktree root only when none was supplied. The logged command describes that resolved directory before the unchanged private supervisor executes it. Test families therefore retain their owning package directory and relative feature paths; short private per-family temporary roots keep socket paths within Darwin limits.

Independent host families run in three concurrent lanes when CPU capacity allows, with one scenario worker per family to avoid multiplying concurrency. A lower lane count records the host CPU constraint. The CPU-bound asset family runs separately with its existing three scenario workers and ten-second budget. Native and generated-host UI targets each have disjoint selections: deterministic `not @real_host` cases stay in the host lanes, while every `@real_host` selection runs separately and serially because its IOKit assertions, NativeHost/IPC admission, and registration belong to the current login session. Each executable is still prepared only once. No forced sleep, screensaver trigger, input simulation, settings change, installation, or additional permission is used.

Guard scenarios use controlled collaborators and never-posted CGEvents; the optional pass-through tap probe is not part of this check. Actual-host owner fixtures explicitly supply real IOKit/clocks/lifecycle with an absent corner collaborator via Rust-only `NativeHost::with_native`, not a path-based bypass. A separate actual-IOKit scenario supplies explicitly denied controlled corner permission and verifies both real assertion IDs before release. No helper installs a global filter or requests the user's grant. Production retains real native admission and optional corner capability independently of basic On. Regression checks are not signed-app/Dock or physical-input acceptance.

The final stage invokes the existing canonical release builder against the same committed source, including generated bindings, optimized production Swift, the separate non-UI AppKit support harness, signed bundle checks, and discovery-only packaged MCP smoke. This is not a second bundler. Every tracked source/lockfile identity is checked again afterward.

Every preparation, command, family, failure, and total records machine-readable start/end/elapsed time, SHA, configuration, worker count, command, actual exit status, observed scenario count, and log paths under `target/check-evidence/`. Counts come from exactly one positive wholly passing Cucumber summary, never feature-file line estimates. The shared five-minute deadline bounds all stages and retains exact owned-descendant cleanup. An exclusive worktree lock prevents overlapping canonical checks. Report the external wall time of literal `cargo xtask check` separately from the mechanical runner total: initial Cargo bootstrap may add time, never disappear from the complete five-minute acceptance requirement. The success summary's same-run host-family duration sum is not a historical serial benchmark.

G1 controlled idle/screensaver behavior, the real GUI/MCP golden path, the macOS/architecture support matrix, audit, and release remain separate acceptance gates. A mechanically passing regression check does not prove product delivery.

```sh
cargo test --locked -p xtask --test check --no-run
cargo test --locked -p xtask --test check -- --concurrency 3 --color never
```

These twelve focused executable requirements include four real check/build executor cases: explicit package and default-root directories must read distinct relative feature files and log the actual directory. The old executors compiled and failed all four cases, without a zero-selected harness or fabricated passing summary. Source-byte/HEAD preparation races, argument/profile/status/zero-selection/evidence/serial-partition assertions remain. These cases never invoke the full check; preparation is measured separately and the family retains its ten-second guard.

Hosted-runner discovery on 2026-11-15 verified GitHub's [standard-runner reference](<https://docs.github.com/en/actions/reference/runners/github-hosted-runners>) and official [xcode-27 ARM64 image inventory](<https://raw.githubusercontent.com/actions/runner-images/main/images/macos/xcode-27-arm64-Readme.md>): the public standard `xcode-27` label has three M1 CPUs and the listed image includes macOS 27.0.1, Xcode 27, Rust 1.99.1, clippy, and rustfmt. It is a public-preview image, not a macOS 13 runtime witness. The [workflow](<../../.github/workflows/check.yml>) runs only the identical `cargo xtask check`, with official checkout/upload actions and no extra installer, secret, altered test flag, or deployment. Push triggers are restricted to main; the delivery PR and an explicitly requested manual dispatch can also run it. Failed and successful stage/build evidence is uploaded. Its existing fifteen-minute job timeout accommodates checkout/evidence upload but does not relax the complete check's five-minute acceptance limit. Native/login-session failures remain mandatory failures, never skipped gates; an image inventory is not hosted success.

## Native application build

```sh
cargo xtask build --profile release
```

Run from the exact clean committed arm64 macOS checkout. The sole application is `target/ai2sw/ai2sw.app`: executable `Contents/MacOS/ai2sw`, unchanged `ai2screenwake-mcp`, the producer's plist, thirteen artwork resources, unchanged license and sealed provenance. Its display name is AI2ScreenWake; bundle ID `com.appliedmedia.ScreenWake`, source folders and UniFFI filenames stay stable. Missing inputs fail before compilation. Release/arm64/macOS 13.0 are fixed; the deployment target is not a backward-runtime witness. The nineteen signed artifact records include six independently rasterized preview PNGs. The shared `build::bundle_paths` inventory also gates completed-check publication.

The existing exclusive same-root publication lock now also protects daily version selection. `build/version.rs` captures host-local time once through Rust/libc `localtime_r`, validates the actual calendar, then allocates in memory from the strictly verified owned signed predecessor. Same-day successful publication increments revision; a newer local day or recognized legacy `0.1.0`/`1` metadata starts at 1. A backward local date, invalid/mismatched provenance, malformed calendar or revision beyond native 9999 fails explicitly. Only successful atomic publication advances the next invocation; failures preserve the previous app/version, and each worktree has an independent history.

One frozen `BuildVersion` supplies `vYYYY.MM.DD(rev)` plus a source/configuration/version stamp to every feature-isolated Cargo invocation. Tracked `option_env!` inputs force rebuild on a same-SHA revision change. Snapshot capture checks actual producer bytes before replacement, consumers recheck hashes/stamps, and actual bindgen/native `--build-info` reports must match. MCP initialization must advertise that same frozen version. The plist template is expanded only into staging with date `CFBundleShortVersionString` and revision `CFBundleVersion`; signed provenance and external receipt carry the same value. Nothing edits tracked source during build. Noncanonical Cargo output explicitly reports `Development`, never an invented publication date.

`@daily_version` in the existing executable build family covers legacy-to-date first publication, same-day increment/new-day reset, single-invocation producer/plist consistency, failed publication without revision consumption, successful atomic advancement, malformed/future/overflowing predecessor refusal and same-source wrong-version producer rejection. Fixtures create their own tiny signed apps, not copies of main or the user app. The assembled normal check runs only after integrated source/test/documentation commits; final immutable-head validation remains a later review-stage command. Same-day worktree publications may advance their own revision without consuming main's first new-format revision.

Three feature-isolated Cargo producers share only this checkout's `target` compiler cache. Each runs with explicit `--no-default-features`, feature/profile/triple flags and actual Cargo JSON completion/artifact validation. Verified bytes are captured before switching features: FFI `.a`/`.dylib` at `target/ai2sw/ffi-target/aarch64-apple-darwin/release/`, bindgen at `target/ai2sw/bindgen-tool/debug/uniffi-bindgen`, MCP at `target/ai2sw/mcp-target/aarch64-apple-darwin/release/ai2screenwake-mcp`. These are verified snapshots, not independent caches. Source/manifest/feature/profile/target/SHA/hash/mode checks reject stale or mismatched producers at capture, use and publication; an exclusive worktree lock prevents overlapping builds.

UniFFI generates unchanged `screenwake.swift`, `screenwakeFFI.h` and `screenwakeFFI.modulemap` under `target/ai2sw/generated`. Production Swift compiles only generated bindings, `main.swift` and `MenuBarController.swift` with `-warnings-as-errors -O`, captured static FFI and system frameworks. Separate `target/ai2sw/contracts/ScreenWakeContracts` adds the support source/define and runs four nonpresenting AppKit groups; neither enters the app. Once FFI/bindings/assets exist, Swift and MCP can overlap on hosts with at least three CPUs, reserving one for Swift; smaller hosts stay serial. Only one Cargo producer writes the cache. Both slices finish before assembly; failures cancel and join exact owned compiler trees. Child-only Rust flags, release stripping workaround and explicit arm64 deployment flags remain unchanged.

Artwork is regenerated and strictly checked through the existing pure-Rust exporter at the same source SHA. Packaging validates the actual plist, executable modes, arm64/minimum-macOS load commands, and system-only dependencies. It writes internal source/toolchain/command/input/payload provenance before ad-hoc signing the MCP binary and app, then verifies signatures. A bounded packaged MCP discovery/EOF smoke proves exactly `is_on`, `turn_on`, and `turn_off` without calling controls, launching the app, or enabling protection. This local ad-hoc bundle is not installed, notarized, distributed, or evidence of actual menu/idle/screensaver behavior.

Every compiler/in-process stage records start/finish/elapsed time, SHA/configuration, worker count, command/status/errors in `target/ai2sw/build-evidence/timing.jsonl`. The existing five-minute no-progress/twenty-minute standalone limit and stricter 300-second full-check limit remain. Private POSIX sessions and descriptor-controlled supervisors pin exact ownership across reparenting/new groups; nested lifetime EOF and caller-loss cancellation prevent escaped writers. Only verified owned members are stopped; foreign/Orca/caller sessions stay untouched, and inspection/lifetime failures fail explicitly within the existing two-second cleanup bound. `<attempt>-bundle.json` records final signed hashes externally. Only complete current-source output reaches `BUILD_OK`.

Staging is worktree-local and must have the strict current nineteen-file inventory. An existing destination may have that inventory or the specific preceding fifteen-file inventory lacking only the four Error PNGs, never unknown extra files. Before atomic exchange, verify same-worktree release provenance, complete unsigned-payload path identity, owned regular single-link files, unchanged non-executable payload hashes/modes and strict deep signatures for the signed executables/resources. Failures preserve the whole previous app and unrelated output. Executable regressions build their own tiny signed Mach-O fixture bundles and leave the predecessor present during old-to-new/current-to-current publication; extra/tampered/foreign predecessors are refused. No main app is used as a fixture. Generated files/logs never enter Git. The command does not launch, install or release an app.

```sh
cargo test --locked -p xtask --test build --no-run
cargo test --locked -p xtask --test build -- --concurrency 3 --color never
```

The maintained executable build scenarios retain three workers and the ten-second guard. Descriptor-backed readiness has a separate two-second fixture bound before the injected 100-ms failure window, including nested cases; failed startup never blindly reads a missing child PID. Existing feature-cache/provenance/overlap and preservation assertions remain. Added real-process cases cover early parent exit with an orphaned new-group writer, nested failure, caller death/EOF, and an actual Cargo-to-nested-stage exec chain. Red fixtures stop their exact controlled surviving writer. Synthetic producer receipts prove only file/provenance boundaries, not real compiler or app acceptance.

Historical hosted run `43931646264` passed assets in 7.781204541 seconds but reached its then-600-second full-check deadline at 600.140862333 seconds during the bundle. FFI/bindgen/MCP took 28.775768208/62.810815583/78.016664292 seconds; 196.383821792 seconds was the bundle's remaining window. Retained `target/cold-performance/` evidence records modest same-host cold-cache improvement, 47.452665210 to 46.106899542 seconds, and the separate Swift/MCP overlap correction. No profile/dependency/check weakening was adopted. Current snapshots hash actual origin and temporary copy, compare bytes/mode/hash, then atomically publish and recheck at consumption. These are historical performance receipts, not live hosted-CI instructions: current delivery uses only canonical local validation within 300 seconds, and the disabled workflow is never triggered or awaited.

## Native artwork assets

```sh
cargo xtask assets
cargo xtask assets --check
```

`assets` renders only `code/macos/ScreenWake/Resources/Artwork/{app.svg,menu-off.svg,menu-on.svg,menu-error.svg}` using pinned Rust resvg/usvg/tiny-skia and PNG-only ICNS encoding. No external renderer, system font lookup, raster import, network resource, or desktop window is used. Inputs must retain their 1024 x 1024 app or 24 x 24 menu viewBoxes. Invalid XML, unsupported/external content, wrong dimensions, empty artwork, colored menu pixels, encoding/write failures, or changing source/commit identity fail explicitly.

The sole output is `target/ai2sw/assets/`. Native bundling consumes these exact paths:

* `app.icns`: ten PNG-backed native representations, preserving logical size and 1x/2x density.
* `app.iconset/icon_NxN.png` and `app.iconset/icon_NxN@2x.png`, where `N` is 16, 32, 128, 256, or 512. Physical sizes are `N` and `2 * N` pixels; the largest is 1024 x 1024.
* `menu-{off,on,error}.png`: 24 x 24 pixels at 1x.
* `menu-{off,on,error}@2x.png`: 48 x 48 pixels at 2x, retaining a 24 x 24 logical size.
* `preview-{off,on,error}.png`: 96 x 96 pixels, independently rendered from the corresponding menu master.
* `preview-{off,on,error}@2x.png`: 192 x 192 pixels at 2x, retaining a 96-point logical size.
* `manifest.json`: schema version, Git SHA, target/platform, renderer settings, four source hashes/viewBoxes and exactly twenty-three output hashes/dimensions/logical sizes/scales. ICNS has ten representations; the manifest does not hash itself.

Menu PNGs are black with alpha and transparent negative space for native template tinting, never a white background. Generation rejects unrecognized existing outputs before rendering, then uses a private worktree-local staging directory, decodes and checks every PNG and ICNS representation, rechecks source identity, and checks replacement eligibility again before publication. A failed replacement retains the previous complete assets; `--check` still rejects them if source or commit changed. Publication may briefly make the directory unavailable, but never exposes partial new assets. Unknown extra files or symlinks are rejected rather than discarded.

`--check` never regenerates: missing/extra outputs, stale source/commit/settings, malformed manifests, wrong dimensions/checksums or different decoded ICNS fail closed. Run it immediately before consumption. Every export/check appends stage/total timing, status, SHA, command and count to `target/ai2sw/assets-evidence/timing.jsonl`. Generated assets/evidence remain ignored. The exporter is not app acceptance.

Focused executable Cucumber requirements use isolated worktree-local fixtures and three concurrent scenario workers:

Fixture commits pass command-local `-c commit.gpgSign=false`; tests inject signing-enabled configuration with a failing signer only into those fixture commands. No host, global, or repository signing configuration is changed.

```sh
cargo test --locked -p xtask --test assets --no-run
cargo test --locked -p xtask --test assets -- --concurrency 3 --color never
```

The eighteen asset scenarios retain prior positive/negative, provenance and publication assertions. `@antenna_fit` checks the full transformed round stroke against the cabinet top stroke's lower edge, rejects even low-alpha interior leakage, and compares actual decoded exports to a same-source antenna-free reference at 24/48/96/192 pixels. All faces share the same antenna raster. `ShortRaysWithoutAntennaBase` uses the resolved usvg stroke and effective local/ancestor transform in its isolated ray raster and checks both sides of the mount, excluding approved ray/join pixels. Equivalent local/ancestor translations must match actual full-export pixels and satisfy containment at every density; left/right horizontal-base mutants must fail. Fixtures derive their translated paths and mutants from actual source geometry, not a frozen path string. Mutants exist only in isolated test strings, never shipped SVGs. Low-alpha antialiasing is not a solid horizontal base but must still stay out of the cabinet interior. Existing eye/pupil/mouth/border/selector/sleep-clearance checks, immutable setup/independent fixtures, three workers and ten-second asset guard remain.

Hosted PR run `43924424220` at input `e14a071` passed all nine scenarios/38 steps but failed the existing ten-second guard at 10.692560708 seconds. Same-host debug evidence for the correction is under `target/asset-performance/`: unchanged preparation 2.40 seconds and family wall 6.276129625 seconds, versus changed preparation 0.97 seconds and family wall 4.900053375 seconds, a 21.93% reduction. Internal guarded duration fell from 5.710405 to 4.383807 seconds. An owned-process sample identified repeated PNG/ICNS compression/decoding; fixture reuse avoids redundant baseline exports and hash-verifies identical copies rather than repeatedly decoding proven setup. Early unknown-output rejection also avoids rendering a replacement that cannot be published, without removing the publication-time recheck. Initial shared-setup experiments and their lesser reductions remain recorded, not substituted for the final measurement.

Compilation is separate preparation; all asset setup, identity checks and cleanup remain inside the unchanged ten-second asset guard. The preceding hosted asset failure and same-host measurements are historical, not current delivery gates. No dependency/profile, worker-count, full-suite budget, artwork or pixel assertion was weakened. The coordinator uses committed-head canonical local validation only; no new hosted run, permission or review is requested. These are asset-boundary checks, not full product acceptance or screenshot-baseline comparison.

## Foundation checks

Run from the exact checkout root:

```sh
cargo xtask prepare
cargo xtask sanity
```

`prepare` captures the source fingerprint and HEAD before Cargo, compiles the focused target with `--locked --no-run`, then rechecks both immediately before publishing its exact executable/hash receipt. Any source or HEAD change rejects publication. `sanity` refuses missing/stale/cross-worktree artifacts and runs the prepared executable with three independent scenario workers. Its ten-second budget still includes setup; no app, native OS resource or full suite is run.

The runner must report the exact positive count of explicit foundation scenarios as passed; zero selection, skipped/failed steps, timeout, malformed output, wrapper failure or provenance mismatch fails. It never silently rebuilds or accepts another checkout's binary. Re-run preparation after a source or HEAD change.

Each invocation appends stage and total JSON rows under `target/foundation-evidence/timing.jsonl`, including a unique attempt, UTC Unix-millisecond start/finish, elapsed seconds, commit, environment/configuration, worker count, scenario count, exit status, command and failure message. Attempt-specific preparation and sanity logs are retained alongside it; earlier failures are not overwritten. The preparation worker count denotes one Cargo driver, whose dependency compilation uses Cargo's normal parallelism.

This is Rust tooling invoking Cargo/the built test executable, not a shell/Node wrapper. Actual controlled `verify-macos` product acceptance is not implemented here.

<!-- end README.md -->
