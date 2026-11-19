// MenuBarController.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
import AppKit

enum MenuViewFailure: Error {
    case statusButtonUnavailable
    case assetsUnavailable
}

@MainActor
final class MenuContentView: NSView {
    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        window?.initialFirstResponder = self
    }
}

private struct StatusPress {
    let window: Int
    let origin: NSPoint
    var cancelled = false
    var held = false
}

@MainActor
final class MenuBarController: NSObject, NSWindowDelegate {
    private static let statusWidth = NSStatusItem.squareLength

    let popover: NSPopover
    let preview = NSButton(frame: NSRect(x: 40, y: 70, width: 96, height: 96))
    let statusLine = NSTextField(labelWithString: "")
    let versionLine = NSTextField(labelWithString: "")
    let infoItem = NSButton(frame: NSRect(x: 148, y: 148, width: 16, height: 16))
    let showLogItem = NSButton(frame: NSRect(x: 12, y: 18, width: 94, height: 24))
    let quitItem = NSButton(frame: NSRect(x: 112, y: 18, width: 48, height: 24))
    let button: NSButton
    private let item: NSStatusItem?
    private let emit: (NativeIntent) -> Void
    private let terminate: () -> Void
    private let openURL: (URL) -> Bool
    private let displaySurfaces: Bool
    private let images: [String: NSImage]
    private var hasProjection = false
    private var toggleEnabled = false
    private var activePress: StatusPress?
    private var pressTimer: Timer?
    private var eventMonitor: Any?
    private(set) var menuAnchor: NSRect?
    private(set) var logWindow: NSWindow?
    private(set) var logTextView: NSTextView?

    init(
        assets: URL?, displaySurfaces: Bool = true, popover suppliedPopover: NSPopover? = nil,
        makeStatusButton: ((CGFloat) -> (NSStatusItem?, NSButton?))? = nil,
        openURL: @escaping (URL) -> Bool = { NSWorkspace.shared.open($0) },
        emit: @escaping (NativeIntent) -> Void, terminate: @escaping () -> Void
    ) throws {
        self.emit = emit
        self.terminate = terminate
        self.openURL = openURL
        self.displaySurfaces = displaySurfaces
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
        let content = MenuContentView(frame: NSRect(x: 0, y: 0, width: 176, height: 168))
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
        statusLine.frame = NSRect(x: 12, y: 46, width: 152, height: 20)
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
        for child in [preview, statusLine, showLogItem, quitItem, versionLine, infoItem] {
            content.addSubview(child)
        }
        showLogItem.bezelStyle = .rounded
        quitItem.bezelStyle = .rounded
        showLogItem.keyEquivalent = "l"
        quitItem.keyEquivalent = "q"
        showLogItem.nextKeyView = quitItem
        quitItem.nextKeyView = preview
        preview.nextKeyView = infoItem
        infoItem.nextKeyView = showLogItem
        content.nextKeyView = preview
        showLogItem.target = self
        showLogItem.action = #selector(showLog)
        quitItem.target = self
        quitItem.action = #selector(quit)
        button.imagePosition = .imageOnly
        button.imageScaling = .scaleProportionallyDown
        button.target = self
        button.action = #selector(accessibilityPressed)
        button.sendAction(on: [.leftMouseUp])
        button.isEnabled = false
        // Intercept this button's app-local stream before AppKit enters cell tracking.
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
        activePress = nil
        button.highlight(false)
    }

    private func routePointer(_ event: NSEvent) -> NSEvent? {
        let sameWindow = event.window != nil && event.window === button.window
        let point = button.convert(event.locationInWindow, from: nil)
        let inside = sameWindow && button.bounds.contains(point)
        if event.type == .keyDown {
            if event.keyCode == 53 && activePress != nil {
                cancelPress()
                popover.performClose(nil)
                return nil
            }
            return event
        }
        guard activePress != nil || inside else { return event }
        switch event.type {
        case .leftMouseDown:
            guard inside, hasProjection, toggleEnabled else { return nil }
            cancelPress()
            activePress = StatusPress(window: event.windowNumber, origin: point)
            button.highlight(true)
            if event.modifierFlags.contains(.control) {
                activePress?.held = true
                openMenu()
            } else {
                let timer = Timer(timeInterval: 0.45, repeats: false) { [weak self] _ in
                    MainActor.assumeIsolated {
                        guard let self, let press = self.activePress, !press.cancelled else { return }
                        self.activePress?.held = true
                        self.openMenu()
                    }
                }
                pressTimer = timer
                RunLoop.main.add(timer, forMode: .common)
                RunLoop.main.add(timer, forMode: .eventTracking)
            }
            return nil
        case .leftMouseDragged:
            if let press = activePress,
               !inside || hypot(point.x - press.origin.x, point.y - press.origin.y) > 4 {
                activePress?.cancelled = true
                pressTimer?.invalidate()
                button.highlight(false)
            }
            return nil
        case .leftMouseUp:
            let press = activePress
            cancelPress()
            if let press, inside, event.windowNumber == press.window,
               !press.held, !press.cancelled, toggleEnabled {
                emit(.toggle)
            }
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

    @objc func showLog() {
        popover.performClose(nil)
        emit(.showLog)
    }
    @objc func quit() {
        popover.performClose(nil)
        emit(.quit)
    }
    @objc private func accessibilityPressed() {
        if hasProjection && toggleEnabled { emit(.toggle) }
    }

    func render(text: [String: String], flags: [String: Bool]) {
        if flags["terminate"] == true {
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
        content.frame.size.width = contentWidth
        popover.contentSize = NSSize(width: contentWidth, height: 168)
        preview.frame.origin.x = (contentWidth - 96) / 2
        statusLine.frame.size.width = contentWidth - 24
        showLogItem.frame.size.width = showWidth
        quitItem.frame = NSRect(x: 12 + showWidth + 6, y: 18, width: quitWidth, height: 24)
        infoItem.frame.origin.x = contentWidth - 28
        versionLine.frame.size.width = contentWidth - 24
        self.toggleEnabled = toggleEnabled
        preview.isEnabled = toggleEnabled
        if !toggleEnabled {
            cancelPress()
        }
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
        emit(.closeLog)
        return false
    }
}
// end MenuBarController.swift
