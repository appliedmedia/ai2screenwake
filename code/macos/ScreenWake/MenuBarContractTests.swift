// MenuBarContractTests.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
#if SCREENWAKE_CONTRACT_TESTS
import AppKit

enum MenuContractFailure: Error {
    case failed(String)
}

@MainActor
func runMenuBarContracts(assets: URL) throws {
    NSApplication.shared.setActivationPolicy(.prohibited)
    var inputs: [NativeIntent] = []
    var exits = 0
    let view = try MenuBarController(
        assets: assets, displaySurfaces: false, emit: { inputs.append($0) },
        terminate: { exits += 1 }
    )
    func require(_ value: @autoclosure () -> Bool, _ why: String) throws {
        if !value() { throw MenuContractFailure.failed(why) }
    }
    let text = [
        "status": "Status: Pending", "icon_asset": "menu-off", "badge": "...",
        "accessibility": "ScreenWake protection: Pending", "show_log": "Show Log...", "open_menu": "Open menu",
        "quit": "Quit", "error": "", "tooltip": "Screensaver protection is unverified.",
        "log_title": "ScreenWake Log", "log_text": "0  Power:Session:Query",
    ]
    var flags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                 "toggle_enabled": true, "activate_log": false]
    let observed = ObservedMenu()
    observed.onTermination = { exits += 1 }
    observed.receive(text: text, flags: flags)
    observed.attach(view)
    try require(view.statusLine.title == "Status: Pending" && !view.statusLine.isEnabled,
                "Given observed Pending, Then the native status is truthful and non-actionable")
    try require(view.button.title == "..." && view.button.image?.isTemplate == true,
                "Given observed Pending, Then the badge is non-color and the artwork is a native template")
    try require(view.button.image?.representations.count == 2,
                "Given canonical assets, Then both native density representations exist")
    print("Given observed Pending, When AppKit renders, Then label/badge/template/density contracts pass")
    try require(view.gestureRecognizer(view.primary, shouldRequireFailureOf: view.longPress),
                "Primary click must require long-press recognition to fail")
    try require(view.handle(.secondary, state: .ended) == .menu && inputs.isEmpty,
                "Right click must open the menu without emitting a toggle")
    try require(view.handle(.longPress, state: .began) == .menu && inputs.isEmpty,
                "Long press must open the menu without emitting a toggle")
    try require(view.handle(.longPress, state: .ended) == .none && inputs.isEmpty,
                "Long-press release must not emit a primary toggle")
    try require(view.handle(.primary, state: .ended) == .toggle && inputs == [.toggle],
                "A normal primary click must emit only the generated typed toggle")
    view.button.performClick(nil)
    try require(inputs == [.toggle, .toggle],
                "Own-process AppKit keyboard/accessibility activation must reach the generated toggle")
    try require(view.button.menu === view.menu &&
                view.button.accessibilityCustomActions()?.first?.handler?() == true,
                "The native menu must remain reachable through context and accessibility actions")
    print("Given native recognizer priority, When secondary/long/primary gestures resolve, Then no double-toggle occurs")
    flags["toggle_enabled"] = false
    view.render(text: text, flags: flags)
    let beforeDisabledClick = inputs.count
    try require(view.handle(.primary, state: .ended) == .none && inputs.count == beforeDisabledClick,
                "Observed disabled primary control cannot emit a new enable during cleanup")
    view.button.performClick(nil)
    try require(inputs.count == beforeDisabledClick,
                "Keyboard/accessibility activation must also respect observed primary availability")
    flags["log_visible"] = true
    flags["activate_log"] = true
    view.render(text: text, flags: flags)
    try require(view.logTextView?.isEditable == false && view.logTextView?.isSelectable == true,
                "The auxiliary AppKit log must be read-only and selectable")
    try require(view.logTextView?.string == text["log_text"],
                "Only the Rust-projected log text may be displayed")
    if let window = view.logWindow {
        try require(!view.windowShouldClose(window) && inputs.last == .closeLog,
                    "Window close reports only the generated log-dismissal input")
    } else {
        throw MenuContractFailure.failed("Rust log presentation did not construct the auxiliary view")
    }
    view.showLog()
    view.quit()
    try require(inputs.suffix(2) == [.showLog, .quit],
                "Menu actions must remain generated typed inputs")
    try require(view.showLogItem.keyEquivalent == "l" && view.quitItem.keyEquivalent == "q",
                "Native menu keyboard actions must be present")
    print("Given projected log content, When native menu/log actions run, Then read-only and keyboard contracts pass")
    observed.receive(text: [:], flags: ["terminate": true])
    try require(exits == 1, "Only the Rust observed terminal instruction may approve application termination")
    print("Given observed Rust termination, When rendered, Then native termination is requested once")
}
#endif
// end MenuBarContractTests.swift
