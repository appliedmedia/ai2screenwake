<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Generated macOS host

The in-progress host uses the one private Rust owner and generated UniFFI 0.32.2 intent/observation port. Rust owns `Ui::Menu::Render` with canonical text/flag maps; generated Swift forwards these unchanged to AppKit. There is no mirrored Status, Swift route string, or state getter. Closing the auxiliary log emits rendering-only `Ui::Log::Dismissed`; it is not an OwnerControl or MCP tool.

## Verified generated-seam preparation

Run from the exact checkout root. Keep production FFI and bindgen target directories separate: another feature build must never overwrite the library linked into Swift. Release stripping on this Xcode 27 toolchain produced a malformed proc-macro LINKEDIT string pool; `CARGO_PROFILE_RELEASE_STRIP=none` is the verified build correction.

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS CARGO_PROFILE_RELEASE_STRIP=none MACOSX_DEPLOYMENT_TARGET=13.0 CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS='-C link-arg=-mmacosx-version-min=13.0' cargo build --locked -p ai2screenwake_client --features ffi --lib --release --target aarch64-apple-darwin --target-dir target/screenwake/ffi-target
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --locked -p ai2screenwake_client --features bindgen --bin uniffi-bindgen --target-dir target/screenwake/bindgen-tool
target/screenwake/bindgen-tool/debug/uniffi-bindgen generate target/screenwake/ffi-target/aarch64-apple-darwin/release/libai2screenwake_client.dylib --language swift --out-dir target/screenwake/generated --no-format
```

The actual generator emits `screenwake.swift`, `screenwakeFFI.h`, and `screenwakeFFI.modulemap`. The tooling-only bindgen feature currently requires Rust 1.88 or later through Askama; actual verification uses Rust 1.97.1 and Swift 6.4.2. Normal client/ffi builds do not enable bindgen, MCP or Tokio.

The generated source has compiled with `xcrun swiftc -target arm64-apple-macosx13.0 -I target/screenwake/generated -Xcc -fmodule-map-file=target/screenwake/generated/screenwakeFFI.modulemap`. Link the full static library path `target/screenwake/ffi-target/aarch64-apple-darwin/release/libai2screenwake_client.a`, not `-lai2screenwake_client`, plus AppKit/CoreGraphics/Security/CoreServices/IOKit/CoreFoundation and `-liconv`. A generated-source static-link probe reports only system dependencies; it is not an application or release artifact.

Handwritten sources are being added at `main.swift`, `MenuBarController.swift` and `MenuBarContractTests.swift`, with `Info.plist`. The bundle contract is `target/screenwake/ScreenWake.app`, executable `ScreenWake`, identifier `com.appliedmedia.ScreenWake`, minimum macOS 13, `LSUIElement=true`, and resources `app.icns` plus four menu PNGs from the worktree-local canonical `cargo xtask assets`. Native AppKit UI, host admission/quit behavior and their tests remain in progress; the independent Rust xtask owner owns canonical packaging.

Screensaver inhibition remains unverified. No real forced-sleep/screensaver test, install, settings change, permission request or release is performed by this preparation.

<!-- end README.md -->
