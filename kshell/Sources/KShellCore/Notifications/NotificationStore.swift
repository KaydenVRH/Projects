import Combine
import Foundation

/// Everything the shell has been told recently, newest first.
///
/// Both sources land here: the system database watcher replays what macOS
/// delivered, the `notify` event adds what scripts posted. The list is the
/// notification centre's content and the popup's queue is derived from it, so
/// dismissing a card in either place is the same operation.
public final class NotificationStore: ObservableObject {
    public static let shared = NotificationStore()

    /// Newest first, capped at `limit`.
    @Published public private(set) var items: [KShellNotification] = []
    /// How many have arrived since the centre was last opened.
    @Published public private(set) var unread = 0

    /// Called for each notification that should pop a card, oldest first.
    var onArrive: ((KShellNotification) -> Void)?
    /// How many to keep. Read from `[notifications] history`.
    var limit: Int = 200 {
        didSet {
            guard limit != oldValue else { return }
            trim()
            save()
        }
    }

    /// Ids the user has acted on. Kept across restarts so a dismissed card is
    /// not resurrected by the next pass over the system database, which still
    /// holds it.
    private var dismissed: [String: Date] = [:]
    private static let tombstoneLimit = 500

    /// On-disk shape. Older files were a bare array; both are accepted.
    private struct Stored: Codable {
        var items: [KShellNotification]
        var dismissed: [String: Date]
    }

    private let file: URL?
    private let lock = NSLock()

    public init(file: URL? = nil) {
        self.file = file ?? KShellPaths.cacheDirectory.appendingPathComponent("notifications.json")
        load()
    }

    /// Add notifications, skipping ones already known.
    ///
    /// `announce` is false when reading history back: those cards fill the
    /// centre but must not pop up all at once on launch.
    public func ingest(_ incoming: [KShellNotification], announce: Bool = true) {
        guard !incoming.isEmpty else { return }
        if !Thread.isMainThread {
            DispatchQueue.main.async { [weak self] in self?.ingest(incoming, announce: announce) }
            return
        }

        let known = Set(items.map(\.id))
        let fresh = incoming.filter { !known.contains($0.id) && dismissed[$0.id] == nil }
        guard !fresh.isEmpty else { return }

        items.append(contentsOf: fresh)
        items.sort { $0.date > $1.date }
        trim()

        if announce {
            unread += fresh.count
            for item in fresh.sorted(by: { $0.date < $1.date }) {
                onArrive?(item)
            }
        }
        save()
    }

    /// Remove one card, in the centre or the popup.
    public func dismiss(_ id: String) {
        guard items.contains(where: { $0.id == id }) else { return }
        items.removeAll { $0.id == id }
        tombstone([id])
        save()
    }

    /// Empty the centre. The system's own copy of each notification is not
    /// touched — this is the shell's list, not macOS's.
    public func clear() {
        guard !items.isEmpty || unread != 0 else { return }
        tombstone(items.map(\.id))
        items.removeAll()
        unread = 0
        save()
    }

    public func markAllRead() {
        guard unread != 0 else { return }
        unread = 0
    }

    // MARK: - keeping it

    /// Remember dismissals, dropping the oldest once there are too many.
    private func tombstone(_ ids: [String]) {
        let now = Date()
        for id in ids {
            dismissed[id] = now
        }
        if dismissed.count > Self.tombstoneLimit {
            let keep = dismissed.sorted { $0.value > $1.value }.prefix(Self.tombstoneLimit)
            dismissed = Dictionary(uniqueKeysWithValues: keep.map { ($0.key, $0.value) })
        }
    }

    private func trim() {
        let cap = max(0, limit)
        if items.count > cap {
            items = Array(items.prefix(cap))
        }
    }

    private func load() {
        guard let file, let data = try? Data(contentsOf: file) else { return }
        let decoder = JSONDecoder()
        if let stored = try? decoder.decode(Stored.self, from: data) {
            items = stored.items.sorted { $0.date > $1.date }
            dismissed = stored.dismissed
        } else if let legacy = try? decoder.decode([KShellNotification].self, from: data) {
            items = legacy.sorted { $0.date > $1.date }
        } else {
            return
        }
        trim()
        // Whatever was waiting when the shell quit is not new any more.
        unread = 0
    }

    private func save() {
        guard let file else { return }
        lock.lock()
        defer { lock.unlock() }
        do {
            try FileManager.default.createDirectory(
                at: file.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data = try JSONEncoder().encode(Stored(items: items, dismissed: dismissed))
            try data.write(to: file, options: .atomic)
        } catch {
            // The history is a convenience; a failed write must not take the
            // shell down with it.
        }
    }
}
