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
    try require(view.button.menu == nil,
                "Given first open, Then no automatic context menu can race the anchored surface")
    try require(view.handle(.secondary, state: .ended) == .none && !view.button.isEnabled,
                "Given no observed frame, Then opening waits for truthful complete content")
    let initialSize = view.popover.contentSize
    let initialButtonSize = view.button.frame.size
    let text = [
        "status": "Status: Pending", "icon_asset": "menu-off", "preview_asset": "preview-off", "badge": "...",
        "accessibility": "AI2ScreenWake protection: Pending", "show_log": "Show Log...", "open_menu": "Open menu",
        "quit": "Quit", "error": "", "tooltip": "Screensaver protection is unverified.",
        "log_title": "ai2sw.log", "log_text": "/private/example/ai2sw.log\n0  Power:Session:Query",
    ]
    var flags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                 "toggle_enabled": true, "activate_log": false]
    let observed = ObservedMenu()
    observed.onTermination = { exits += 1 }
    observed.receive(text: text, flags: flags)
    observed.attach(view)
    try require(view.statusLine.stringValue == "Status: Pending" && !view.statusLine.isEditable,
                "Given observed Pending, Then the native status is truthful and non-actionable")
    try require(view.button.title == "..." && view.button.image?.isTemplate == true,
                "Given observed Pending, Then the badge is non-color and the artwork is a native template")
    try require(view.button.image?.representations.count == 2,
                "Given canonical assets, Then both native density representations exist")
    try require(view.preview.image?.size == NSSize(width: 96, height: 96) &&
                view.preview.image?.representations.map { $0.pixelsWide }.sorted() == [96, 192],
                "Given observed Pending, Then the same asleep TV has crisp large native densities")
    try require(view.popover.contentSize == initialSize && initialSize.height == 276 &&
                view.popover.behavior == .transient && !view.popover.animates,
                "Given first presentation, Then a complete fixed native popover reserves preview/status/error/control space")
    print("Given observed Pending, When AppKit renders, Then label/badge/template/density contracts pass")
    try require(view.gestureRecognizer(view.primary, shouldRequireFailureOf: view.longPress),
                "Primary click must require long-press recognition to fail")
    try require(view.handle(.secondary, state: .ended) == .menu && inputs.isEmpty,
                "Right click must open the menu without emitting a toggle")
    try require(view.handle(.longPress, state: .began) == .menu && inputs.isEmpty,
                "Long press must open the menu without emitting a toggle")
    try require(view.handle(.longPress, state: .ended) == .none && inputs.isEmpty,
                "Long-press release must not emit a primary toggle")
    try require(view.handle(.primary, state: .ended) == .none && inputs.isEmpty,
                "A primary release delivered after a recognized long press must also be suppressed")
    try require(view.menuAnchor == view.button.bounds,
                "The native upward-beak popover is positioned from the actual status-button bounds, not a screen origin")
    guard let controlClick = NSEvent.mouseEvent(with: .leftMouseDown, location: .zero,
            modifierFlags: .control, timestamp: 0, windowNumber: 0, context: nil,
            eventNumber: 0, clickCount: 1, pressure: 1),
          let rightClick = NSEvent.mouseEvent(with: .rightMouseDown, location: .zero,
            modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
            eventNumber: 1, clickCount: 1, pressure: 1) else {
        throw MenuContractFailure.failed("Native mouse-event construction failed")
    }
    for event in [controlClick, rightClick] {
        try require(view.gestureRecognizer(view.secondary, shouldAttemptToRecognizeWith: event) &&
                    !view.gestureRecognizer(view.primary, shouldAttemptToRecognizeWith: event) &&
                    !view.gestureRecognizer(view.longPress, shouldAttemptToRecognizeWith: event),
                    "Control-click and right-click have exactly one secondary presentation path and no primary toggle")
    }
    try require(view.handle(.primary, state: .ended) == .toggle && inputs == [.toggle],
                "A normal primary click must emit only the generated typed toggle")
    view.button.performClick(nil)
    try require(inputs == [.toggle, .toggle],
                "Own-process AppKit keyboard/accessibility activation must reach the generated toggle")
    try require(view.button.menu == nil &&
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
    try require(view.logWindow?.title == "ai2sw.log" &&
                view.logTextView?.string.components(separatedBy: "\n").first == "/private/example/ai2sw.log",
                "The read-only .log viewer preserves the real export's full-path first line")
    var awake = text
    awake["status"] = "Status: On"
    awake["icon_asset"] = "menu-on"
    awake["preview_asset"] = "preview-on"
    view.render(text: awake, flags: flags)
    try require(view.preview.image !== nil && view.preview.image !== view.button.image &&
                view.preview.image?.isTemplate == true && view.popover.contentSize == initialSize &&
                view.button.frame.size == initialButtonSize,
                "Later observed On selects a large template without moving the reserved first-open layout")
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
    var replies: [Bool] = []
    let application = ScreenWakeApplication(reply: { replies.append($0) })
    observed.onTerminationRejected = { application.cancelTermination() }
    observed.onTermination = { application.finishTermination() }
    try require(application.applicationShouldTerminate(NSApplication.shared) == .terminateLater,
                "Given a native termination request, Then cleanup is deferred")
    observed.receive(text: text, flags: ["termination_failed": true])
    try require(replies == [false],
                "Given failed cleanup, Then the pending AppKit termination is explicitly denied")
    observed.receive(text: text, flags: ["termination_failed": true])
    try require(replies == [false], "A settled failure cannot reply again without a new termination request")
    try require(application.applicationShouldTerminate(NSApplication.shared) == .terminateLater,
                "Given a denied termination, When requested again, Then cleanup can retry")
    observed.receive(text: [:], flags: ["terminate": true])
    try require(replies == [false, true], "Only a later successful observed cleanup may approve the retry")
    print("Given observed Rust termination, When rendered, Then native termination is requested once")
}
#endif
// end MenuBarContractTests.swift
