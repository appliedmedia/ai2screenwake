<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Worktree-local tooling

## Completed-wave regression check

```sh
cargo xtask check
```

Run this full check only after the active wave's implementation and affected checks are complete, not at every source checkpoint. The entry and flags are identical locally and in CI. It requires a clean committed arm64 macOS checkout, actual login-session/native resources, and the native build tools. A missing resource, failed stage, zero-selected family, stale executable, or ten-minute deadline fails explicitly; no platform or dependency fallback skips a required gate.

The runner discovers every integration-test target and its required features using Cargo metadata, including newly added history/replay families. It requires exactly the backend/client/native runtime packages plus tooling-only xtask, empty backend/client defaults, and the maintained foundation/runtime/IPC/MCP/UI/native/assets/build/check families. Default, `ffi`, `mcp`, and tooling-only `bindgen` profiles stay separate. Formatting, deliberate-profile clippy, backend empty-default compilation, and normal-dependency graph checks reject MCP/server/schema SDK or bindgen leakage into default/FFI consumers. The frozen license blob is verified unchanged.

Preparation compiles each package/feature group's targets once with Cargo's JSON artifact output. Every test executable and supporting binary is retained with source SHA, feature profile, absolute target path, hash, and mode in `target/check-evidence/<attempt>-prepared.json`. Tests run from their actual package directory after provenance verification; short private per-family temporary roots keep socket paths within Darwin limits.

Independent host families run in three concurrent lanes when CPU capacity allows, with one scenario worker per family to avoid multiplying concurrency. A lower lane count records the host CPU constraint. The CPU-bound asset family runs separately with its existing three scenario workers and ten-second budget. Native and generated-host UI targets each have disjoint selections: deterministic `not @real_host` cases stay in the host lanes, while every `@real_host` selection runs separately and serially because its IOKit assertions, NativeHost/IPC admission, and registration belong to the current login session. Each executable is still prepared only once. No forced sleep, screensaver trigger, input simulation, settings change, installation, or additional permission is used.

The final stage invokes the existing canonical release builder against the same committed source, including generated bindings, optimized production Swift, the separate non-UI AppKit support harness, signed bundle checks, and discovery-only packaged MCP smoke. This is not a second bundler. Every tracked source/lockfile identity is checked again afterward.

Every preparation, command, family, failure, and total records machine-readable start/end/elapsed time, SHA, configuration, worker count, command, actual exit status, observed scenario count, and log paths under `target/check-evidence/`. Counts come from exactly one positive wholly passing Cucumber summary, never feature-file line estimates. Child process groups are limited to that stage's descendants and stopped at the shared ten-minute deadline or five minutes without output. An exclusive worktree lock prevents overlapping canonical checks. The success summary records measured parallel-host wall time and the sum of those same observed family durations; no prior full-suite baseline exists, and that sum is not presented as a historical serial benchmark. Earlier isolated observations were approximately 1.2 seconds for foundation, 5.8 seconds for assets, and 13 seconds for a warm bundle; they do not establish an old full-suite duration.

G1 controlled idle/screensaver behavior, the real GUI/MCP golden path, the macOS/architecture support matrix, audit, and release remain separate acceptance gates. A mechanically passing regression check does not prove product delivery.

```sh
cargo test --locked -p xtask --test check --no-run
cargo test --locked -p xtask --test check -- --concurrency 3 --color never
```

These six focused executable requirements exercise argument rejection, newly discovered target/profile planning, exact failed-stage status, zero-selection rejection, concurrent output/evidence isolation, and disjoint serial real-host UI selection. They never invoke the full check. Compilation is separate preparation; the focused run must finish within ten seconds.

Hosted-runner discovery on 2026-11-15 verified GitHub's [standard-runner reference](<https://docs.github.com/en/actions/reference/runners/github-hosted-runners>) and official [xcode-27 ARM64 image inventory](<https://raw.githubusercontent.com/actions/runner-images/main/images/macos/xcode-27-arm64-Readme.md>): the public standard `xcode-27` label has three M1 CPUs and the listed image includes macOS 27.0.1, Xcode 27, Rust 1.99.1, clippy, and rustfmt. It is a public-preview image, not a macOS 13 runtime witness. The [workflow](<../../.github/workflows/check.yml>) runs only the identical `cargo xtask check`, with official checkout/upload actions and no extra installer, secret, altered test flag, or deployment. Push triggers are restricted to main; the delivery PR and an explicitly requested manual dispatch can also run it. Failed and successful stage/build evidence is uploaded. The fifteen-minute job limit leaves bootstrap/evidence-upload room around the runner's strict ten-minute regression budget. Documentation does not certify the project's IOKit/login-session calls; those remain mandatory named real-host failures if unavailable, never skipped gates, and no successful hosted run is claimed from an image inventory.

## Native application build

```sh
cargo xtask build --profile release
```

