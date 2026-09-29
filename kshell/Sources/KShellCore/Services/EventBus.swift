import Foundation

/// A tiny in-process event bus. Scripts reach it through `kshell trigger`,
/// which delivers a named event (with an optional payload) to every subscriber.
public enum EventBus {
    public static func post(_ event: String, payload: String = "") {
        NotificationCenter.default.post(
            name: Notification.Name("kshell.event." + event),
            object: payload
        )
    }

    /// Subscribe to an event; returns a token to pass to `unobserve`.
    public static func observe(
        _ event: String,
        _ handler: @escaping (String) -> Void
    ) -> NSObjectProtocol {
        NotificationCenter.default.addObserver(
            forName: Notification.Name("kshell.event." + event),
            object: nil,
            queue: .main
        ) { note in
            handler(note.object as? String ?? "")
        }
    }

    public static func unobserve(_ token: NSObjectProtocol) {
        NotificationCenter.default.removeObserver(token)
    }
}
