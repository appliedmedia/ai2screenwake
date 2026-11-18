// MenuBarContractTests.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
#if SCREENWAKE_CONTRACT_TESTS
import AppKit

enum MenuContractFailure: Error {
    case failed(String)
}

@MainActor
final class MenuImageButton: NSButton {
    var imageWrites = 0
    override var image: NSImage? {
        get { super.image }
        set {
            imageWrites += 1
            super.image = newValue
        }
    }
}

@MainActor
final class MenuContractPopover: NSPopover {
    let host = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 172, height: 224),
        styleMask: .borderless, backing: .buffered, defer: false
    )
    var presentations = 0

    override func show(relativeTo positioningRect: NSRect, of positioningView: NSView, preferredEdge: NSRectEdge) {
        host.isReleasedWhenClosed = false
        host.contentView = contentViewController?.view
        host.makeFirstResponder(host.initialFirstResponder)
        presentations += 1
    }
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
            let thickness = NSStatusBar.system.thickness
            let length = width == NSStatusItem.squareLength ? thickness : width
            let button = MenuImageButton(frame: NSRect(x: 0, y: 0, width: length, height: thickness))
            button.isBordered = false
            return (nil, button)
        },
        emit: { inputs.append($0) },
        terminate: { exits += 1 }
    )
    func require(_ value: @autoclosure () -> Bool, _ why: String) throws {
        if !value() { throw MenuContractFailure.failed(why) }
    }
    var nativeFailures: [String] = []
    if requestedWidths != [NSStatusItem.squareLength] {
        nativeFailures.append("NativeSquareStatusFootprint: actual factory requested \(requestedWidths), expected native squareLength")
    }
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
        "status": "Status: Pending", "icon_asset": "menu-off", "preview_asset": "preview-off", "badge": "",
        "accessibility": "AI2ScreenWake protection: Pending", "show_log": "Show Log...", "open_menu": "Open menu",
        "quit": "Quit", "error": "", "tooltip": "Screensaver protection is unverified.",
        "log_title": "20261117tu100720_ai2sw.log", "log_text": "/private/example/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log\n0  Power:Session:Query",
    ]
    var flags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                 "toggle_enabled": true, "activate_log": false]
    let observed = ObservedMenu()
    observed.onTermination = { exits += 1 }
    observed.receive(text: text, flags: flags)
    observed.attach(view)
    try require(view.statusLine.stringValue == "Status: Pending" && !view.statusLine.isEditable,
                "Given observed Pending, Then the native status is truthful and non-actionable")
    try require(view.button.title.isEmpty && view.button.image?.isTemplate == true &&
                view.button.accessibilityValue() as? String == "Status: Pending",
                "Given observed Pending, Then explicit accessible status replaces moving punctuation")
    try require(view.button.image?.representations.count == 2,
                "Given canonical assets, Then both native density representations exist")
    try require(view.preview.image?.size == NSSize(width: 96, height: 96) &&
                view.preview.image?.representations.map { $0.pixelsWide }.sorted() == [96, 192],
                "Given observed Pending, Then the same asleep TV has crisp large native densities")
    try requirePreview("off")
    let asleep = view.preview.image
    guard let content = view.popover.contentViewController?.view,
          let cell = view.button.cell as? NSButtonCell,
          let counted = view.button as? MenuImageButton else {
        throw MenuContractFailure.failed("Missing actual native content/button cell")
    }
    let firstImageRect = cell.imageRect(forBounds: view.button.bounds)
    if view.button.frame.width != view.button.frame.height ||
        !view.button.bounds.contains(firstImageRect) ||
        abs(firstImageRect.midX - view.button.bounds.midX) >= 0.5 ||
        abs(firstImageRect.midY - view.button.bounds.midY) >= 0.5 ||
        firstImageRect.width < min(24, NSStatusBar.system.thickness) ||
        firstImageRect.height < min(24, NSStatusBar.system.thickness) {
        nativeFailures.append("NativeSquareStatusFootprint: actual frame \(view.button.frame), image \(firstImageRect)")
    }
    let hiddenPopover = MenuContractPopover()
    var pointerInputs: [NativeIntent] = []
    let pointerView = try MenuBarController(
        assets: assets, displaySurfaces: true, popover: hiddenPopover,
        makeStatusButton: { _ in
            let thickness = NSStatusBar.system.thickness
            return (nil, NSButton(frame: NSRect(x: 0, y: 0, width: thickness, height: thickness)))
        },
        emit: { pointerInputs.append($0) }, terminate: {}
    )
    defer { hiddenPopover.host.close() }
    pointerView.render(text: text, flags: flags)
    for (gesture, state) in [
        (MenuBarController.Gesture.secondary, NSGestureRecognizer.State.ended),
        (.longPress, .began),
    ] {
        try require(pointerView.handle(gesture, state: state) == .menu && pointerInputs.isEmpty,
                    "Pointer context opens the same production popover without emitting an action")
        if hiddenPopover.host.firstResponder === pointerView.showLogItem ||
            hiddenPopover.host.firstResponder === pointerView.quitItem {
            nativeFailures.append("NeutralContextPopoverFocus: opening \(gesture) selected an action")
        }
        try require(hiddenPopover.host.initialFirstResponder === hiddenPopover.contentViewController?.view &&
                    hiddenPopover.host.firstResponder === hiddenPopover.contentViewController?.view,
                    "NeutralContextPopoverFocus begins on the actual non-action content, before any control is selected")
        if NSApplication.shared.isFullKeyboardAccessEnabled {
            hiddenPopover.host.selectNextKeyView(nil)
            try require(hiddenPopover.host.firstResponder === pointerView.showLogItem,
                        "Tab from neutral content reaches Show Log when native full keyboard access is enabled")
            hiddenPopover.host.selectNextKeyView(nil)
            try require(hiddenPopover.host.firstResponder === pointerView.quitItem,
                        "Tab from Show Log reaches Quit")
        }
        try require(hiddenPopover.contentViewController?.view.nextKeyView === pointerView.showLogItem &&
                    hiddenPopover.host.makeFirstResponder(pointerView.showLogItem) &&
                    hiddenPopover.host.firstResponder === pointerView.showLogItem &&
                    hiddenPopover.host.makeFirstResponder(pointerView.quitItem) &&
                    hiddenPopover.host.firstResponder === pointerView.quitItem,
                    "Both actions retain their native responder eligibility and the ordered key-view chain")
    }
    try require(hiddenPopover.presentations == 2 && !hiddenPopover.host.isVisible &&
                pointerView.popover === hiddenPopover &&
                pointerView.showLogItem.nextKeyView === pointerView.quitItem &&
                pointerView.quitItem.nextKeyView === pointerView.showLogItem &&
                pointerView.showLogItem.focusRingType == .default &&
                pointerView.quitItem.focusRingType == .default,
                "The nonpresenting pointer witness retains one surface, native focus rings and keyboard navigation")
    let firstRelease = pointerView.handle(.primary, state: .ended)
    let lateRelease = pointerView.handle(.primary, state: .ended)
    if firstRelease != .none || lateRelease != .none || !pointerInputs.isEmpty {
        nativeFailures.append("PrimaryHoldContextReleaseGate: repeated primary completion after a hold emitted \(pointerInputs)")
    }
    try require(pointerView.longPress.view === pointerView.button &&
                pointerView.longPress.target === pointerView &&
                pointerView.longPress.action == NSSelectorFromString("pressed:") &&
                pointerView.longPress.buttonMask == 1 &&
                pointerView.longPress.minimumPressDuration == 0.45 &&
                pointerView.gestureRecognizer(pointerView.primary, shouldRequireFailureOf: pointerView.longPress),
                "PrimaryHoldContextReleaseGate retains the actual attached press target, duration and failure priority")
    let beforeCancel = pointerInputs.count
    try require(pointerView.handle(.longPress, state: .cancelled) == .none &&
                pointerView.handle(.primary, state: .ended) == .none &&
                pointerInputs.count == beforeCancel,
                "A cancelled hold and any later primary callback cannot toggle")
    guard let down = NSEvent.mouseEvent(with: .leftMouseDown, location: .zero,
            modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
            eventNumber: 0, clickCount: 1, pressure: 1),
          let drag = NSEvent.mouseEvent(with: .leftMouseDragged, location: .zero,
            modifierFlags: [], timestamp: 0.5, windowNumber: 0, context: nil,
            eventNumber: 1, clickCount: 1, pressure: 1) else {
        throw MenuContractFailure.failed("Cannot construct local primary gesture admission events")
    }
    try require(pointerView.gestureRecognizer(pointerView.longPress, shouldAttemptToRecognizeWith: down) &&
                pointerView.handle(.primary, state: .began) == .none &&
                pointerView.handle(.longPress, state: .began) == .menu &&
                pointerView.gestureRecognizer(pointerView.longPress, shouldAttemptToRecognizeWith: drag) &&
                pointerView.handle(.longPress, state: .changed) == .none &&
                pointerView.handle(.longPress, state: .cancelled) == .none &&
                pointerView.handle(.primary, state: .ended) == .none &&
                pointerInputs.isEmpty,
                "PrimaryHoldContextReleaseGate opens the same popover on hold, never down, drag, cancellation or late release")
    try require(pointerView.gestureRecognizer(pointerView.primary, shouldAttemptToRecognizeWith: down) &&
                pointerView.handle(.longPress, state: .failed) == .none &&
                pointerView.handle(.primary, state: .ended) == .toggle &&
                pointerInputs == [.toggle],
                "A fresh ordinary short click after a hold emits exactly one typed toggle")
    print("NATIVE_STATUS_FOCUS_TRACE requested=\(requestedWidths) frame=\(view.button.frame) image=\(firstImageRect) neutralInitialResponder=\(String(describing: hiddenPopover.host.initialFirstResponder)) fullKeyboardAccess=\(NSApplication.shared.isFullKeyboardAccessEnabled)")
    try require(nativeFailures.isEmpty, nativeFailures.joined(separator: "\n"))
    let firstWrites = counted.imageWrites
    view.render(text: text, flags: flags)
    var legacyPending = text
    legacyPending["badge"] = "..."
    view.render(text: legacyPending, flags: flags)
    let pendingImageRect = cell.imageRect(forBounds: view.button.bounds)
    let repeatedWrites = counted.imageWrites - firstWrites
    view.render(text: text, flags: flags)
    let orderedControls: [NSView] = [view.quitItem, view.showLogItem, view.statusLine, view.preview]
    let gaps = zip(orderedControls, orderedControls.dropFirst()).map { $1.frame.minY - $0.frame.maxY }
    let compact = content.subviews.count == 4 &&
        content.bounds.width <= view.preview.frame.width + 80 &&
        content.bounds.height <= orderedControls.reduce(CGFloat(0)) { $0 + $1.frame.height } + 48 &&
        orderedControls.allSatisfy { content.bounds.contains($0.frame) } &&
        gaps.allSatisfy { $0 >= 4 && $0 <= 12 } &&
        orderedControls.allSatisfy { abs($0.frame.midX - content.bounds.midX) < 0.5 } &&
        [view.statusLine, view.showLogItem, view.quitItem].allSatisfy {
            $0.fittingSize.width <= $0.frame.width && $0.fittingSize.height <= $0.frame.height
        }
    print("NATIVE_COMPACT_TRACE size=\(initialSize) subviews=\(content.subviews.count) gaps=\(gaps) image=\(firstImageRect) pendingTitleImage=\(pendingImageRect) repeatedImageWrites=\(repeatedWrites)")
    try require(compact && firstImageRect == pendingImageRect && repeatedWrites == 0 &&
                abs(firstImageRect.midX - view.button.bounds.midX) < 0.5 &&
                view.popover.contentSize == initialSize &&
                view.popover.behavior == .transient && !view.popover.animates,
                "Given actual native frames and repeated Pending observations, Then compact complete content has no error row/gap, images stay centered and cached without redundant assignments")
    print("Given observed Pending, When AppKit renders, Then compact geometry and stable cached image contracts pass")
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
    let point = view.button.convert(NSPoint(x: view.button.bounds.midX, y: view.button.bounds.midY), to: nil)
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
    try require(view.logWindow?.title == "20261117tu100720_ai2sw.log" &&
                view.logTextView?.string.components(separatedBy: "\n").first == "/private/example/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log",
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
    error["badge"] = ""
    error["icon_asset"] = "menu-error"
    error["preview_asset"] = "preview-error"
    error["status"] = "Status: Error"
    error["error"] = "Protection could not be confirmed."
    error["accessibility"] = "AI2ScreenWake status: Error. Observed protection: On. Protection could not be confirmed."
    error["tooltip"] = error["accessibility"]
    view.render(text: error, flags: flags)
    try requirePreview("error")
    try require(view.button.title.isEmpty && view.statusLine.stringValue == "Status: Error" &&
                view.button.accessibilityLabel() == error["accessibility"] &&
                view.button.toolTip == error["tooltip"] &&
                view.statusLine.toolTip == error["tooltip"] &&
                view.button.accessibilityHelp() == error["tooltip"] &&
                view.statusLine.accessibilityHelp() == error["tooltip"],
                "Observed attention retains core-owned error and power detail in tooltip/AX help without a separate error row")
    var off = text
    off["status"] = "Status: Off"
    off["badge"] = ""
    off["accessibility"] = "AI2ScreenWake protection: Off"
    for (frame, state) in [(error, "error"), (text, "off"), (awake, "on"), (off, "off")] {
        view.render(text: frame, flags: flags)
        try require(view.button.title == frame["badge"] &&
                    view.button.frame.size == initialButtonSize &&
                    view.popover.contentSize == initialSize &&
                    cell.imageRect(forBounds: view.button.bounds) == firstImageRect,
                    "Observed ordinary, Pending and Error faces cannot resize the production-configured anchor")
        try requirePreview(state)
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
