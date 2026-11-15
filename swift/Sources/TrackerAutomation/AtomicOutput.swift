import Foundation
import Darwin

public enum AtomicOutput {
    public static func write(_ data: Data, to destination: URL, overwrite: Bool) throws {
        guard destination.isFileURL else { throw AutomationError("invalid_path", "An absolute local file path is required.") }
        let temporary = destination.deletingLastPathComponent().appendingPathComponent(".tracker-\(UUID().uuidString).tmp")
        try data.write(to: temporary, options: .withoutOverwriting)
        defer { try? FileManager.default.removeItem(at: temporary) }
        let result = overwrite ? rename(temporary.path, destination.path) : link(temporary.path, destination.path)
        guard result == 0 else {
            if errno == EEXIST { throw AutomationError("destination_exists", "Destination exists; supply overwrite=true to replace it.") }
            throw AutomationError("file_write_failed", String(cString: strerror(errno)))
        }
    }
}
