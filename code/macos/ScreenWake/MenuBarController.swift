// MenuBarController.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
import AppKit

enum MenuViewFailure: Error {
    case statusButtonUnavailable
    case assetsUnavailable
    case statusQueueUnavailable
}

@MainActor
class StatusApplication: NSApplication {
    @MainActor
    private final class Tracking {
        weak var control: NSControl?
        weak var window: NSWindow?
        let cell: NSCell
        let timestamp: TimeInterval
        let release: (NSEvent) -> Void
        var releaseEvent: NSEvent?
        var finalized = false

        init(control: NSControl, window: NSWindow, cell: NSCell, timestamp: TimeInterval,
             release: @escaping (NSEvent) -> Void) {
            self.control = control
            self.window = window
            self.cell = cell
            self.timestamp = timestamp
            self.release = release
        }

        func matches(_ event: NSEvent) -> Bool {
            control?.cell === cell && control?.window === window && window != nil &&
                event.type == .leftMouseUp && event.windowNumber == window?.windowNumber &&
                event.timestamp >= timestamp
        }
    }

    private var statusTracking: Tracking?

    fileprivate func trackStatusCell(_ cell: NSCell, in view: NSView, event: NSEvent,
                                     release: @escaping (NSEvent) -> Void, body: () -> Bool) -> Bool {
        guard let control = view as? NSControl, let window = control.window,
              control.cell === cell, event.type == .leftMouseDown else { return body() }
        let previous = statusTracking
        let tracking = Tracking(control: control, window: window, cell: cell,
                                timestamp: event.timestamp, release: release)
        statusTracking = tracking
        defer { statusTracking = previous }
        return body()
    }

    override func nextEvent(matching mask: NSEvent.EventTypeMask, until expiration: Date?,
                            inMode mode: RunLoop.Mode, dequeue flag: Bool) -> NSEvent? {
        let event = super.nextEvent(matching: mask, until: expiration, inMode: mode, dequeue: flag)
        if flag, mask.contains(.leftMouseUp), let event, let tracking = statusTracking,
           tracking.releaseEvent == nil, tracking.matches(event) {
            tracking.releaseEvent = event
        }
        return event
    }

    override func discardEvents(matching mask: NSEvent.EventTypeMask, before lastEvent: NSEvent?) {
        guard mask.contains(.keyDown), let event = lastEvent, let tracking = statusTracking,
              tracking.releaseEvent === event, tracking.matches(event) else {
            super.discardEvents(matching: mask, before: lastEvent)
            return
        }
        if !tracking.finalized {
            tracking.finalized = true
            tracking.release(event)
        }
        // NSCell discards before stopTracking; preserve only this gesture's queued keys.
        super.discardEvents(matching: mask.subtracting(.keyDown), before: lastEvent)
    }
}

@MainActor
final class MenuContentView: NSView {
    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        window?.initialFirstResponder = self
    }
}

@MainActor
final class ApplicationIconDragView: NSImageView, NSDraggingSource {
    let applicationURL: URL
    private let beginDrag: ((ApplicationIconDragView, NSEvent, NSDraggingItem) -> Void)?

    init(applicationURL: URL, beginDrag: ((ApplicationIconDragView, NSEvent, NSDraggingItem) -> Void)? = nil) {
        self.applicationURL = applicationURL
        self.beginDrag = beginDrag
        super.init(frame: NSRect(x: 182, y: 200, width: 96, height: 96))
        image = NSWorkspace.shared.icon(forFile: applicationURL.path)
        imageScaling = .scaleProportionallyDown
        setAccessibilityLabel(applicationURL.deletingPathExtension().lastPathComponent)
        setAccessibilityRole(.image)
    }

    required init?(coder: NSCoder) { nil }

    func draggingItem() -> NSDraggingItem {
        let item = NSDraggingItem(pasteboardWriter: applicationURL as NSURL)
        item.setDraggingFrame(bounds, contents: image)
        return item
    }

    override func mouseDown(with event: NSEvent) {
        let item = draggingItem()
        if let beginDrag {
            beginDrag(self, event, item)
        } else {
            beginDraggingSession(with: [item], event: event, source: self)
        }
    }

