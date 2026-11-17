<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake runtime

Exactly three runtime artifacts share one contract and one owner:

* [Backend](<ai2screenwake_backend/README.md>): feature-isolated contracts and Actions framework, without an unnecessary cloud service.
* [Client](<ai2screenwake_client/README.md>): owner policy, ordered projections, private IPC, durable history, MCP and generated native-host attachment.
* [Native](<ai2screenwake_native/README.md>): actual macOS resources, clocks/input age, login-session identity, exact-bundle launch and sleep/wake glue.

Run `cargo xtask check` for completed-wave regression and `cargo xtask build --profile release` for the canonical worktree-local signed bundle, from the repository root. The tooling-only `code/xtask` package is not a fourth runtime artifact. Real physical menu/log, natural idle/screensaver, security-preservation, supported-runtime and external-telemetry/release evidence remain separate gates. The root license is unchanged.

<!-- end README.md -->