Run from the exact clean, committed checkout root on arm64 macOS. The sole application output is `target/screenwake/ScreenWake.app`: `Contents/MacOS/ScreenWake`, the real `Contents/MacOS/ai2screenwake-mcp`, the producer's `Info.plist`, five canonical native artwork resources, the unchanged license, and sealed build provenance. Missing native producer sources fail by name before compilation; the command never substitutes a probe/test app or silently skips an unfinished stage. The profile is fixed to release, arm64, and macOS 13.0; this is a deployment target, not a backward-runtime compatibility claim.

The Rust pipeline builds the client's `ffi` library into `target/screenwake/ffi-target`, builds the debug-host `bindgen` tool separately into `target/screenwake/bindgen-tool`, and generates `screenwake.swift`, `screenwakeFFI.h`, and `screenwakeFFI.modulemap` under `target/screenwake/generated`. UniFFI generation uses the actual production dylib as metadata input. Swift compiles only the generated binding plus hand-authored `main.swift` and `MenuBarController.swift`, with `-warnings-as-errors -O`, no conditional define, and static linkage to the production `.a` and system frameworks. A separate `target/screenwake/contracts/ScreenWakeContracts` adds `MenuBarContractTests.swift` and `-D SCREENWAKE_CONTRACT_TESTS`, then runs its non-UI renderer/input-sink assertions against the current canonical assets; neither that executable nor the test define enters the app. The real three-tool MCP binary builds with `mcp` in `target/screenwake/mcp-target`, without overwriting the FFI library. Global Rust flags are removed only from child Cargo commands; release stripping is disabled there to avoid malformed host proc-macro dylibs with Xcode 27, and the explicit deployment link flag applies only to the arm64 target.

Artwork is regenerated and strictly checked through the existing pure-Rust exporter at the same source SHA. Packaging validates the actual plist, executable modes, arm64/minimum-macOS load commands, and system-only dependencies. It writes internal source/toolchain/command/input/payload provenance before ad-hoc signing the MCP binary and app, then verifies signatures. A bounded packaged MCP discovery/EOF smoke proves exactly `is_on`, `turn_on`, and `turn_off` without calling controls, launching the app, or enabling protection. This local ad-hoc bundle is not installed, notarized, distributed, or evidence of actual menu/idle/screensaver behavior.

Every compiler and in-process stage records start/finish/elapsed time, commit, configuration, worker count, command, status, and errors in `target/screenwake/build-evidence/timing.jsonl`. Attempt-specific stdout/stderr remain observable during compilation; five minutes without new output or the absolute twenty-minute command limit fails the command. Each build/check stage creates its own process group. Failed commands share owned-tree cleanup: suspend the exact group, inspect PID/parent/group relationships, suspend verified descendant groups and stop them together, including nested compiler groups on an outer timeout or descendants after direct-parent exit. Direct-child reaping and every owned group's absence from the process table must complete within two seconds or cleanup itself fails explicitly. Inspection errors never authorize foreign-group control or successful cleanup. Compiler/linker stages are serial because generated inputs and sealed identity are dependent. The external `<attempt>-bundle.json` receipt records final signed-file hashes without a self-referential hash inside the signature. Only a complete current-source bundle reaches `BUILD_OK`; source/target symlinks, stale inputs, unexpected files, missing tools, compiler failures, or malformed artifacts fail explicitly.

Staging is worktree-local. Publication uses the macOS atomic directory-exchange primitive when replacing an existing recognized bundle; failed preflight/compilation/verification leaves the previous complete app and unrelated output untouched. Generated files/logs never enter Git. The command does not change settings, launch the GUI, install an app, run an idle witness, or post a release.

```sh
cargo test --locked -p xtask --test build --no-run
cargo test --locked -p xtask --test build -- --concurrency 3 --color never
```

Focused executable Cucumber requirements cover configuration rejection, missing sources, changed inputs, partial-publication preservation, failed-stage evidence, symlink rejection and owned descendant cleanup after nonzero exit, injected short timeout, nested build-under-check failure and outer timeout. A controlled child accepts a loopback write command only after the stage returns; EOF, exact PID absence and unchanged output prove cleanup without a sleep-based assertion. Preparation is measured separately; the ten isolated scenarios run with three workers within ten seconds. These boundary fixtures do not substitute for the actual canonical app build.

## Native artwork assets

```sh
cargo xtask assets
cargo xtask assets --check
```

`assets` renders only `code/macos/ScreenWake/Resources/Artwork/{app.svg,menu-off.svg,menu-on.svg}` using pinned Rust resvg/usvg/tiny-skia and PNG-only ICNS encoding. No external renderer, system font lookup, raster import, network resource, or desktop window is used. Inputs must retain their 1024 x 1024 app or 24 x 24 menu viewBoxes. Invalid XML, unsupported/external content, wrong dimensions, empty artwork, colored menu pixels, encoding/write failures, or changing source/commit identity fail explicitly.

The sole output is `target/screenwake/assets/`. Native bundling must consume these exact paths:

