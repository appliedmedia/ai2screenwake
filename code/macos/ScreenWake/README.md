<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Generated macOS host

The host uses the one private Rust owner and generated UniFFI 0.32.2 intent/observation port. `NativeHost(observer:)` registers the generated `NativeObserver` before asynchronous OS admission; `emit(intent:) -> ()` forwards only `NativeIntent::{Toggle,ShowLog,CloseLog,Quit,RendererFailed}`. Rust owns `Ui::Menu::Render` with canonical text/flag maps; generated Swift forwards these unchanged to AppKit. There is no mirrored Status, Swift route string, or state getter. Closing the auxiliary log emits rendering-only `Ui::Log::Dismissed`; it is not an OwnerControl or MCP tool.

## Verified generated-seam preparation

Run from the exact checkout root. Keep production FFI and bindgen target directories separate: another feature build must never overwrite the library linked into Swift. Release stripping on this Xcode 27 toolchain produced a malformed proc-macro LINKEDIT string pool; `CARGO_PROFILE_RELEASE_STRIP=none` is the verified build correction.

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS CARGO_PROFILE_RELEASE_STRIP=none MACOSX_DEPLOYMENT_TARGET=13.0 CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS='-C link-arg=-mmacosx-version-min=13.0' cargo build --locked -p ai2screenwake_client --features ffi --lib --release --target aarch64-apple-darwin --target-dir target/screenwake/ffi-target
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --locked -p ai2screenwake_client --features bindgen --bin uniffi-bindgen --target-dir target/screenwake/bindgen-tool
target/screenwake/bindgen-tool/debug/uniffi-bindgen generate target/screenwake/ffi-target/aarch64-apple-darwin/release/libai2screenwake_client.dylib --language swift --out-dir target/screenwake/generated --no-format
```

The actual generator emits `screenwake.swift`, `screenwakeFFI.h`, and `screenwakeFFI.modulemap`. The tooling-only bindgen feature currently requires Rust 1.88 or later through Askama; actual verification uses Rust 1.97.1 and Swift 6.4.2. Normal client/ffi builds do not enable bindgen, MCP or Tokio.

The generated source has compiled with `xcrun swiftc -target arm64-apple-macosx13.0 -I target/screenwake/generated -Xcc -fmodule-map-file=target/screenwake/generated/screenwakeFFI.modulemap`. Link the full static library path `target/screenwake/ffi-target/aarch64-apple-darwin/release/libai2screenwake_client.a`, not `-lai2screenwake_client`, plus AppKit/CoreGraphics/Security/CoreServices/IOKit/CoreFoundation and `-liconv`. A generated-source static-link probe reports only system dependencies; it is not an application or release artifact.

Production handwritten inputs are exactly `main.swift` and `MenuBarController.swift`, plus generated `screenwake.swift`, with no conditional compilation flag. The following actual production command has compiled with warnings denied:

```sh
xcrun swiftc -warnings-as-errors -O -target arm64-apple-macosx13.0 -I target/screenwake/generated -Xcc -fmodule-map-file=target/screenwake/generated/screenwakeFFI.modulemap target/screenwake/generated/screenwake.swift code/macos/ScreenWake/MenuBarController.swift code/macos/ScreenWake/main.swift target/screenwake/ffi-target/aarch64-apple-darwin/release/libai2screenwake_client.a -framework AppKit -framework CoreGraphics -framework Security -framework CoreServices -framework IOKit -framework CoreFoundation -liconv -o target/screenwake/host-development/ScreenWake
```

The separate support harness adds `MenuBarContractTests.swift` and `-D SCREENWAKE_CONTRACT_TESTS` to that same source/link configuration, producing `target/screenwake/host-development/ScreenWakeContracts`. Invoke it with exactly one argument, the absolute canonical `target/screenwake/assets` directory. It constructs actual AppKit objects without presenting a status item/menu/window and records only generated typed inputs; production contains neither that test source nor the test define. Four Given/When/Then groups check observed labels/template/densities, recognizer failure dependency/no secondary-release toggle, read-only log/keyboard/typed actions, and observed terminal instruction. This support harness is not a real built-app/MCP acceptance witness.

The actual shell uses primary-click recognition that requires long-press recognition to fail. Right click and long-press begin open the native dropdown; long-press end never adds a primary toggle. Only Rust observations select TV assets, badges, accessible status, short errors, read-only log text and termination. Log-window close reports the generated dismissal input and waits for Rust's next projection without changing protection.

Rust retains the concrete `MacPower`, `OwnerServer`, lifecycle guard and scoped observation lifetimes on its host worker, not the Swift main thread. It observes the exact Quit settlement and confirmed Off before stopping the lifecycle observer and private server, then emits a terminal rendering instruction. Failed normal cleanup remains explicit/retryable; the generated port is retained until observed completion. Exceptional foreign-port disposal is only fallback, never claimed successful cleanup. Startup failure is an explicit Error projection with no claimed Off and no admitted private owner; a bootstrap error-only Actions port is not a second power owner. Final concurrent Quit/IPC acceptance remains pending D3's admitted-Quit fence; no server lease is released early as a workaround.

The bundle contract is `target/screenwake/ScreenWake.app`, executable `ScreenWake`, identifier `com.appliedmedia.ScreenWake`, minimum macOS 13, `LSUIElement=true`, and resources `app.icns` plus four menu PNGs from the worktree-local canonical `cargo xtask assets`. Hand-authored `Info.plist` passes `plutil -lint`. The independent Rust xtask owner owns canonical packaging; development executables are not a posted, signed or released build.

Screensaver inhibition remains unverified. No real forced-sleep/screensaver test, install, settings change, permission request or release is performed by this preparation.

<!-- end README.md -->
