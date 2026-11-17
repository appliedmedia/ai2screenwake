<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Worktree-local tooling

## Completed-wave regression check

```sh
cargo xtask check
```

Run this full check only after the active wave's implementation and affected checks are complete, not at every source checkpoint. The entry and flags are identical locally and in CI. It requires a clean committed arm64 macOS checkout, actual login-session/native resources, and the native build tools. The whole check has a 300-second budget, including all preparation, feature/lint checks, families and the release build; missing resources, failed stages, zero-selected families, stale executables or deadline expiry fail explicitly. Family guards remain ten seconds; no gate is skipped.

The runner discovers every integration-test target and its required features using Cargo metadata, including newly added history/replay families. It requires exactly the backend/client/native runtime packages plus tooling-only xtask, empty backend/client defaults, and the maintained foundation/runtime/IPC/MCP/UI/native/assets/build/check families. Default, `ffi`, `mcp`, and tooling-only `bindgen` profiles stay separate. Formatting, deliberate-profile clippy, backend empty-default compilation, and normal-dependency graph checks reject MCP/server/schema SDK or bindgen leakage into default/FFI consumers. The frozen license blob is verified unchanged.

Preparation compiles each package/feature group's targets once with Cargo's JSON artifact output. Every test executable and supporting binary is retained with source SHA, feature profile, absolute target path, hash, and mode in `target/check-evidence/<attempt>-prepared.json`. Both check/build executors preserve an explicit caller-selected working directory and default to the worktree root only when none was supplied. The logged command describes that resolved directory before the unchanged private supervisor executes it. Test families therefore retain their owning package directory and relative feature paths; short private per-family temporary roots keep socket paths within Darwin limits.

Independent host families run in three concurrent lanes when CPU capacity allows, with one scenario worker per family to avoid multiplying concurrency. A lower lane count records the host CPU constraint. The CPU-bound asset family runs separately with its existing three scenario workers and ten-second budget. Native and generated-host UI targets each have disjoint selections: deterministic `not @real_host` cases stay in the host lanes, while every `@real_host` selection runs separately and serially because its IOKit assertions, NativeHost/IPC admission, and registration belong to the current login session. Each executable is still prepared only once. No forced sleep, screensaver trigger, input simulation, settings change, installation, or additional permission is used.

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

Run from the exact clean committed arm64 macOS checkout. The sole application is `target/ai2sw/ai2sw.app`: executable `Contents/MacOS/ai2sw`, unchanged `ai2screenwake-mcp`, the producer's plist, nine artwork resources, unchanged license and sealed provenance. Its display name is AI2ScreenWake; bundle ID `com.appliedmedia.ScreenWake`, source folders and UniFFI filenames stay stable. Missing inputs fail before compilation. Release/arm64/macOS 13.0 are fixed; the deployment target is not a backward-runtime witness. The fifteen signed artifact records include four independently rasterized preview PNGs, not an old eleven-file inventory.

Three feature-isolated Cargo producers share only this checkout's `target` compiler cache. Each runs with explicit `--no-default-features`, feature/profile/triple flags and actual Cargo JSON completion/artifact validation. Verified bytes are captured before switching features: FFI `.a`/`.dylib` at `target/ai2sw/ffi-target/aarch64-apple-darwin/release/`, bindgen at `target/ai2sw/bindgen-tool/debug/uniffi-bindgen`, MCP at `target/ai2sw/mcp-target/aarch64-apple-darwin/release/ai2screenwake-mcp`. These are verified snapshots, not independent caches. Source/manifest/feature/profile/target/SHA/hash/mode checks reject stale or mismatched producers at capture, use and publication; an exclusive worktree lock prevents overlapping builds.

UniFFI generates unchanged `screenwake.swift`, `screenwakeFFI.h` and `screenwakeFFI.modulemap` under `target/ai2sw/generated`. Production Swift compiles only generated bindings, `main.swift` and `MenuBarController.swift` with `-warnings-as-errors -O`, captured static FFI and system frameworks. Separate `target/ai2sw/contracts/ScreenWakeContracts` adds the support source/define and runs four nonpresenting AppKit groups; neither enters the app. Once FFI/bindings/assets exist, Swift and MCP can overlap on hosts with at least three CPUs, reserving one for Swift; smaller hosts stay serial. Only one Cargo producer writes the cache. Both slices finish before assembly; failures cancel and join exact owned compiler trees. Child-only Rust flags, release stripping workaround and explicit arm64 deployment flags remain unchanged.

