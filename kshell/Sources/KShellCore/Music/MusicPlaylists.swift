import Foundation

/// A saved, named collection of tracks.
///
/// The queue is what plays next; a playlist is something you start from any
/// time, and it survives the queue being cleared or replaced.
public struct Playlist: Identifiable, Equatable, Codable, Sendable {
    public var id: String { name }
    public var name: String
    public var tracks: [MusicTrack]

    public init(name: String, tracks: [MusicTrack] = []) {
        self.name = name
        self.tracks = tracks
    }

    public var count: Int { tracks.count }

    public var subtitle: String {
        let songs = count == 1 ? "1 track" : "\(count) tracks"
        guard let first = tracks.first else { return songs }
        return "\(songs) · \(first.displayArtist)"
    }
}

/// The user's playlists, persisted next to the queue.
public final class MusicPlaylists: ObservableObject {
    @Published public private(set) var playlists: [Playlist] = []

    private let file: URL
    private let lock = NSLock()

    public init(file: URL? = nil) {
        self.file = file ?? KShellPaths.cacheDirectory.appendingPathComponent("playlists.json")
        load()
    }

    public var isEmpty: Bool { playlists.isEmpty }
    public var count: Int { playlists.count }

    public func playlist(named name: String) -> Playlist? {
        playlists.first { $0.name == name }
    }

    public func contains(_ name: String) -> Bool {
        playlists.contains { $0.name == name }
    }

    /// Save `tracks` under `name`. An existing playlist with the same name is
    /// replaced — saving the queue twice under one name means "update it".
    @discardableResult
    public func save(name: String, tracks: [MusicTrack]) -> Playlist? {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let playlist = Playlist(name: trimmed, tracks: tracks)
        lock.lock()
        if let at = playlists.firstIndex(where: { $0.name == trimmed }) {
            playlists[at] = playlist
        } else {
            playlists.append(playlist)
        }
        playlists.sort { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
        lock.unlock()
        save()
        return playlist
    }

    public func remove(named name: String) {
        lock.lock()
        playlists.removeAll { $0.name == name }
        lock.unlock()
        save()
    }

    /// Add a track to a playlist, ignoring one it already has.
    @discardableResult
    public func add(_ track: MusicTrack, to name: String) -> Bool {
        lock.lock()
        guard let at = playlists.firstIndex(where: { $0.name == name }) else {
            lock.unlock()
            return false
        }
        let exists = playlists[at].tracks.contains { $0.path == track.path }
        if !exists { playlists[at].tracks.append(track) }
        lock.unlock()
        if !exists { save() }
        return !exists
    }

    @discardableResult
    public func remove(_ track: MusicTrack, from name: String) -> Bool {
        lock.lock()
        guard let at = playlists.firstIndex(where: { $0.name == name }) else {
            lock.unlock()
            return false
        }
        let before = playlists[at].tracks.count
        playlists[at].tracks.removeAll { $0.path == track.path }
        let changed = playlists[at].tracks.count != before
        lock.unlock()
        if changed { save() }
        return changed
    }

    /// A name that does not collide yet: `Playlist`, `Playlist 2`, …
    public func uniqueName(base: String = "Playlist") -> String {
        guard contains(base) else { return base }
        var n = 2
        while contains("\(base) \(n)") { n += 1 }
        return "\(base) \(n)"
    }

    // MARK: - persistence

    private func save() {
        lock.lock()
        let snapshot = playlists
        lock.unlock()
        do {
            try FileManager.default.createDirectory(
                at: file.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            try encoder.encode(snapshot).write(to: file, options: .atomic)
        } catch {
            // A playlist that cannot be written still works for this session.
        }
    }

    private func load() {
        guard let data = try? Data(contentsOf: file),
              let stored = try? JSONDecoder().decode([Playlist].self, from: data)
        else { return }
        playlists = stored
    }
}