    func draggingSession(_ session: NSDraggingSession, sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation {
        .copy
    }
}

struct SettingsWindowAnchor {
    let bounds: NSRect
    let primaryHeight: CGFloat
    let visibleFrames: [NSRect]
}

private struct StatusPress {
    let window: Int
    let origin: NSPoint
    let timestamp: TimeInterval
    var held = false
}

@MainActor
private final class StatusPointerCell: NSButtonCell {
    var down: ((NSEvent) -> Void)?
    var finish: ((NSPoint, Bool) -> Void)?
    var beforeDiscard: ((NSEvent, TimeInterval) -> Void)?
    private(set) var performingClick = false

    override func performClick(_ sender: Any?) {
        performingClick = true
        defer { performingClick = false }
        super.performClick(sender)
    }

    override func trackMouse(with event: NSEvent, in cellFrame: NSRect, of controlView: NSView,
                             untilMouseUp flag: Bool) -> Bool {
        down?(event)
        guard let application = NSApplication.shared as? StatusApplication else { return false }
        return application.trackStatusCell(self, in: controlView, event: event, release: { [weak self] release in
            self?.beforeDiscard?(release, event.timestamp)
        }) {
            super.trackMouse(with: event, in: cellFrame, of: controlView, untilMouseUp: flag)
        }
    }

    override func startTracking(at startPoint: NSPoint, in controlView: NSView) -> Bool { true }

    override func continueTracking(last lastPoint: NSPoint, current currentPoint: NSPoint,
                                   in controlView: NSView) -> Bool {
        return true
    }

    override func stopTracking(last lastPoint: NSPoint, current stopPoint: NSPoint,
                               in controlView: NSView, mouseIsUp flag: Bool) {
        finish?(stopPoint, flag)
    }
}

@MainActor
final class MenuBarController: NSObject, NSWindowDelegate {
    private static let statusWidth = NSStatusItem.squareLength
    private static let releaseKeyBudget = 32
    private static let pressPollInterval: TimeInterval = 0.015
    private static let holdDelay: TimeInterval = 0.45
    // Mirrored status items require global press-origin travel, not one window's bounds.
    private static let pressTravelTolerance: CGFloat = 16

    let popover: NSPopover
    let preview = NSButton(frame: NSRect(x: 40, y: 70, width: 96, height: 96))
    let statusLine = NSTextField(labelWithString: "")
    let versionLine = NSTextField(labelWithString: "")
    let infoItem = NSButton(frame: NSRect(x: 148, y: 148, width: 16, height: 16))
    let setupItem = NSButton(frame: NSRect(x: 12, y: 46, width: 152, height: 40))
    let showLogItem = NSButton(frame: NSRect(x: 12, y: 18, width: 94, height: 24))
    let quitItem = NSButton(frame: NSRect(x: 112, y: 18, width: 48, height: 24))
    let button: NSButton
    private let item: NSStatusItem?
    private let emit: (NativeIntent) -> Void
    private let terminate: () -> Void
    private let openURL: (URL) -> Bool
    private let displaySurfaces: Bool
    private let currentApplication: () -> URL
    private let pressedMouseButtons: () -> Int
    private let mouseLocation: () -> NSPoint
    private let settingsWindow: () -> SettingsWindowAnchor?
    private let beginSetupDrag: ((ApplicationIconDragView, NSEvent, NSDraggingItem) -> Void)?
    private let images: [String: NSImage]
    private var hasProjection = false
    private var toggleEnabled = false
    private var activePress: StatusPress?
    private var lastPressTimestamp: TimeInterval?
    private var lastPressWindow: Int?
    private var pressTimer: Timer?
    private var keyboardTimer: Timer?
    private var eventMonitor: Any?
    private(set) var menuAnchor: NSRect?
    private(set) var logWindow: NSWindow?
    private(set) var logTextView: NSTextView?
    private(set) var setupWindow: NSWindow?
    private(set) var setupIcon: ApplicationIconDragView?
    private(set) var setupInstruction: NSTextField?
    private(set) var setupExplanation: NSTextField?
    private(set) var setupMessage: NSTextField?
    private var setupSession: UInt64?

