import AVFoundation
import Foundation

/// Plays the library from inside kshell, through AVFoundation.
///
/// This is what replaces termusic: music plays in this process, so kshell knows
/// exactly what is playing — the media centre, the bar's widget and this panel
/// all read the same state, with real position, seeking and volume.
///
/// State is also mirrored into a small JSON file so the bar's shell-script
/// widgets (separate processes) can show it.
public final class MusicPlayer: NSObject, ObservableObject {
    public static let shared = MusicPlayer()

    @Published public private(set) var current: MusicTrack?
    @Published public private(set) var queue: [MusicTrack] = []
    @Published public private(set) var isPlaying = false
    @Published public private(set) var position: Double = 0
    @Published public private(set) var duration: Double = 0

    @Published public var volume: Double = 0.8 {
        didSet { player?.volume = Float(min(max(volume, 0), 1)) }
    }

    /// A short history, so "previous" can step back through what was played.
    @Published public private(set) var history: [MusicTrack] = []

    private var player: AVAudioPlayer?
    private var ticker: DispatchSourceTimer?
    private var index = 0
    private var artworkCache: [String: String] = [:]

    public var hasTrack: Bool { current != nil }

    public var progress: Double {
        guard duration > 0 else { return 0 }
        return min(max(position / duration, 0), 1)
    }

    /// What the media monitor should show, or nil when nothing is loaded.
    public func mediaState() -> MediaState? {
        guard let current else { return nil }
        return MediaState(
            source: "kshell",
            title: current.displayTitle,
            artist: current.displayArtist,
            album: current.album,
            state: isPlaying ? "playing" : "paused",
            position: position,
            duration: duration,
            artworkPath: current.artworkPath
        )
    }

    private override init() {
        super.init()
    }

    // MARK: - playing

    /// Play a single track, replacing the queue.
    public func play(_ track: MusicTrack) {
        begin(track, queue: [track], index: 0)
    }

    /// Play a list, starting at `start`.
    public func play(_ tracks: [MusicTrack], startAt start: Int = 0) {
        guard !tracks.isEmpty else { return }
        begin(tracks[min(max(start, 0), tracks.count - 1)], queue: tracks, index: start)
    }

    private func begin(_ track: MusicTrack, queue tracks: [MusicTrack], index start: Int) {
        queue = tracks
        index = min(max(start, 0), max(tracks.count - 1, 0))
        history.removeAll()
        beginPlayback(track)
    }

    /// Append to the end of the current queue (used by the library's "play all"
    /// and by downloads finishing).
    public func enqueue(_ track: MusicTrack) {
        queue.append(track)
        if current == nil { beginPlayback(track) }
    }

    private func beginPlayback(_ track: MusicTrack) {
        ticker?.cancel()
        ticker = nil
        do {
            let player = try AVAudioPlayer(contentsOf: track.url)
            player.delegate = self
            player.volume = Float(min(max(volume, 0), 1))
            player.prepareToPlay()
            player.play()
            self.player = player
        } catch {
            // An unplayable file (a codec AVFoundation does not handle) should not
            // wedge the queue: skip to the next one.
            current = nil
            isPlaying = false
            writeSidecar()
            advance(step: 1, skippingUnplayable: true)
            return
        }
        if let previous = current, previous != track { history.append(previous) }
        current = track
        isPlaying = true
        position = 0
        duration = player?.duration ?? track.duration
        startTicker()
        writeSidecar()
    }

    public func toggle() {
        isPlaying ? pause() : resume()
    }

    public func resume() {
        guard let player else { return }
        player.play()
        isPlaying = true
        startTicker()
        writeSidecar()
    }

    public func pause() {
        player?.pause()
        isPlaying = false
        ticker?.cancel()
        ticker = nil
        writeSidecar()
    }

    public func next() {
        advance(step: 1)
    }

    public func previous() {
        // Typical player behaviour: restart the track first, step back on a
        // second press.
        if position > 3 {
            seek(toFraction: 0)
            return
        }
        if let last = history.popLast() {
            if let existing = queue.firstIndex(of: last) { index = existing }
            beginPlayback(last)
            return
        }
        advance(step: -1)
    }

    public func seek(toFraction fraction: Double) {
        guard let player else { return }
        let clamped = min(max(fraction, 0), 1)
        player.currentTime = player.duration * clamped
        position = player.currentTime
        writeSidecar()
    }

    public func remove(_ track: MusicTrack) {
        guard let at = queue.firstIndex(of: track) else { return }
        queue.remove(at: at)
        if track == current {
            if queue.isEmpty {
                stop()
            } else {
                index = min(at, queue.count - 1)
                beginPlayback(queue[index])
            }
        } else if at < index {
            index -= 1
        }
    }

    public func stop() {
        player?.stop()
        player = nil
        ticker?.cancel()
        ticker = nil
        current = nil
        isPlaying = false
        position = 0
        duration = 0
        writeSidecar()
    }

    private func advance(step: Int, skippingUnplayable: Bool = false) {
        guard !queue.isEmpty else {
            stop()
            return
        }
        let next = index + step
        if next >= queue.count {
            // Wrap round rather than stopping: a music player that runs out of
            // queue mid-session is more annoying than one that loops.
            index = 0
        } else if next < 0 {
            index = queue.count - 1
        } else {
            index = next
        }
        beginPlayback(queue[index])
    }

    // MARK: - position

    private func startTicker() {
        guard ticker == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 0.5)
        timer.setEventHandler { [weak self] in
            guard let self, let player = self.player else { return }
            self.position = player.currentTime
            if self.duration <= 0 { self.duration = player.duration }
            // Keep the sidecar (and through it the bar's script widget) in step,
            // and notice if playback stopped underneath us.
            if self.isPlaying != player.isPlaying { self.isPlaying = player.isPlaying }
            self.writeSidecar()
        }
        timer.resume()
        ticker = timer
    }

    // MARK: - sidecar

    /// Mirrors the state to `now-playing.json` for the bar's script widgets.
    private func writeSidecar() {
        let payload: [String: Any] = [
            "source": "kshell",
            "title": current?.displayTitle ?? "",
            "artist": current?.displayArtist ?? "",
            "album": current?.album ?? "",
            "state": isPlaying ? "playing" : (current == nil ? "stopped" : "paused"),
            "position": position,
            "duration": duration,
            "artwork": current?.artworkPath ?? "",
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload) else { return }
        let url = KShellPaths.cacheDirectory.appendingPathComponent("now-playing.json")
        try? FileManager.default.createDirectory(
            at: KShellPaths.cacheDirectory,
            withIntermediateDirectories: true
        )
        try? data.write(to: url)
    }
}

extension MusicPlayer: AVAudioPlayerDelegate {
    public func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        advance(step: 1)
    }
}
