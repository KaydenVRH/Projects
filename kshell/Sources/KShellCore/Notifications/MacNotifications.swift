import AppKit
import Foundation
import SQLite3

/// The database macOS keeps for Notification Center.
///
/// Every notification an app delivers is recorded here, whether or not its
/// banner was shown: `record` holds one row per notification and `app` maps the
/// numeric `app_id` to a bundle identifier. The payload in `record.data` is an
/// ordinary binary property list, so reading it needs no private framework —
/// only permission to open the file.
///
/// This is an implementation detail of macOS, not an API. It is read defensively:
/// anything unexpected is skipped rather than allowed to break the shell, and a
/// missing or unreadable database simply means no mirrored notifications.
enum MacNotificationDatabase {
    static var url: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Group Containers/group.com.apple.usernoted/db2/db")
    }

    static var isAvailable: Bool {
        FileManager.default.isReadableFile(atPath: url.path)
    }

    struct Row: Equatable {
        var uuid: String
        var bundleID: String?
        var date: Date
    }

    /// The newest rows, without their payloads — cheap enough to poll.
    static func rows(limit: Int) -> [Row] {
        guard let db = open() else { return [] }
        defer { sqlite3_close(db) }

        let sql = """
            SELECT hex(r.uuid), a.identifier, r.delivered_date, r.request_date
            FROM record r LEFT JOIN app a ON a.app_id = r.app_id
            ORDER BY r.delivered_date DESC, r.rec_id DESC
            LIMIT ?
            """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else { return [] }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_int(statement, 1, Int32(max(1, limit)))

        var rows: [Row] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let uuidText = sqlite3_column_text(statement, 0) else { continue }
            let uuid = String(cString: uuidText)
            let bundle = sqlite3_column_text(statement, 1).map { String(cString: $0) }
            let delivered = sqlite3_column_double(statement, 2)
            let requested = sqlite3_column_double(statement, 3)
            // These are Core Foundation absolute times: seconds since 2001.
            let stamp = delivered > 0 ? delivered : requested
            guard stamp > 0 else { continue }
            rows.append(Row(
                uuid: uuid,
                bundleID: bundle?.isEmpty == false ? bundle : nil,
                date: Date(timeIntervalSinceReferenceDate: stamp)
            ))
        }
        return rows
    }

    /// The content of one row, read from its property-list payload. The payload
    /// also names the app with its original capitalisation, which the `app`
    /// table lowercases — worth keeping for apps that have no bundle to look up.
    static func content(uuid: String) -> (title: String, subtitle: String?, body: String, bundle: String?)? {
        guard let db = open() else { return nil }
        defer { sqlite3_close(db) }

        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, "SELECT data FROM record WHERE hex(uuid) = ? LIMIT 1", -1, &statement, nil) == SQLITE_OK else {
            return nil
        }
        defer { sqlite3_finalize(statement) }
        sqlite3_bind_text(statement, 1, uuid, -1, SQLITE_TRANSIENT)
        guard sqlite3_step(statement) == SQLITE_ROW,
              let blob = sqlite3_column_blob(statement, 0) else { return nil }
        let length = Int(sqlite3_column_bytes(statement, 0))
        guard length > 0 else { return nil }
        let data = Data(bytes: blob, count: length)

        guard let plist = try? PropertyListSerialization.propertyList(
            from: data,
            options: [],
            format: nil
        ) as? [String: Any] else { return nil }

        // The usual shape: the request itself under `req`.
        if let request = plist["req"] as? [String: Any],
           let content = fields(from: request) {
            return content
        }
        if let content = fields(from: plist) {
            return content
        }
        // Some versions archive the request; the keys live inside `$objects`.
        if let objects = plist["$objects"] as? [Any] {
            for case let object as [String: Any] in objects {
                if let content = fields(from: object) { return content }
            }
        }
        return nil
    }

    private static func fields(from table: [String: Any]) -> (title: String, subtitle: String?, body: String, bundle: String?)? {
        let title = (table["titl"] as? String) ?? ""
        let body = (table["body"] as? String) ?? ""
        guard !title.isEmpty || !body.isEmpty else { return nil }
        let subtitle = table["subt"] as? String
        let bundle = table["app"] as? String
        return (title, subtitle?.isEmpty == true ? nil : subtitle, body, bundle)
    }

    private static let SQLITE_TRANSIENT = unsafeBitCast(
        -1,
        to: sqlite3_destructor_type.self
    )

    private static func open() -> OpaquePointer? {
        guard isAvailable else { return nil }
        var db: OpaquePointer?
        let flags = SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX
        guard sqlite3_open_v2(url.path, &db, flags, nil) == SQLITE_OK else {
            sqlite3_close(db)
            return nil
        }
        // usernoted writes whenever a notification arrives; never block on it.
        sqlite3_busy_timeout(db, 250)
        return db
    }
}