* `app.icns`: ten PNG-backed native representations, preserving logical size and 1x/2x density.
* `app.iconset/icon_NxN.png` and `app.iconset/icon_NxN@2x.png`, where `N` is 16, 32, 128, 256, or 512. Physical sizes are `N` and `2 * N` pixels; the largest is 1024 x 1024.
* `menu-off.png` and `menu-on.png`: 24 x 24 pixels at 1x.
* `menu-off@2x.png` and `menu-on@2x.png`: 48 x 48 pixels at 2x, retaining a 24 x 24 logical size.
* `manifest.json`: schema version, Git SHA, canonical target/platform, renderer versions/settings, all three source paths/SHA-256/viewBoxes, and exactly fifteen output paths/SHA-256/physical dimensions/logical sizes/scales. ICNS lists all ten representations; the manifest does not hash itself.

Menu PNGs are black with alpha and transparent negative space for native template tinting, never a white background. Generation rejects unrecognized existing outputs before rendering, then uses a private worktree-local staging directory, decodes and checks every PNG and ICNS representation, rechecks source identity, and checks replacement eligibility again before publication. A failed replacement retains the previous complete assets; `--check` still rejects them if source or commit changed. Publication may briefly make the directory unavailable, but never exposes partial new assets. Unknown extra files or symlinks are rejected rather than discarded.

`--check` never regenerates: missing/extra outputs, stale source/commit/settings, malformed manifest, incorrect dimensions, checksum mismatch, or an undecodable/different ICNS representation fail closed. Run this immediately before consuming assets. Every export/check attempt appends stage and total timing, exit status, commit, command, and artifact count to `target/screenwake/assets-evidence/timing.jsonl`. Generated assets and evidence are ignored, never committed. This exporter is not a finished application or bundle.

Focused executable Cucumber requirements use isolated worktree-local fixtures and three concurrent scenario workers:

```sh
cargo test --locked -p xtask --test assets --no-run
cargo test --locked -p xtask --test assets -- --concurrency 3 --color never
```

The nine scenarios/38 steps retain all positive, invalid-master, stale-source/regeneration, stale-commit, extra-output and previous-output-preservation assertions. One per-process fixture creates the complete initial export with the actual production exporter and canonical masters from this checkout. Complete-export Given steps make ordinary independent file copies, including the fixture's Git identity, then check their manifest, current commit and every source/output SHA-256 against that verified export. Mutation/regeneration assertions still use the real exporter/verifier on each private directory; no hard link, shared mutation, old-checkout artifact, fake renderer or production cache is involved. Three scenario workers share only immutable setup, generated lazily within the measured run; the seed's identities are checked afterward and its temporary directory is dropped before the performance guard.

Hosted PR run `43924424220` at input `e14a071` passed all nine scenarios/38 steps but failed the existing ten-second guard at 10.692560708 seconds. Same-host debug evidence for the correction is under `target/asset-performance/`: unchanged preparation 2.40 seconds and family wall 6.276129625 seconds, versus changed preparation 0.97 seconds and family wall 4.900053375 seconds, a 21.93% reduction. Internal guarded duration fell from 5.710405 to 4.383807 seconds. An owned-process sample identified repeated PNG/ICNS compression/decoding; fixture reuse avoids redundant baseline exports and hash-verifies identical copies rather than repeatedly decoding proven setup. Early unknown-output rejection also avoids rendering a replacement that cannot be published, without removing the publication-time recheck. Initial shared-setup experiments and their lesser reductions remain recorded, not substituted for the final measurement.

Compilation is separate preparation; all setup, identity checks and cleanup remain inside the unchanged ten-second scenario guard. No dependency/profile, worker-count, full-suite budget, artwork, pixel contract or local/CI command changes. These same-host measurements are not proof that the corrected hosted run passes; the coordinator runs canonical validation and new hosted CI on the committed correction. These are asset-boundary checks, not a full suite or screenshot-baseline comparison.

## Foundation checks

Run from the exact checkout root:

```sh
cargo xtask prepare
cargo xtask sanity
```

`prepare` compiles the focused Cucumber target with `--locked --no-run`, reads Cargo's exact executable artifact, and records the current commit, source fingerprint and executable checksum. `sanity` refuses missing/stale/cross-worktree artifacts, then runs the prepared debug executable with three independent scenario workers. All scenario setup is included in its 10-second wall-clock budget. No app, simulator, native OS resource or full suite is run.

The runner must report the exact positive count of explicit foundation scenarios as passed; zero selection, skipped/failed steps, timeout, malformed output, wrapper failure or provenance mismatch fails. It never silently rebuilds or accepts another checkout's binary. Re-run preparation after a source or HEAD change.

Each invocation appends stage and total JSON rows under `target/foundation-evidence/timing.jsonl`, including a unique attempt, UTC Unix-millisecond start/finish, elapsed seconds, commit, environment/configuration, worker count, scenario count, exit status, command and failure message. Attempt-specific preparation and sanity logs are retained alongside it; earlier failures are not overwritten. The preparation worker count denotes one Cargo driver, whose dependency compilation uses Cargo's normal parallelism.

This is Rust tooling invoking Cargo/the built test executable, not a shell/Node wrapper. Actual controlled `verify-macos` product acceptance is not implemented here.

<!-- end README.md -->
