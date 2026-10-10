import Foundation

/// The songs put aside to play later.
///
/// Separate from the player's own queue, which is whatever was last handed to
/// `play(_:)`: this one is yours to build up — a mood, a set — and it is kept
/// between runs, so it survives a restart. Playing it hands the list to the
/// player, which then walks it with next/previous as usual.
public final class MusicQueue: ObservableObject {
    @Published public private(set) var tracks: [MusicTrack] = []

    private let file: URL
    private let lock = NSLock()

    public init(file: URL? = nil) {
        self.file = file ?? KShellPaths.cacheDirectory.appendingPathComponent("queue.json")
        load()
    }

    public var isEmpty: Bool { tracks.isEmpty }
    public var count: Int { tracks.count }

    public func contains(_ track: MusicTrack) -> Bool {
        tracks.contains { $0.path == track.path }
    }

    /// Append, unless it is already queued — a mood list rarely wants the same
    /// song twice, and a stray double press should be harmless.
    @discardableResult
    public func add(_ track: MusicTrack) -> Bool {
        guard !contains(track) else { return false }
        tracks.append(track)
        save()
        return true
    }

    public func remove(_ track: MusicTrack) {
        let before = tracks.count
        tracks.removeAll { $0.path == track.path }
        if tracks.count != before { save() }
    }

    public func remove(at index: Int) {
        guard tracks.indices.contains(index) else { return }
        tracks.remove(at: index)
        save()
    }

    /// Slide a song up or down the list, clamping at the ends.
    public func move(_ index: Int, by delta: Int) {
        guard tracks.indices.contains(index) else { return }
        let target = min(max(index + delta, 0), tracks.count - 1)
        guard target != index else { return }
        let track = tracks.remove(at: index)
        tracks.insert(track, at: target)
        save()
    }

    public func clear() {
        guard !tracks.isEmpty else { return }
        tracks.removeAll()
        save()
    }

    /// Replace the queue outright (used when reading the list back in).
    public func replace(_ tracks: [MusicTrack]) {
        self.tracks = tracks
        save()
    }

    // MARK: - keeping it

    private func save() {
        lock.lock()
        defer { lock.unlock() }
        do {
            try FileManager.default.createDirectory(
                at: file.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data = try JSONEncoder().encode(tracks)
            try data.write(to: file, options: .atomic)
        } catch {
            // A queue that cannot be written is still usable in memory.
        }
    }

    private func load() {
        guard let data = try? Data(contentsOf: file),
              let stored = try? JSONDecoder().decode([MusicTrack].self, from: data) else { return }
        // Drop anything that has been deleted since, so the list never points at
        // a file that is no longer there.
        tracks = stored.filter { FileManager.default.fileExists(atPath: $0.path) }
    }
}
