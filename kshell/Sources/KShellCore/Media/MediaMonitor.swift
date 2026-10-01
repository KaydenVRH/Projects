import AppKit
import Foundation

/// One thing that can be controlled on the now-playing backend.
public enum MediaControl: Sendable {
    case playPause, next, previous
}

/// What a media backend is currently playing.
public struct MediaState: Equatable {
    public var source: String = ""          // "termusic" | "spotify" | "music" | ""
    public var title: String = ""
    public var artist: String = ""
    public var album: String = ""
    public var state: String = "stopped"    // playing | paused | stopped
    public var position: Double = 0
    public var duration: Double = 0
    public var artworkPath: String?

    public var isPlaying: Bool { state == "playing" }
    public var hasTrack: Bool { !title.isEmpty }

    /// 0…1 through the current track, when the backend reports a duration.
    public var progress: Double {
        guard duration > 0 else { return 0 }
        return min(max(position / duration, 0), 1)
    }

    public init() {}

    public init(source: String, title: String, artist: String, album: String,
                state: String, position: Double, duration: Double, artworkPath: String?) {
        self.source = source
        self.title = title
        self.artist = artist
        self.album = album
        self.state = state
        self.position = position
        self.duration = duration
        self.artworkPath = artworkPath
    }
}

/// Polls whichever media backend is active and hands state to observers.
///
/// termusic is checked first: it plays through its own Rust backend and never
/// registers with macOS' now-playing, so `nowplaying-cli` cannot see it. When
/// termusic is idle the macOS now-playing APIs (Spotify, Music, browsers…) are
/// used instead.
public final class MediaMonitor: ObservableObject {
    public static let shared = MediaMonitor()

    /// termusic's helper script (speaks gRPC to its unix socket).
    public var termusicScript: String = "~/programs/projects/kshell/scripts/termusic/termusic.sh"

    /// Seconds between polls while the media centre is open.
    public var interval: TimeInterval = 1.0

    @Published public private(set) var state = MediaState()

    private var timer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "kshell.media", qos: .utility)
    private var inFlight = false

    /// Start polling (call when the media centre is shown).
    public func start() {
        if timer == nil {
            let timer = DispatchSource.makeTimerSource(queue: .main)
            timer.schedule(deadline: .now(), repeating: interval)
            timer.setEventHandler { [weak self] in self?.refresh() }
            timer.resume()
            self.timer = timer
        }
        refresh()
    }

    /// Stop polling (call when the media centre is hidden).
    public func stop() {
        timer?.cancel()
        timer = nil
    }

    /// Ask the backend for the current state now.
    public func refresh() {
        // The shell's own player comes first: it is the primary backend now, and
        // its state is right here in the process — no polling needed.
        if let own = MusicPlayer.shared.mediaState() {
            if own != state { state = own }
            return
        }
        guard !inFlight else { return }
        inFlight = true
        let script = termusicScript
        queue.async { [weak self] in
            let next = MediaMonitor.query(script: script)
            DispatchQueue.main.async {
                guard let self else { return }
                self.inFlight = false
                guard next != self.state else { return }
                self.state = next
            }
        }
    }

    /// Send a control command to whichever backend is currently active.
    public func control(_ action: MediaControl) {
        if MusicPlayer.shared.hasTrack {
            switch action {
            case .playPause: MusicPlayer.shared.toggle()
            case .next: MusicPlayer.shared.next()
            case .previous: MusicPlayer.shared.previous()
            }
            return
        }
        let script = termusicScript
        let source = state.source
        queue.async { [weak self] in
            if source == "termusic" || source.isEmpty {
                if MediaMonitor.termusicAvailable(script: script) {
                    let command: String
                    switch action {
                    case .playPause: command = "toggle"
                    case .next: command = "next"
                    case .previous: command = "prev"
                    }
                    _ = Shell.capture("\(Shell.quote(MediaMonitor.scriptPath(script))) \(command) 2>/dev/null")
                }
            } else {
                let command: String
                switch action {
                case .playPause: command = "togglePlayPause"
                case .next: command = "next"
                case .previous: command = "previous"
                }
                _ = Shell.capture("nowplaying-cli \(command) 2>/dev/null")
            }
            // Let the backend apply it before the next poll.
            Thread.sleep(forTimeInterval: 0.25)
            DispatchQueue.main.async { self?.refresh() }
        }
    }

    // MARK: - backends

    private static func scriptPath(_ script: String) -> String {
        (script as NSString).expandingTildeInPath
    }

    private static func termusicAvailable(script: String) -> Bool {
        let path = scriptPath(script)
        guard FileManager.default.isExecutableFile(atPath: path) else { return false }
        // `available` is exit-code based, so look for our own marker.
        return Shell.capture("\(Shell.quote(path)) available >/dev/null 2>&1 && echo up").contains("up")
    }

    private static func query(script: String) -> MediaState {
        let termusic = queryTermusic(script: script)
        // termusic is playing → that is what the user is listening to.
        if let termusic, termusic.isPlaying { return termusic }
        if let system = querySystem(), system.hasTrack { return system }
        if let termusic, termusic.hasTrack { return termusic }
        return MediaState()
    }

    private static func queryTermusic(script: String) -> MediaState? {
        let path = scriptPath(script)
        guard FileManager.default.isExecutableFile(atPath: path) else { return nil }
        let output = Shell.capture("\(Shell.quote(path)) info 2>/dev/null")
        guard !output.isEmpty,
              let object = try? JSONSerialization.jsonObject(with: Data(output.utf8)) as? [String: Any]
        else { return nil }

        var state = MediaState()
        state.source = "termusic"
        state.title = object["title"] as? String ?? ""
        state.artist = object["artist"] as? String ?? ""
        state.album = object["album"] as? String ?? ""
        state.state = object["state"] as? String ?? "stopped"
        state.position = object["position"] as? Double ?? 0
        state.duration = object["duration"] as? Double ?? 0
        if let art = object["artwork"] as? String, !art.isEmpty {
            state.artworkPath = art
        }
        // A radio stream has no useful file tags — use its stream title.
        return state
    }

    private static func querySystem() -> MediaState? {
        guard let nowPlaying = Shell.capture("command -v nowplaying-cli").split(separator: "\n").first,
              !nowPlaying.isEmpty
        else { return nil }

        let output = Shell.capture(
            "nowplaying-cli get --json title artist album duration elapsedTime playbackRate 2>/dev/null"
        )
        guard !output.isEmpty,
              let object = try? (JSONSerialization.jsonObject(with: Data(output.utf8)) as? [String: Any])
        else { return nil }

        func text(_ key: String) -> String {
            if let value = object[key] as? String { return value }
            if let value = object[key] as? NSNumber { return value.stringValue }
            return ""
        }
        func number(_ key: String) -> Double {
            if let value = object[key] as? Double { return value }
            if let value = object[key] as? NSNumber { return value.doubleValue }
            if let value = object[key] as? String { return Double(value) ?? 0 }
            return 0
        }

        var state = MediaState()
        state.source = "system"
        state.title = text("title")
        state.artist = text("artist")
        state.album = text("album")
        state.duration = number("duration")
        state.position = number("elapsedTime")
        state.state = number("playbackRate") > 0 ? "playing" : (state.title.isEmpty ? "stopped" : "paused")
        return state
    }
}
