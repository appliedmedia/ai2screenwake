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
    var requestedWidths: [CGFloat] = []
    let view = try MenuBarController(
        assets: assets, displaySurfaces: false,
        makeStatusButton: { width in
            requestedWidths.append(width)
            return (nil, NSButton(frame: NSRect(x: 0, y: 0, width: 48, height: 24)))
        },
        emit: { inputs.append($0) },
        terminate: { exits += 1 }
    )
    func require(_ value: @autoclosure () -> Bool, _ why: String) throws {
        if !value() { throw MenuContractFailure.failed(why) }
    }
    try require(requestedWidths == [48],
                "Given production status-button construction, Then its actual factory requests fixed width 48, never variableLength")
    func pixels(_ representation: NSBitmapImageRep) throws -> Data {
        guard let source = representation.cgImage,
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                data: nil, width: source.width, height: source.height, bitsPerComponent: 8,
                bytesPerRow: source.width * 4, space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              ), let data = context.data else {
            throw MenuContractFailure.failed("Cannot decode canonical preview pixels")
        }
        context.draw(source, in: CGRect(x: 0, y: 0, width: source.width, height: source.height))
        return Data(bytes: data, count: source.width * source.height * 4)
    }
    func requirePreview(_ state: String) throws {
        guard let image = view.preview.image else {
            throw MenuContractFailure.failed("Missing observed preview")
        }
        for (suffix, width) in [("", 96), ("@2x", 192)] {
            let data = try Data(contentsOf: assets.appendingPathComponent("preview-\(state)\(suffix).png"))
            guard let expected = NSBitmapImageRep(data: data),
                  let actual = image.representations.compactMap({ $0 as? NSBitmapImageRep })
                    .first(where: { $0.pixelsWide == width && $0.pixelsHigh == width }) else {
                throw MenuContractFailure.failed("Missing canonical preview density")
            }
            let matches = try pixels(actual) == pixels(expected)
            try require(matches,
                        "Given observed \(state), Then rendered \(width)-pixel preview matches the corresponding canonical asset")
        }
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
    try requirePreview("off")
    let asleep = view.preview.image
    try require(view.popover.contentSize == initialSize && initialSize.height == 276 &&
                view.popover.behavior == .transient && !view.popover.animates,
                "Given first presentation, Then a complete fixed native popover reserves preview/status/error/control space")
    print("Given observed Pending, When AppKit renders, Then label/badge/template/density contracts pass")
    let recognitionWindow = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 120, height: 100),
        styleMask: .borderless, backing: .buffered, defer: false
    )
    recognitionWindow.isReleasedWhenClosed = false
    recognitionWindow.contentView?.addSubview(view.button)
    defer {
        view.button.removeFromSuperview()
        recognitionWindow.close()
    }
    let point = view.button.convert(NSPoint(x: 12, y: 12), to: nil)
    for type in [NSEvent.EventType.rightMouseDown, .rightMouseUp] {
        guard let event = NSEvent.mouseEvent(
            with: type, location: point, modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: recognitionWindow.windowNumber, context: nil,
            eventNumber: type == .rightMouseDown ? 1 : 2, clickCount: 1,
            pressure: type == .rightMouseDown ? 1 : 0
        ) else { throw MenuContractFailure.failed("Cannot construct local recognition event") }
        recognitionWindow.sendEvent(event)
    }
    try require(view.menuAnchor == view.button.bounds && inputs.isEmpty,
                "Given a standalone right-click, When the real recognizer receives local window events, Then context presentation occurs without a toggle")
    try require(!recognitionWindow.isVisible,
                "Recognition proof must never present or activate a status item or test window")
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
    for (event, expected, rejected, required) in [
        (controlClick, view.controlPrimary, view.secondary, 1),
        (rightClick, view.secondary, view.controlPrimary, 2),
    ] {
        try require(expected.buttonMask == required &&
                    expected.view === view.button && expected.target === view &&
                    expected.action == view.secondary.action &&
                    view.gestureRecognizer(expected, shouldAttemptToRecognizeWith: event) &&
                    !view.gestureRecognizer(rejected, shouldAttemptToRecognizeWith: event) &&
                    !view.gestureRecognizer(view.primary, shouldAttemptToRecognizeWith: event) &&
                    !view.gestureRecognizer(view.longPress, shouldAttemptToRecognizeWith: event),
                    "Context input \(event.type.rawValue): required=\(required), mask=\(expected.buttonMask), attached=\(expected.view === view.button), target=\(expected.target === view), selector=\(expected.action == view.secondary.action), accepted=\(view.gestureRecognizer(expected, shouldAttemptToRecognizeWith: event)), rejected=\(view.gestureRecognizer(rejected, shouldAttemptToRecognizeWith: event))")
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
    awake["badge"] = ""
    awake["accessibility"] = "AI2ScreenWake protection: On"
    view.render(text: awake, flags: flags)
    try requirePreview("on")
    try require(view.preview.image !== asleep && view.button.title.isEmpty &&
                view.preview.image?.isTemplate == true && view.popover.contentSize == initialSize &&
                view.button.frame.size == initialButtonSize,
                "Later observed On selects a large template without moving the reserved first-open layout")
    var error = text
    error["badge"] = "!"
    error["status"] = "Status: Error"
    error["error"] = "Protection could not be confirmed."
    error["accessibility"] = "AI2ScreenWake protection: Error. Protection could not be confirmed."
    var off = text
    off["status"] = "Status: Off"
    off["badge"] = ""
    off["accessibility"] = "AI2ScreenWake protection: Off"
    for frame in [error, text, awake, off] {
        view.render(text: frame, flags: flags)
        try require(view.button.title == frame["badge"] &&
                    view.button.frame.size == initialButtonSize &&
                    view.popover.contentSize == initialSize,
                    "Observed empty, dots and exclamation badges cannot resize the production-configured anchor")
        try requirePreview(frame["preview_asset"] == "preview-on" ? "on" : "off")
    }
    try require(view.preview.image === asleep, "Returning Off must restore the matching asleep preview")
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
