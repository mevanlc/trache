import AppKit
import Foundation

let start = ProcessInfo.processInfo.systemUptime
precondition(CommandLine.arguments.count >= 3, "Usage: appkit MODE FILE...")
let mode = CommandLine.arguments[1]
let paths = Array(CommandLine.arguments.dropFirst(2))
var window: NSWindow?

func emit(_ event: String, _ fields: [String: Any] = [:]) {
    var object = fields
    object["event"] = event
    object["seconds"] = ProcessInfo.processInfo.systemUptime - start
    let data = try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    FileHandle.standardOutput.write(data + Data([10]))
}

func trashFiles() {
    for path in paths {
        do {
            var destination: NSURL?
            try FileManager.default.trashItem(at: URL(fileURLWithPath: path), resultingItemURL: &destination)
            emit("move", ["source": path, "destination": destination?.path ?? ""])
        } catch {
            emit("error", ["source": path, "description": String(describing: error)])
            exit(1)
        }
    }
    emit("work_done")
}

@MainActor
func finish(_ app: NSApplication) {
    emit("finish", ["running": app.isRunning])
    if mode.hasSuffix("-exit") {
        exit(0)
    }
    app.terminate(nil)
    emit("terminate_returned")
}

@MainActor
func recycle(_ remaining: [String]) {
    let batch = mode == "workspace-serial" ? Array(remaining.prefix(1)) : remaining
    emit("recycle_start", ["count": batch.count])
    NSWorkspace.shared.recycle(batch.map { URL(fileURLWithPath: $0) }) { destinations, error in
        DispatchQueue.main.async {
            emit("recycle_callback", ["count": destinations.count, "error": error.map(String.init(describing:)) ?? ""])
            for path in batch {
                if let destination = destinations[URL(fileURLWithPath: path)] {
                    emit("move", ["source": path, "destination": destination.path])
                }
            }
            if error != nil || destinations.count != batch.count { exit(1) }
            let rest = Array(remaining.dropFirst(batch.count))
            if !rest.isEmpty {
                recycle(rest)
            } else {
                emit("work_done")
                finish(NSApp)
            }
        }
    }
}

@MainActor
func work() {
    emit("work_start", ["running": NSApp.isRunning, "main_thread": Thread.isMainThread])
    if mode == "workspace-recycle" || mode == "workspace-serial" {
        recycle(paths)
        return
    }
    if mode == "lifecycle-services" {
        _ = NSWorkspace.shared
        _ = NSDocumentController.shared
        emit("services_initialized")
    }
    if mode == "lifecycle-window" {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 100, height: 100),
                          styleMask: [.titled], backing: .buffered, defer: false)
        window?.isReleasedWhenClosed = false
        emit("window_created", ["visible": window?.isVisible ?? false])
    }
    trashFiles()
    if mode == "lifecycle-services" {
        NSApp.updateWindows()
        NSDocumentController.shared.saveAllDocuments(nil)
        emit("services_updated")
    }
    if mode == "lifecycle-window" {
        NSApp.updateWindows()
        window?.close()
        window = nil
        emit("window_closed")
    }
    if mode == "lifecycle-linger" {
        DispatchQueue.main.asyncAfter(deadline: .now() + 3) { finish(NSApp) }
    } else if mode == "lifecycle-turn" {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { finish(NSApp) }
    } else {
        finish(NSApp)
    }
}

@MainActor
final class Delegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        emit("will_launch")
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        emit("did_launch")
        // Let the launch notification finish before starting the operation.
        DispatchQueue.main.asyncAfter(deadline: .now() + (mode == "lifecycle-ready" ? 0.25 : 0)) { work() }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        emit("should_terminate")
        if mode == "lifecycle-defer" {
            perform(#selector(replyToTermination), with: nil, afterDelay: 0.05,
                    inModes: [.default, .modalPanel])
            return .terminateLater
        }
        return .terminateNow
    }

    @objc func replyToTermination() {
        emit("reply_to_termination")
        NSApp.reply(toApplicationShouldTerminate: true)
    }

    func applicationWillTerminate(_ notification: Notification) {
        emit("will_terminate")
    }
}

precondition(Thread.isMainThread)
emit("start", ["pid": ProcessInfo.processInfo.processIdentifier, "mode": mode])
if mode == "appkit-link" {
    trashFiles()
    exit(0)
}
MainActor.assumeIsolated {
    let app = NSApplication.shared
    let policy: NSApplication.ActivationPolicy =
        mode == "lifecycle-accessory" || mode == "lifecycle-window" ? .accessory : .prohibited
    let policyResult = app.setActivationPolicy(policy)
    emit("initialized", ["set_policy_returned": policyResult,
                         "activation_policy": app.activationPolicy().rawValue,
                         "bundle": Bundle.main.bundlePath])
    let delegate = Delegate()
    app.delegate = delegate
    withExtendedLifetime(delegate) {
        if mode.hasPrefix("init-") {
            trashFiles()
            finish(app)
        } else {
            app.run()
            fatalError("NSApplication.run returned unexpectedly")
        }
    }
}
