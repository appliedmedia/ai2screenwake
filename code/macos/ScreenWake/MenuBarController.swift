// MenuBarController.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
import AppKit

enum MenuViewFailure: Error {
    case statusButtonUnavailable
    case assetsUnavailable
}

@MainActor
final class MenuBarController: NSObject, NSGestureRecognizerDelegate, NSWindowDelegate {
    private static let statusWidth: CGFloat = 48
    enum Gesture { case primary, secondary, longPress }
    enum GestureResult { case toggle, menu, none }

    let popover = NSPopover()
    let preview = NSImageView(frame: NSRect(x: 82, y: 166, width: 96, height: 96))
    let statusLine = NSTextField(labelWithString: "")
    let errorLine = NSTextField(wrappingLabelWithString: "")
    let showLogItem = NSButton(frame: NSRect(x: 14, y: 48, width: 232, height: 30))
    let quitItem = NSButton(frame: NSRect(x: 14, y: 12, width: 232, height: 30))
    let primary = NSClickGestureRecognizer()
    let secondary = NSClickGestureRecognizer()
    let controlPrimary = NSClickGestureRecognizer()
    let longPress = NSPressGestureRecognizer()
    let button: NSButton
    private let item: NSStatusItem?
    private let emit: (NativeIntent) -> Void
    private let terminate: () -> Void
    private let displaySurfaces: Bool
    private let images: [String: NSImage]
    private var hasProjection = false
    private var suppressPrimaryRelease = false
    private(set) var menuAnchor: NSRect?
    private(set) var logWindow: NSWindow?
    private(set) var logTextView: NSTextView?

