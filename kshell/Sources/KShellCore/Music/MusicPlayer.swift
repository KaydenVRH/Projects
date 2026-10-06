import AVFoundation
import AppKit
import Foundation
import MediaPlayer

/// Plays the library from inside kshell, through AVFoundation.
///
/// This is what replaces termusic: music plays in this process, so kshell knows
/// exactly what is playing — the media centre, the bar's widget and this panel
/// all read the same state, with real position, seeking and volume.
///
/// Playback goes through an `AVAudioEngine` rather than an `AVAudioPlayer`
/// because the engine's mixer can be tapped: that copy of the mix is what feeds
/// the desktop visualiser, with no permissions and no capture device.
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
        didSet { node.volume = Float(min(max(volume, 0), 1)) }
    }

    /// A short history, so "previous" can step back through what was played.
    @Published public private(set) var history: [MusicTrack] = []

    /// Live bands for the desktop visualiser.
    public let spectrum = AudioSpectrum()
    /// Whether anyone is watching the spectrum. With the visualiser switched off
    /// the analyser stays idle rather than FFT-ing into the void.
    public var spectrumWanted = true

    private let engine = AVAudioEngine()
    private let node = AVAudioPlayerNode()
    private var file: AVAudioFile?
    /// Frame the current segment started at, for position and seeking.
    private var seekFrame: AVAudioFramePosition = 0
    /// Bumped on every (re)schedule, so a `stop()`'s completion callback — which
    /// fires immediately and unconditionally — can't look like a track ending.
    private var scheduleToken = 0
    private var engineReady = false
    /// The format the node is currently connected with, so the graph is only
    /// rebuilt when it actually has to be.
    private var connectedFormat: AVAudioFormat?
    /// How many tracks in a row have failed to start; bounds the skip loop.
    private var consecutiveFailures = 0
    private var ticker: DispatchSourceTimer?
    private var index = 0
    private var artworkCache: [String: String] = [:]

    public var hasTrack: Bool { current != nil }

    /// Supplies the library when a transport command arrives with nothing
    /// loaded — so a media key from cold starts the music rather than doing
    /// nothing. Set by the controller, which owns the library.
    public var libraryProvider: (() -> [MusicTrack])?

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
        engine.attach(node)
        registerRemoteCommands()
        NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange,
            object: engine,
            queue: .main
        ) { [weak self] _ in
            self?.engineConfigurationChanged()
        }
    }

    /// Point the player node at a file and make sure the engine is running.
    ///
    /// The graph is only rebuilt when the file's format actually changes — a
    /// track with a different sample rate or channel count — because tearing it
    /// down on every track is what left the engine momentarily stopped, and
    /// `AVAudioPlayerNode.play()` *raises* (rather than throws) if the engine
    /// isn't running, which used to abort the whole shell.
    private func startEngine(with file: AVAudioFile) throws {
        let format = file.processingFormat
        if connectedFormat != format {
            if engine.isRunning { engine.pause() }
            engine.disconnectNodeOutput(node)
            engine.connect(node, to: engine.mainMixerNode, format: format)
            connectedFormat = format
            engine.prepare()
        }
        if !engine.isRunning { try engine.start() }
        guard !engineReady else { return }
        engineReady = true
        // One tap for the life of the process; it survives stop/start.
        engine.mainMixerNode.installTap(onBus: 0, bufferSize: 512, format: nil) { [weak self] buffer, _ in
            self?.spectrum.process(buffer)
        }
    }

    /// A device change (headphones, a display with speakers) invalidates the
    /// running graph. Rebuild it around whatever is playing rather than letting
    /// the next `play()` raise.
    private func engineConfigurationChanged() {
        connectedFormat = nil
        engineReady = false
        guard isPlaying, let file else { return }
        do {
            try startEngine(with: file)
            schedule(from: seekFrame)
            node.play()
        } catch {
            // Leave it stopped; the next transport press will try again.
            isPlaying = false
            spectrum.isEnabled = false
            writeSidecar()
        }
    }

    /// Register with the system's now-playing centre: this is what makes the
    /// media keys, Control Centre and headphone controls drive kshell.
    private func registerRemoteCommands() {
        let centre = MPRemoteCommandCenter.shared()
        centre.playCommand.addTarget { [weak self] _ in
            self?.resume()
            return .success
        }
        centre.pauseCommand.addTarget { [weak self] _ in
            self?.pause()
            return .success
        }
        centre.togglePlayPauseCommand.addTarget { [weak self] _ in
            self?.toggle()
            return .success
        }
        centre.nextTrackCommand.addTarget { [weak self] _ in
            self?.next()
            return .success
        }
        centre.previousTrackCommand.addTarget { [weak self] _ in
            self?.previous()
            return .success
        }
        centre.changePlaybackPositionCommand.addTarget { [weak self] event in
            guard let event = event as? MPChangePlaybackPositionCommandEvent else {
                return .commandFailed
            }
            self?.seek(toSeconds: event.positionTime)
            return .success
        }
    }

    /// Publish to the system, so the media keys and the Control Centre panel
    /// follow this player rather than whatever was playing before.
    private func publishNowPlaying() {
        let centre = MPNowPlayingInfoCenter.default()
        guard let current else {
            centre.nowPlayingInfo = nil
            centre.playbackState = .stopped
            return
        }
        var info: [String: Any] = [
            MPMediaItemPropertyTitle: current.displayTitle,
            MPMediaItemPropertyArtist: current.displayArtist,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: position,
            MPNowPlayingInfoPropertyPlaybackRate: isPlaying ? 1.0 : 0.0,
        ]
        if !current.album.isEmpty { info[MPMediaItemPropertyAlbumTitle] = current.album }
        if duration > 0 { info[MPMediaItemPropertyPlaybackDuration] = duration }
        if let path = current.artworkPath, let image = NSImage(contentsOfFile: path) {
            info[MPMediaItemPropertyArtwork] = MPMediaItemArtwork(boundsSize: image.size) { _ in image }
        }
        centre.nowPlayingInfo = info
        centre.playbackState = isPlaying ? .playing : .paused
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
        consecutiveFailures = 0
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
            let file = try AVAudioFile(forReading: track.url)
            try startEngine(with: file)
            // `play()` raises — not throws — when the engine isn't running or the
            // node has come unattached, so it is only called once both are true.
            guard engine.isRunning, node.engine === engine else { throw PlayerError.engineStopped }
            self.file = file
            node.volume = Float(min(max(volume, 0), 1))
            schedule(from: 0)
            node.play()
            let rate = file.processingFormat.sampleRate
            duration = rate > 0 && file.length > 0 ? Double(file.length) / rate : track.duration
        } catch {
            // An unplayable file (a codec AVFoundation does not handle, or an
            // audio device that has gone away) should not wedge the queue. Skip
            // to the next one — but give up after a full lap, rather than
            // recursing round a queue where nothing plays.
            consecutiveFailures += 1
            if consecutiveFailures >= max(queue.count, 1) {
                consecutiveFailures = 0
                stop()
                return
            }
            current = nil
            isPlaying = false
            spectrum.isEnabled = false
            writeSidecar()
            advance(step: 1, skippingUnplayable: true)
            return
        }
        consecutiveFailures = 0
        if let previous = current, previous != track { history.append(previous) }
        current = track
        isPlaying = true
        position = 0
        spectrum.isEnabled = spectrumWanted
        startTicker()
        writeSidecar()
    }

    enum PlayerError: Error {
        /// The engine refused to start (usually a device that has gone away).
        case engineStopped
    }

    /// Stop the node and queue `file` from `frame` onwards.
    ///
    /// `node.stop()` fires the previous segment's completion handler, so the
    /// token check in the handler is what keeps that from advancing the queue.
    private func schedule(from frame: AVAudioFramePosition) {
        guard let file else { return }
        scheduleToken += 1
        let token = scheduleToken
        node.stop()
        seekFrame = frame
        let remaining = AVAudioFrameCount(max(0, file.length - frame))
        guard remaining > 0 else { return }
        node.scheduleSegment(
            file,
            startingFrame: frame,
            frameCount: remaining,
            at: nil,
            completionCallbackType: .dataPlayedBack
        ) { [weak self] _ in
            DispatchQueue.main.async {
                guard let self, self.scheduleToken == token else { return }
                self.advance(step: 1)
            }
        }
    }

    public func toggle() {
        isPlaying ? pause() : resume()
    }

    public func resume() {
        guard file != nil else {
            // Nothing loaded: start the library.
            if let tracks = libraryProvider?(), !tracks.isEmpty { play(tracks) }
            return
        }
        node.play()
        isPlaying = true
        spectrum.isEnabled = spectrumWanted
        startTicker()
        writeSidecar()
    }

    public func pause() {
        node.pause()
        isPlaying = false
        ticker?.cancel()
        ticker = nil
        // Leave the analyser running for a moment: the engine is still pouring
        // silence into the tap, so the visualiser's bars drain instead of
        // freezing mid-air behind the fade.
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            guard let self, !self.isPlaying else { return }
            self.spectrum.isEnabled = false
        }
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

    public func seek(toSeconds seconds: Double) {
        guard duration > 0 else { return }
        seek(toFraction: seconds / duration)
    }

    public func seek(toFraction fraction: Double) {
        guard let file else { return }
        let clamped = min(max(fraction, 0), 1)
        let frame = AVAudioFramePosition((Double(file.length) * clamped).rounded())
        let wasPlaying = isPlaying
        schedule(from: frame)
        if wasPlaying { node.play() }
        position = Double(frame) / file.processingFormat.sampleRate
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
        scheduleToken += 1
        consecutiveFailures = 0
        node.stop()
        file = nil
        ticker?.cancel()
        ticker = nil
        current = nil
        isPlaying = false
        position = 0
        duration = 0
        spectrum.isEnabled = false
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
            guard let self, let file = self.file else { return }
            self.position = self.currentPosition(of: file)
            self.writeSidecar()
        }
        timer.resume()
        ticker = timer
    }

    /// Where playback is now: the frame the segment started at, plus how far the
    /// node has rendered into it.
    private func currentPosition(of file: AVAudioFile) -> Double {
        let rate = file.processingFormat.sampleRate
        guard let nodeTime = node.lastRenderTime, let playerTime = node.playerTime(forNodeTime: nodeTime) else {
            return Double(seekFrame) / rate
        }
        let frame = min(seekFrame + playerTime.sampleTime, file.length)
        return Double(frame) / rate
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
        publishNowPlaying()
    }
}