    init(
        assets: URL?, displaySurfaces: Bool = true, popover suppliedPopover: NSPopover? = nil,
        makeStatusButton: ((CGFloat) -> (NSStatusItem?, NSButton?))? = nil,
        openURL: @escaping (URL) -> Bool = { NSWorkspace.shared.open($0) },
        currentApplication: @escaping () -> URL = { Bundle.main.bundleURL },
        pressedMouseButtons: @escaping () -> Int = { NSEvent.pressedMouseButtons },
        mouseLocation: @escaping () -> NSPoint = { NSEvent.mouseLocation },
        settingsWindow: @escaping () -> SettingsWindowAnchor? = { nil },
        beginSetupDrag: ((ApplicationIconDragView, NSEvent, NSDraggingItem) -> Void)? = nil,
        emit: @escaping (NativeIntent) -> Void, terminate: @escaping () -> Void
    ) throws {
        guard NSApplication.shared is StatusApplication else {
            emit(.rendererFailed)
            throw MenuViewFailure.statusQueueUnavailable
        }
        self.emit = emit
        self.terminate = terminate
        self.openURL = openURL
        self.displaySurfaces = displaySurfaces
        self.currentApplication = currentApplication
        self.pressedMouseButtons = pressedMouseButtons
        self.mouseLocation = mouseLocation
        self.settingsWindow = settingsWindow
        self.beginSetupDrag = beginSetupDrag
        self.popover = suppliedPopover ?? NSPopover()
        let factory = makeStatusButton ?? { width in
            if displaySurfaces {
                let item = NSStatusBar.system.statusItem(withLength: width)
                return (item, item.button)
            }
            let thickness = NSStatusBar.system.thickness
            let length = width == NSStatusItem.squareLength ? thickness : width
            let button = NSButton(frame: NSRect(x: 0, y: 0, width: length, height: thickness))
            button.isBordered = false
            return (nil, button)
        }
        let (item, button) = factory(Self.statusWidth)
        guard let button else {
            if let item { NSStatusBar.system.removeStatusItem(item) }
            emit(.rendererFailed)
            throw MenuViewFailure.statusButtonUnavailable
        }
        self.item = item
        self.button = button
        var images: [String: NSImage] = [:]
        for (name, logical) in [
            ("menu-off", 24), ("menu-on", 24), ("menu-error", 24),
            ("preview-off", 96), ("preview-on", 96), ("preview-error", 96),
        ] {
            let size = NSSize(width: CGFloat(logical), height: CGFloat(logical))
            let image = NSImage(size: size)
            for suffix in ["", "@2x"] {
                guard let url = assets?.appendingPathComponent(name + suffix + ".png") else {
                    emit(.rendererFailed)
                    throw MenuViewFailure.assetsUnavailable
                }
                let data: Data
                do {
                    data = try Data(contentsOf: url)
                } catch {
                    emit(.rendererFailed)
                    throw error
                }
                let pixels = suffix.isEmpty ? logical : logical * 2
                guard let representation = NSBitmapImageRep(data: data),
                      representation.pixelsWide == pixels, representation.pixelsHigh == pixels
                else {
                    emit(.rendererFailed)
                    throw MenuViewFailure.assetsUnavailable
                }
                representation.size = size
                image.addRepresentation(representation)
            }
            image.isTemplate = true
            if image.representations.count == 2 { images[name] = image }
        }
        self.images = images
        super.init()
        let content = MenuContentView(frame: NSRect(x: 0, y: 0, width: 176, height: 212))
        let controller = NSViewController()
        controller.view = content
        popover.contentViewController = controller
        popover.contentSize = content.frame.size
        popover.behavior = .transient
        popover.animates = false
        preview.setButtonType(.momentaryChange)
        preview.isBordered = false
        preview.setAccessibilityRole(.button)
        preview.imagePosition = .imageOnly
        preview.imageScaling = .scaleProportionallyDown
        preview.target = self
        preview.action = #selector(cyclePreview)
        statusLine.frame = NSRect(x: 12, y: 90, width: 152, height: 20)
        preview.frame.origin.y = 114
        infoItem.frame.origin.y = 192
        statusLine.alignment = .center
        statusLine.font = .systemFont(ofSize: 13, weight: .semibold)
        versionLine.frame = NSRect(x: 12, y: 3, width: 152, height: 11)
        versionLine.font = .monospacedDigitSystemFont(ofSize: 8, weight: .regular)
        versionLine.textColor = .secondaryLabelColor
        infoItem.isBordered = false
        infoItem.imagePosition = .imageOnly
        infoItem.imageScaling = .scaleProportionallyDown
        infoItem.image = NSImage(systemSymbolName: "info.circle", accessibilityDescription: nil)
        infoItem.contentTintColor = .systemBlue
        infoItem.target = self
        infoItem.action = #selector(showInfo)
        setupItem.bezelStyle = .rounded
        setupItem.font = .systemFont(ofSize: 11)
        setupItem.cell?.wraps = true
        setupItem.cell?.lineBreakMode = .byWordWrapping
        setupItem.target = self
        setupItem.action = #selector(showSetup)
        for child in [preview, statusLine, showLogItem, quitItem, versionLine, infoItem, setupItem] {
            content.addSubview(child)
        }
        showLogItem.bezelStyle = .rounded
        quitItem.bezelStyle = .rounded
        showLogItem.keyEquivalent = "l"
        quitItem.keyEquivalent = "q"
        showLogItem.nextKeyView = quitItem
        quitItem.nextKeyView = preview
        preview.nextKeyView = infoItem
        infoItem.nextKeyView = setupItem
        setupItem.nextKeyView = showLogItem
        content.nextKeyView = preview
        showLogItem.target = self
        showLogItem.action = #selector(showLog)
        quitItem.target = self
        quitItem.action = #selector(quit)
        let pointer = StatusPointerCell(textCell: "")
        pointer.setButtonType(.momentaryChange)
        pointer.isBordered = false
        pointer.down = { [weak self] event in
            _ = self?.routePointer(event, fromButton: true)
        }
        pointer.beforeDiscard = { [weak self] release, timestamp in
            guard let self, let press = self.activePress, press.timestamp == timestamp,
                  release.timestamp >= timestamp, release.windowNumber == press.window else { return }
            self.prepareRelease(for: press)
        }
        pointer.finish = { [weak self] _, released in
            guard let self else { return }
            if released {
                self.button.highlight(self.activePress != nil)
            } else {
                self.cancelPress()
            }
        }
        button.cell = pointer
        button.imagePosition = .imageOnly
        button.imageScaling = .scaleProportionallyDown
        button.target = self
        button.action = #selector(statusPressed)
        button.sendAction(on: [])
        button.isEnabled = false
        // Forwarded status-window events need not be NSApplication.currentEvent.
        eventMonitor = NSEvent.addLocalMonitorForEvents(
            matching: [.leftMouseDown, .leftMouseUp, .leftMouseDragged, .rightMouseDown, .rightMouseUp, .keyDown]
        ) { [weak self] event in
            let forward = MainActor.assumeIsolated {
                guard let self else { return true }
                return self.routePointer(event) != nil
            }
            return forward ? event : nil
        }
        if images.count != 6 {
            emit(.rendererFailed)
        }
    }

