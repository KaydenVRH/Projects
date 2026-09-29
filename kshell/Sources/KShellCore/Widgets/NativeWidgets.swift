import AppKit
import CoreWLAN
import Foundation
import IOBluetooth

/// Frontmost application name.
final class FrontAppRuntime: WidgetRuntime {
    let model: WidgetModel

    init(spec: WidgetSpec, theme: Theme) {
        model = WidgetFactory.baseModel(spec, theme: theme)
        model.labelColor = spec.color("label_color") ?? spec.color("color") ?? theme.highlight
    }

    func start() {
        refresh()
        NSWorkspace.shared.notificationCenter.addObserver(
            self,
            selector: #selector(refresh),
            name: NSWorkspace.didActivateApplicationNotification,
            object: nil
        )
    }

    func stop() {
        NSWorkspace.shared.notificationCenter.removeObserver(self)
    }

    @objc private func refresh() {
        model.label = NSWorkspace.shared.frontmostApplication?.localizedName ?? ""
    }
}

/// Shared ticker base for metric widgets.
class MetricRuntime: WidgetRuntime {
    let model: WidgetModel
    private var timer: DispatchSourceTimer?
    private let defaultIcon: String

    init(spec: WidgetSpec, theme: Theme, defaultIcon: String) {
        self.defaultIcon = defaultIcon
        model = WidgetFactory.baseModel(
            spec,
            theme: theme,
            icon: WidgetFactory.resolvedIcon(spec) ?? defaultIcon
        )
        if let action = spec.string("action"), !action.isEmpty {
            model.action = { Shell.run(action) }
        }
    }

    func start() {
        let interval = model.spec.number("interval") ?? 5
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: max(interval, 1))
        timer.setEventHandler { [weak self] in self?.tick() }
        timer.resume()
        self.timer = timer
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }

    func tick() {}
}

final class CPURuntime: MetricRuntime {
    private var previous: host_cpu_load_info?

    init(spec: WidgetSpec, theme: Theme) {
        super.init(spec: spec, theme: theme, defaultIcon: "\u{f2db}")
        _ = SystemMetrics.cpuUsage(previous: &previous)
    }

    override func tick() {
        let usage = SystemMetrics.cpuUsage(previous: &previous)
        model.label = String(format: "%.0f%%", usage * 100)
    }
}

final class RAMRuntime: MetricRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        super.init(spec: spec, theme: theme, defaultIcon: "\u{efc1}")
    }

    override func tick() {
        let usage = SystemMetrics.memoryUsage()
        model.label = String(format: "%.0f%%", usage * 100)
    }
}

final class BatteryRuntime: MetricRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        super.init(spec: spec, theme: theme, defaultIcon: "\u{f244}")
    }

    override func tick() {
        let battery = SystemMetrics.battery()
        guard battery.present else {
            model.icon = "\u{f244}"
            model.label = ""
            return
        }
        model.icon = battery.charging ? "\u{f0e7}" : icon(for: battery.percent)
        model.label = "\(battery.percent)%"
    }

    private func icon(for percent: Int) -> String {
        switch percent {
        case 90...: return "\u{f240}"
        case 65..<90: return "\u{f241}"
        case 40..<65: return "\u{f242}"
        case 15..<40: return "\u{f243}"
        default: return "\u{f244}"
        }
    }
}

/// Wi-Fi state. SSID needs Location permission on recent macOS, so the label
/// stays empty unless it is available; the icon reflects power state.
final class WifiRuntime: MetricRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        super.init(spec: spec, theme: theme, defaultIcon: "\u{f1eb}")
    }

    override func tick() {
        let interface = CWWiFiClient.shared().interface()
        let on = interface?.powerOn() ?? false
        model.icon = on ? "\u{f1eb}" : "\u{f05e}"
        model.label = on ? (interface?.ssid() ?? "") : ""
    }
}

/// Output volume + mute via CoreAudio. Clicking is wired through `action`.
final class VolumeRuntime: MetricRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        super.init(spec: spec, theme: theme, defaultIcon: "\u{f028}")
    }

    override func tick() {
        let state = SystemAudio.defaultOutput()
        let percent = Int((state.volume * 100).rounded())
        if state.muted || percent == 0 {
            model.icon = "\u{f026}"   // volume-off
        } else if percent < 50 {
            model.icon = "\u{f027}"   // volume-down
        } else {
            model.icon = "\u{f028}"   // volume-up
        }
        model.label = state.muted ? "muted" : "\(percent)%"
    }
}

/// Bluetooth controller power state via IOBluetooth.
final class BluetoothRuntime: MetricRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        super.init(spec: spec, theme: theme, defaultIcon: "\u{f293}")
    }

    override func tick() {
        let state = IOBluetoothHostController.default()?.powerState
        let on = state == kBluetoothHCIPowerStateON
        model.icon = on ? "\u{f293}" : "\u{f294}"   // bluetooth-b / bluetooth
        model.label = on ? "" : "off"
    }
}