/// Builds notifications from the database rows.
enum MacNotificationReader {
    static func notification(for row: MacNotificationDatabase.Row) -> KShellNotification? {
        guard let content = MacNotificationDatabase.content(uuid: row.uuid) else { return nil }
        let title = content.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let body = content.body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty || !body.isEmpty else { return nil }
        let bundle = content.bundle ?? row.bundleID
        return KShellNotification(
            id: "mac:" + row.uuid,
            source: .macos,
            bundleID: bundle,
            app: AppIdentity.name(for: bundle),
            title: title,
            subtitle: content.subtitle,
            body: title.isEmpty ? "" : body,
            date: row.date
        )
    }
}

/// Polls the system database and feeds the store.
///
/// The first pass is history, not news: it fills the centre with what macOS has
/// delivered in the last day and announces nothing. Everything after that is new
/// and pops a card. Polling is a small query for the row list; payloads are only
/// read for rows that weren't known before.
final class MacNotificationWatcher {
    private let store: NotificationStore
    private let interval: TimeInterval
    private let history: Int
    private var timer: DispatchSourceTimer?
    private var known: Set<String> = []
    private var seeded = false

    init(store: NotificationStore = .shared, interval: TimeInterval = 1.5, history: Int = 60) {
        self.store = store
        self.interval = max(0.5, interval)
        self.history = max(1, history)
    }

    func start() {
        guard MacNotificationDatabase.isAvailable, timer == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "kshell.notifications"))
        timer.schedule(deadline: .now(), repeating: interval, leeway: .milliseconds(200))
        timer.setEventHandler { [weak self] in self?.tick() }
        self.timer = timer
        timer.resume()
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }

    private func tick() {
        let rows = MacNotificationDatabase.rows(limit: 300)
        guard !rows.isEmpty else { return }

        if !seeded {
            seeded = true
            seed(rows)
            return
        }

        let fresh = rows.filter { !known.contains($0.uuid) }
        guard !fresh.isEmpty else { return }
        for row in fresh { known.insert(row.uuid) }

        let items = fresh.compactMap(MacNotificationReader.notification(for:))
        guard !items.isEmpty else { return }
        DispatchQueue.main.async { [store] in
            store.ingest(items)
        }
    }

    /// The first pass: whatever macOS delivered in the last day, as history.
    private func seed(_ rows: [MacNotificationDatabase.Row]) {
        let cutoff = Date().addingTimeInterval(-24 * 60 * 60)
        let recent = rows.filter { $0.date >= cutoff }.prefix(history)
        for row in rows { known.insert(row.uuid) }
        let items = recent.compactMap(MacNotificationReader.notification(for:))
        guard !items.isEmpty else { return }
        DispatchQueue.main.async { [store] in
            store.ingest(items, announce: false)
        }
    }
}

/// Bundle identifier → what to call the app, and its icon.
enum AppIdentity {
    private static var names: [String: String] = [:]
    private static var icons: [String: NSImage] = [:]
    private static let lock = NSLock()

    static func name(for bundleID: String?) -> String {
        guard let raw = bundleID, !raw.isEmpty else { return "Notification" }
        let id = normalized(raw)
        lock.lock()
        if let cached = names[id] { lock.unlock(); return cached }
        lock.unlock()

        let resolved = resolveName(id)
        lock.lock()
        names[id] = resolved
        lock.unlock()
        return resolved
    }

    static func icon(for bundleID: String?) -> NSImage? {
        guard let raw = bundleID, !raw.isEmpty else { return nil }
        let id = normalized(raw)
        lock.lock()
        if let cached = icons[id] { lock.unlock(); return cached }
        lock.unlock()

        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: id) else { return nil }
        let image = NSWorkspace.shared.icon(forFile: url.path)
        lock.lock()
        icons[id] = image
        lock.unlock()
        return image
    }

    /// `_system_center_:com.apple.followup.alert` is still an Apple app.
    static func normalized(_ bundleID: String) -> String {
        bundleID.hasPrefix("_system_center_:")
            ? String(bundleID.dropFirst("_system_center_:".count))
            : bundleID
    }

    private static func resolveName(_ id: String) -> String {
        if let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: id) {
            let bundle = Bundle(url: url)
            if let display = bundle?.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String,
               !display.isEmpty {
                return display
            }
            if let name = bundle?.object(forInfoDictionaryKey: "CFBundleName") as? String,
               !name.isEmpty {
                return name
            }
            let file = FileManager.default.displayName(atPath: url.path)
            return file.hasSuffix(".app") ? String(file.dropLast(4)) : file
        }
        // Unknown app: "com.acme.fancy-notes" reads better as "Fancy-notes".
        let tail = id.split(separator: ".").last.map(String.init) ?? id
        return tail.prefix(1).uppercased() + tail.dropFirst()
    }
}
