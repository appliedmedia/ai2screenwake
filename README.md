<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# ScreenWake (ai2screenwake)

This public repository is the ScreenWake codebase and planning home. It is currently planning-only: no implemented app, build, or artwork exists. See the [project plan](<_orch.md>).

## Planned utility

A minimal macOS idle-protection utility with an old-school antenna-TV menu-bar toggle: closed eyes and zzZ when off, wide-open eyes when on. Sleep/screensaver protection remains unverified; manual locking and forced sleep stay effective.

* Three local stdio MCP tools, `is_on`, `turn_on`, and `turn_off`, share the app's single owner.
* Observed status includes protected-idle duration and confirmed suppressed events when on, with `epoch_ms` and `minutes_into_protected_idle`; no inferred events.
* Three Rust runtime artifacts, Actions-only control, and minimal Swift rendering.

## Code transparency

Transparency means verifiability instead of promises: inspect the public source yourself, following Applied Media's full-source-transparency approach in [print2paper4vscode](<https://github.com/appliedmedia/print2paper4vscode>).

[Code Transparency v1](<LICENSE.txt>) permits viewing, study, audit, feedback, and pull requests. This is source-available, not open source; derivatives, redistribution, and commercial use require a separate license. The license governs all rights.

<!-- end README.md -->
