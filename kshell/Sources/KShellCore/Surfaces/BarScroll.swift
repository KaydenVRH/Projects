import AppKit

/// Scrolling over the bar's widgets.
///
/// SwiftUI on macOS has no scroll-wheel modifier, and an AppKit view laid over
/// a widget to catch the wheel would also take its clicks. So the widget under
/// the pointer registers itself here as it's hovered, and one app-wide monitor
/// hands it the bar's scroll events.
enum BarScroll {
    /// The hovered widget, if it takes scrolling.
    static weak var target: WidgetModel?
    private static var monitor: Any?

    static func start() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { event in
            guard event.window is BarPanel, let handler = target?.onScroll else { return event }
            // Trackpads report points; a mouse wheel reports notches, ~10 points each.
            let distance = Double(event.scrollingDeltaY) * (event.hasPreciseScrollingDeltas ? 1 : 10)
            // "Natural" scrolling flips the delta; up should still mean up.
            handler(event.isDirectionInvertedFromDevice ? -distance : distance)
            return nil
        }
    }

    static func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    /// For things that move in whole steps (workspaces): one step per flick,
    /// not one per scroll event.
    static func stepper(_ step: @escaping (_ forward: Bool) -> Void) -> (Double) -> Void {
        var gathered = 0.0
        var lastStep = Date.distantPast
        return { distance in
            gathered += distance
            guard abs(gathered) >= 18, Date().timeIntervalSince(lastStep) > 0.25 else { return }
            step(gathered < 0)
            gathered = 0
            lastStep = Date()
        }
    }
}
