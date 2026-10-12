import AppKit

// A regular, foreground GUI app. The harness supplies disposable input files.
let started = ProcessInfo.processInfo.systemUptime
precondition(CommandLine.arguments.count >= 3, "Usage: probe GUI_MODE FILE...")
let mode = CommandLine.arguments[1]
let paths = Array(CommandLine.arguments.dropFirst(2))

func emit(_ event: String, _ fields: [String: Any] = [:]) {
    var record = fields
    record["event"] = event
    record["seconds"] = ProcessInfo.processInfo.systemUptime - started
    let data = try! JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
    FileHandle.standardOutput.write(data + Data([10]))
}

@MainActor
final class ProbeView: NSView {
    var drew = false

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        NSColor.windowBackgroundColor.setFill()
        dirtyRect.fill()
        ("Recycle \(paths.count) disposable files, then quit." as NSString).draw(
            at: NSPoint(x: 24, y: 65),
            withAttributes: [.font: NSFont.systemFont(ofSize: 17),
                             .foregroundColor: NSColor.labelColor])
        if !drew { emit("view_drew") }
        drew = true
    }
}

@MainActor
final class Delegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var view: ProbeView!
    var readinessTimer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        emit("did_launch")
        let menu = NSMenu()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit Trash Probe", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let item = NSMenuItem()
        item.submenu = appMenu
        menu.addItem(item)
        NSApp.mainMenu = menu

        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 440, height: 160),
                          styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Trache — GUI Trash Probe"
        window.isReleasedWhenClosed = false
        view = ProbeView(frame: window.contentView!.bounds)
        window.contentView = view
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
        // Require actual foreground/visible/drawn state, rather than merely
        // constructing a window. This wait occurs BEFORE the file operations.
        readinessTimer = Timer.scheduledTimer(withTimeInterval: 0.01, repeats: true) { [self] _ in
            MainActor.assumeIsolated { checkReady() }
        }
    }

    func checkReady() {
        if NSApp.isActive && window.isVisible && window.isKeyWindow &&
            window.occlusionState.contains(.visible) && view.drew {
            readinessTimer?.invalidate()
            readinessTimer = nil
            emit("window_ready", ["active": NSApp.isActive, "visible": window.isVisible,
                                  "key": window.isKeyWindow, "drew": view.drew,
                                  "occlusion_visible": window.occlusionState.contains(.visible),
                                  "window_number": window.windowNumber,
                                  "activation_policy": NSApp.activationPolicy().rawValue])
            recycle()
        } else if ProcessInfo.processInfo.systemUptime - started > 5 {
            emit("error", ["description": "Window did not become active, visible, key, and drawn"])
            exit(1)
        }
    }

    func recycle() {
        emit("work_start")
        if mode == "gui-filemanager" {
            do {
                for path in paths {
                    var destination: NSURL?
                    try FileManager.default.trashItem(at: URL(fileURLWithPath: path), resultingItemURL: &destination)
                    emit("move", ["source": path, "destination": destination?.path ?? ""])
                }
                finish()
            } catch {
                emit("error", ["description": String(describing: error)])
                exit(1)
            }
        } else {
            NSWorkspace.shared.recycle(paths.map { URL(fileURLWithPath: $0) }) { destinations, error in
                DispatchQueue.main.async { [self] in
                    emit("recycle_callback", ["count": destinations.count, "error": error.map(String.init(describing:)) ?? ""])
                    for path in paths {
                        if let destination = destinations[URL(fileURLWithPath: path)] {
                            emit("move", ["source": path, "destination": destination.path])
                        }
                    }
                    if error != nil || destinations.count != paths.count { exit(1) }
                    finish()
                }
            }
        }
    }

    func finish() {
        emit("work_done")
        if mode == "gui-linger" {
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) { NSApp.terminate(nil) }
        } else {
            NSApp.terminate(nil)
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        emit("should_terminate")
        return .terminateNow
    }

    func applicationWillTerminate(_ notification: Notification) { emit("will_terminate") }
}

precondition(Thread.isMainThread)
emit("start", ["pid": ProcessInfo.processInfo.processIdentifier, "mode": mode])
MainActor.assumeIsolated {
    let app = NSApplication.shared
    app.setActivationPolicy(.regular)
    emit("initialized", ["bundle": Bundle.main.bundlePath, "activation_policy": app.activationPolicy().rawValue])
    let delegate = Delegate()
    app.delegate = delegate
    withExtendedLifetime(delegate) { app.run() }
}
