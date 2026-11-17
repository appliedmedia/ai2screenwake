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

Run from the exact clean, committed checkout root on arm64 macOS. The sole application output is `target/screenwake/ScreenWake.app`: `Contents/MacOS/ScreenWake`, the real `Contents/MacOS/ai2screenwake-mcp`, the producer's `Info.plist`, five canonical native artwork resources, the unchanged license, and sealed build provenance. Missing native producer sources fail by name before compilation; the command never substitutes a probe/test app or silently skips an unfinished stage. The profile is fixed to release, arm64, and macOS 13.0; this is a deployment target, not a backward-runtime compatibility claim.

The three feature-isolated Cargo producers share only this checkout's `target` compiler cache, reusing identical compiler units from normal check preparation and successive release builds. Every producer still runs an explicit successful Cargo command with `--no-default-features`, its exact feature and actual profile/triple flags. The builder reads Cargo's JSON artifact/completion messages, verifies exact source/manifest, features, profile, target and canonical output paths, then atomically copies the actual bytes into feature-stable consumer paths before another feature runs. `ffi` release `.a`/`.dylib` snapshots stay at `target/screenwake/ffi-target/aarch64-apple-darwin/release/`, the dev-host `bindgen` executable at `target/screenwake/bindgen-tool/debug/uniffi-bindgen`, and the release MCP binary at `target/screenwake/mcp-target/aarch64-apple-darwin/release/ai2screenwake-mcp`. These directories are verified snapshots, not independent Cargo caches. Receipts/provenance record both compiler origins and snapshots with hashes, feature, profile and Git SHA; failed, incomplete, duplicate, foreign or mismatched producers cannot relabel an old output. Snapshot hashes are rechecked before dependent use and publication. An exclusive worktree build lock prevents overlapping builders.

UniFFI reads the captured actual production dylib and generates `screenwake.swift`, `screenwakeFFI.h`, and `screenwakeFFI.modulemap` under `target/screenwake/generated`. Swift compiles only those bindings plus `main.swift` and `MenuBarController.swift`, with `-warnings-as-errors -O`, no test define and the captured FFI `.a` plus system frameworks. The separate `target/screenwake/contracts/ScreenWakeContracts` adds `MenuBarContractTests.swift` and `-D SCREENWAKE_CONTRACT_TESTS`, then runs the same four non-presenting AppKit groups against canonical assets; neither executable nor define enters the app. Once FFI snapshots, bindings and assets exist, production/support Swift compilation and support execution are independent of MCP compilation. Hosts with at least three logical CPUs overlap those slices, reserving one CPU for Swift and passing the remaining capacity to Cargo; smaller hosts retain the serial dependency-safe path. Only one Cargo producer writes the shared compiler cache at a time. Both slices finish before assembly; a foreground failure cancels and joins the exact owned compiler tree before return, never leaving a detached writer. Global Rust flags are removed only from child Cargo commands; release stripping stays disabled to avoid the recorded Xcode 27 proc-macro issue, and deployment flags still apply only to the explicit arm64 release target.

Artwork is regenerated and strictly checked through the existing pure-Rust exporter at the same source SHA. Packaging validates the actual plist, executable modes, arm64/minimum-macOS load commands, and system-only dependencies. It writes internal source/toolchain/command/input/payload provenance before ad-hoc signing the MCP binary and app, then verifies signatures. A bounded packaged MCP discovery/EOF smoke proves exactly `is_on`, `turn_on`, and `turn_off` without calling controls, launching the app, or enabling protection. This local ad-hoc bundle is not installed, notarized, distributed, or evidence of actual menu/idle/screensaver behavior.

Every compiler and in-process stage records start/finish/elapsed time, commit, configuration, worker count, command, status, and errors in `target/screenwake/build-evidence/timing.jsonl`. Five minutes without output or the absolute twenty-minute command limit still fails. Each build/check command has a private POSIX session and a descriptor-controlled supervisor, using public `setsid`/`getsid` and `LOCAL_PEERPID`; the unreaped supervisor pins ownership through cleanup. Reparenting or a new process group cannot escape that session. Nested stages have independent private sessions and inherited lifetime descriptors: caller EOF cancels them, and a stage cannot return status before every nested lifetime reaches EOF. New control/lifetime descriptors remain CLOEXEC in the spawning parent and are enabled only in the exact child, preventing parallel sibling cross-inheritance. Teardown stops only verified private-session members, preserves caller/Orca/foreign sessions, rejects inspection/missing-receipt/lifetime failures explicitly, and retains the two-second cleanup bound. No absent-parent-group or ancestry-polling inference proves exit. The external `<attempt>-bundle.json` records final signed hashes without a self-referential hash inside the signature. Only a complete current-source bundle reaches `BUILD_OK`; stale, unsafe, missing, or malformed inputs still fail.

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

The sole output is `target/screenwake/assets/`. Native bundling must consume these exact paths:

* `app.icns`: ten PNG-backed native representations, preserving logical size and 1x/2x density.
* `app.iconset/icon_NxN.png` and `app.iconset/icon_NxN@2x.png`, where `N` is 16, 32, 128, 256, or 512. Physical sizes are `N` and `2 * N` pixels; the largest is 1024 x 1024.
* `menu-off.png` and `menu-on.png`: 24 x 24 pixels at 1x.
* `menu-off@2x.png` and `menu-on@2x.png`: 48 x 48 pixels at 2x, retaining a 24 x 24 logical size.
* `manifest.json`: schema version, Git SHA, canonical target/platform, renderer versions/settings, all three source paths/SHA-256/viewBoxes, and exactly fifteen output paths/SHA-256/physical dimensions/logical sizes/scales. ICNS lists all ten representations; the manifest does not hash itself.

Menu PNGs are black with alpha and transparent negative space for native template tinting, never a white background. Generation rejects unrecognized existing outputs before rendering, then uses a private worktree-local staging directory, decodes and checks every PNG and ICNS representation, rechecks source identity, and checks replacement eligibility again before publication. A failed replacement retains the previous complete assets; `--check` still rejects them if source or commit changed. Publication may briefly make the directory unavailable, but never exposes partial new assets. Unknown extra files or symlinks are rejected rather than discarded.

`--check` never regenerates: missing/extra outputs, stale source/commit/settings, malformed manifest, incorrect dimensions, checksum mismatch, or an undecodable/different ICNS representation fail closed. Run this immediately before consuming assets. Every export/check attempt appends stage and total timing, exit status, commit, command, and artifact count to `target/screenwake/assets-evidence/timing.jsonl`. Generated assets and evidence are ignored, never committed. This exporter is not a finished application or bundle.

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
