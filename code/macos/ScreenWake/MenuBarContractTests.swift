// MenuBarContractTests.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
#if SCREENWAKE_CONTRACT_TESTS
import AppKit

enum MenuContractFailure: Error {
    case failed(String)
}

@MainActor
final class MenuTrackingStatusButton: NSStatusBarButton {
    var actionTypes: [NSEvent.EventType] = []

    override func sendAction(_ action: Selector?, to target: Any?) -> Bool {
        if let event = NSApplication.shared.currentEvent { actionTypes.append(event.type) }
        return super.sendAction(action, to: target)
    }
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
    let routePopover = MenuContractPopover()
    var routeInputs: [NativeIntent] = []
    var openedURLs: [String] = []
    var acceptURL = true
    let routeView = try MenuBarController(
        assets: assets, displaySurfaces: true, popover: routePopover,
        makeStatusButton: { _ in
            let size = NSStatusBar.system.thickness
            return (nil, MenuTrackingStatusButton(frame: NSRect(x: 0, y: 0, width: size, height: size)))
        },
        openURL: { url in openedURLs.append(url.absoluteString); return acceptURL },
        emit: { routeInputs.append($0) }, terminate: {}
    )
    let routeWindow = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 100, height: 100),
        styleMask: .borderless, backing: .buffered, defer: false
    )
    routeWindow.isReleasedWhenClosed = false
    routeWindow.contentView?.addSubview(routeView.button)
    defer { routeView.button.removeFromSuperview(); routeWindow.close(); routePopover.host.close() }
    let routeText = [
        "status": "Mode: Can Sleep", "icon_asset": "menu-off", "preview_asset": "preview-off", "badge": "",
        "accessibility": "AI2ScreenWake protection: Off", "show_log": "Show Log...", "open_menu": "Open menu",
        "quit": "Quit", "error": "", "tooltip": "Off", "cycle_preview": "Cycle TV",
        "log_title": "ai2sw.log", "log_text": "", "version": buildVersion(),
        "info_label": "About AI2ScreenWake", "info_help": "Open the AI2ScreenWake website",
        "info_url": "https://ai2sw.info",
    ]
    let routeFlags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                      "toggle_enabled": true, "activate_log": false]
    routeView.render(text: routeText, flags: routeFlags)
    func routed(_ type: NSEvent.EventType, x: CGFloat = 11, control: Bool = false) -> NSEvent {
        NSEvent.mouseEvent(
            with: type, location: routeView.button.convert(NSPoint(x: x, y: 11), to: nil),
            modifierFlags: control ? .control : [], timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: routeWindow.windowNumber, context: nil, eventNumber: 1,
            clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1
        )!
    }
    func pumpRoute(until deadline: Date) {
        while Date() < deadline {
            if let event = NSApplication.shared.nextEvent(
                matching: .any, until: Date(timeIntervalSinceNow: 0.005), inMode: .default, dequeue: true
            ) {
                NSApplication.shared.sendEvent(event)
            }
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.001))
        }
    }
    func dequeueOwn(_ event: NSEvent) throws -> NSEvent {
        NSApplication.shared.postEvent(event, atStart: true)
        guard let next = NSApplication.shared.nextEvent(
            matching: NSEvent.EventTypeMask(rawValue: UInt64(1) << event.type.rawValue),
            until: Date(timeIntervalSinceNow: 0.1), inMode: .default, dequeue: true
        ) else {
            throw MenuContractFailure.failed("The own-process queue did not return \(event.type)")
        }
        try require(next.type == event.type && next.windowNumber == event.windowNumber,
                    "Own queue expected \(event), got \(next), current \(String(describing: NSApplication.shared.currentEvent))")
        return next
    }
    func routeCase(_ name: String, held: Bool = false, control: Bool = false, tracking: Bool = true,
                   dragX: CGFloat? = nil, outside: Bool = false,
                   escape: Bool = false, disable: Bool = false, refreshDisabled: Bool = false) throws {
        let before = routeInputs.count
        let opened = routePopover.presentations
        var noDownToggle = false
        var openedBeforeUp = false
        let inspect = Timer(timeInterval: held ? 0.49 : 0.01, repeats: false) { _ in
            MainActor.assumeIsolated {
                noDownToggle = routeInputs.count == before
                openedBeforeUp = routePopover.presentations == opened + 1
            }
        }
        let release = Timer(timeInterval: held ? 0.55 : 0.03, repeats: false) { _ in
            MainActor.assumeIsolated {
                NSApplication.shared.postEvent(routed(.leftMouseUp, x: outside ? -20 : 11), atStart: false)
            }
        }
        let change = Timer(timeInterval: 0.005, repeats: false) { _ in
            MainActor.assumeIsolated {
                if let dragX {
                    NSApplication.shared.postEvent(routed(.leftMouseDragged, x: dragX), atStart: false)
                }
                if escape {
                    NSApplication.shared.postEvent(NSEvent.keyEvent(
                        with: .keyDown, location: .zero, modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: routeWindow.windowNumber,
                        context: nil, characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}",
                        isARepeat: false, keyCode: 53
                    )!, atStart: false)
                }
                if disable {
                    var disabled = routeFlags
                    disabled["toggle_enabled"] = false
                    routeView.render(text: routeText, flags: disabled)
                }
                if refreshDisabled {
                    var disabled = routeFlags
                    disabled["toggle_enabled"] = false
                    routeView.render(text: routeText, flags: disabled)
                }
            }
        }
        for timer in [inspect, release, change] {
            RunLoop.main.add(timer, forMode: .default)
            RunLoop.main.add(timer, forMode: .eventTracking)
        }
        let sent = ProcessInfo.processInfo.systemUptime
        let deadline = Date(timeIntervalSinceNow: held ? 0.6 : 0.06)
        let actionCount = (routeView.button as! MenuTrackingStatusButton).actionTypes.count
        let down = try dequeueOwn(routed(.leftMouseDown, control: control))
        if tracking {
            // Exercise the real AppKit button tracker, not the controller's local monitor.
            routeView.button.mouseDown(with: down)
            try require(ProcessInfo.processInfo.systemUptime - sent < 0.8,
                        "AppKit tracking must finish after its own queued release")
        } else {
            NSApplication.shared.sendEvent(down)
            try require(ProcessInfo.processInfo.systemUptime - sent < 0.05,
                        "ActualStatusButtonRoute: Down must return without a nested tracking/event wait")
        }
        pumpRoute(until: deadline)
        inspect.invalidate()
        release.invalidate()
        change.invalidate()
        let cancelled = escape || disable || outside || (dragX != nil && abs(dragX! - 11) > 4)
        let context = control || (held && !cancelled)
        let expected = before + (context || cancelled ? 0 : 1)
        if !noDownToggle || routeInputs.count != expected ||
            (context && !openedBeforeUp) || routePopover.presentations != opened + (context ? 1 : 0) {
            nativeFailures.append("ActualStatusButtonRoute: \(name), downUnchanged=\(noDownToggle), before=\(before), after=\(routeInputs.count), openedBeforeUp=\(openedBeforeUp)")
        }
        let settled = routeInputs.count
        if tracking {
            try require((routeView.button as! MenuTrackingStatusButton).actionTypes.dropFirst(actionCount).contains(.leftMouseDown),
                        "The native AppKit status-button tracker must deliver its Down action for \(name)")
        }
        NSApplication.shared.sendEvent(try dequeueOwn(routed(.leftMouseUp)))
        NSApplication.shared.sendEvent(try dequeueOwn(routed(.leftMouseUp)))
        try require(routeInputs.count == settled, "Repeated late Up cannot toggle after \(name)")
        routeView.render(text: routeText, flags: routeFlags)
        print("ACTUAL_STATUS_ROUTE class=\(type(of: routeView.button)) case=\(name) downUnchanged=\(noDownToggle) inputs=\(routeInputs.count - before) beforeUp=\(openedBeforeUp)")
    }
    try routeCase("short")
    try routeCase("hold", held: true)
    try routeCase("inside-jitter-hold", held: true, dragX: 12)
    try routeCase("control", control: true)
    try routeCase("deliberate-drag", held: true, dragX: 18)
    try routeCase("leave-bounds", held: true, dragX: -20)
    try routeCase("outside-release", outside: true)
    try routeCase("escape", escape: true)
    try routeCase("disabled", held: true, disable: true)
    try routeCase("recovered")
    var menuOnlyFlags = routeFlags
    menuOnlyFlags["toggle_enabled"] = false
    routeView.render(text: routeText, flags: menuOnlyFlags)
    try routeCase("disabled-new-control", control: true)
    routeView.render(text: routeText, flags: menuOnlyFlags)
    try routeCase("disabled-new-hold", held: true, refreshDisabled: true)
    try routeCase("application-window-short", tracking: false)
    try routeCase("application-window-hold", held: true, tracking: false)
    if routeView.popover.contentSize.height > 178 {
        nativeFailures.append("CompactTopSlack: actual menu height \(routeView.popover.contentSize.height) still exceeds 178")
    }
    if routeView.popover.contentViewController?.view.subviews.compactMap({ $0 as? NSTextField })
        .contains(where: { $0.stringValue == buildVersion() }) != true {
        nativeFailures.append("FrozenVersionFooter: no Rust-projected development version is rendered")
    }
    let beforeInfo = routeInputs.count
    routeView.infoItem.performClick(nil)
    try require(Array(routeInputs.dropFirst(beforeInfo)) == [.openInfo] && openedURLs.isEmpty,
                "Information activation emits intent before any URL side effect")
    try require(routeView.infoItem.accessibilityLabel() == routeText["info_label"] &&
                routeView.infoItem.accessibilityHelp() == routeText["info_help"] &&
                routeView.infoItem.contentTintColor == .systemBlue,
                "The native info-circle has accessible name/help and semantic blue tint")
    guard let infoCell = routeView.infoItem.cell as? NSButtonCell else {
        throw MenuContractFailure.failed("Missing native information button cell")
    }
    let infoRect = infoCell.imageRect(forBounds: routeView.infoItem.bounds)
    try require(routeView.infoItem.bounds.contains(infoRect) && infoRect.width > 0 && infoRect.height > 0,
                "The full native information symbol fits its measured top-right frame")
    print("NATIVE_INFO_FOOTER symbol=\(infoRect) footer=\(routeView.versionLine.stringValue) fitting=\(routeView.versionLine.fittingSize) frame=\(routeView.versionLine.frame)")
    var effectFlags = routeFlags
    effectFlags["open_info"] = true
    routeView.render(text: routeText, flags: effectFlags)
    try require(openedURLs == ["https://ai2sw.info"] && routeInputs.last == .infoOpened,
                "Only a logged rendering effect opens the exact website and reports actual acceptance")
    acceptURL = false
    routeView.render(text: routeText, flags: effectFlags)
    try require(openedURLs == ["https://ai2sw.info", "https://ai2sw.info"] && routeInputs.last == .infoFailed,
                "Native workspace refusal reports typed failure instead of successful opening")
    var invalidInfo = routeText
    invalidInfo["info_url"] = "https://example.invalid"
    routeView.render(text: invalidInfo, flags: effectFlags)
    try require(openedURLs.count == 2 && routeInputs.last == .infoFailed,
                "An unexpected target never becomes an unlogged or substituted external side effect")
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
    let primaryPhases = Int(NSEvent.EventTypeMask([.leftMouseDown, .leftMouseUp]).rawValue)
    try require(oldMask & primaryPhases == primaryPhases,
                "The AppKit action mask must include Down and Up, actual mask \(oldMask)")
    let text = [
        "status": "Mode: Can Sleep", "icon_asset": "menu-off", "preview_asset": "preview-off", "badge": "",
        "accessibility": "AI2ScreenWake protection: Pending", "show_log": "Show Log...", "open_menu": "Open menu",
        "quit": "Quit", "error": "", "tooltip": "Pending. Screensaver protection is unverified.",
        "cycle_preview": "Cycle TV: No Sleep, Can Sleep, Error preview",
        "version": buildVersion(), "info_label": "About AI2ScreenWake",
        "info_help": "Open the AI2ScreenWake website", "info_url": "https://ai2sw.info",
        "log_title": "20261117tu100720_ai2sw.log", "log_text": "/private/example/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log\n0  Power:Session:Protection:Query",
    ]
    var flags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                 "toggle_enabled": true, "activate_log": false]
    let observed = ObservedMenu()
    observed.onTermination = { exits += 1 }
    observed.receive(text: text, flags: flags)
    observed.attach(view)
    let initialSize = view.popover.contentSize
    try require(view.statusLine.stringValue == "Mode: Can Sleep" && !view.statusLine.isEditable,
                "Given observed Pending, Then the native status is truthful and non-actionable")
    print("SLEEP_MENU_RENDER inputs=\(inputs) fitting=\(view.showLogItem.fittingSize)/\(view.quitItem.fittingSize) value=\(String(describing: view.button.accessibilityValue())) template=\(String(describing: view.button.image?.isTemplate)) title=\(view.button.title)")
    try require(view.button.title.isEmpty && view.button.image?.isTemplate == true &&
                view.button.accessibilityValue() as? String == "Mode: Can Sleep",
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
    try require(!routeWindow.isVisible && !routePopover.host.isVisible &&
                routePopover.host.firstResponder === routePopover.contentViewController?.view &&
                routeView.popover === routePopover &&
                routePopover.contentViewController?.view.nextKeyView === routeView.preview &&
                routeView.preview.nextKeyView === routeView.infoItem &&
                routeView.infoItem.nextKeyView === routeView.showLogItem &&
                routeView.showLogItem.nextKeyView === routeView.quitItem &&
                routeView.quitItem.nextKeyView === routeView.preview,
                "Neutral first focus and TV/info/Show Log/Quit navigation use one never-visible native surface")
    for action in [routeView.preview, routeView.infoItem, routeView.showLogItem, routeView.quitItem] {
        try require(routePopover.host.makeFirstResponder(action) && action.focusRingType == .default,
                    "All four native controls retain actual responder eligibility and focus rings")
    }
    let beforePreview = routeInputs.count
    routeView.preview.performClick(nil)
    try require(Array(routeInputs.dropFirst(beforePreview)) == [.cyclePreview],
                "One large TV click forwards exactly one generated cycle intent")
    try require(routeView.preview.accessibilityRole() == .button,
                "The large TV retains native accessible button semantics")
    print("NATIVE_STATUS_FOCUS_TRACE requested=\(requestedWidths) frame=\(view.button.frame) image=\(firstImageRect) neutralInitialResponder=\(String(describing: routePopover.host.initialFirstResponder)) fullKeyboardAccess=\(NSApplication.shared.isFullKeyboardAccessEnabled)")
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
    let compact = content.subviews.count == 6 &&
        content.bounds.width <= view.preview.frame.width + 80 &&
        content.bounds.height == 168 &&
        content.bounds.maxY - view.preview.frame.maxY == 2 &&
        orderedControls.allSatisfy { content.bounds.contains($0.frame) } &&
        content.bounds.contains(view.quitItem.frame) &&
        view.showLogItem.frame.maxX + 6 <= view.quitItem.frame.minX &&
        view.showLogItem.frame.minY == view.quitItem.frame.minY &&
        gaps.allSatisfy { $0 >= 4 && $0 <= 12 } &&
        [view.statusLine, view.preview].allSatisfy { abs($0.frame.midX - content.bounds.midX) < 0.5 } &&
        content.bounds.contains(view.infoItem.frame) &&
        content.bounds.contains(view.versionLine.frame) &&
        view.versionLine.frame.minY == 3 &&
        view.versionLine.font?.pointSize == 8 &&
        [view.statusLine, view.showLogItem, view.quitItem, view.versionLine].allSatisfy {
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
        NSApplication.shared.sendEvent(event)
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
    awake["status"] = "Mode: No Sleep"
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
    error["status"] = "Off: Error"
    error["error"] = "Protection could not be confirmed."
    error["accessibility"] = "AI2ScreenWake status: Error. Observed protection: On. Protection could not be confirmed."
    error["tooltip"] = error["accessibility"]
    view.render(text: error, flags: flags)
    try requirePreview("error")
    try require(view.button.title.isEmpty && view.statusLine.stringValue == "Off: Error" &&
                view.button.accessibilityLabel() == error["accessibility"] &&
                view.button.toolTip == error["tooltip"] &&
                view.statusLine.toolTip == error["tooltip"] &&
                view.button.accessibilityHelp() == error["tooltip"] &&
                view.statusLine.accessibilityHelp() == error["tooltip"],
                "Observed attention retains core-owned error and power detail in tooltip/AX help without a separate error row")
    var off = text
    off["status"] = "Mode: Can Sleep"
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
        try require(view.statusLine.stringValue == frame["status"] &&
                    view.statusLine.fittingSize.width <= view.statusLine.frame.width &&
                    view.statusLine.fittingSize.height <= view.statusLine.frame.height &&
                    initialSize == NSSize(width: 176, height: 168),
                    "LiteralModeLabelsFit: exact Rust copy must fit the approved native menu without resizing")
        print("MODE_LABEL_FIT text=\(view.statusLine.stringValue) fitting=\(view.statusLine.fittingSize) frame=\(view.statusLine.frame.size) menu=\(initialSize)")
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
