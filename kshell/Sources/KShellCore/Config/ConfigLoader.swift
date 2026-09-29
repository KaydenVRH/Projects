import Foundation

/// Loads `~/.config/kshell/config.toml` and re-loads it whenever the file (or
/// its directory) changes. Parse failures fall back to the last good config.
public final class ConfigLoader {
    public private(set) var config: ShellConfig
    public let path: URL

    private var source: DispatchSourceFileSystemObject?
    private var directoryFD: Int32 = -1
    private var onChange: ((ShellConfig) -> Void)?

    public static var defaultPath: URL {
        let env = ProcessInfo.processInfo.environment
        let base: URL
        if let xdg = env["XDG_CONFIG_HOME"], !xdg.isEmpty {
            base = URL(fileURLWithPath: xdg, isDirectory: true)
        } else {
            base = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".config", isDirectory: true)
        }
        return base.appendingPathComponent("kshell/config.toml")
    }

    public init(path: URL = ConfigLoader.defaultPath) {
        self.path = path
        self.config = ShellConfig()
        createDirectoryIfNeeded()
        reload(notify: false)
    }

    public func startWatching(_ onChange: @escaping (ShellConfig) -> Void) {
        self.onChange = onChange

        let directory = path.deletingLastPathComponent()
        directoryFD = open(directory.path, O_EVTONLY)
        guard directoryFD >= 0 else {
            FileHandle.standardError.write(Data("kshell: could not watch \(directory.path)\n".utf8))
            return
        }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: directoryFD,
            eventMask: [.write, .rename, .delete, .extend, .attrib],
            queue: .main
        )
        source.setEventHandler { [weak self] in
            self?.reload(notify: true)
        }
        source.setCancelHandler { [weak self] in
            guard let self, self.directoryFD >= 0 else { return }
            close(self.directoryFD)
            self.directoryFD = -1
        }
        source.resume()
        self.source = source
    }

    public func stopWatching() {
        source?.cancel()
        source = nil
    }

    /// Re-read the config now and notify observers.
    public func reloadNow() {
        reload(notify: true)
    }

    private func reload(notify: Bool) {
        guard FileManager.default.fileExists(atPath: path.path) else {
            if notify { onChange?(config) }
            return
        }
        do {
            let text = try String(contentsOf: path, encoding: .utf8)
            let parsed = try ShellConfig.parse(toml: text)
            config = parsed
            if notify { onChange?(config) }
        } catch {
            FileHandle.standardError.write(Data("kshell: config error: \(error)\n".utf8))
        }
    }

    private func createDirectoryIfNeeded() {
        let directory = path.deletingLastPathComponent()
        try? FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
    }
}
