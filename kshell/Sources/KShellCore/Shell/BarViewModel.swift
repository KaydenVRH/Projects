import Foundation

/// Holds the live appearance and widget runtimes for the bar. One instance is
/// shared by every display's panel.
public final class BarViewModel: ObservableObject {
    @Published public var appearance: BarConfig
    @Published public var theme: Theme
    @Published public var left: [WidgetRuntime]
    @Published public var center: [WidgetRuntime]
    @Published public var right: [WidgetRuntime]

    public init(config: ShellConfig) {
        appearance = config.bar
        theme = config.theme
        left = []
        center = []
        right = []
        build(from: config)
    }

    /// Replace all widget runtimes from a new config.
    public func apply(_ config: ShellConfig) {
        stopAll()
        appearance = config.bar
        theme = config.theme
        build(from: config)
    }

    private func build(from config: ShellConfig) {
        left = config.left.flatMap { WidgetFactory.make(spec: $0, theme: config.theme, bar: config.bar) }
        center = config.center.flatMap { WidgetFactory.make(spec: $0, theme: config.theme, bar: config.bar) }
        right = config.right.flatMap { WidgetFactory.make(spec: $0, theme: config.theme, bar: config.bar) }
        startAll()
    }

    private func startAll() {
        for runtime in left + center + right { runtime.start() }
    }

    private func stopAll() {
        for runtime in left + center + right { runtime.stop() }
    }

    public func stop() {
        stopAll()
    }
}