Artwork is regenerated and strictly checked through the existing pure-Rust exporter at the same source SHA. Packaging validates the actual plist, executable modes, arm64/minimum-macOS load commands, and system-only dependencies. It writes internal source/toolchain/command/input/payload provenance before ad-hoc signing the MCP binary and app, then verifies signatures. A bounded packaged MCP discovery/EOF smoke proves exactly `is_on`, `turn_on`, and `turn_off` without calling controls, launching the app, or enabling protection. This local ad-hoc bundle is not installed, notarized, distributed, or evidence of actual menu/idle/screensaver behavior.

Every compiler/in-process stage records start/finish/elapsed time, SHA/configuration, worker count, command/status/errors in `target/ai2sw/build-evidence/timing.jsonl`. The existing five-minute no-progress/twenty-minute standalone limit and stricter 300-second full-check limit remain. Private POSIX sessions and descriptor-controlled supervisors pin exact ownership across reparenting/new groups; nested lifetime EOF and caller-loss cancellation prevent escaped writers. Only verified owned members are stopped; foreign/Orca/caller sessions stay untouched, and inspection/lifetime failures fail explicitly within the existing two-second cleanup bound. `<attempt>-bundle.json` records final signed hashes externally. Only complete current-source output reaches `BUILD_OK`.

Staging is worktree-local. Publication uses the macOS atomic directory-exchange primitive when replacing an existing recognized bundle; failed preflight/compilation/verification leaves the previous complete app and unrelated output untouched. Generated files/logs never enter Git. The command does not change settings, launch the GUI, install an app, run an idle witness, or post a release.

```sh
cargo test --locked -p xtask --test build --no-run
cargo test --locked -p xtask --test build -- --concurrency 3 --color never
```

All twenty-three executable build scenarios retain three workers and the ten-second guard. Descriptor-backed readiness has a separate two-second fixture bound before the injected 100-ms failure window, including nested cases; failed startup never blindly reads a missing child PID. Existing feature-cache/provenance/overlap and preservation assertions remain. Added real-process cases cover early parent exit with an orphaned new-group writer, nested failure, caller death/EOF, and an actual Cargo-to-nested-stage exec chain. Red fixtures stop their exact controlled surviving writer. Synthetic producer receipts prove only file/provenance boundaries, not real compiler or app acceptance.

Hosted run `43931646264` passed assets in 7.781204541 seconds but reached the unchanged full-check deadline at 600.140862333 seconds during the release bundle. Its completed FFI/bindgen/MCP compiler stages were 28.775768208, 62.810815583 and 78.016664292 seconds respectively; 196.383821792 seconds was the entire bundle's remaining deadline window, not FFI compile time. The retained cold-slice evidence under `target/cold-performance/` shows twenty-five fewer newly compiled MCP units with a shared release cache; local cold separate-producer total was 47.452665210 seconds versus 46.106899542 shared. Root-cache bindgen after existing checks still required 24.683954542 seconds versus a 26.020795917-second isolated cold generator: this is limited reuse, not a cold-hosted pass. Preparation-first ordering and debug-info candidates gave little benefit and were not adopted; profiles, dependencies, all clippy/preparation gates, strict asset/full-suite budgets and the identical local/CI command remain unchanged. The additional critical-path correction overlaps only already-independent Swift and MCP work. An actual committed production build and cached repeat passed all generation/link/support/sign/smoke gates and identical producer hashes; timing exposed duplicate hashing in the initial snapshot implementation. Captures now hash the actual origin and temporary copy once each, compare bytes/mode/hash and atomically rename that verified copy; dependent inputs are rechecked at use/publication, while the generator is checked before use and retained as provenance rather than repeatedly hashed as a packaged payload. Snapshot stages are separately timed. Exact final committed build/feature-alternation evidence is returned in the worker receipt; new hosted CI remains authoritative.

