import AppKit

/// Handing the front back.
///
/// A panel calls `NSApp.activate` so it can take the keyboard — a launcher needs
/// it for its search field, the island for its transport keys. Nothing used to
/// give the front back, so once a panel closed the shell was still the active
/// application with no key window: the next keystroke went nowhere and the
/// keyboard looked dead until you clicked something. (Clicking also launches an
/// app, which then takes the front by itself, so only the "closed it and went
/// back to what I was doing" case loses the keys.)
///
/// So remember who had the front before the shell took it, and hand it back when
/// the last panel closes.
enum Frontmost {
    private static var previous: NSRunningApplication?

    /// Take the front, remembering who had it. Safe to call repeatedly — only
    /// the first call records, since later panels are not the ones that stole it.
    static func claim() {
        if previous == nil {
            previous = NSWorkspace.shared.frontmostApplication
        }
        if #available(macOS 14.0, *) {
            NSApp.activate()
        } else {
            NSApp.activate(ignoringOtherApps: true)
        }
    }

    /// Give the front back, if the shell still has it. If something else took it
    /// in the meantime — the app the user just launched, say — there is nothing
    /// to do.
    static func handBack() {
        guard let app = previous else { return }
        previous = nil
        let me = ProcessInfo.processInfo.processIdentifier
        guard app.processIdentifier != me else { return }
        guard NSWorkspace.shared.frontmostApplication?.processIdentifier == me else { return }
        app.activate(options: [])
    }

    /// Forget the remembered app — for when the shell opened something that owns
    /// the front itself.
    static func forget() {
        previous = nil
    }
}
