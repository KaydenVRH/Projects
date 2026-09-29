import AppKit

/// Reveals auto-hiding bars when the pointer nears their screen edge and hides
/// them again after the pointer leaves. Uses a light pointer poll, which avoids
/// the quirks of global/local event monitors over non-activating windows.
final class BarAutoHide {
    private let barsProvider: () -> [BarPanel]
    private let isSuspended: () -> Bool
    private var timer: DispatchSourceTimer?
    private var pendingHide: [ObjectIdentifier: DispatchWorkItem] = [:]

    init(bars: @escaping () -> [BarPanel], suspended: @escaping () -> Bool) {
        self.barsProvider = bars
        self.isSuspended = suspended
    }

    func start() {
        guard timer == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 0.15)
        timer.setEventHandler { [weak self] in self?.tick() }
        timer.resume()
        self.timer = timer
    }

    func stop() {
        timer?.cancel()
        timer = nil
        for work in pendingHide.values { work.cancel() }
        pendingHide.removeAll()
    }

    private func tick() {
        guard !isSuspended() else { return }
        for bar in barsProvider() where bar.autohideEnabled {
            if bar.mouseInRevealZone() || bar.containsMouseLocation() {
                cancelHide(bar)
                bar.setRevealed(true, animated: true)
            } else if bar.isRevealed {
                scheduleHide(bar)
            }
        }
    }

    private func scheduleHide(_ bar: BarPanel) {
        let key = ObjectIdentifier(bar)
        guard pendingHide[key] == nil else { return }
        let work = DispatchWorkItem { [weak self, weak bar] in
            guard let self, let bar else { return }
            self.pendingHide[key] = nil
            guard !self.isSuspended() else { return }
            if !bar.containsMouseLocation() && !bar.mouseInRevealZone() {
                bar.setRevealed(false, animated: true)
            }
        }
        pendingHide[key] = work
        DispatchQueue.main.asyncAfter(deadline: .now() + bar.hideDelay, execute: work)
    }

    private func cancelHide(_ bar: BarPanel) {
        let key = ObjectIdentifier(bar)
        pendingHide[key]?.cancel()
        pendingHide[key] = nil
    }
}
