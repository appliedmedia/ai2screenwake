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
        contentRect: NSRect(x: 0, y: 0, width: 176, height: 188),
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
    view.button.performClick(nil)
    try require(inputs.isEmpty && !view.button.isEnabled,
                "Given no observed frame, Then opening waits for truthful complete content")
    let initialButtonSize = view.button.frame.size
    let oldMask = view.button.sendAction(on: [.leftMouseUp])
    view.button.sendAction(on: NSEvent.EventTypeMask(rawValue: UInt64(oldMask)))
    try require(oldMask == Int(NSEvent.EventTypeMask.leftMouseUp.rawValue),
                "ReleaseTrackingOpensHeldContext: production target/action admits ordinary mouse-up, not recognizer-only delivery")
    let text = [
        "status": "Can Sleep", "icon_asset": "menu-off", "preview_asset": "preview-off", "badge": "",
        "accessibility": "AI2ScreenWake protection: Pending", "show_log": "Show Log...", "open_menu": "Open menu",
        "quit": "Quit", "error": "", "tooltip": "Pending. Screensaver protection is unverified.",
        "cycle_preview": "Cycle TV: No Sleep, Can Sleep, Error preview",
        "log_title": "20261117tu100720_ai2sw.log", "log_text": "/private/example/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log\n0  Power:Session:Query",
    ]
    var flags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                 "toggle_enabled": true, "activate_log": false]
    let observed = ObservedMenu()
    observed.onTermination = { exits += 1 }
    observed.receive(text: text, flags: flags)
    observed.attach(view)
    let initialSize = view.popover.contentSize
    try require(view.statusLine.stringValue == "Can Sleep" && !view.statusLine.isEditable,
                "Given observed Pending, Then the native status is truthful and non-actionable")
    print("SLEEP_MENU_RENDER inputs=\(inputs) fitting=\(view.showLogItem.fittingSize)/\(view.quitItem.fittingSize) value=\(String(describing: view.button.accessibilityValue())) template=\(String(describing: view.button.image?.isTemplate)) title=\(view.button.title)")
    try require(view.button.title.isEmpty && view.button.image?.isTemplate == true &&
                view.button.accessibilityValue() as? String == "Can Sleep",
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
    guard let previewCell = view.preview.cell as? NSButtonCell else {
        throw MenuContractFailure.failed("Missing actual large-TV native cell")
    }
    let previewRect = previewCell.imageRect(forBounds: view.preview.bounds)
    try require(view.preview.bounds.contains(previewRect) && previewRect.size == NSSize(width: 96, height: 96),
                "The actual preview cell fits the full 96-point image without clipping: \(previewRect)")
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
    let trackingWindow = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 100, height: 100),
        styleMask: .borderless, backing: .buffered, defer: false
    )
    trackingWindow.isReleasedWhenClosed = false
    trackingWindow.contentView?.addSubview(pointerView.button)
    defer { pointerView.button.removeFromSuperview(); trackingWindow.close() }
    func mouse(_ type: NSEvent.EventType, outside: Bool = false, control: Bool = false) -> NSEvent {
        let point = pointerView.button.convert(
            outside ? NSPoint(x: -20, y: -20) : NSPoint(x: 11, y: 11), to: nil
        )
        return NSEvent.mouseEvent(
            with: type, location: point, modifierFlags: control ? .control : [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: trackingWindow.windowNumber, context: nil, eventNumber: 1,
            clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1
        )!
    }
    func tracking(duration: TimeInterval, control: Bool = false, drag: Bool = false, outside: Bool = false,
                  escape: Bool = false, disable: Bool = false) throws {
        let before = pointerInputs.count
        let presentations = hiddenPopover.presentations
        var noDownToggle = false
        var observedHeldMenu = false
        let check = Timer(timeInterval: duration > 0.45 ? 0.49 : 0.01, repeats: false) { _ in
            MainActor.assumeIsolated {
                noDownToggle = pointerInputs.count == before
                observedHeldMenu = hiddenPopover.presentations > presentations
                if disable {
                    var disabled = flags
                    disabled["toggle_enabled"] = false
                    pointerView.render(text: text, flags: disabled)
                }
            }
        }
        let end = Timer(timeInterval: duration, repeats: false) { _ in
            MainActor.assumeIsolated {
                NSApplication.shared.postEvent(mouse(.leftMouseUp, outside: outside), atStart: false)
            }
        }
        RunLoop.main.add(check, forMode: .eventTracking)
        RunLoop.main.add(end, forMode: .eventTracking)
        if drag { NSApplication.shared.postEvent(mouse(.leftMouseDragged, outside: true), atStart: false) }
        if escape {
            let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: trackingWindow.windowNumber,
                context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}", isARepeat: false,
                keyCode: 53)!
            NSApplication.shared.postEvent(event, atStart: false)
        }
        let hit = pointerView.button.hitTest(NSPoint(x: 11, y: 11))
        try require(hit === pointerView.inputView,
                    "The production button hit-tests to its transparent public pointer view")
        hit?.mouseDown(with: mouse(.leftMouseDown, control: control))
        check.invalidate()
        end.invalidate()
        try require(noDownToggle || drag || escape, "ReleaseTrackingOpensHeldContext: no Toggle before actual mouse-up")
        if duration > 0.45 && !drag && !outside {
            try require(observedHeldMenu && pointerInputs.count == before,
                        "ReleaseTrackingOpensHeldContext: actual tracking-mode timer opens while held and release never toggles")
        } else if control || drag || outside || escape || disable {
            try require(pointerInputs.count == before, "Context, drag and outside release never toggle")
        } else {
            try require(pointerInputs.count == before + 1 && pointerInputs.last == .toggle,
                        "Exactly one production target/action fires on short release")
        }
        let settled = pointerInputs.count
        hit?.mouseUp(with: mouse(.leftMouseUp))
        hit?.mouseUp(with: mouse(.leftMouseUp))
        try require(pointerInputs.count == settled, "Late physical release callbacks cannot duplicate an activation")
        if disable { pointerView.render(text: text, flags: flags) }
    }
    try tracking(duration: 0.04)
    try tracking(duration: 0.55)
    try tracking(duration: 0.04, control: true)
    try tracking(duration: 0.55, drag: true)
    try tracking(duration: 0.04, outside: true)
    try tracking(duration: 0.04, escape: true)
    try tracking(duration: 0.04, disable: true)
    try tracking(duration: 0.04)
    try require(!trackingWindow.isVisible && !hiddenPopover.host.isVisible &&
                hiddenPopover.host.firstResponder === hiddenPopover.contentViewController?.view &&
                pointerView.popover === hiddenPopover &&
                hiddenPopover.contentViewController?.view.nextKeyView === pointerView.preview &&
                pointerView.preview.nextKeyView === pointerView.showLogItem &&
                pointerView.showLogItem.nextKeyView === pointerView.quitItem &&
                pointerView.quitItem.nextKeyView === pointerView.preview,
                "Neutral first focus and preview/Show Log/Quit navigation use one never-visible native surface")
    for action in [pointerView.preview, pointerView.showLogItem, pointerView.quitItem] {
        try require(hiddenPopover.host.makeFirstResponder(action) && action.focusRingType == .default,
                    "All three native controls retain actual responder eligibility and focus rings")
    }
    pointerView.preview.performClick(nil)
    try require(pointerInputs.last == .cyclePreview && pointerView.preview.accessibilityRole() == .button,
                "The large TV forwards one generated cycle intent and has native accessible button semantics: inputs=\(pointerInputs), role=\(String(describing: pointerView.preview.accessibilityRole()))")
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
    let orderedControls: [NSView] = [view.showLogItem, view.statusLine, view.preview]
    let gaps = zip(orderedControls, orderedControls.dropFirst()).map { $1.frame.minY - $0.frame.maxY }
    let compact = content.subviews.count == 4 &&
        content.bounds.width <= view.preview.frame.width + 80 &&
        content.bounds.height < 224 && content.bounds.height <= 188 &&
        orderedControls.allSatisfy { content.bounds.contains($0.frame) } &&
        content.bounds.contains(view.quitItem.frame) &&
        view.showLogItem.frame.maxX + 6 <= view.quitItem.frame.minX &&
        view.showLogItem.frame.minY == view.quitItem.frame.minY &&
        gaps.allSatisfy { $0 >= 4 && $0 <= 12 } &&
        [view.statusLine, view.preview].allSatisfy { abs($0.frame.midX - content.bounds.midX) < 0.5 } &&
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
    try require(view.menuAnchor == view.button.bounds,
                "The native upward-beak popover is positioned from the actual status-button bounds, not a screen origin")
    view.button.performClick(nil)
    try require(inputs == [.toggle],
                "Own-process AppKit keyboard/accessibility activation must reach the generated toggle")
    try require(view.button.menu == nil &&
                view.button.accessibilityCustomActions()?.first?.handler?() == true,
                "The native menu must remain reachable through context and accessibility actions")
    print("Given native hit testing and tracking-mode timing, When short held and cancelled input resolves, Then no double-toggle occurs")
    flags["toggle_enabled"] = false
    view.render(text: text, flags: flags)
    let beforeDisabledClick = inputs.count
    view.button.performClick(nil)
    view.preview.performClick(nil)
    try require(inputs.count == beforeDisabledClick,
                "Keyboard/accessibility activation must also respect observed primary availability")
    flags["log_visible"] = true
    flags["activate_log"] = true
    view.render(text: text, flags: flags)
    try require(view.logTextView?.isEditable == false && view.logTextView?.isSelectable == true,
                "The auxiliary AppKit log must be read-only and selectable")
    try require(view.logTextView?.string == text["log_text"],
                "Only the Rust-projected log text may be displayed")
    view.logTextView?.setSelectedRange(NSRange(location: 4, length: 5))
    flags["activate_log"] = false
    view.render(text: text, flags: flags)
    try require(view.logTextView?.selectedRange() == NSRange(location: 4, length: 5),
                "Identical unrelated render preserves the reader's actual selection")
    flags["activate_log"] = true
    view.render(text: text, flags: flags)
    try require(view.logTextView?.selectedRange() == NSRange(location: 0, length: 0),
                "Explicit reopen/refresh reveals the actual newest text at the top")
    try require(view.logWindow?.title == "20261117tu100720_ai2sw.log" &&
                view.logTextView?.string.components(separatedBy: "\n").first == "/private/example/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log",
                "The read-only .log viewer preserves the real export's full-path first line")
    var awake = text
    awake["status"] = "No Sleep"
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
    error["status"] = "Error"
    error["error"] = "Protection could not be confirmed."
    error["accessibility"] = "AI2ScreenWake status: Error. Observed protection: On. Protection could not be confirmed."
    error["tooltip"] = error["accessibility"]
    view.render(text: error, flags: flags)
    try requirePreview("error")
    try require(view.button.title.isEmpty && view.statusLine.stringValue == "Error" &&
                view.button.accessibilityLabel() == error["accessibility"] &&
                view.button.toolTip == error["tooltip"] &&
                view.statusLine.toolTip == error["tooltip"] &&
                view.button.accessibilityHelp() == error["tooltip"] &&
                view.statusLine.accessibilityHelp() == error["tooltip"],
                "Observed attention retains core-owned error and power detail in tooltip/AX help without a separate error row")
    var off = text
    off["status"] = "Can Sleep"
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
