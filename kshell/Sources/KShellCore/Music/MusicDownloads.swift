import Foundation

/// One queued download and how far it has got.
public struct MusicDownloadJob: Identifiable, Equatable {
    public enum State: Equatable {
        case queued
        case downloading
        case done(String)
        case failed(String)
    }

    public var id: String
    public var title: String
    public var progress: Double
    public var state: State

    public var isActive: Bool {
        switch state {
        case .queued, .downloading: return true
        case .done, .failed: return false
        }
    }
}

/// Searches and downloads through `yt-dlp`.
///
/// A query runs through YouTube's search; anything that looks like a URL is
/// fetched directly. Downloads stream their progress so the panel can show it.
public final class MusicDownloads: ObservableObject {
    public static let shared = MusicDownloads()

    @Published public private(set) var results: [MusicSearchResult] = []
    @Published public private(set) var searching = false
    @Published public private(set) var jobs: [MusicDownloadJob] = []
    @Published public private(set) var lastQuery: String = ""

    /// Where finished files land.
    public var directory: URL = KShellPaths.resolve("~/Music/kshell")
    public var searchCount = 15
    public var audioFormat = "m4a"
    public var ytdlp = "yt-dlp"

    /// Called after a download finishes so the library can rescan.
    public var onDownloadFinished: (() -> Void)?

    public init() {}

    // MARK: - search

    public func search(_ query: String) {
        let text = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else {
            results = []
            return
        }
        lastQuery = text
        searching = true

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let isURL = text.lowercased().hasPrefix("http")
            let target = isURL ? text : "ytsearch\(self.searchCount):\(text)"
            let json = ProcessRunner.capture(
                "\(self.ytdlp) \(self.quote(target)) --flat-playlist --dump-json "
                + "--no-warnings --no-call-home 2>/dev/null"
            )
            var parsed: [MusicSearchResult] = []
            for line in json.split(separator: "\n") {
                guard let data = line.data(using: .utf8),
                      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
                else { continue }
                let id = (object["id"] as? String) ?? ""
                let title = (object["title"] as? String) ?? ""
                guard !id.isEmpty, !title.isEmpty else { continue }
                let channel = (object["uploader"] as? String)
                    ?? (object["channel"] as? String)
                    ?? (object["artist"] as? String)
                    ?? ""
                let duration = (object["duration"] as? Double) ?? 0
                parsed.append(MusicSearchResult(
                    id: id,
                    title: title,
                    channel: channel,
                    duration: duration,
                    thumbnailURL: "https://i.ytimg.com/vi/\(id)/mqdefault.jpg"
                ))
            }
            DispatchQueue.main.async {
                self.results = parsed
                self.searching = false
            }
        }
    }

    public func clearResults() {
        results = []
        lastQuery = ""
    }

    // MARK: - downloads

    public func download(_ result: MusicSearchResult) {
        guard !jobs.contains(where: { $0.id == result.id && $0.isActive }) else { return }
        jobs.append(MusicDownloadJob(id: result.id, title: result.title, progress: 0, state: .queued))

        let folder = directory.path
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let template = "\(folder)/%(title)s [%(id)s].%(ext)s"
        let command = "\(ytdlp) -x --audio-format \(audioFormat) --embed-thumbnail --add-metadata "
            + "--no-playlist --newline --no-warnings --no-call-home "
            + "-o \(quote(template)) \(quote(result.webURL))"

        stream(command) { [weak self] line in
            guard let self else { return }
            if let percent = MusicDownloads.percent(in: line) {
                self.update(result.id) { job in
                    job.progress = percent
                    job.state = .downloading
                }
            }
        } completion: { [weak self] status in
            guard let self else { return }
            if status == 0 {
                self.update(result.id) { job in
                    job.progress = 1
                    job.state = .done(result.title)
                }
                self.onDownloadFinished?()
            } else {
                self.update(result.id) { job in
                    job.state = .failed("yt-dlp exited \(status)")
                }
            }
        }
    }

    public func job(for result: MusicSearchResult) -> MusicDownloadJob? {
        jobs.last { $0.id == result.id }
    }

    public func clearFinished() {
        jobs.removeAll { !$0.isActive }
    }

    private func update(_ id: String, _ change: @escaping (inout MusicDownloadJob) -> Void) {
        DispatchQueue.main.async {
            guard let at = self.jobs.lastIndex(where: { $0.id == id }) else { return }
            change(&self.jobs[at])
        }
    }

    // MARK: - process plumbing

    /// Run a command, handing each output line to `line` (on a background queue).
    private func stream(
        _ command: String,
        line: @escaping (String) -> Void,
        completion: @escaping (Int32) -> Void
    ) {
        DispatchQueue.global(qos: .utility).async {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/bin/zsh")
            process.arguments = ["-lc", command]
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = pipe
            do {
                try process.run()
            } catch {
                completion(-1)
                return
            }
            let handle = pipe.fileHandleForReading
            var buffer = Data()
            while true {
                let chunk = handle.availableData
                if chunk.isEmpty { break }
                buffer.append(chunk)
                while let newline = buffer.firstIndex(of: 0x0A) {
                    let text = String(data: buffer.subdata(in: 0..<newline), encoding: .utf8)
                    buffer.removeSubrange(0...newline)
                    if let text { line(text) }
                }
            }
            process.waitUntilExit()
            completion(process.terminationStatus)
        }
    }

    static func percent(in line: String) -> Double? {
        guard let range = line.range(of: "[download]") else { return nil }
        let rest = line[range.upperBound...]
        guard let percentSign = rest.firstIndex(of: "%") else { return nil }
        let number = rest[..<percentSign].trimmingCharacters(in: .whitespaces)
        guard let value = Double(number) else { return nil }
        return min(max(value / 100, 0), 1)
    }

    private func quote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}
