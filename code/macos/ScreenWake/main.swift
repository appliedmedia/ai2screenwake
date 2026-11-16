// main.swift Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
import AppKit

final class ObservedMenu: NativeObserver, @unchecked Sendable {
    weak var controller: MenuBarController?
    var onTermination: (@MainActor () -> Void)?

    func observed(text: [String: String], flags: [String: Bool]) {
        DispatchQueue.main.async { [weak self] in
            if flags["terminate"] == true {
                self?.onTermination?()
            } else {
                self?.controller?.render(text: text, flags: flags)
            }
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

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            let observer = ObservedMenu()
            observer.onTermination = { [weak self] in self?.finishTermination() }
            let host = try NativeHost(observer: observer)
            self.host = host
            self.observer = observer
            do {
                let controller = try MenuBarController(
                    assets: Bundle.main.resourceURL, emit: { host.emit(intent: $0) },
                    terminate: { [weak self] in self?.finishTermination() }
                )
                observer.controller = controller
                self.controller = controller
            } catch {
                host.emit(intent: .rendererFailed)
                host.emit(intent: .quit)
            }
        } catch {
            // The generated constructor throws only before host admission; normal
            // owner/resource failures are bus-observed and rendered in the menu.
            fputs("ScreenWake host startup failed: \(error)\n", stderr)
            exit(EXIT_FAILURE)
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if terminationApproved { return .terminateNow }
        terminationPending = true
        host?.emit(intent: .quit)
        return .terminateLater
    }

    private func finishTermination() {
        terminationApproved = true
        if terminationPending {
            NSApplication.shared.reply(toApplicationShouldTerminate: true)
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
    fputs("ScreenWake AppKit contract failed: \(error)\n", stderr)
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
