// MenuBarContractTests.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
#if SCREENWAKE_CONTRACT_TESTS
import AppKit
import Darwin

@MainActor
final class MenuContractApplication: StatusApplication {
    var deliveredKeys: [NSEvent] = []
    var dequeuedKeys: [NSEvent] = []
    var keyDequeuesAtMouseUp: [Int] = []
    var mouseUpBoundary: ((NSEvent) -> Void)?
    var keyDeliveryBoundary: ((NSEvent) -> Void)?
    var lastMouseUp: NSEvent?

    override func nextEvent(matching mask: NSEvent.EventTypeMask, until expiration: Date?,
                            inMode mode: RunLoop.Mode, dequeue flag: Bool) -> NSEvent? {
        let event = super.nextEvent(matching: mask, until: expiration, inMode: mode, dequeue: flag)
        if flag, let event, event.type == .keyDown { dequeuedKeys.append(event) }
        if flag, let event, event.type == .leftMouseUp {
            keyDequeuesAtMouseUp.append(dequeuedKeys.count)
            lastMouseUp = event
            mouseUpBoundary?(event)
        }
        return event
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown { deliveredKeys.append(event) }
        if event.type == .keyDown { keyDeliveryBoundary?(event) }
        super.sendEvent(event)
    }

}

@MainActor
final class OriginalKeyEvents {
    var values: [NSEvent] = []
}

