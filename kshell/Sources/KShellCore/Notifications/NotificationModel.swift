import AppKit
import Foundation

/// One notification the shell knows about, whatever delivered it.
///
/// Two sources feed the same list: the system's own notifications, read from the
/// database macOS keeps for Notification Center, and notifications posted by the
/// shell itself (`kshell notify "Title" "Body"`). Keeping one type for both means
/// the popup and the centre don't care where a card came from — only how to open
/// it.
public struct KShellNotification: Codable, Identifiable, Equatable, Sendable {
    public enum Source: String, Codable, Sendable {
        /// Delivered by macOS for another app; mirrored, not re-delivered.
        case macos
        /// Posted by kshell or a script through the socket.
        case kshell
    }

    /// How urgent the notification is, used for its accent colour.
    public enum Tone: String, Codable, Sendable {
        case info, success, warning, error
    }

    /// Stable identity: `mac:<uuid>` for mirrored ones, `kshell:<uuid>` otherwise.
    /// Ingest is deduplicated by this, so a database poll can re-read a row.
    public var id: String
    public var source: Source
    /// The app that posted it, when there is one (`com.apple.mail`).
    public var bundleID: String?
    /// What to call that app (`Mail`), already resolved for display.
    public var app: String
    public var title: String
    public var subtitle: String?
    public var body: String
    public var date: Date
    /// A shell command to run when the card is chosen. Only shell-posted cards
    /// carry one; mirrored cards open their app instead.
    public var action: String?
    public var tone: Tone

    public init(
        id: String = "kshell:" + UUID().uuidString,
        source: Source,
        bundleID: String? = nil,
        app: String,
        title: String,
        subtitle: String? = nil,
        body: String = "",
        date: Date = Date(),
        action: String? = nil,
        tone: Tone = .info
    ) {
        self.id = id
        self.source = source
        self.bundleID = bundleID
        self.app = app
        self.title = title
        self.subtitle = subtitle
        self.body = body
        self.date = date
        self.action = action
        self.tone = tone
    }

    /// Whether there is a second line worth drawing.
    public var hasBody: Bool { !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// One line of text for somewhere with no room for two: the title, or the
    /// body when an app posted a notification with no title at all.
    public var headline: String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { return trimmed }
        return body.components(separatedBy: .newlines).first ?? body
    }

    // MARK: - the `notify` payload

    /// Decode the payload of a `notify` event.
    ///
    /// JSON is the documented form — `{"title": "...", "body": "...", "app":
    /// "Downloads"}` — because a body may contain anything, including the
    /// fallback separator. The older `Title|Body|App` form still works so a shell
    /// one-liner stays short.
    public static func parse(payload: String, now: Date = Date()) -> KShellNotification? {
        let trimmed = payload.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        if trimmed.hasPrefix("{"),
           let data = trimmed.data(using: .utf8),
           let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            return from(object: object, now: now)
        }

        let parts = trimmed.components(separatedBy: "|")
        let title = parts[0].trimmingCharacters(in: .whitespaces)
        guard !title.isEmpty else { return nil }
        let body = parts.count > 1 ? parts[1].trimmingCharacters(in: .whitespaces) : ""
        let app = parts.count > 2 && !parts[2].isEmpty ? parts[2] : "kshell"
        return KShellNotification(source: .kshell, app: app, title: title, body: body, date: now)
    }

    private static func from(object: [String: Any], now: Date) -> KShellNotification? {
        let title = (object["title"] as? String)?.trimmingCharacters(in: .whitespaces) ?? ""
        let body = (object["body"] as? String) ?? ""
        guard !title.isEmpty || !body.isEmpty else { return nil }
        let app = (object["app"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "kshell"
        let tone = Tone(rawValue: (object["tone"] as? String)?.lowercased() ?? "") ?? .info
        return KShellNotification(
            source: .kshell,
            bundleID: object["bundle"] as? String,
            app: app,
            title: title,
            subtitle: object["subtitle"] as? String,
            body: body,
            date: now,
            action: (object["action"] as? String).flatMap { $0.isEmpty ? nil : $0 },
            tone: tone
        )
    }
}

/// Doing something with a notification.
public enum NotificationAction {
    /// Run what the card asked for: a shell-posted card runs its command, a
    /// mirrored one brings its app forward.
    public static func open(_ item: KShellNotification) {
        if let action = item.action, !action.isEmpty {
            Shell.runDetached(action)
            return
        }
        guard let bundle = item.bundleID, !bundle.isEmpty else { return }
        let id = bundle.hasPrefix("_system_center_:")
            ? String(bundle.dropFirst("_system_center_:".count))
            : bundle
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: id) else { return }
        NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
    }
}