    init(
        assets: URL?, displaySurfaces: Bool = true,
        makeStatusButton: ((CGFloat) -> (NSStatusItem?, NSButton?))? = nil,
        emit: @escaping (NativeIntent) -> Void, terminate: @escaping () -> Void
    ) throws {
        self.emit = emit
        self.terminate = terminate
        self.displaySurfaces = displaySurfaces
        let factory = makeStatusButton ?? { width in
            if displaySurfaces {
                let item = NSStatusBar.system.statusItem(withLength: width)
                return (item, item.button)
            }
            return (nil, NSButton(frame: NSRect(x: 0, y: 0, width: width, height: 24)))
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
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 260, height: 276))
        let controller = NSViewController()
        controller.view = content
        popover.contentViewController = controller
        popover.contentSize = content.frame.size
        popover.behavior = .transient
        popover.animates = false
        preview.imageScaling = .scaleProportionallyDown
        preview.setAccessibilityRole(.image)
        statusLine.frame = NSRect(x: 14, y: 134, width: 232, height: 24)
        statusLine.alignment = .center
        statusLine.font = .systemFont(ofSize: 13, weight: .semibold)
        errorLine.frame = NSRect(x: 14, y: 84, width: 232, height: 42)
        errorLine.font = .systemFont(ofSize: 11)
        errorLine.textColor = .labelColor
        for child in [preview, statusLine, errorLine, showLogItem, quitItem] {
            content.addSubview(child)
        }
        showLogItem.bezelStyle = .rounded
        quitItem.bezelStyle = .rounded
        showLogItem.keyEquivalent = "l"
        quitItem.keyEquivalent = "q"
        showLogItem.nextKeyView = quitItem
        quitItem.nextKeyView = showLogItem
        showLogItem.target = self
        showLogItem.action = #selector(showLog)
        quitItem.target = self
        quitItem.action = #selector(quit)
        button.imagePosition = .imageLeft
        button.imageScaling = .scaleProportionallyDown
        button.target = self
        button.action = #selector(accessibilityPressed)
        button.sendAction(on: [])
        button.isEnabled = false
        primary.target = self
        primary.action = #selector(primaryClicked(_:))
        primary.buttonMask = 1
        primary.numberOfClicksRequired = 1
        primary.delegate = self
        secondary.target = self
        secondary.action = #selector(secondaryClicked(_:))
        secondary.buttonMask = 2
        secondary.numberOfClicksRequired = 1
        secondary.delegate = self
        secondary.delaysSecondaryMouseButtonEvents = true
        controlPrimary.target = self
        controlPrimary.action = #selector(secondaryClicked(_:))
        controlPrimary.buttonMask = 1
        controlPrimary.numberOfClicksRequired = 1
        controlPrimary.delegate = self
        controlPrimary.delaysPrimaryMouseButtonEvents = true
        longPress.target = self
        longPress.action = #selector(pressed(_:))
        longPress.buttonMask = 1
        longPress.minimumPressDuration = 0.45
        longPress.delegate = self
        button.addGestureRecognizer(primary)
        button.addGestureRecognizer(secondary)
        button.addGestureRecognizer(controlPrimary)
        button.addGestureRecognizer(longPress)
        if images.count != 6 {
            emit(.rendererFailed)
        }
    }

    func gestureRecognizer(
        _ gestureRecognizer: NSGestureRecognizer,
        shouldRequireFailureOf otherGestureRecognizer: NSGestureRecognizer
    ) -> Bool {
        gestureRecognizer === primary && otherGestureRecognizer === longPress
    }

    func gestureRecognizer(
        _ gestureRecognizer: NSGestureRecognizer, shouldAttemptToRecognizeWith event: NSEvent
    ) -> Bool {
        if event.type == .leftMouseDown || event.type == .rightMouseDown {
            suppressPrimaryRelease = false
        }
        if gestureRecognizer === secondary {
            return event.type == .rightMouseDown || event.type == .rightMouseUp
        }
        let control = event.modifierFlags.contains(.control)
        let primaryEvent = event.type == .leftMouseDown || event.type == .leftMouseUp ||
            event.type == .leftMouseDragged
        return primaryEvent && (gestureRecognizer === controlPrimary ? control : !control)
    }

    @discardableResult
    func handle(_ gesture: Gesture, state: NSGestureRecognizer.State) -> GestureResult {
        switch (gesture, state) {
        case (.primary, .ended) where suppressPrimaryRelease:
            suppressPrimaryRelease = false
            return .none
        case (.primary, .ended) where hasProjection && primary.isEnabled:
            emit(.toggle)
            return .toggle
        case (.secondary, .ended), (.longPress, .began):
            guard hasProjection else { return .none }
            if gesture == .longPress { suppressPrimaryRelease = true }
            let anchor = button.bounds
            menuAnchor = anchor
            if displaySurfaces {
                popover.show(relativeTo: anchor, of: button, preferredEdge: .minY)
                popover.contentViewController?.view.window?.makeFirstResponder(showLogItem)
            }
            return .menu
        default:
            return .none
        }
    }

    @objc private func primaryClicked(_ recognizer: NSClickGestureRecognizer) {
        handle(.primary, state: recognizer.state)
    }

    @objc private func secondaryClicked(_ recognizer: NSClickGestureRecognizer) {
        handle(.secondary, state: recognizer.state)
    }

    @objc private func pressed(_ recognizer: NSPressGestureRecognizer) {
        handle(.longPress, state: recognizer.state)
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
        if hasProjection && primary.isEnabled { emit(.toggle) }
    }

    func render(text: [String: String], flags: [String: Bool]) {
        if flags["terminate"] == true {
            terminate()
            return
        }
        guard let status = text["status"], let asset = text["icon_asset"], let previewAsset = text["preview_asset"],
              let badge = text["badge"], let accessible = text["accessibility"],
              let showLog = text["show_log"], let quit = text["quit"], let openMenu = text["open_menu"],
              let error = text["error"], let tooltip = text["tooltip"],
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
        errorLine.stringValue = error
        errorLine.isHidden = error.isEmpty
        showLogItem.title = showLog
        showLogItem.isEnabled = true
        quitItem.title = quit
        quitItem.isEnabled = quitEnabled
        primary.isEnabled = toggleEnabled
        hasProjection = true
        button.isEnabled = true
        button.image = image
        preview.image = largeImage
        preview.setAccessibilityLabel(accessible)
        button.title = badge
        button.toolTip = tooltip
        button.setAccessibilityLabel(accessible)
        button.setAccessibilityValue(status)
        button.setAccessibilityCustomActions([
            NSAccessibilityCustomAction(name: openMenu, handler: { [weak self] in
                guard let self else { return false }
                self.handle(.secondary, state: .ended)
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
        logTextView?.string = text
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
