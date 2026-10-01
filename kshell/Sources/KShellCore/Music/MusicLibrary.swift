import Foundation

/// One track in the music library.
public struct MusicTrack: Identifiable, Equatable, Codable, Sendable {
    public var id: String { path }
    public var path: String
    public var title: String
    public var artist: String
    public var album: String
    public var duration: Double
    public var artworkPath: String?

    public init(
        path: String,
        title: String,
        artist: String = "",
        album: String = "",
        duration: Double = 0,
        artworkPath: String? = nil
    ) {
        self.path = path
        self.title = title
        self.artist = artist
        self.album = album
        self.duration = duration
        self.artworkPath = artworkPath
    }

    public var url: URL { URL(fileURLWithPath: path) }

    public var displayTitle: String { title.isEmpty ? url.deletingPathExtension().lastPathComponent : title }
    public var displayArtist: String { artist.isEmpty ? "Unknown artist" : artist }

    /// `m:ss`, or `h:mm:ss` for anything long.
    public static func clock(_ seconds: Double) -> String {
        guard seconds.isFinite, seconds > 0 else { return "0:00" }
        let total = Int(seconds.rounded())
        let hours = total / 3600
        let minutes = (total % 3600) / 60
        let secs = total % 60
        return hours > 0
            ? String(format: "%d:%02d:%02d", hours, minutes, secs)
            : String(format: "%d:%02d", minutes, secs)
    }
}

/// A search result from the download service, before it is downloaded.
public struct MusicSearchResult: Identifiable, Equatable, Sendable {
    public var id: String
    public var title: String
    public var channel: String
    public var duration: Double
    public var thumbnailURL: String?

    public init(id: String, title: String, channel: String, duration: Double, thumbnailURL: String?) {
        self.id = id
        self.title = title
        self.channel = channel
        self.duration = duration
        self.thumbnailURL = thumbnailURL
    }

    public var webURL: String { "https://www.youtube.com/watch?v=\(id)" }
}

/// The local library: a folder of audio files.
///
/// Metadata comes from `ffprobe`, cached in a JSON file beside kshell's other
/// caches — a library of a few hundred tracks would otherwise re-probe every
/// file on every scan. Artwork is extracted once into the same cache, because
/// the downloader embeds it in the file rather than leaving it alongside.
public final class MusicLibrary: ObservableObject {
    @Published public private(set) var tracks: [MusicTrack] = []
    @Published public private(set) var isScanning = false
    @Published public private(set) var lastError: String?

    /// Where downloads land. Settable from config.
    public var directory: URL

    private let queue = DispatchQueue(label: "kshell.music.library", qos: .utility)
    private var cache: [String: MusicTrack] = [:]
    private var cacheLoaded = false

    static let audioExtensions: Set<String> = ["mp3", "m4a", "aac", "flac", "wav", "aiff", "aif"]

    public init(directory: URL) {
        self.directory = directory
    }

    public func setDirectory(_ url: URL) {
        guard url != directory else { return }
        directory = url
        refresh()
    }

    /// Rescan the folder on a background queue, using and updating the cache.
    public func refresh() {
        guard !isScanning else { return }
        isScanning = true
        let folder = directory
        queue.async { [weak self] in
            guard let self else { return }
            self.loadCacheIfNeeded()
            let found = self.scan(folder)
            DispatchQueue.main.async {
                self.tracks = found
                self.isScanning = false
                self.lastError = nil
            }
        }
    }

    /// Delete a track's file (and its cached artwork) and drop it from the list.
    public func remove(_ track: MusicTrack) {
        try? FileManager.default.removeItem(at: track.url)
        if let artwork = track.artworkPath {
            try? FileManager.default.removeItem(atPath: artwork)
        }
        cache[track.path] = nil
        saveCache()
        tracks.removeAll { $0.path == track.path }
    }

    // MARK: - scanning

    private func scan(_ folder: URL) -> [MusicTrack] {
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        // Recursive: a library of album folders should work, and the folder can
        // be pointed at an existing collection as well as the downloads folder.
        let contents = (FileManager.default.enumerator(
            at: folder,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles, .skipsPackageDescendants]
        )?.allObjects as? [URL]) ?? []

        var result: [MusicTrack] = []
        for url in contents.sorted(by: { $0.path < $1.path }) {
            let ext = url.pathExtension.lowercased()
            guard MusicLibrary.audioExtensions.contains(ext) else { continue }
            let path = url.path
            if let cached = cache[path], FileManager.default.fileExists(atPath: path) {
                result.append(cached)
                continue
            }
            var track = MusicLibrary.probe(path: path)
            track.artworkPath = MusicLibrary.extractArtwork(path: path)
            cache[path] = track
            result.append(track)
        }
        // Drop cache entries for files that are gone.
        let live = Set(result.map(\.path))
        cache = cache.filter { live.contains($0.key) }
        saveCache()
        return result
    }

    /// Tags via ffprobe. Falls back to the file name.
    static func probe(path: String) -> MusicTrack {
        let fallback = MusicTrack(
            path: path,
            title: URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
        )
        let json = ProcessRunner.capture("ffprobe -v quiet -print_format json -show_format \(shellQuote(path))")
        guard let data = json.data(using: .utf8),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let format = root["format"] as? [String: Any]
        else { return fallback }

        let tags = (format["tags"] as? [String: Any]) ?? [:]
        func tag(_ names: String...) -> String {
            for name in names {
                for (key, value) in tags where key.lowercased() == name.lowercased() {
                    if let text = value as? String, !text.isEmpty { return text }
                }
            }
            return ""
        }
        var title = tag("title")
        let artist = tag("artist", "album_artist", "uploader", "creator")
        let album = tag("album")
        if title.isEmpty {
            // `Artist - Title` is the usual shape for downloaded singles.
            title = fallback.title
            if artist.isEmpty, let dash = title.range(of: " - ") {
                title = String(title[dash.upperBound...])
            }
        }
        let duration = Double((format["duration"] as? String) ?? "") ?? 0
        return MusicTrack(
            path: path,
            title: title,
            artist: artist,
            album: album,
            duration: duration
        )
    }

    /// Pull the embedded cover out into the cache once.
    static func extractArtwork(path: String) -> String? {
        let hash = abs(path.hashValue)
        let target = KShellPaths.cacheDirectory.appendingPathComponent("artwork-\(hash).jpg")
        if FileManager.default.fileExists(atPath: target.path) { return target.path }

        // Transcoded rather than copied: the embedded cover is often a PNG, and
        // writing that into a `.jpg` is a lie that some loaders punish.
        _ = ProcessRunner.capture(
            "ffmpeg -v quiet -y -i \(shellQuote(path)) -an -vcodec mjpeg -q:v 3 "
            + "\(shellQuote(target.path)) 2>/dev/null"
        )
        return FileManager.default.fileExists(atPath: target.path) ? target.path : nil
    }

    static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    // MARK: - cache

    private var cacheURL: URL {
        KShellPaths.cacheDirectory.appendingPathComponent("library.json")
    }

    private func loadCacheIfNeeded() {
        guard !cacheLoaded else { return }
        cacheLoaded = true
        guard let data = try? Data(contentsOf: cacheURL),
              let decoded = try? JSONDecoder().decode([MusicTrack].self, from: data)
        else { return }
        cache = Dictionary(uniqueKeysWithValues: decoded.map { ($0.path, $0) })
    }

    private func saveCache() {
        guard let data = try? JSONEncoder().encode(Array(cache.values)) else { return }
        try? data.write(to: cacheURL)
    }
}