@MainActor
final class MenuContractPointer {
    var buttons = 0
    var buttonReads = 0
    var location = NSPoint(x: 3000, y: 4000)
}

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
    guard let nativeApplication = MenuContractApplication.shared as? MenuContractApplication else {
        throw MenuContractFailure.failed("The native event delivery witness must use the shared contract application")
    }
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
    let pointer = MenuContractPointer()
    var toggleButtonStates: [Int] = []
    var openedURLs: [String] = []
    var acceptURL = true
    let routeView = try MenuBarController(
        assets: assets, displaySurfaces: true, popover: routePopover,
        makeStatusButton: { _ in
            let size = NSStatusBar.system.thickness
            return (nil, NSStatusBarButton(frame: NSRect(x: 0, y: 0, width: size, height: size)))
        },
        openURL: { url in openedURLs.append(url.absoluteString); return acceptURL },
        pressedMouseButtons: {
            pointer.buttonReads += 1
            return pointer.buttons
        },
        mouseLocation: { pointer.location },
        emit: {
            routeInputs.append($0)
            if $0 == .toggle { toggleButtonStates.append(pointer.buttons) }
        }, terminate: {}
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
        "corner_help": "Prevent corner sleep: Install",
    ]
    let routeFlags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                      "toggle_enabled": true, "activate_log": false, "setup_enabled": true,
                      "corner_installed": false, "corner_on": true]
    routeView.render(text: routeText, flags: routeFlags)
    func routed(_ type: NSEvent.EventType, x: CGFloat = 11, control: Bool = false,
                timestamp: TimeInterval? = nil) -> NSEvent {
        NSEvent.mouseEvent(
            with: type, location: routeView.button.convert(NSPoint(x: x, y: 11), to: nil),
            modifierFlags: control ? .control : [], timestamp: timestamp ?? ProcessInfo.processInfo.systemUptime,
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
    func verifyUnrelatedDiscard(before marker: NSEvent, context: String) throws {
        let delivered = nativeApplication.deliveredKeys.count
        let key = NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: [], timestamp: marker.timestamp - 1,
            windowNumber: marker.windowNumber, context: nil, characters: "z",
            charactersIgnoringModifiers: "z", isARepeat: false, keyCode: 6
        )!
        NSApplication.shared.postEvent(key, atStart: true)
        try require(NSApplication.shared.nextEvent(
            matching: .keyDown, until: .distantPast, inMode: .eventTracking, dequeue: false
        ) != nil, "The unrelated discard fixture must actually be queued")
        NSApplication.shared.discardEvents(matching: .keyDown, before: marker)
        try require(NSApplication.shared.nextEvent(
            matching: .keyDown, until: .distantPast, inMode: .eventTracking, dequeue: false
        ) == nil && nativeApplication.deliveredKeys.count == delivered,
                    "Out-of-scope discard must retain the original AppKit behavior for \(context)")
        print("NATIVE_UNRELATED_DISCARD context=\(context) original_behavior=true")
    }
    func releaseRaceCase(_ name: String, nextPress: Bool) throws {
        routeView.render(text: routeText, flags: routeFlags)
        let before = routeInputs.count
        let opened = routePopover.presentations
        let togglesBefore = toggleButtonStates.count
        let image = routeView.button.image
        pointer.buttons = 1
        pointer.location = NSPoint(x: 3000, y: 4000)
        let down = routed(.leftMouseDown)
        NSApplication.shared.sendEvent(down)
        NSApplication.shared.sendEvent(routed(.leftMouseUp, timestamp: down.timestamp))
        try require(routeInputs.count == before, "Synthetic Up never resolves the first press in \(name)")
        // Keep both hardware transitions between actual production timer deliveries.
        Thread.sleep(forTimeInterval: 0.08)
        pointer.buttons = 0
        let releasedAt = ProcessInfo.processInfo.systemUptime
        var gap: TimeInterval = 0
        if nextPress {
            let reads = pointer.buttonReads
            let nextDown = routed(.leftMouseDown)
            pointer.buttons = 1
            gap = ProcessInfo.processInfo.systemUptime - releasedAt
            NSApplication.shared.sendEvent(nextDown)
            NSApplication.shared.sendEvent(nextDown)
            NSApplication.shared.sendEvent(routed(.leftMouseUp, timestamp: nextDown.timestamp))
            try require(nextDown.timestamp > down.timestamp && gap < 0.015 && pointer.buttonReads == reads,
                        "The next distinct Down and its deduplicated fallback must occur before a poll observes the released interval")
            Thread.sleep(forTimeInterval: 0.08)
            pointer.buttons = 0
        } else {
            pointer.location.x += 20
        }
        pumpRoute(until: Date(timeIntervalSinceNow: 0.05))
        let expected = nextPress ? 2 : 1
        let actual = Array(routeInputs.dropFirst(before))
        let passed = actual == Array(repeating: NativeIntent.toggle, count: expected) &&
            routePopover.presentations == opened && routeView.button.image === image
        if !passed {
            nativeFailures.append("ReleaseBoundaryRace: \(name), expected=\(expected), actual=\(actual)")
        }
        let settled = routeInputs.count
        NSApplication.shared.sendEvent(routed(.leftMouseUp))
        NSApplication.shared.sendEvent(routed(.leftMouseUp))
        try require(routeInputs.count == settled, "Late Up cannot duplicate a resolved click in \(name)")
        print("Given \(name), When release transitions occur before the next poll, Then each completed press toggles once")
        print("ACTUAL_STATUS_ROUTE class=\(type(of: routeView.button)) case=\(name) expected=\(expected) inputs=\(actual.count) releaseGap=\(gap) toggleButtonStates=\(Array(toggleButtonStates.dropFirst(togglesBefore))) passed=\(passed)")
        pointer.location = NSPoint(x: 3000, y: 4000)
        routeView.render(text: routeText, flags: routeFlags)
    }
    let pressSanity = ProcessInfo.processInfo.environment["AI2SW_NATIVE_PRESS_SANITY"] == "1"
    func routeCase(_ name: String, held: Bool = false, control: Bool = false, tracking: Bool = true,
                   forwarded: Bool = false,
                   dragX: CGFloat? = nil, outside: Bool = false,
                   escape: Bool = false, cancelKey: UInt16? = nil,
                   disable: Bool = false, refreshDisabled: Bool = false,
                   staleKey: Bool = false, otherWindowKey: Bool = false,
                   earlyRelease: Bool = false, staleKeyCount: Int = 1, exhausted: Bool = false,
                   unrelatedDiscard: Bool = false, retireDuringDelivery: Bool = false,
                   realMouseUp: Bool = false, closing: Bool = false) throws {
        if pressSanity && ![
            "synthetic-up-hold", "synthetic-up-short", "real-up-short", "within-16-point-release",
            "deliberate-drag", "control", "held-escape", "disabled", "closing",
        ].contains(name) { return }
        let before = routeInputs.count
        let opened = routePopover.presentations
        let togglesBefore = toggleButtonStates.count
        var noDownToggle = false
        var openedBeforeUp = false
        pointer.buttons = 1
        pointer.location = NSPoint(x: 3000, y: 4000)
        let pointerOrigin = pointer.location
        let originalImage = routeView.button.image
        let deliveredBefore = nativeApplication.deliveredKeys.count
        let dequeuedBefore = nativeApplication.dequeuedKeys.count
        let mouseUpsBefore = nativeApplication.keyDequeuesAtMouseUp.count
        let originalKeys = OriginalKeyEvents()
        if staleKey {
            let timestamp = ProcessInfo.processInfo.systemUptime - 1
            for index in 0..<staleKeyCount {
                let stale = NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [],
                    timestamp: timestamp + Double(index) * 0.000001,
                    windowNumber: otherWindowKey ? routePopover.host.windowNumber : routeWindow.windowNumber,
                    context: nil, characters: "y", charactersIgnoringModifiers: "y", isARepeat: false, keyCode: 16
                )!
                originalKeys.values.append(stale)
                NSApplication.shared.postEvent(stale, atStart: false)
            }
        }
        let inspect = Timer(timeInterval: held ? 0.5 : 0.05, repeats: false) { _ in
            MainActor.assumeIsolated {
                noDownToggle = routeInputs.dropFirst(before).allSatisfy { $0 == .queryCorners }
                openedBeforeUp = pointer.buttons == 1 && routePopover.presentations == opened + 1
            }
        }
        let release = Timer(timeInterval: held ? 0.6 : 0.1, repeats: false) { _ in
            MainActor.assumeIsolated {
                pointer.buttons = 0
                if realMouseUp {
                    NSApplication.shared.postEvent(routed(.leftMouseUp, x: outside ? -20 : 11), atStart: false)
                }
            }
        }
        let change = Timer(timeInterval: 0.005, repeats: false) { _ in
            MainActor.assumeIsolated {
                if outside {
                    pointer.location.x = pointerOrigin.x - 31
                    NSApplication.shared.postEvent(routed(.leftMouseDragged, x: -20), atStart: false)
                }
                if let dragX {
                    pointer.location.x = pointerOrigin.x + dragX - 11
                    NSApplication.shared.postEvent(routed(.leftMouseDragged, x: dragX), atStart: false)
                }
                if otherWindowKey {
                    let key = NSEvent.keyEvent(
                        with: .keyDown, location: .zero, modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime,
                        windowNumber: routePopover.host.windowNumber, context: nil,
                        characters: "y", charactersIgnoringModifiers: "y", isARepeat: false, keyCode: 16
                    )!
                    originalKeys.values.append(key)
                    NSApplication.shared.postEvent(key, atStart: false)
                }
                if let keyCode = escape ? UInt16(53) : cancelKey {
                    let key = NSEvent.keyEvent(
                        with: .keyDown, location: .zero, modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: routeWindow.windowNumber,
                        context: nil, characters: escape ? "\u{1b}" : "x",
                        charactersIgnoringModifiers: escape ? "\u{1b}" : "x",
                        isARepeat: false, keyCode: keyCode
                    )!
                    originalKeys.values.append(key)
                    NSApplication.shared.postEvent(key, atStart: false)
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
                if closing {
                    routeView.render(text: [:], flags: ["terminate": true])
                }
            }
        }
        for timer in [inspect, release] + (earlyRelease ? [] : [change]) {
            RunLoop.main.add(timer, forMode: .common)
            RunLoop.main.add(timer, forMode: .eventTracking)
        }
        defer {
            pointer.buttons = 0
            inspect.invalidate()
            release.invalidate()
            change.invalidate()
            nativeApplication.mouseUpBoundary = nil
            nativeApplication.keyDeliveryBoundary = nil
        }
        let sent = ProcessInfo.processInfo.systemUptime
        let deadline = Date(timeIntervalSinceNow: held ? 0.65 : 0.15)
        let down: NSEvent
        if forwarded {
            _ = try dequeueOwn(NSEvent.otherEvent(
                with: .applicationDefined, location: .zero, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: routeWindow.windowNumber,
                context: nil, subtype: 0, data1: 0, data2: 0
            )!)
            down = routed(.leftMouseDown, control: control)
        } else {
            down = try dequeueOwn(routed(.leftMouseDown, control: control))
        }
        if earlyRelease {
            if unrelatedDiscard {
                nativeApplication.mouseUpBoundary = { event in
                    nativeApplication.mouseUpBoundary = nil
                    let marker = NSEvent.mouseEvent(
                        with: .leftMouseUp, location: .zero, modifierFlags: [],
                        timestamp: event.timestamp + 1, windowNumber: routePopover.host.windowNumber,
                        context: nil, eventNumber: 1, clickCount: 1, pressure: 0
                    )!
                    do { try verifyUnrelatedDiscard(before: marker, context: "other-window-during-tracking") }
                    catch { nativeFailures.append("Unrelated discard failed: \(error)") }
                }
            }
            if retireDuringDelivery {
                nativeApplication.keyDeliveryBoundary = { _ in
                    nativeApplication.keyDeliveryBoundary = nil
                    if let release = nativeApplication.lastMouseUp {
                        NSApplication.shared.discardEvents(matching: .keyDown, before: release)
                    }
                    var disabled = routeFlags
                    disabled["toggle_enabled"] = false
                    routeView.render(text: routeText, flags: disabled)
                }
            }
            if escape || otherWindowKey {
                let key = NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [],
                    timestamp: down.timestamp + 0.000001,
                    windowNumber: otherWindowKey ? routePopover.host.windowNumber : routeWindow.windowNumber,
                    context: nil, characters: escape ? "\u{1b}" : "y",
                    charactersIgnoringModifiers: escape ? "\u{1b}" : "y",
                    isARepeat: false, keyCode: escape ? 53 : 16
                )!
                originalKeys.values.append(key)
                NSApplication.shared.postEvent(key, atStart: false)
            }
            noDownToggle = routeInputs.dropFirst(before).allSatisfy { $0 == .queryCorners }
            if !originalKeys.values.isEmpty {
                try require(NSApplication.shared.nextEvent(
                    matching: .keyDown, until: .distantPast, inMode: .eventTracking, dequeue: false
                ) != nil, "The early-release native fixture must actually have queued keys before tracking")
            }
        }
        let syntheticUp = routed(.leftMouseUp, timestamp: down.timestamp)
        try require(syntheticUp.timestamp == down.timestamp,
                    "Given the macOS 27 status stream, Then synthetic Up has the exact Down timestamp")
        print("Given \(name) with button-state 1, When status Down and synthetic Up arrive, Then only polled hardware release may toggle")
        if !realMouseUp && (tracking || forwarded) {
            NSApplication.shared.postEvent(syntheticUp, atStart: false)
        }
        if tracking || forwarded {
            try require(routeWindow.contentView?.hitTest(down.locationInWindow) === routeView.button,
                        "The native event must hit the actual status button")
            routeView.button.mouseDown(with: down)
            try require(ProcessInfo.processInfo.systemUptime - sent < 0.8,
                        "Inherited native tracking must return after its own queued release")
        } else {
            NSApplication.shared.sendEvent(down)
            if !realMouseUp { NSApplication.shared.sendEvent(try dequeueOwn(syntheticUp)) }
            try require(ProcessInfo.processInfo.systemUptime - sent < 0.05,
                        "ActualStatusButtonRoute: Down must return without a nested tracking/event wait")
        }
        if !realMouseUp {
            try require(pointer.buttons == 1 &&
                        routeInputs.dropFirst(before).allSatisfy { $0 == .queryCorners } &&
                        routeView.button.image === originalImage,
                        "SyntheticUpWhilePressed: \(name) must retain button state/icon and permit only the menu's typed corner query before release: \(Array(routeInputs.dropFirst(before)))")
        }
        let keysAtTrackingReturn = nativeApplication.dequeuedKeys.count - dequeuedBefore
        if earlyRelease {
            try require(nativeApplication.keyDequeuesAtMouseUp.count > mouseUpsBefore &&
                        nativeApplication.keyDequeuesAtMouseUp[mouseUpsBefore] == dequeuedBefore,
                        "Early release must reach inherited tracking before any timer key dequeue in \(name)")
            if exhausted {
                try require(keysAtTrackingReturn > 0 && keysAtTrackingReturn < originalKeys.values.count,
                            "Release finalization must be bounded and leave overflow keys for normal delivery")
            }
            print("NATIVE_EARLY_RELEASE case=\(name) keys_before_up=0 queued=\(originalKeys.values.count) keys_at_tracking_return=\(keysAtTrackingReturn)")
        }
        pumpRoute(until: deadline)
        nativeApplication.mouseUpBoundary = nil
        nativeApplication.keyDeliveryBoundary = nil
        inspect.invalidate()
        release.invalidate()
        change.invalidate()
        let cancelled = escape || cancelKey != nil || disable || outside || exhausted || retireDuringDelivery || closing ||
            (dragX != nil && abs(dragX! - 11) > 16)
        let context = control || (held && !cancelled)
        let expected: [NativeIntent] = context ? [.queryCorners] : (cancelled ? [] : [.toggle])
        if !noDownToggle || Array(routeInputs.dropFirst(before)) != expected ||
            (context && !openedBeforeUp) || routePopover.presentations != opened + (context ? 1 : 0) {
            nativeFailures.append("ActualStatusButtonRoute: \(name), downUnchanged=\(noDownToggle), before=\(before), after=\(routeInputs.count), openedBeforeUp=\(openedBeforeUp)")
        }
        let settled = routeInputs.count
        NSApplication.shared.sendEvent(try dequeueOwn(routed(.leftMouseUp)))
        NSApplication.shared.sendEvent(try dequeueOwn(routed(.leftMouseUp)))
        try require(routeInputs.count == settled &&
                    toggleButtonStates.dropFirst(togglesBefore).allSatisfy { $0 == 0 } &&
                    routeView.button.image === originalImage,
                    "Only detected hardware release may toggle once, and repeated late Up or held release never changes the icon after \(name)")
        routeView.render(text: routeText, flags: routeFlags)
        print("ACTUAL_STATUS_ROUTE class=\(type(of: routeView.button)) case=\(name) syntheticUp=\(!realMouseUp) downUnchanged=\(noDownToggle) inputs=\(routeInputs.count - before) beforeUp=\(openedBeforeUp) toggleButtonStates=\(Array(toggleButtonStates.dropFirst(togglesBefore)))")
        if staleKey || otherWindowKey {
            let delivered = Array(nativeApplication.deliveredKeys.dropFirst(deliveredBefore))
            let dequeued = Array(nativeApplication.dequeuedKeys.dropFirst(dequeuedBefore))
            // AppKit serializes queued timestamps to nanoseconds; dequeue-to-send identity stays exact.
            try require(originalKeys.values.allSatisfy { key in delivered.filter {
                $0.keyCode == key.keyCode && abs($0.timestamp - key.timestamp) < 0.000000001 &&
                    $0.windowNumber == key.windowNumber && $0.characters == key.characters &&
                    $0.modifierFlags == key.modifierFlags
            }.count == 1 } && dequeued.allSatisfy { key in delivered.filter { $0 === key }.count == 1 } &&
                        delivered.count == originalKeys.values.count && dequeued.count == delivered.count,
                        "Every queued original key is delivered exactly once unchanged through normal sendEvent in \(name)")
            print("NATIVE_ORIGINAL_KEY_DELIVERY case=\(name) originals=\(originalKeys.values.count) delivered=\(delivered.count) unchanged=true")
        }
    }
    try releaseRaceCase("post-release-travel", nextPress: false)
    try releaseRaceCase("between-poll-clicks", nextPress: true)
    try routeCase("synthetic-up-hold", held: true)
    try routeCase("synthetic-up-short")
    try routeCase("real-up-short", realMouseUp: true)
    try routeCase("inside-jitter-hold", held: true, dragX: 12)
    try routeCase("control", control: true)
    try routeCase("deliberate-drag", held: true, dragX: 28)
    try routeCase("within-16-point-release", dragX: 26)
    try routeCase("leave-bounds", held: true, dragX: -20)
    try routeCase("outside-release", outside: true)
    try routeCase("escape", escape: true)
    try routeCase("held-escape", held: true, escape: true)
    try routeCase("keyboard-cancel", cancelKey: 7)
    try routeCase("held-keyboard-cancel", held: true, cancelKey: 7)
    try routeCase("disabled", held: true, disable: true)
    try routeCase("closing", held: true, closing: true)
    try routeCase("recovered")
    var menuOnlyFlags = routeFlags
    menuOnlyFlags["toggle_enabled"] = false
    routeView.render(text: routeText, flags: menuOnlyFlags)
    try routeCase("disabled-new-control", control: true)
    routeView.render(text: routeText, flags: menuOnlyFlags)
    try routeCase("disabled-new-hold", held: true, refreshDisabled: true)
    try routeCase("application-window-short", tracking: false)
    try routeCase("application-window-hold", held: true, tracking: false)
    try routeCase("application-window-keyboard-cancel", held: true, tracking: false, cancelKey: 7)
    try routeCase("forwarded-window-short", forwarded: true)
    try routeCase("forwarded-window-hold", held: true, forwarded: true)
    try routeCase("forwarded-window-escape", held: true, forwarded: true, escape: true)
    try routeCase("stale-key-current-escape", escape: true, staleKey: true)
    try routeCase("stale-key-only", staleKey: true)
    try routeCase("other-window-key-only", otherWindowKey: true)
    try routeCase("early-backlog-current-escape", escape: true, staleKey: true, earlyRelease: true, staleKeyCount: 8)
    try routeCase("early-stale-backlog-only", staleKey: true, earlyRelease: true, staleKeyCount: 8)
    try routeCase("early-other-window-key-only", otherWindowKey: true, earlyRelease: true)
    try routeCase("early-exhausted-backlog", escape: true, staleKey: true,
                  earlyRelease: true, staleKeyCount: 64, exhausted: true)
    try routeCase("early-unrelated-discard", earlyRelease: true, unrelatedDiscard: true)
    try routeCase("early-reentrant-retirement", staleKey: true, earlyRelease: true,
                  staleKeyCount: 8, retireDuringDelivery: true)
    routeView.render(text: routeText, flags: routeFlags)
    try verifyUnrelatedDiscard(before: routed(.leftMouseUp), context: "outside-tracking")
    let beforeActivation = routeInputs.count
    let beforeActivationMenu = routePopover.presentations
    routeView.button.performClick(nil)
    try require(Array(routeInputs.dropFirst(beforeActivation)) == [.toggle] &&
                routePopover.presentations == beforeActivationMenu,
                "One enabled programmatic button activation toggles once without opening the pointer menu: \(Array(routeInputs.dropFirst(beforeActivation)))")
    routeView.render(text: routeText, flags: menuOnlyFlags)
    routeView.button.performClick(nil)
    try require(routeInputs.count == beforeActivation + 1,
                "Programmatic button activation respects disabled Toggle while menu access remains available")
    routeView.render(text: routeText, flags: routeFlags)
    if routeView.popover.contentSize.height > 222 {
        nativeFailures.append("CompactTopSlack: actual menu height \(routeView.popover.contentSize.height) exceeds the requested setup-action layout")
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
    let text = [
        "status": "Mode: Can Sleep", "icon_asset": "menu-off", "preview_asset": "preview-off", "badge": "",
        "accessibility": "AI2ScreenWake protection: Pending", "show_log": "Show Log...", "open_menu": "Open menu",
        "quit": "Quit", "error": "", "tooltip": "Pending. Screensaver protection is unverified.",
        "cycle_preview": "Cycle TV: No Sleep, Can Sleep, Error preview",
        "version": buildVersion(), "info_label": "About AI2ScreenWake",
        "info_help": "Open the AI2ScreenWake website", "info_url": "https://ai2sw.info",
        "corner_help": "Prevent corner sleep: Install",
        "log_title": "20261117tu100720_ai2sw.log", "log_text": "/private/example/Library/Logs/AI2ScreenWake/20261117tu100720_ai2sw.log\n0  Power:Session:Protection:Query",
    ]
    var flags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                 "toggle_enabled": true, "activate_log": false, "setup_enabled": true,
                 "corner_installed": false, "corner_on": true]
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
                routeView.infoItem.nextKeyView === routeView.cornerItem &&
                routeView.cornerItem.nextKeyView === routeView.showLogItem &&
                routeView.showLogItem.nextKeyView === routeView.quitItem &&
                routeView.quitItem.nextKeyView === routeView.preview,
                "Neutral first focus and TV/info/setup/Show Log/Quit navigation use one never-visible native surface")
    for action in [routeView.preview, routeView.infoItem, routeView.cornerItem, routeView.showLogItem, routeView.quitItem] {
        try require(routePopover.host.makeFirstResponder(action) && action.focusRingType == .default,
                    "All five native controls retain actual responder eligibility and focus rings")
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
    let compact = content.subviews.count == 7 &&
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
        content.bounds.contains(view.cornerItem.frame) &&
        view.cornerItem.frame.size == NSSize(width: 20, height: 20) &&
        view.infoItem.frame.size == view.cornerItem.frame.size &&
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
    try require(view.menuAnchor == view.button.bounds && inputs == [.queryCorners],
                "Given a standalone right-click, When the real recognizer receives local window events, Then one corner query accompanies presentation without a toggle")
    try require(!recognitionWindow.isVisible,
                "Recognition proof must never present or activate a status item or test window")
    try require(view.menuAnchor == view.button.bounds,
                "The native upward-beak popover is positioned from the actual status-button bounds, not a screen origin")
    view.button.performClick(nil)
    try require(inputs == [.queryCorners, .toggle],
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
    try runAccessibilitySetupContracts(assets: assets, text: text, flags: flags)
    try runInstallerWindowContracts(assets: assets)
}

@MainActor
func runInstallerWindowContracts(assets: URL) throws {
    guard MenuContractApplication.shared is MenuContractApplication else {
        throw MenuContractFailure.failed("Installer contracts require the nonpresenting contract application")
    }
    NSApplication.shared.setActivationPolicy(.prohibited)
    var failures: [String] = []
    func check(_ value: @autoclosure () -> Bool, _ requirement: String) {
        if !value() { failures.append(requirement) }
    }
    var anchor: SettingsWindowAnchor? = SettingsWindowAnchor(
        bounds: NSRect(x: 3076, y: 30, width: 740, height: 625), primaryHeight: 2160,
        visibleFrames: [NSRect(x: 0, y: 0, width: 5120, height: 2135)]
    )
    var reads = 0
    var openings = 0
    var drags: [NSDraggingItem] = []
    var inputs: [NativeIntent] = []
    let application = assets.deletingLastPathComponent().appendingPathComponent("Installer Contract.app")
    let view = try MenuBarController(
        assets: assets, displaySurfaces: false,
        openURL: { _ in openings += 1; return true }, currentApplication: { application },
        settingsWindow: { reads += 1; return anchor },
        beginSetupDrag: { _, _, item in drags.append(item) },
        emit: { inputs.append($0) }, terminate: {}
    )
    let text = [
        "status": "Mode: Can Sleep", "icon_asset": "menu-off", "preview_asset": "preview-off",
        "badge": "", "accessibility": "AI2ScreenWake protection: Off", "show_log": "Show Log...",
        "open_menu": "Open menu", "quit": "Quit", "error": "", "tooltip": "Off",
        "cycle_preview": "Cycle TV", "version": buildVersion(), "info_label": "About AI2ScreenWake",
        "info_help": "Open the AI2ScreenWake website", "info_url": "https://ai2sw.info",
        "log_title": "ai2sw.log", "log_text": "", "setup_session": "7",
        "corner_help": "Prevent corner sleep: Install",
        "setup_title": "Stop sleep corners",
        "setup_instruction": "To stop sleep corners, enable device control by dragging the icon to the panel above",
        "setup_explanation": "Legacy explanation must no longer render.",
        "setup_drag_label": "Drag AI2ScreenWake to the panel above",
        "setup_message": "",
        "setup_url": "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    ]
    var flags = ["terminate": false, "log_visible": false, "quit_enabled": true,
                 "toggle_enabled": true, "activate_log": false, "setup_enabled": true,
                 "corner_installed": false, "corner_on": true]
    view.render(text: text, flags: flags)
    check(reads == 0 && openings == 0, "Closed setup must not inspect Settings windows")
    check(view.popover.contentSize == NSSize(width: 176, height: 168),
          "CompactCornerControls: replacing the wrapping setup button must restore 176 x 168")
    guard let menu = view.popover.contentViewController?.view else {
        throw MenuContractFailure.failed("Corner controls require the native menu")
    }
    check(view.cornerItem.frame.size == NSSize(width: 20, height: 20) &&
          view.infoItem.frame.size == view.cornerItem.frame.size &&
          view.cornerItem.frame.minX == 12 &&
          menu.bounds.maxX - view.infoItem.frame.maxX == 12 &&
          view.cornerItem.frame.maxY == view.infoItem.frame.maxY &&
          view.cornerItem.frame.maxX < view.preview.frame.minX &&
          view.infoItem.frame.minX > view.preview.frame.maxX,
          "BoldCornerSymbols: equal 20-point controls must fit symmetrically clear of the TV")
    let symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 15, weight: .bold)
    for (control, symbol) in [(view.infoItem, "info.circle"), (view.cornerItem, "arrow.down.to.line")] {
        let expected = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(symbolConfiguration)
        check(!control.isBordered && control.image != nil && expected != nil &&
              control.image?.tiffRepresentation != nil &&
              control.image?.tiffRepresentation == expected?.tiffRepresentation,
              "BoldCornerSymbols: actual configured native image must be \(symbol) at the common bold weight")
    }
    for (state, installed, on, tint) in [
        ("Install", false, true, NSColor.systemBlue),
        ("On", true, true, NSColor.systemGreen),
        ("Off", true, false, NSColor.secondaryLabelColor),
    ] {
        var packet = text
        packet["corner_help"] = "Prevent corner sleep: \(state)"
        var rendered = flags
        rendered["corner_installed"] = installed
        rendered["corner_on"] = on
        view.render(text: packet, flags: rendered)
        check(view.cornerItem.toolTip == packet["corner_help"] &&
              view.cornerItem.accessibilityLabel() == packet["corner_help"] &&
              view.cornerItem.accessibilityHelp() == packet["corner_help"] &&
              view.cornerItem.contentTintColor == tint,
              "CornerControlProjection: \(state) tint, tooltip and accessibility must follow Rust")
        let before = inputs.count
        view.cornerItem.performClick(nil)
        check(Array(inputs.dropFirst(before)) == [.activateCorners] && openings == 0,
              "CornerControlRouting: every state emits one typed input without native permission policy")
    }
    view.render(text: text, flags: flags)
    flags["setup_visible"] = true
    flags["open_setup"] = true
    view.render(text: text, flags: flags)
    guard let window = view.setupWindow, let content = window.contentView,
          let icon = view.setupIcon, let heading = view.setupInstruction,
          let status = view.setupMessage else {
        throw MenuContractFailure.failed("Installer effect must construct its nonpresenting window")
    }
    defer { window.orderOut(nil); window.close() }
    check(!window.isVisible && openings == 1 && inputs.last == .setupOpened(session: 7, opened: true),
          "Injected Settings acceptance must not present a real window or grant permission")
    check(window.frame.width == 508 && window.frame.minX == 3308 && window.frame.maxY == 1505,
          "SectionAnchoredPlacement: measured Settings content must align at x=3308, width=508, Cocoa top=1505; actual \(window.frame)")
    flags["open_setup"] = false
    anchor = SettingsWindowAnchor(
        bounds: NSRect(x: -1000, y: 80, width: 740, height: 625), primaryHeight: 1080,
        visibleFrames: [NSRect(x: 0, y: 0, width: 1920, height: 1055),
                        NSRect(x: -1280, y: -200, width: 1280, height: 1280)]
    )
    view.render(text: text, flags: flags)
    check(window.frame.width == 508 && window.frame.minX == -768 && window.frame.maxY == 375,
          "NegativeOriginPlacement: re-anchor to the Settings secondary screen using primary-height conversion; actual \(window.frame)")
    anchor = SettingsWindowAnchor(
        bounds: NSRect(x: 300, y: 700, width: 740, height: 300), primaryHeight: 1080,
        visibleFrames: [NSRect(x: 0, y: 0, width: 1920, height: 1055)]
    )
    view.render(text: text, flags: flags)
    check(window.frame.width == 508 && window.frame.minX == 532 && window.frame.minY == 0,
          "VisibleFrameClamp: insufficient space below clamps the complete installer into the same screen; actual \(window.frame)")
    check(reads == 3 && openings == 1, "Setup ticks must follow Settings movement without reopening it")
    let point = NSPoint(x: 24, y: icon.frame.midY)
    let source = content.hitTest(point)
    check(source === icon, "WholeButtonDrag: a point far from the icon must hit the whole drag source")
    let event = NSEvent.mouseEvent(
        with: .leftMouseDown, location: content.convert(point, to: nil), modifierFlags: [],
        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
        context: nil, eventNumber: 1, clickCount: 1, pressure: 1
    )!
    if source === icon { icon.mouseDown(with: event) }
    let pasteboard = NSPasteboard.withUniqueName()
    defer { pasteboard.releaseGlobally() }
    if let writer = drags.first?.item as? NSPasteboardWriting {
        check(pasteboard.writeObjects([writer]) && pasteboard.string(forType: .fileURL) == application.absoluteString,
              "WholeButtonDrag must preserve the real NSURL file writer with spaces encoded")
    } else {
        failures.append("WholeButtonDrag: mouse-down did not begin the injected native file drag")
    }
    check(drags.count == 1 && icon.accessibilityRole() == .button,
          "WholeButtonDrag must be one keyboard-accessible labelled button source")
    check(icon.icon.frame.size == NSSize(width: 288, height: 288) &&
          icon.bounds.contains(icon.icon.frame) &&
          icon.icon.frame.midX == icon.bounds.midX && icon.icon.frame.midY == icon.bounds.midY &&
          icon.accessibilityLabel() == text["setup_drag_label"],
          "WholeButtonDrag: the actual proportional icon must be 288 points, centered in the labelled button")
    check(content.subviews.count == 2 && status.superview == nil,
          "ConciseInstallerCopy: normal setup must contain only its instruction and drag source")
    check(heading.frame.minY > icon.frame.maxY && icon.frame.minY == 20,
          "CleanInstallerLayout: instruction and large drag target fit without an empty status row")
    for label in [heading] {
        let measured = label.cell?.cellSize(forBounds: label.bounds) ?? .zero
        check(content.bounds.contains(label.frame) && measured.height > 0 &&
              measured.height <= label.frame.height && measured.width <= label.frame.width,
              "CleanInstallerLayout: actual wrapping copy must fit \(label.frame)")
    }
    _ = view.windowShouldClose(window)
    let closedReads = reads
    view.render(text: text, flags: flags)
    check(reads == closedReads && openings == 1, "Retired setup must never re-anchor or reopen")
    var normal = text
    normal["setup_session"] = "8"
    normal["setup_message"] = ""
    flags["open_setup"] = true
    view.render(text: normal, flags: flags)
    guard let normalWindow = view.setupWindow, let normalContent = normalWindow.contentView,
          let failureLine = view.setupMessage else {
        throw MenuContractFailure.failed("Setup status contract requires the nonpresenting window")
    }
    check(normalContent.subviews.count == 2 && !normalContent.subviews.contains(failureLine),
          "SetupNormalHasNoStatus: ordinary untrusted setup must show only the instruction and drag source")
    normal["setup_message"] = "Couldn't open System Settings."
    flags["open_setup"] = false
    view.render(text: normal, flags: flags)
    check(normalContent.subviews.contains(failureLine) && !failureLine.isHidden &&
          !failureLine.isEditable && !failureLine.isSelectable &&
          failureLine.currentEditor() == nil && failureLine.stringValue == normal["setup_message"],
          "SetupFailureIsPlainLabel: a real failure must be short, not selectable or highlighted")
    _ = view.windowShouldClose(normalWindow)
    for failure in failures { print("INSTALLER_CONTRACT_FAILED \(failure)") }
    if !failures.isEmpty { throw MenuContractFailure.failed(failures.joined(separator: "\n")) }
    print("Given supplied Settings frames and a real bundle URL, When the installer renders and its edge is pressed, Then section anchoring, whole-button dragging and unclipped concise copy pass")
}

@MainActor
private func runAccessibilitySetupContracts(assets: URL, text: [String: String], flags: [String: Bool]) throws {
    func require(_ value: @autoclosure () -> Bool, _ why: String) throws {
        if !value() { throw MenuContractFailure.failed(why) }
    }
    let current = Bundle.main.bundleURL.standardizedFileURL
    var currentItems: [NSDraggingItem] = []
    let currentDrag = ApplicationIconDragView(applicationURL: current, beginDrag: { _, _, item in
        currentItems.append(item)
    })
    let dragWindow = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 460, height: 400),
                              styleMask: .borderless, backing: .buffered, defer: false)
    dragWindow.isReleasedWhenClosed = false
    dragWindow.contentView?.addSubview(currentDrag)
    currentDrag.needsLayout = true
    currentDrag.layoutSubtreeIfNeeded()
    defer { currentDrag.removeFromSuperview(); dragWindow.close() }
    let edge = NSPoint(x: currentDrag.frame.minX + 4, y: currentDrag.frame.midY)
    try require(dragWindow.contentView?.hitTest(edge) === currentDrag,
                "The entire current-bundle drag button receives a point outside its icon")
    let down = NSEvent.mouseEvent(
        with: .leftMouseDown, location: edge, modifierFlags: [],
        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: dragWindow.windowNumber,
        context: nil, eventNumber: 1, clickCount: 1, pressure: 1
    )!
    currentDrag.mouseDown(with: down)
    let pasteboard = NSPasteboard.withUniqueName()
    defer { pasteboard.releaseGlobally() }
    guard currentItems.count == 1, let writer = currentItems.first?.item as? NSPasteboardWriting else {
        throw MenuContractFailure.failed("Native dragging item has no file-URL pasteboard writer")
    }
    try require(pasteboard.writeObjects([writer]) &&
                pasteboard.string(forType: .fileURL) == current.absoluteString,
                "A current-bundle button-edge Down begins one real NSURL file drag without a general-clipboard write")
    var template = Array(FileManager.default.temporaryDirectory.appendingPathComponent("ai2sw-drag-XXXXXX").path.utf8CString)
    let temporaryPath = template.withUnsafeMutableBufferPointer { buffer -> String? in
        guard let created = mkdtemp(buffer.baseAddress!) else { return nil }
        return String(cString: created)
    }
    guard let temporaryPath else {
        throw MenuContractFailure.failed("Cannot create the owned temporary drag fixture")
    }
    let temporary = URL(fileURLWithPath: temporaryPath, isDirectory: true)
    defer { try! FileManager.default.removeItem(at: temporary) }
    let fixture = temporary.appendingPathComponent("Accessibility Contract.app", isDirectory: true)
    try require(!FileManager.default.fileExists(atPath: fixture.path),
                "The owned application-path fixture must not replace existing work")
    try FileManager.default.createDirectory(at: fixture, withIntermediateDirectories: false)
    var inputs: [NativeIntent] = []
    var opened: [URL] = []
    let setup = try MenuBarController(
        assets: assets, displaySurfaces: false,
        openURL: { opened.append($0); return false },
        currentApplication: { fixture },
        settingsWindow: { nil },
        emit: { inputs.append($0) }, terminate: {}
    )
    var packet = text
    packet["setup_session"] = "7"
    packet["setup_title"] = "Stop sleep corners"
    packet["setup_instruction"] = "To stop sleep corners, enable device control by dragging the icon to the panel above"
    packet["setup_drag_label"] = "Drag AI2ScreenWake to the panel above"
    packet["setup_message"] = ""
    packet["setup_url"] = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
    var rendering = flags
    rendering["log_visible"] = false
    rendering["activate_log"] = false
    rendering["toggle_enabled"] = true
    setup.render(text: packet, flags: rendering)
    setup.cornerItem.performClick(nil)
    try require(inputs == [.activateCorners] && opened.isEmpty && setup.setupWindow == nil,
                "Only explicit selection emits corner-control intent; Rust decides setup versus preference")
    let delayed = try MenuBarController(
        assets: assets, displaySurfaces: false,
        openURL: { opened.append($0); return false },
        currentApplication: { fixture },
        settingsWindow: { nil },
        emit: { inputs.append($0) }, terminate: {}
    )
    let delayedObserver = ObservedMenu()
    delayedObserver.attach(delayed)
    var delayedFlags = rendering
    delayedFlags["setup_visible"] = true
    delayedFlags["open_setup"] = true
    delayedObserver.observed(text: packet, flags: delayedFlags)
    delayedObserver.observed(text: packet, flags: rendering.merging(["cancel_setup": true], uniquingKeysWith: { _, new in new }))
    let drained = DispatchSemaphore(value: 0)
    DispatchQueue.main.async { drained.signal() }
    let deadline = Date(timeIntervalSinceNow: 0.2)
    var completedDrain = false
    while Date() < deadline {
        if drained.wait(timeout: .now()) == .success { completedDrain = true; break }
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.001))
    }
    try require(completedDrain && opened.isEmpty && delayed.setupWindow == nil,
                "An opening already queued through actual ObservedMenu.observed must be revoked before native execution")
    print("NATIVE_SETUP_ASYNC_RETIREMENT queued_observed=true settings_calls=0 window_created=false")
    delayedObserver.observed(text: packet, flags: delayedFlags)
    let reopened = DispatchSemaphore(value: 0)
    DispatchQueue.main.async { reopened.signal() }
    let reopenDeadline = Date(timeIntervalSinceNow: 0.2)
    var didDrain = false
    while Date() < reopenDeadline {
        if reopened.wait(timeout: .now()) == .success { didDrain = true; break }
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.001))
    }
    try require(didDrain && opened.count == 1 && delayed.setupWindow != nil,
                "A current opening through actual ObservedMenu.observed still executes exactly once")
    if let window = delayed.setupWindow { _ = delayed.windowShouldClose(window) }
    opened.removeAll()
    rendering["setup_visible"] = true
    rendering["open_setup"] = true
    setup.render(text: packet, flags: rendering)
    guard let window = setup.setupWindow, let content = window.contentView,
          let icon = setup.setupIcon, let heading = setup.setupInstruction,
          let message = setup.setupMessage else {
        throw MenuContractFailure.failed("The logged effect did not construct all native setup content")
    }
    try require(!window.isVisible && inputs.last == .setupOpened(session: 7, opened: false) &&
                opened.map(\.absoluteString) == [packet["setup_url"]!],
                "The nonpresenting collaborator reports actual workspace refusal without opening real settings")
    pasteboard.clearContents()
    guard let appWriter = icon.draggingItem().item as? NSPasteboardWriting else {
        throw MenuContractFailure.failed("The current-application icon has no actual native file drag")
    }
    try require(icon.applicationURL == fixture && pasteboard.writeObjects([appWriter]) &&
                pasteboard.string(forType: .fileURL) == fixture.absoluteString &&
                icon.image != nil && icon.icon.frame.size == NSSize(width: 288, height: 288),
                "The setup icon uses the actual resolved application URL, with spaces safely encoded, not a helper or picture-only drag")
    for label in [heading] {
        let measured = label.cell?.cellSize(forBounds: label.bounds) ?? .zero
        try require(content.bounds.contains(label.frame) && measured.height > 0 &&
                    measured.height <= label.frame.height && measured.width <= label.frame.width,
                    "Rust setup copy must fit its real wrapping native geometry: \(label.stringValue) \(measured)/\(label.frame.size)")
    }
    try require(heading.stringValue == packet["setup_instruction"] &&
                content.bounds.contains(icon.frame) && content.subviews.count == 2 &&
                heading.frame.minY > icon.frame.maxY && message.superview == nil,
                "Normal setup has only instructions and the drag button, without status or a UI permission timer")
    rendering["open_setup"] = false
    setup.render(text: packet, flags: rendering)
    try require(opened.count == 1, "Background setup updates never reopen System Settings")
    try require(!setup.windowShouldClose(window) && inputs.last == .closeSetup(session: 7),
                "Setup close stops observation and emits only its typed setup lifetime, never CloseLog")
    rendering["setup_visible"] = false
    setup.render(text: packet, flags: rendering)
    try require(opened.count == 1,
                "Closed and stale ordinary rendering cannot restart setup observation or settings")
    print("NATIVE_ACCESSIBILITY_SETUP current_bundle=\(current.absoluteString) file_drag=\(fixture.absoluteString) menu=\(setup.popover.contentSize) instruction=\(heading.frame) drag=\(icon.frame) status=\(message.frame) ui_permission_timer=false workspace_calls=\(opened.count)")
}
#endif
// end MenuBarContractTests.swift
