import Foundation

/// Watches a single file (via its parent directory, so replace-on-save works)
/// and fires `onChange` when it is modified.
public final class FileWatcher {
    private var source: DispatchSourceFileSystemObject?
    private var directoryFD: Int32 = -1
    private let onChange: () -> Void

    public init?(path: URL, onChange: @escaping () -> Void) {
        self.onChange = onChange
        let directory = path.deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: directory.path) else { return nil }

        directoryFD = open(directory.path, O_EVTONLY)
        guard directoryFD >= 0 else { return nil }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: directoryFD,
            eventMask: [.write, .rename, .delete, .extend, .attrib],
            queue: .main
        )
        source.setEventHandler { [weak self] in self?.onChange() }
        source.setCancelHandler { [weak self] in
            guard let self, self.directoryFD >= 0 else { return }
            close(self.directoryFD)
            self.directoryFD = -1
        }
        source.resume()
        self.source = source
    }

    public func stop() {
        source?.cancel()
        source = nil
    }

    deinit { stop() }
}
