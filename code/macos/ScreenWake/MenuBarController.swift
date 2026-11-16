// MenuBarController.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
import AppKit

enum MenuViewFailure: Error {
    case statusButtonUnavailable
    case assetsUnavailable
}

@MainActor
final class MenuBarController: NSObject, NSGestureRecognizerDelegate, NSWindowDelegate {
    enum Gesture { case primary, secondary, longPress }
    enum GestureResult { case toggle, menu, none }

    let menu = NSMenu()
    let statusLine = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    let errorLine = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    let showLogItem = NSMenuItem(title: "", action: nil, keyEquivalent: "l")
    let quitItem = NSMenuItem(title: "", action: nil, keyEquivalent: "q")
    let primary = NSClickGestureRecognizer()
    let secondary = NSClickGestureRecognizer()
    let longPress = NSPressGestureRecognizer()
    let button: NSButton
    private let item: NSStatusItem?
    private let emit: (NativeIntent) -> Void
    private let terminate: () -> Void
    private let displaySurfaces: Bool
    private let images: [String: NSImage]
    private(set) var logWindow: NSWindow?
    private(set) var logTextView: NSTextView?

    init(
        assets: URL?, displaySurfaces: Bool = true,
        emit: @escaping (NativeIntent) -> Void, terminate: @escaping () -> Void
    ) throws {
        self.emit = emit
        self.terminate = terminate
        self.displaySurfaces = displaySurfaces
        if displaySurfaces {
            let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
            guard let button = item.button else {
                NSStatusBar.system.removeStatusItem(item)
                emit(.rendererFailed)
                throw MenuViewFailure.statusButtonUnavailable
            }
            self.item = item
            self.button = button
        } else {
            self.item = nil
            self.button = NSButton(frame: NSRect(x: 0, y: 0, width: 48, height: 24))
        }
        var images: [String: NSImage] = [:]
        for name in ["menu-off", "menu-on"] {
            let image = NSImage(size: NSSize(width: 24, height: 24))
            for suffix in ["", "@2x"] {
                guard let url = assets?.appendingPathComponent(name + suffix + ".png") else {
                    emit(.rendererFailed)
                    throw MenuViewFailure.assetsUnavailable
                }
                let data = try Data(contentsOf: url)
                let pixels = suffix.isEmpty ? 24 : 48
                guard let representation = NSBitmapImageRep(data: data),
                      representation.pixelsWide == pixels, representation.pixelsHigh == pixels
                else {
                    emit(.rendererFailed)
                    throw MenuViewFailure.assetsUnavailable
                }
                representation.size = NSSize(width: 24, height: 24)
                image.addRepresentation(representation)
            }
            image.isTemplate = true
            if image.representations.count == 2 { images[name] = image }
        }
        self.images = images
        super.init()
        menu.autoenablesItems = false
        menu.minimumWidth = 220
        statusLine.isEnabled = false
        errorLine.isEnabled = false
        menu.addItem(statusLine)
        menu.addItem(errorLine)
        menu.addItem(.separator())
        menu.addItem(showLogItem)
        menu.addItem(quitItem)
        showLogItem.target = self
        showLogItem.action = #selector(showLog)
        quitItem.target = self
        quitItem.action = #selector(quit)
        button.imagePosition = .imageLeft
        button.imageScaling = .scaleProportionallyDown
        button.target = self
        button.action = #selector(accessibilityPressed)
        button.sendAction(on: [])
        button.menu = menu
        primary.target = self
        primary.action = #selector(primaryClicked(_:))
        primary.buttonMask = 1
        primary.numberOfClicksRequired = 1
        primary.delegate = self
        secondary.target = self
        secondary.action = #selector(secondaryClicked(_:))
        secondary.buttonMask = 2
        secondary.numberOfClicksRequired = 1
        secondary.delaysSecondaryMouseButtonEvents = true
        longPress.target = self
        longPress.action = #selector(pressed(_:))
        longPress.buttonMask = 1
        longPress.minimumPressDuration = 0.45
        button.addGestureRecognizer(primary)
        button.addGestureRecognizer(secondary)
        button.addGestureRecognizer(longPress)
        if images.count != 2 {
            emit(.rendererFailed)
        }
    }

    func gestureRecognizer(
        _ gestureRecognizer: NSGestureRecognizer,
        shouldRequireFailureOf otherGestureRecognizer: NSGestureRecognizer
    ) -> Bool {
        gestureRecognizer === primary && otherGestureRecognizer === longPress
    }

    @discardableResult
    func handle(_ gesture: Gesture, state: NSGestureRecognizer.State) -> GestureResult {
        switch (gesture, state) {
        case (.primary, .ended) where primary.isEnabled:
            emit(.toggle)
            return .toggle
        case (.secondary, .ended), (.longPress, .began):
            if displaySurfaces {
                menu.popUp(positioning: nil, at: NSPoint(x: 0, y: 0), in: button)
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

    @objc func showLog() { emit(.showLog) }
    @objc func quit() { emit(.quit) }
    @objc private func accessibilityPressed() {
        if primary.isEnabled { emit(.toggle) }
    }

    func render(text: [String: String], flags: [String: Bool]) {
        if flags["terminate"] == true {
            terminate()
            return
        }
        guard let status = text["status"], let asset = text["icon_asset"],
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
        statusLine.title = status
        errorLine.title = error
        errorLine.isHidden = error.isEmpty
        showLogItem.title = showLog
        showLogItem.isEnabled = true
        quitItem.title = quit
        quitItem.isEnabled = quitEnabled
        primary.isEnabled = toggleEnabled
        guard let image = images[asset] else {
            emit(.rendererFailed)
            return
        }
        button.image = image
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
        if displaySurfaces && activate { logWindow?.makeKeyAndOrderFront(nil) }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        emit(.closeLog)
        return false
    }
}
// end MenuBarController.swift
