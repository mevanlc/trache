import Foundation

// A deliberately small Foundation-only baseline. No AppKit, run loop, or helper.
let start = ProcessInfo.processInfo.systemUptime
func emit(_ event: String, _ fields: [String: Any] = [:]) {
    var object = fields
    object["event"] = event
    object["seconds"] = ProcessInfo.processInfo.systemUptime - start
    let data = try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    FileHandle.standardOutput.write(data + Data([10]))
}

guard CommandLine.arguments.count >= 3,
      let linger = Double(CommandLine.arguments[1]), linger >= 0 else {
    fatalError("Usage: foundation LINGER_SECONDS FILE...")
}
emit("start", ["pid": ProcessInfo.processInfo.processIdentifier])
for path in CommandLine.arguments.dropFirst(2) {
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
if linger > 0 { Thread.sleep(forTimeInterval: linger) }
emit("exit")
