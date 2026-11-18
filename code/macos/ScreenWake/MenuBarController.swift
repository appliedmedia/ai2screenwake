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

@MainActor
final class StatusInputView: NSView {
    var track: ((NSEvent, NSRect, NSView) -> Void)?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        track?(event, bounds, self)
    }
}

@MainActor
private final class StatusPress {
    var cancelled = false
    var held = false
}

@MainActor
final class MenuBarController: NSObject, NSWindowDelegate {
    private static let statusWidth = NSStatusItem.squareLength

    let popover: NSPopover
    let preview = NSButton(frame: NSRect(x: 38, y: 80, width: 96, height: 96))
    let statusLine = NSTextField(labelWithString: "")
    let showLogItem = NSButton(frame: NSRect(x: 12, y: 12, width: 94, height: 30))
    let quitItem = NSButton(frame: NSRect(x: 112, y: 12, width: 48, height: 30))
    let inputView = StatusInputView()
    let secondary = NSClickGestureRecognizer()
    let button: NSButton
    private let item: NSStatusItem?
    private let emit: (NativeIntent) -> Void
    private let terminate: () -> Void
    private let displaySurfaces: Bool
    private let images: [String: NSImage]
    private var hasProjection = false
    private var toggleEnabled = false
    private var activePress: StatusPress?
    private var pressTimer: Timer?
    private(set) var menuAnchor: NSRect?
    private(set) var logWindow: NSWindow?
    private(set) var logTextView: NSTextView?

    init(
        assets: URL?, displaySurfaces: Bool = true, popover suppliedPopover: NSPopover? = nil,
        makeStatusButton: ((CGFloat) -> (NSStatusItem?, NSButton?))? = nil,
        emit: @escaping (NativeIntent) -> Void, terminate: @escaping () -> Void
    ) throws {
        self.emit = emit
        self.terminate = terminate
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
        let content = MenuContentView(frame: NSRect(x: 0, y: 0, width: 172, height: 188))
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
        statusLine.frame = NSRect(x: 12, y: 48, width: 148, height: 24)
        statusLine.alignment = .center
        statusLine.font = .systemFont(ofSize: 13, weight: .semibold)
        for child in [preview, statusLine, showLogItem, quitItem] {
            content.addSubview(child)
        }
        showLogItem.bezelStyle = .rounded
        quitItem.bezelStyle = .rounded
        showLogItem.keyEquivalent = "l"
        quitItem.keyEquivalent = "q"
        showLogItem.nextKeyView = quitItem
        quitItem.nextKeyView = preview
        preview.nextKeyView = showLogItem
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
        inputView.frame = button.bounds
        inputView.autoresizingMask = [.width, .height]
        inputView.setAccessibilityElement(false)
        inputView.setAccessibilityHidden(true)
        inputView.track = { [weak self] event, frame, control in
            _ = self?.trackPrimary(event, frame: frame, control: control)
        }
        button.addSubview(inputView)
        secondary.target = self
        secondary.action = #selector(secondaryClicked(_:))
        secondary.buttonMask = 2
        secondary.numberOfClicksRequired = 1
        secondary.delaysSecondaryMouseButtonEvents = true
        button.addGestureRecognizer(secondary)
        if images.count != 6 {
            emit(.rendererFailed)
        }
    }

    private func openMenu() {
        guard hasProjection else { return }
        menuAnchor = button.bounds
        if displaySurfaces {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        }
    }

    private func trackPrimary(_ down: NSEvent, frame: NSRect, control: NSView) -> Bool {
        guard down.type == .leftMouseDown, hasProjection, button.isEnabled else { return false }
        let press = StatusPress()
        activePress = press
        button.highlight(true)
        defer {
            pressTimer?.invalidate()
            pressTimer = nil
            activePress = nil
            button.highlight(false)
        }
        if down.modifierFlags.contains(.control) {
            press.held = true
            openMenu()
        } else {
            let timer = Timer(timeInterval: 0.45, repeats: false) { [weak self, weak press] _ in
                MainActor.assumeIsolated {
                    guard let self, let press, !press.cancelled else { return }
                    press.held = true
                    self.openMenu()
                }
            }
            pressTimer = timer
            RunLoop.main.add(timer, forMode: .eventTracking)
        }
        // A transparent hit-tested view owns pointer tracking, not NSControl's
        // additional cell loop. The status button retains drawing and AX/keys.
        while !press.cancelled {
            guard let event = NSApplication.shared.nextEvent(
                matching: [.leftMouseUp, .leftMouseDragged, .keyDown],
                until: Date(timeIntervalSinceNow: 0.05), inMode: .eventTracking, dequeue: true
            ) else { continue }
            switch event.type {
            case .leftMouseUp:
                let inside = event.windowNumber == down.windowNumber &&
                    frame.contains(control.convert(event.locationInWindow, from: nil))
                if inside && !press.cancelled && !press.held {
                    button.sendAction(button.action, to: button.target)
                }
                return inside && !press.cancelled
            case .leftMouseDragged:
                press.cancelled = true
                return false
            case .keyDown where event.keyCode == 53:
                press.cancelled = true
                popover.performClose(nil)
                return false
            default:
                NSApplication.shared.sendEvent(event)
            }
        }
        return false
    }

    @objc private func cyclePreview() {
        if hasProjection && toggleEnabled { emit(.cyclePreview) }
    }

    @objc private func secondaryClicked(_ recognizer: NSClickGestureRecognizer) {
        if recognizer.state == .ended { openMenu() }
    }

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
        popover.contentSize = NSSize(width: contentWidth, height: 188)
        preview.frame.origin.x = (contentWidth - 96) / 2
        statusLine.frame.size.width = contentWidth - 24
        showLogItem.frame.size.width = showWidth
        quitItem.frame = NSRect(x: 12 + showWidth + 6, y: 12, width: quitWidth, height: 30)
        self.toggleEnabled = toggleEnabled
        preview.isEnabled = toggleEnabled
        if !toggleEnabled {
            activePress?.cancelled = true
            pressTimer?.invalidate()
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
