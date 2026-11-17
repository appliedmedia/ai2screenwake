// main.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
import AppKit

final class ObservedMenu: NativeObserver, @unchecked Sendable {
    @MainActor private weak var controller: MenuBarController?
    @MainActor private var pending: (text: [String: String], flags: [String: Bool])?
    @MainActor var onTermination: (() -> Void)?
    @MainActor var onTerminationRejected: (() -> Void)?

    @MainActor
    func attach(_ controller: MenuBarController) {
        self.controller = controller
        if let pending {
            self.pending = nil
            controller.render(text: pending.text, flags: pending.flags)
        }
    }

    @MainActor
    func receive(text: [String: String], flags: [String: Bool]) {
        if flags["terminate"] == true {
            onTermination?()
        } else {
            if flags["termination_failed"] == true { onTerminationRejected?() }
            if let controller {
                controller.render(text: text, flags: flags)
            } else {
                pending = (text, flags)
            }
        }
    }

    func observed(text: [String: String], flags: [String: Bool]) {
        DispatchQueue.main.async { [weak self] in
            self?.receive(text: text, flags: flags)
        }
    }
}

@MainActor
final class ScreenWakeApplication: NSObject, NSApplicationDelegate {
    private var host: NativeHost?
    private var controller: MenuBarController?
    private var observer: ObservedMenu?
    private var terminationApproved = false
    private var terminationPending = false
    private let reply: @MainActor (Bool) -> Void

    init(reply: @escaping @MainActor (Bool) -> Void = { NSApplication.shared.reply(toApplicationShouldTerminate: $0) }) {
        self.reply = reply
        super.init()
    }

    func cancelTermination() {
        if terminationPending {
            terminationPending = false
            reply(false)
        }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            let observer = ObservedMenu()
            observer.onTermination = { [weak self] in self?.finishTermination() }
            observer.onTerminationRejected = { [weak self] in self?.cancelTermination() }
            let host = try NativeHost(observer: observer)
            self.host = host
            self.observer = observer
            do {
                let controller = try MenuBarController(
                    assets: Bundle.main.resourceURL, emit: { host.emit(intent: $0) },
                    terminate: { [weak self] in self?.finishTermination() }
                )
                observer.attach(controller)
                self.controller = controller
            } catch {
                host.emit(intent: .rendererFailed)
                host.emit(intent: .quit)
            }
        } catch {
            // The generated constructor throws only before host admission; normal
            // owner/resource failures are bus-observed and rendered in the menu.
            fputs("AI2ScreenWake host startup failed: \(error)\n", stderr)
            exit(EXIT_FAILURE)
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if terminationApproved { return .terminateNow }
        terminationPending = true
        host?.emit(intent: .quit)
        return .terminateLater
    }

    func finishTermination() {
        terminationApproved = true
        if terminationPending {
            reply(true)
        } else {
            NSApplication.shared.terminate(nil)
        }
    }
}

MainActor.assumeIsolated {
#if SCREENWAKE_CONTRACT_TESTS
do {
    guard CommandLine.arguments.count == 2 else {
        throw MenuContractFailure.failed("contract harness requires the canonical asset directory")
    }
    try runMenuBarContracts(assets: URL(fileURLWithPath: CommandLine.arguments[1]))
} catch {
    fputs("AI2ScreenWake AppKit contract failed: \(error)\n", stderr)
    exit(EXIT_FAILURE)
}
#else
let application = NSApplication.shared
let applicationDelegate = ScreenWakeApplication()
application.delegate = applicationDelegate
application.setActivationPolicy(.accessory)
application.run()
#endif
}
// end main.swift
