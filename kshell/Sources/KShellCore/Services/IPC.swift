import Foundation

/// Unix-domain-socket IPC so external scripts (and aerospace) can push events
/// into the running shell: `kshell trigger <event> [payload]`.
public enum IPCPaths {
    public static var socketPath: String {
        let base = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/kshell", isDirectory: true)
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        return base.appendingPathComponent("kshell.sock").path
    }
}

private func withSockAddr(
    path: String,
    _ body: (UnsafePointer<sockaddr>, socklen_t) -> Int32
) -> Int32 {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.utf8CString)
    withUnsafeMutableBytes(of: &address.sun_path) { raw in
        let destination = raw.bindMemory(to: CChar.self)
        for (index, byte) in bytes.enumerated() where index < destination.count {
            destination[index] = byte
        }
    }
    return withUnsafePointer(to: &address) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { pointer in
            body(pointer, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
}

/// Listens on the socket and posts each received line to the `EventBus`.
public final class IPCServer {
    private var listenFD: Int32 = -1
    private var source: DispatchSourceRead?
    private let queue = DispatchQueue(label: "kshell.ipc")

    public init() {}

    public func start() {
        let path = IPCPaths.socketPath
        unlink(path)

        listenFD = socket(AF_UNIX, SOCK_STREAM, 0)
        guard listenFD >= 0 else { return }

        let bound = withSockAddr(path: path) { bind(listenFD, $0, $1) }
        guard bound == 0, listen(listenFD, 8) == 0 else {
            close(listenFD)
            listenFD = -1
            return
        }

        let source = DispatchSource.makeReadSource(fileDescriptor: listenFD, queue: queue)
        source.setEventHandler { [weak self] in self?.acceptConnection() }
        source.setCancelHandler { [weak self] in
            guard let self, self.listenFD >= 0 else { return }
            close(self.listenFD)
            self.listenFD = -1
        }
        source.resume()
        self.source = source
    }

    public func stop() {
        source?.cancel()
        source = nil
        unlink(IPCPaths.socketPath)
    }

    private func acceptConnection() {
        let client = accept(listenFD, nil, nil)
        guard client >= 0 else { return }

        var buffer = [UInt8](repeating: 0, count: 4096)
        let count = read(client, &buffer, buffer.count)
        close(client)
        guard count > 0 else { return }

        let line = String(decoding: buffer[0..<count], as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !line.isEmpty else { return }

        let parts = line.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true)
        guard let event = parts.first else { return }
        let payload = parts.count > 1 ? String(parts[1]) : ""

        FileHandle.standardError.write(Data("kshell: event \(event) \(payload)\n".utf8))
        DispatchQueue.main.async { EventBus.post(String(event), payload: payload) }
    }
}

/// Client side of the socket, used by the `kshell trigger` subcommand.
public enum IPCClient {
    @discardableResult
    public static func send(_ line: String) -> Bool {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return false }
        defer { close(fd) }

        let connected = withSockAddr(path: IPCPaths.socketPath) { connect(fd, $0, $1) } == 0
        guard connected else { return false }

        let data = Array((line + "\n").utf8)
        let written = data.withUnsafeBufferPointer { write(fd, $0.baseAddress, $0.count) }
        return written == data.count
    }

    /// Whether another kshell instance is currently listening on the socket.
    public static func isInstanceRunning() -> Bool {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return false }
        defer { close(fd) }
        return withSockAddr(path: IPCPaths.socketPath) { connect(fd, $0, $1) } == 0
    }
}