    deinit {
        if let eventMonitor { NSEvent.removeMonitor(eventMonitor) }
        pressTimer?.invalidate()
        keyboardTimer?.invalidate()
    }

    private func openMenu() {
        guard hasProjection else { return }
        menuAnchor = button.bounds
        if displaySurfaces {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        }
    }

    private func cancelPress() {
        pressTimer?.invalidate()
        pressTimer = nil
        keyboardTimer?.invalidate()
        keyboardTimer = nil
        activePress = nil
        button.highlight(false)
    }

    private func pollPress() {
        guard let press = activePress else { return }
        guard button.window?.windowNumber == press.window else {
            cancelPress()
            return
        }
        if pressedMouseButtons() & 1 == 0 {
            finishPress()
            return
        }
        let point = mouseLocation()
        if pressedMouseButtons() & 1 == 0 {
            finishPress()
            return
        }
        // Charge only travel observed while held, never movement after release.
        guard hypot(point.x - press.origin.x, point.y - press.origin.y) <= Self.pressTravelTolerance else {
            cancelPress()
            return
        }
        if ProcessInfo.processInfo.systemUptime - press.timestamp >= Self.holdDelay {
            activePress?.held = true
            cancelPress()
            openMenu()
        }
    }

    private func finishPress() {
        guard let initial = activePress else { return }
        pressTimer?.invalidate()
        keyboardTimer?.invalidate()
        if button.window?.windowNumber == initial.window, !initial.held, toggleEnabled {
            prepareRelease(for: initial)
        }
        guard let press = activePress, press.timestamp == initial.timestamp else { return }
        cancelPress()
        if button.window?.windowNumber == press.window, !press.held, toggleEnabled {
            emit(.toggle)
        }
    }

