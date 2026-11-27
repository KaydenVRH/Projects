import AppKit
import Foundation

/// Holds the live appearance and widget runtimes for the bar. One instance is
/// shared by every display's panel.
public final class BarViewModel: ObservableObject {
    @Published public var appearance: BarConfig
    @Published public var theme: Theme
    /// Set while the frame is on: launchers then take their glass from the
    /// frame's surface and curve into it. Nil when the frame is off.
    @Published public var panelSurface: PanelSurfaceSettings?
    /// Hands out a piece of the frame's glass for a display, when the frame is on.
    public var makeGlass: ((NSScreen) -> SurfaceGlass?)?

    @Published public var left: [WidgetRuntime]
    @Published public var center: [WidgetRuntime]
    @Published public var right: [WidgetRuntime]
    /// The audio visualiser, when it is drawn inside the bar (behind the
    /// widgets) rather than along the bottom of the screen.
    @Published public var barVisualiser: VisualiserConfig?

    public init(config: ShellConfig) {
        appearance = config.bar
        theme = config.theme
        Hud.update(for: config.theme)
        Arcane.update(for: config.theme)
        Neon.update(for: config.theme)
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
        // Switching to the HUD theme boots it, once, across every display.
        if Hud.update(for: config.theme) { HudBoot.play() }
        if Arcane.update(for: config.theme) { ArcaneCast.play() }
        if Neon.update(for: config.theme) { NeonBoot.play() }
        build(from: config)
    }

    /// The outline a launcher anchored to `edge` should use, or nil when the
    /// frame is off and the panel draws its own surface.
    public func panelStyle(for edge: OverlayEdge) -> PanelStyle? {
        guard let panelSurface else { return nil }
        return PanelStyle(
            radius: panelSurface.radius,
            filletRadius: panelSurface.filletRadius,
            attachment: edge.attachment,
            cut: Neon.isNeon(theme) ? Neon.panelCut : 0,
            notch: Arcane.isArcane(theme) ? Arcane.panelNotch : 0,
            hud: Hud.isHud(theme) ? Hud.panelChamfer : 0
        )
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
