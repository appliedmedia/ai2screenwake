<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Worktree-local tooling

## Native application build

```sh
cargo xtask build --profile release
```

Run from the exact clean, committed checkout root on arm64 macOS. The sole application output is `target/screenwake/ScreenWake.app`: `Contents/MacOS/ScreenWake`, the real `Contents/MacOS/ai2screenwake-mcp`, the producer's `Info.plist`, five canonical native artwork resources, the unchanged license, and sealed build provenance. Missing native producer sources fail by name before compilation; the command never substitutes a probe/test app or silently skips an unfinished stage. The profile is fixed to release, arm64, and macOS 13.0; this is a deployment target, not a backward-runtime compatibility claim.

The Rust pipeline builds the client's `ffi` library into `target/screenwake/ffi-target`, builds the debug-host `bindgen` tool separately into `target/screenwake/bindgen-tool`, and generates `screenwake.swift`, `screenwakeFFI.h`, and `screenwakeFFI.modulemap` under `target/screenwake/generated`. UniFFI generation uses the actual production dylib as metadata input. Swift compiles only the generated binding plus hand-authored `main.swift` and `MenuBarController.swift`, statically linking the production `.a` and system frameworks. The real three-tool MCP binary builds with `mcp` in `target/screenwake/mcp-target`, without overwriting the FFI library. Global Rust flags are removed only from child Cargo commands; release stripping is disabled there to avoid malformed host proc-macro dylibs with Xcode 27, and the explicit deployment link flag applies only to the arm64 target.

Artwork is regenerated and strictly checked through the existing pure-Rust exporter at the same source SHA. Packaging validates the actual plist, executable modes, arm64/minimum-macOS load commands, and system-only dependencies. It writes internal source/toolchain/command/input/payload provenance before ad-hoc signing the MCP binary and app, then verifies signatures. A bounded packaged MCP discovery/EOF smoke proves exactly `is_on`, `turn_on`, and `turn_off` without calling controls, launching the app, or enabling protection. This local ad-hoc bundle is not installed, notarized, distributed, or evidence of actual menu/idle/screensaver behavior.

Every compiler and in-process stage records start/finish/elapsed time, commit, configuration, worker count, command, status, and errors in `target/screenwake/build-evidence/timing.jsonl`. Attempt-specific stdout/stderr remain observable during compilation; five minutes without new output or the absolute twenty-minute command limit terminates that child. Compiler/linker stages are serial because generated inputs and sealed identity are dependent. The external `<attempt>-bundle.json` receipt records final signed-file hashes without a self-referential hash inside the signature. Only a complete current-source bundle reaches `BUILD_OK`; source/target symlinks, stale inputs, unexpected files, missing tools, compiler failures, or malformed artifacts fail explicitly.

Staging is worktree-local. Publication uses the macOS atomic directory-exchange primitive when replacing an existing recognized bundle; failed preflight/compilation/verification leaves the previous complete app and unrelated output untouched. Generated files/logs never enter Git. The command does not change settings, launch the GUI, install an app, run an idle witness, or post a release.

```sh
cargo test --locked -p xtask --test build --no-run
cargo test --locked -p xtask --test build -- --concurrency 3 --color never
```

Focused executable Cucumber requirements cover configuration rejection, missing sources, changed inputs, partial-publication preservation, failed-stage evidence, and symlink rejection. Preparation is measured separately; the six isolated scenarios run with three workers within ten seconds. These boundary fixtures do not substitute for the actual canonical app build.

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

Menu PNGs are black with alpha and transparent negative space for native template tinting, never a white background. Generation renders into a private worktree-local staging directory, decodes and checks every PNG and ICNS representation, rechecks source identity, then publishes the complete directory. A failed replacement retains the previous complete assets; `--check` still rejects them if source or commit changed. Publication may briefly make the directory unavailable, but never exposes partial new assets. Unknown extra files or symlinks are rejected rather than discarded.

`--check` never regenerates: missing/extra outputs, stale source/commit/settings, malformed manifest, incorrect dimensions, checksum mismatch, or an undecodable/different ICNS representation fail closed. Run this immediately before consuming assets. Every export/check attempt appends stage and total timing, exit status, commit, command, and artifact count to `target/screenwake/assets-evidence/timing.jsonl`. Generated assets and evidence are ignored, never committed. This exporter is not a finished application or bundle.

Focused executable Cucumber requirements use isolated worktree-local fixtures and three concurrent scenario workers:

```sh
cargo test --locked -p xtask --test assets --no-run
cargo test --locked -p xtask --test assets -- --concurrency 3 --color never
```

Compilation is separate preparation; the measured scenario run must finish within ten seconds. These are asset-boundary checks, not a full suite or screenshot-baseline comparison.

## Foundation checks

Run from the exact checkout root:

```sh
cargo xtask prepare
cargo xtask sanity
```

`prepare` compiles the focused Cucumber target with `--locked --no-run`, reads Cargo's exact executable artifact, and records the current commit, source fingerprint and executable checksum. `sanity` refuses missing/stale/cross-worktree artifacts, then runs the prepared debug executable with three independent scenario workers. All scenario setup is included in its 10-second wall-clock budget. No app, simulator, native OS resource or full suite is run.

The runner must report the exact positive count of explicit foundation scenarios as passed; zero selection, skipped/failed steps, timeout, malformed output, wrapper failure or provenance mismatch fails. It never silently rebuilds or accepts another checkout's binary. Re-run preparation after a source or HEAD change.

Each invocation appends stage and total JSON rows under `target/foundation-evidence/timing.jsonl`, including a unique attempt, UTC Unix-millisecond start/finish, elapsed seconds, commit, environment/configuration, worker count, scenario count, exit status, command and failure message. Attempt-specific preparation and sanity logs are retained alongside it; earlier failures are not overwritten. The preparation worker count denotes one Cargo driver, whose dependency compilation uses Cargo's normal parallelism.

This is Rust tooling invoking Cargo/the built test executable, not a shell/Node wrapper. Full `check`, CI artifact upload and actual `verify-macos` are not implemented here.

<!-- end README.md -->