    private func prepareRelease(for press: StatusPress) {
        guard !press.held, toggleEnabled else { return }
        for _ in 0..<Self.releaseKeyBudget {
            guard activePress?.timestamp == press.timestamp else { return }
            if !deliverQueuedKey(for: press) { break }
        }
        guard activePress?.timestamp == press.timestamp else { return }
        // An undelivered backlog can still hide cancellation; leave it queued and reject this click.
        if NSApplication.shared.nextEvent(
            matching: .keyDown, until: .distantPast, inMode: .eventTracking, dequeue: false
        ) != nil {
            cancelPress()
        }
    }

    private func deliverQueuedKey(for press: StatusPress) -> Bool {
        guard let key = NSApplication.shared.nextEvent(
            matching: .keyDown, until: .distantPast, inMode: .eventTracking, dequeue: true
        ) else { return false }
        if activePress?.timestamp == press.timestamp,
           key.timestamp >= press.timestamp,
           key.windowNumber == press.window || key.windowNumber == 0 {
            cancelPress()
            if key.keyCode == 53 { popover.performClose(nil) }
        }
        NSApplication.shared.sendEvent(key)
        return true
    }

    private func routePointer(_ event: NSEvent, fromButton: Bool = false) -> NSEvent? {
        let sameWindow = event.window != nil && event.window === button.window
        let location: NSPoint
        if fromButton, !sameWindow, let window = button.window {
            let screen = event.window?.convertPoint(toScreen: event.locationInWindow) ?? event.locationInWindow
            location = window.convertPoint(fromScreen: screen)
        } else {
            location = event.locationInWindow
        }
        let point = button.convert(location, from: nil)
        let inside = (sameWindow || fromButton) && button.bounds.contains(point)
        if event.type == .leftMouseUp, let timestamp = lastPressTimestamp,
           event.timestamp >= timestamp, event.windowNumber == lastPressWindow {
            return nil
        }
        if event.type == .keyDown {
            if let press = activePress, event.timestamp >= press.timestamp,
               event.windowNumber == press.window || event.windowNumber == 0 {
                cancelPress()
                if event.keyCode == 53 {
                    popover.performClose(nil)
                }
            }
            return event
        }
        guard activePress != nil || inside else { return event }
        switch event.type {
        case .leftMouseDown:
            guard inside, hasProjection else { return nil }
            let window = button.window?.windowNumber ?? event.windowNumber
            if lastPressTimestamp == event.timestamp, lastPressWindow == window { return nil }
            if let press = activePress, !press.held, event.timestamp > press.timestamp {
                finishPress()
                // Key delivery may have started another press or closed the view.
                guard activePress == nil, hasProjection else { return nil }
            }
            cancelPress()
            lastPressTimestamp = event.timestamp
            lastPressWindow = window
            activePress = StatusPress(
                window: window, origin: mouseLocation(), timestamp: event.timestamp
            )
            button.highlight(true)
            if event.modifierFlags.contains(.control) {
                activePress?.held = true
                cancelPress()
                openMenu()
            } else {
                // Deliver one original app-queued key per tick; a stale head must not hide Escape.
                let keyboard = Timer(timeInterval: 0.01, repeats: true) { [weak self] _ in
                    MainActor.assumeIsolated {
                        guard let self, let press = self.activePress, !press.held,
                              self.deliverQueuedKey(for: press) else { return }
                    }
                }
                keyboardTimer = keyboard
                RunLoop.main.add(keyboard, forMode: .common)
                RunLoop.main.add(keyboard, forMode: .eventTracking)
                let timer = Timer(timeInterval: Self.pressPollInterval, repeats: true) { [weak self] _ in
                    MainActor.assumeIsolated {
                        self?.pollPress()
                    }
                }
                pressTimer = timer
                RunLoop.main.add(timer, forMode: .common)
                RunLoop.main.add(timer, forMode: .eventTracking)
            }
            return nil
        case .leftMouseDragged:
            return nil
        case .leftMouseUp:
            return nil
        case .rightMouseDown:
            cancelPress()
            return nil
        case .rightMouseUp:
            if inside { openMenu() }
            return nil
        default:
            return event
        }
    }

    @objc private func cyclePreview() {
        if hasProjection && toggleEnabled { emit(.cyclePreview) }
    }