## Native artwork assets

```sh
cargo xtask assets
cargo xtask assets --check
```

`assets` renders only `code/macos/ScreenWake/Resources/Artwork/{app.svg,menu-off.svg,menu-on.svg}` using pinned Rust resvg/usvg/tiny-skia and PNG-only ICNS encoding. No external renderer, system font lookup, raster import, network resource, or desktop window is used. Inputs must retain their 1024 x 1024 app or 24 x 24 menu viewBoxes. Invalid XML, unsupported/external content, wrong dimensions, empty artwork, colored menu pixels, encoding/write failures, or changing source/commit identity fail explicitly.

The sole output is `target/ai2sw/assets/`. Native bundling consumes these exact paths:

* `app.icns`: ten PNG-backed native representations, preserving logical size and 1x/2x density.
* `app.iconset/icon_NxN.png` and `app.iconset/icon_NxN@2x.png`, where `N` is 16, 32, 128, 256, or 512. Physical sizes are `N` and `2 * N` pixels; the largest is 1024 x 1024.
* `menu-off.png` and `menu-on.png`: 24 x 24 pixels at 1x.
* `menu-off@2x.png` and `menu-on@2x.png`: 48 x 48 pixels at 2x, retaining a 24 x 24 logical size.
* `preview-off.png` and `preview-on.png`: 96 x 96 pixels, independently rendered from the corresponding menu master.
* `preview-off@2x.png` and `preview-on@2x.png`: 192 x 192 pixels at 2x, retaining a 96-point logical size.
* `manifest.json`: schema version, Git SHA, target/platform, renderer settings, three source hashes/viewBoxes and exactly nineteen output hashes/dimensions/logical sizes/scales. ICNS has ten representations; the manifest does not hash itself.

Menu PNGs are black with alpha and transparent negative space for native template tinting, never a white background. Generation rejects unrecognized existing outputs before rendering, then uses a private worktree-local staging directory, decodes and checks every PNG and ICNS representation, rechecks source identity, and checks replacement eligibility again before publication. A failed replacement retains the previous complete assets; `--check` still rejects them if source or commit changed. Publication may briefly make the directory unavailable, but never exposes partial new assets. Unknown extra files or symlinks are rejected rather than discarded.

`--check` never regenerates: missing/extra outputs, stale source/commit/settings, malformed manifests, wrong dimensions/checksums or different decoded ICNS fail closed. Run it immediately before consumption. Every export/check appends stage/total timing, status, SHA, command and count to `target/ai2sw/assets-evidence/timing.jsonl`. Generated assets/evidence remain ignored. The exporter is not app acceptance.

Focused executable Cucumber requirements use isolated worktree-local fixtures and three concurrent scenario workers:

Fixture commits pass command-local `-c commit.gpgSign=false`; tests inject signing-enabled configuration with a failing signer only into those fixture commands. No host, global, or repository signing configuration is changed.

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

`prepare` captures the source fingerprint and HEAD before Cargo, compiles the focused target with `--locked --no-run`, then rechecks both immediately before publishing its exact executable/hash receipt. Any source or HEAD change rejects publication. `sanity` refuses missing/stale/cross-worktree artifacts and runs the prepared executable with three independent scenario workers. Its ten-second budget still includes setup; no app, native OS resource or full suite is run.

The runner must report the exact positive count of explicit foundation scenarios as passed; zero selection, skipped/failed steps, timeout, malformed output, wrapper failure or provenance mismatch fails. It never silently rebuilds or accepts another checkout's binary. Re-run preparation after a source or HEAD change.

Each invocation appends stage and total JSON rows under `target/foundation-evidence/timing.jsonl`, including a unique attempt, UTC Unix-millisecond start/finish, elapsed seconds, commit, environment/configuration, worker count, scenario count, exit status, command and failure message. Attempt-specific preparation and sanity logs are retained alongside it; earlier failures are not overwritten. The preparation worker count denotes one Cargo driver, whose dependency compilation uses Cargo's normal parallelism.

This is Rust tooling invoking Cargo/the built test executable, not a shell/Node wrapper. Actual controlled `verify-macos` product acceptance is not implemented here.

<!-- end README.md -->
