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
        let me = ProcessInfo.processInfo.processIdentifier
        let front = NSWorkspace.shared.frontmostApplication

        // Only remember a *different* app. When a panel closes without handing the
        // front back the shell is itself frontmost, and recording that makes
        // `handBack()` a no-op — which is how the shell gets stranded below.
        if previous == nil, front?.processIdentifier != me {
            previous = front
        }

        guard !NSApp.isActive else { return }

        // The WindowServer can still report the shell frontmost while NSApp is
        // inactive (a panel closed, the front was never handed back). In that
        // state `activate(ignoringOtherApps:)` is a no-op — AppKit believes we
        // already have the front — so no window can become key: every keystroke
        // beeps and clicks do nothing at all until the shell is restarted.
        // Resigning first clears that belief and makes the activation real.
        if front?.processIdentifier == me {
            NSApp.deactivate()
        }
        NSApp.activate(ignoringOtherApps: true)

        // Activation is asynchronous, so verify a turn later and force it through
        // the running-application path if AppKit still has not taken it.
        DispatchQueue.main.async {
            guard !NSApp.isActive else { return }
            NSRunningApplication.current.activate(options: [.activateAllWindows])
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
        if app.activate(options: []) { return }
        // Dropped: the shell still owns the front with no key window, which is
        // the dead-keyboard state. Resign so the WindowServer promotes someone
        // else, rather than sitting in front owning nothing.
        if NSApp.isActive { NSApp.deactivate() }
    }

    /// Forget the remembered app — for when the shell opened something that owns
    /// the front itself.
    static func forget() {
        previous = nil
    }
}