    @objc private func showInfo() { emit(.openInfo) }
    @objc private func showSetup() {
        popover.performClose(nil)
        emit(.openSetup)
    }

    @objc func showLog() {
        popover.performClose(nil)
        emit(.showLog)
    }
    @objc func quit() {
        popover.performClose(nil)
        emit(.quit)
    }
    @objc private func statusPressed() {
        if let event = NSApplication.shared.currentEvent, event.type == .keyDown, event.keyCode == 53 {
            cancelPress()
            popover.performClose(nil)
            return
        }
        guard activePress == nil else { return }
        guard (button.cell as? StatusPointerCell)?.performingClick == true else { return }
        if hasProjection && toggleEnabled { emit(.toggle) }
    }

    func render(text: [String: String], flags: [String: Bool], setupAdmission: (() -> Bool)? = nil) {
        if flags["terminate"] == true {
            cancelPress()
            hasProjection = false
            stopSetup()
            terminate()
            return
        }
        guard let status = text["status"], let asset = text["icon_asset"], let previewAsset = text["preview_asset"],
              let badge = text["badge"], let accessible = text["accessibility"],
              let showLog = text["show_log"], let quit = text["quit"], let openMenu = text["open_menu"],
              let cycle = text["cycle_preview"],
              let version = text["version"], let infoLabel = text["info_label"],
              let infoHelp = text["info_help"],
              text["error"] != nil, let tooltip = text["tooltip"],
              let logTitle = text["log_title"], let logText = text["log_text"],
              let logVisible = flags["log_visible"], let quitEnabled = flags["quit_enabled"],
              let toggleEnabled = flags["toggle_enabled"], let activateLog = flags["activate_log"]
        else {
            emit(.rendererFailed)
            return
        }
        guard let image = images[asset], let largeImage = images[previewAsset] else {
            emit(.rendererFailed)
            return
        }
        statusLine.stringValue = status
        statusLine.toolTip = tooltip
        statusLine.setAccessibilityHelp(tooltip)
        versionLine.stringValue = version
        infoItem.setAccessibilityLabel(infoLabel)
        infoItem.setAccessibilityHelp(infoHelp)
        infoItem.toolTip = infoHelp
        setupItem.title = text["setup_label"] ?? ""
        setupItem.setAccessibilityLabel(setupItem.title)
        setupItem.isEnabled = flags["setup_enabled"] == true
        showLogItem.title = showLog
        showLogItem.isEnabled = true
        quitItem.title = quit
        quitItem.isEnabled = quitEnabled
        let quitWidth = max(44, ceil(quitItem.fittingSize.width))
        let contentWidth = max(172, ceil((30 + showLogItem.fittingSize.width + quitWidth) / 2) * 2)
        let showWidth = contentWidth - 30 - quitWidth
        guard let content = popover.contentViewController?.view else {
            emit(.rendererFailed)
            return
        }
        content.frame.size = NSSize(width: contentWidth, height: 212)
        popover.contentSize = content.frame.size
        preview.frame.origin.x = (contentWidth - 96) / 2
        statusLine.frame.size.width = contentWidth - 24
        showLogItem.frame.size.width = showWidth
        quitItem.frame = NSRect(x: 12 + showWidth + 6, y: 18, width: quitWidth, height: 24)
        infoItem.frame.origin.x = contentWidth - 28
        versionLine.frame.size.width = contentWidth - 24
        setupItem.frame.size.width = contentWidth - 24
        if self.toggleEnabled && !toggleEnabled {
            cancelPress()
        }
        self.toggleEnabled = toggleEnabled
        preview.isEnabled = toggleEnabled
        hasProjection = true
        button.isEnabled = true
        if button.image !== image { button.image = image }
        if preview.image !== largeImage { preview.image = largeImage }
        preview.setAccessibilityLabel(cycle)
        preview.setAccessibilityHelp(tooltip)
        preview.setAccessibilityValue(status)
        button.title = badge
        button.toolTip = tooltip
        button.setAccessibilityHelp(tooltip)
        button.setAccessibilityLabel(accessible)
        button.setAccessibilityValue(status)
        button.setAccessibilityCustomActions([
            NSAccessibilityCustomAction(name: openMenu, handler: { [weak self] in
                guard let self else { return false }
                self.openMenu()
                return true
            })
        ])
        if flags["open_info"] == true {
            guard let target = text["info_url"], target == "https://ai2sw.info",
                  let url = URL(string: target) else {
                emit(.infoFailed)
                return
            }
            emit(openURL(url) ? .infoOpened : .infoFailed)
        }
        if logVisible {
            presentLog(title: logTitle, text: logText, activate: activateLog)
        } else {
            logWindow?.orderOut(nil)
        }
        if flags["setup_visible"] == true {
            presentSetup(text: text, open: flags["open_setup"] == true, admission: setupAdmission)
        } else {
            stopSetup()
        }
    }

