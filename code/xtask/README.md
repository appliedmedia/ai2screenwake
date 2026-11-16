<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Foundation checks

Run from the exact checkout root:

```sh
cargo xtask prepare
cargo xtask sanity
```

`prepare` compiles the focused Cucumber target with `--locked --no-run`, reads Cargo's exact executable artifact, and records the current commit, source fingerprint and executable checksum. `sanity` refuses missing/stale/cross-worktree artifacts, then runs the prepared debug executable with three independent scenario workers. All scenario setup is included in its 10-second wall-clock budget. No app, simulator, native OS resource or full suite is run.

The runner must report the exact positive count of explicit foundation scenarios as passed; zero selection, skipped/failed steps, timeout, malformed output, wrapper failure or provenance mismatch fails. It never silently rebuilds or accepts another checkout's binary. Re-run preparation after a source or HEAD change.

Each invocation appends stage and total JSON rows under `target/foundation-evidence/timing.jsonl`, including a unique attempt, UTC Unix-millisecond start/finish, elapsed seconds, commit, environment/configuration, worker count, scenario count, exit status, command and failure message. Attempt-specific preparation and sanity logs are retained alongside it; earlier failures are not overwritten. The preparation worker count denotes one Cargo driver, whose dependency compilation uses Cargo's normal parallelism.

This is Rust tooling invoking Cargo/the built test executable, not a shell/Node wrapper. Build/bundle, full `check`, CI artifact upload and actual `verify-macos` are deliberately not advertised as implemented. Lane 3 may extend this tooling after coordinating the canonical runner and shared manifests with Lane 1.

<!-- end README.md -->
