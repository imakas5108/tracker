import Foundation
import Darwin

public enum LocalEndpoint {
    public static var directory: URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("tracker-control-\(getuid())", isDirectory: true)
    }
    public static var path: String { directory.appendingPathComponent("control.sock").path }
    public static func prepare() throws {
        let path = directory.path
        var info = stat()
        if lstat(path, &info) == 0 {
            guard info.st_uid == getuid(), info.st_mode & S_IFMT == S_IFDIR, info.st_mode & 0o077 == 0 else {
                throw AutomationError("unsafe_endpoint", "The control directory must be a private directory owned by this user.")
            }
        } else {
            guard mkdir(path, 0o700) == 0 || errno == EEXIST else { throw socketError("create directory") }
            try prepare()
        }
    }
}

private func socketError(_ action: String) -> AutomationError {
    AutomationError("connection_error", "Could not \(action): \(String(cString: strerror(errno)))")
}

public final class LocalConnection: @unchecked Sendable {
    public let fd: Int32
    public init(fd: Int32) {
        self.fd = fd
        var enabled: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &enabled, socklen_t(MemoryLayout.size(ofValue: enabled)))
        var timeout = timeval(tv_sec: 30, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout.size(ofValue: timeout)))
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout.size(ofValue: timeout)))
    }
    deinit { Darwin.close(fd) }
    public static func connect(path: String = LocalEndpoint.path) throws -> LocalConnection {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw socketError("create socket") }
        let connection = LocalConnection(fd: fd)
        var address = try Self.address(path)
        let result = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
        }
        guard result == 0 else { throw socketError("connect to Tracker") }
        try connection.checkPeer()
        return connection
    }
    public func checkPeer() throws {
        var uid: uid_t = 0, gid: gid_t = 0
        guard getpeereid(fd, &uid, &gid) == 0, uid == getuid() else { throw AutomationError("unauthorized", "Local peer is not the current user.") }
    }
    static func address(_ path: String) throws -> sockaddr_un {
        var address = sockaddr_un()
        let bytes = Array(path.utf8CString)
        guard bytes.count <= MemoryLayout.size(ofValue: address.sun_path) else { throw AutomationError("invalid_endpoint", "Control socket path is too long.") }
        address.sun_family = sa_family_t(AF_UNIX)
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        withUnsafeMutableBytes(of: &address.sun_path) { destination in destination.copyBytes(from: bytes.map { UInt8(bitPattern: $0) }) }
        return address
    }
    public func write(_ data: Data) throws {
        guard data.count <= 16*1024*1024 else { throw AutomationError("message_too_large", "Control message exceeds 16 MiB.") }
        var length = UInt32(data.count).bigEndian
        try send(Data(bytes: &length, count: 4)); try send(data)
    }
    private func send(_ data: Data) throws {
        try data.withUnsafeBytes { buffer in
            var offset = 0
            while offset < data.count {
                let count = Darwin.write(fd, buffer.baseAddress!.advanced(by: offset), data.count-offset)
                if count < 0 && errno == EINTR { continue }
                guard count > 0 else { throw socketError("write message") }; offset += count
            }
        }
    }
    public func read(limit: Int = 16*1024*1024) throws -> Data {
        let header = try receive(4)
        let length = header.withUnsafeBytes { UInt32(bigEndian: $0.loadUnaligned(as: UInt32.self)) }
        guard length > 0, length <= limit else { throw AutomationError("message_too_large", "Invalid control message length.") }
        return try receive(Int(length))
    }
    private func receive(_ count: Int) throws -> Data {
        var data = Data(count: count)
        try data.withUnsafeMutableBytes { buffer in
            var offset = 0
            while offset < count {
                let n = Darwin.read(fd, buffer.baseAddress!.advanced(by: offset), count-offset)
                if n < 0 && errno == EINTR { continue }
                guard n > 0 else { throw socketError("read message") }; offset += n
            }
        }
        return data
    }
    public func call(_ request: AutomationRequest) throws -> AutomationResponse {
        try write(JSONEncoder().encode(request))
        let response = try JSONDecoder().decode(AutomationResponse.self, from: read())
        guard response.version == 1 else { throw AutomationError("incompatible_protocol", "Unsupported Tracker protocol version.") }
        return response
    }
}

public final class LocalSocketServer: @unchecked Sendable {
    private var listener: Int32 = -1
    private let lock = NSLock()
    private let slots = DispatchSemaphore(value: 4)
    private let path: String
    private var ownership: Int32 = -1
    public init(path: String = LocalEndpoint.path) { self.path = path }
    public func start(handler: @escaping @Sendable (AutomationRequest) async -> AutomationResponse) throws {
        try LocalEndpoint.prepare()
        // A persistent lock file prevents two launching apps from removing each other's socket.
        let owner = open(path+".lock", O_CREAT | O_RDWR | O_NOFOLLOW, 0o600)
        guard owner >= 0 else { throw socketError("open endpoint lock") }
        var ownerInfo = stat()
        guard fstat(owner, &ownerInfo) == 0, ownerInfo.st_uid == getuid(),
              ownerInfo.st_mode & S_IFMT == S_IFREG, ownerInfo.st_mode & 0o077 == 0 else {
            Darwin.close(owner); throw AutomationError("unsafe_endpoint", "Invalid endpoint lock file.")
        }
        guard flock(owner, LOCK_EX | LOCK_NB) == 0 else {
            Darwin.close(owner); throw AutomationError("already_running", "Another Tracker control endpoint is active.")
        }
        var started = false
        defer { if !started { Darwin.close(owner) } }
        var info = stat()
        if lstat(path, &info) == 0 {
            guard info.st_uid == getuid(), info.st_mode & S_IFMT == S_IFSOCK else { throw AutomationError("unsafe_endpoint", "Unexpected control endpoint file.") }
            guard unlink(path) == 0 else { throw socketError("remove stale socket") }
        }
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw socketError("create listener") }
        var address = try LocalConnection.address(path)
        let bound = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
        }
        guard bound == 0, chmod(path, 0o600) == 0, listen(fd, 8) == 0 else { Darwin.close(fd); throw socketError("bind listener") }
        lock.lock(); listener = fd; ownership = owner; lock.unlock(); started = true
        DispatchQueue.global(qos: .utility).async { [self] in
            while true {
                slots.wait()
                let peer = accept(fd, nil, nil)
                if peer < 0 { slots.signal(); break }
                let connection = LocalConnection(fd: peer)
                DispatchQueue.global(qos: .utility).async { [self] in
                    do {
                        try connection.checkPeer()
                        let request = try JSONDecoder().decode(AutomationRequest.self, from: connection.read(limit: 1024*1024))
                        Task {
                            defer { self.slots.signal() }
                            let result = await handler(request)
                            try? connection.write(JSONEncoder().encode(result))
                        }
                    } catch {
                        try? connection.write(JSONEncoder().encode(AutomationResponse(error: error as? AutomationError ?? AutomationError("invalid_request", error.localizedDescription))))
                        slots.signal()
                    }
                }
            }
        }
    }
    public func stop() {
        lock.lock(); let fd = listener, owner = ownership; listener = -1; ownership = -1; lock.unlock()
        if fd >= 0 { shutdown(fd, SHUT_RDWR); Darwin.close(fd); unlink(path) }
        if owner >= 0 { Darwin.close(owner) }
    }
    deinit { stop() }
}