    private func stopSetup() {
        setupSession = nil
        setupWindow?.orderOut(nil)
    }

    private func presentSetup(text: [String: String], open: Bool, admission: (() -> Bool)?) {
        guard let value = text["setup_session"], let session = UInt64(value),
              let instruction = text["setup_instruction"], let explanation = text["setup_explanation"],
              let message = text["setup_message"], let title = text["setup_title"] else {
            emit(.rendererFailed)
            return
        }
        if open {
            if let admission, !admission() {
                stopSetup()
                return
            }
            stopSetup()
            let app = currentApplication().standardizedFileURL
            guard app.isFileURL, app.pathExtension == "app",
                  let target = text["setup_url"],
                  target == "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                  let url = URL(string: target) else {
                emit(.setupOpened(session: session, opened: false))
                return
            }
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 460, height: 360),
                styleMask: [.titled, .closable], backing: .buffered, defer: false
            )
            window.isReleasedWhenClosed = false
            window.delegate = self
            let heading = NSTextField(wrappingLabelWithString: instruction)
            heading.frame = NSRect(x: 20, y: 312, width: 420, height: 28)
            heading.font = .systemFont(ofSize: 22, weight: .semibold)
            heading.alignment = .center
            let body = NSTextField(wrappingLabelWithString: explanation)
            body.frame = NSRect(x: 20, y: 105, width: 420, height: 80)
            body.font = .systemFont(ofSize: 13)
            let status = NSTextField(wrappingLabelWithString: message)
            status.frame = NSRect(x: 20, y: 20, width: 420, height: 72)
            status.font = .systemFont(ofSize: 13)
            status.setAccessibilityElement(true)
            let icon = ApplicationIconDragView(applicationURL: app, beginDrag: beginSetupDrag)
            icon.setAccessibilityHelp(instruction)
            for child in [heading, icon, body, status] { window.contentView?.addSubview(child) }
            setupWindow = window
            setupInstruction = heading
            setupExplanation = body
            setupMessage = status
            setupIcon = icon
            setupSession = session
            window.title = title
            if displaySurfaces {
                window.center()
                window.makeKeyAndOrderFront(nil)
            }
            emit(.setupOpened(session: session, opened: openURL(url)))
        }
        if setupSession == session {
            setupWindow?.title = title
            setupMessage?.stringValue = message
        }
    }

    private func presentLog(title: String, text: String, activate: Bool) {
        if logWindow == nil {
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 640, height: 420),
                styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false
            )
            window.delegate = self
            window.isReleasedWhenClosed = false
            window.minSize = NSSize(width: 320, height: 180)
            let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 640, height: 420))
            scroll.autoresizingMask = [.width, .height]
            scroll.hasVerticalScroller = true
            let view = NSTextView(frame: scroll.bounds)
            view.isEditable = false
            view.isSelectable = true
            view.isRichText = false
            view.autoresizingMask = [.width]
            view.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
            scroll.documentView = view
            window.contentView = scroll
            logWindow = window
            logTextView = view
        }
        logWindow?.title = title
        let changed = logTextView?.string != text
        if changed { logTextView?.string = text }
        if changed || activate {
            logTextView?.setSelectedRange(NSRange(location: 0, length: 0))
            logTextView?.scrollRangeToVisible(NSRange(location: 0, length: 0))
        }
        if displaySurfaces && activate {
            if #available(macOS 14.0, *) {
                NSApplication.shared.activate()
            } else {
                NSApplication.shared.activate(ignoringOtherApps: true)
            }
            logWindow?.makeKeyAndOrderFront(nil)
        }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if sender === setupWindow {
            if let session = setupSession {
                stopSetup()
                emit(.closeSetup(session: session))
            }
        } else if sender === logWindow {
            emit(.closeLog)
        }
        return false
    }
}
// end MenuBarController.swift
