import Foundation

/// A clock that refreshes on an interval from a DateFormatter pattern.
final class ClockRuntime: WidgetRuntime {
    let model: WidgetModel
    private var timer: DispatchSourceTimer?

    init(spec: WidgetSpec, theme: Theme) {
        model = WidgetFactory.baseModel(spec, theme: theme, icon: spec.string("icon"))
        model.labelColor = spec.color("label_color") ?? spec.color("color") ?? theme.highlight
    }

    func start() {
        let interval = model.spec.number("interval") ?? 1
        let format = model.spec.string("format") ?? "HH:mm"

        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: max(interval, 0.25))
        timer.setEventHandler { [weak self] in
            let formatter = DateFormatter()
            formatter.dateFormat = format
            self?.model.label = formatter.string(from: Date())
        }
        timer.resume()
        self.timer = timer
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }
}
