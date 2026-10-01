import AppKit

/// Watches for windows that have gone full screen, so the shell can get out of
/// the way.
///
/// A shell drawn above everything else — the bar and the border especially —
/// would otherwise sit on top of a full-screen app. Three things catch that:
///
/// - the shell's windows do not carry `fullScreenAuxiliary`, so they never join
///   a full-screen space;
/// - a window that fills a display *within* the current space counts, which
///   catches anything that drops the window manager's gaps;
/// - the window manager is asked outright, because aerospace keeps its outer
///   gaps in full screen, making it geometrically identical to a maximised
///   window.
///
/// The window manager query runs through `ProcessRunner`, i.e. on a background
/// queue. Spawning and reading a process on the main thread blocks it — and
/// blocking the main thread stops every widget in the bar from updating.
final class FullscreenWatch {
    private let runner = ProcessRunner()
    private var flagged: Set<Int> = []
    private var covered: Set<CGDirectDisplayID> = []
    private let handler: (Set<CGDirectDisplayID>) -> Void

    init(handler: @escaping (Set<CGDirectDisplayID>) -> Void) {
        self.handler = handler
    }

    func start() {
        runner.run(command: Self.command, interval: 0.5) { [weak self] output in
            guard let self else { return }
            self.flagged = Self.parse(output)
            self.evaluate()
        }
    }

    func stop() {
        runner.stop()
        flagged = []
        covered = []
    }

    /// Re-check now: geometry from the window server, plus whatever the window
    /// manager last reported.
    private func evaluate() {
        let found = FullscreenWatch.covers(flagged: flagged)
        guard found != covered else { return }
        covered = found
        handler(found)
    }

    private static let command =
        "aerospace list-windows --all --format '%{window-id}|%{window-is-fullscreen}' 2>/dev/null"

    static func parse(_ output: String) -> Set<Int> {
        var ids: Set<Int> = []
        for line in output.split(separator: "\n") {
            let fields = line.split(separator: "|")
            guard fields.count == 2, fields[1] == "true", let id = Int(fields[0]) else { continue }
            ids.insert(id)
        }
        return ids
    }

    /// The displays with a full-screen window on them.
    static func covers(flagged: Set<Int>) -> Set<CGDirectDisplayID> {
        let list = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements],
            kCGNullWindowID
        ) as? [[String: Any]] ?? []

        let screens = NSScreen.screens
        guard let main = screens.first(where: { $0.frame.origin == .zero }) ?? screens.first else {
            return []
        }
        let flipHeight = main.frame.maxY

        var covered: Set<CGDirectDisplayID> = []

        for window in list {
            guard (window[kCGWindowLayer as String] as? Int) == 0,
                  (window[kCGWindowOwnerName as String] as? String) != "kshell",
                  let alpha = window[kCGWindowAlpha as String] as? Double, alpha > 0.05,
                  let bounds = window[kCGWindowBounds as String] as? [String: Any],
                  let x = bounds["X"] as? Double,
                  let y = bounds["Y"] as? Double,
                  let width = bounds["Width"] as? Double,
                  let height = bounds["Height"] as? Double,
                  width > 0, height > 0
            else { continue }

            // Window server coordinates have their origin at the main display's
            // top-left; screen frames are measured from its bottom-left.
            let rect = CGRect(
                x: x,
                y: flipHeight - y - height,
                width: width,
                height: height
            )

            let number = window[kCGWindowNumber as String] as? Int
            let isFullscreen = number.map { flagged.contains($0) } ?? false
            guard isFullscreen || covers(rect, mainDisplay: main, screens: screens) else { continue }

            for screen in screens where rect.intersects(screen.frame) {
                if let id = displayID(screen) { covered.insert(id) }
            }
        }
        return covered
    }

    /// A window that genuinely reaches a display's edges counts as full screen.
    /// A maximised window keeps the layout's gaps, so it does not.
    private static func covers(_ rect: CGRect, mainDisplay: NSScreen, screens: [NSScreen]) -> Bool {
        screens.contains { covers(rect, $0.frame) }
    }

    /// Whether `window` fills `screen`.
    ///
    /// The top edge is deliberately not required to reach the screen's top: a
    /// native full-screen window stops short of it (the menu bar and notch keep
    /// their strip — measured at 33pt on a notched display), so requiring it
    /// would never match. The other three edges do the discriminating: a merely
    /// maximised window keeps the layout's gaps on them.
    static func covers(_ window: CGRect, _ screen: CGRect) -> Bool {
        let slack: CGFloat = 2
        let topSlack: CGFloat = 60
        return window.minX <= screen.minX + slack
            && window.maxX >= screen.maxX - slack
            && window.minY <= screen.minY + slack
            && window.maxY >= screen.maxY - topSlack
    }

    static func displayID(_ screen: NSScreen) -> CGDirectDisplayID? {
        let key = NSDeviceDescriptionKey("NSScreenNumber")
        return screen.deviceDescription[key] as? CGDirectDisplayID
    }
}
