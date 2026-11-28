import AppKit
import QuartzCore

/// A trademark theme's own way of bringing a panel in and taking it away,
/// instead of the jelly (`PanelMotion`).
///
/// Each frame the timeline says where the panel is — its rectangle, its
/// outline, how its content is drawn — and places the light it throws (drawn
/// over the panel, unclipped). The panels (`OverlayPanel`, `MediaIsland`) only
/// apply what it says, every frame, glass included.
protocol PanelTimeline: AnyObject {
    /// Each frame; read `layout` and place the lights.
    var onStep: (() -> Void)? { get set }
    /// Once it has arrived (true) or gone (false).
    var onSettle: ((Bool) -> Void)? { get set }
    /// The light the motion throws, to be added over the panel, unmasked.
    var lights: CALayer { get }
    /// Room the window needs past the panel while it moves: away from its edge,
    /// and on each side along it.
    var slack: (out: CGFloat, side: CGFloat) { get }

    func open()
    func close()
    /// Back to nothing, at once.
    func reset()
    /// Where a panel resting at `full`, anchored to `edge` with outline `style`,
    /// is now.
    func layout(full: NSRect, edge: OverlayEdge, style: PanelStyle) -> PanelTimelineLayout
    /// Put the lights round `rect`, in the window's coordinates.
    func placeLights(rect: CGRect, edge: OverlayEdge)
}

/// One frame of a timeline, for a panel to apply.
struct PanelTimelineLayout {
    var rect: NSRect
    var style: PanelStyle
    /// The content drawn into the rectangle (squashed or scaled with it)
    /// rather than laid out at full size and revealed by it.
    var squash: Bool
    var hostAlpha: CGFloat
    var glassFade: CGFloat
    var chromeAlpha: CGFloat
    /// A wash of colour over the content, 0…1.
    var bloom: CGFloat = 0
    var bloomColour: NSColor = .white
}

enum PanelTimelines {
    /// How far a floating panel rests off the frame's edge.
    static let floatGap: CGFloat = 10

    /// Whether panels float in this theme: the themes whose panels appear
    /// where they'll be (Stark, Arcane, Eclipse, Abyss) rest a little off the
    /// frame, free on every side. Panels that grow out of the edge — Neon's
    /// and the jelly — stay fastened to it.
    static func floats(_ theme: Theme) -> Bool {
        Hud.isHud(theme) || Arcane.isArcane(theme) || Eclipse.isEclipse(theme) || Abyss.isAbyss(theme)
    }

    /// The same, for the theme that's on.
    static var floating: Bool { Hud.active || Arcane.active || Eclipse.active || Abyss.active }

    /// The slack the current theme's timeline will need, before it is made.
    static var slack: (out: CGFloat, side: CGFloat)? {
        guard !Motion.reduced else { return nil }
        if Neon.active { return NeonPanelMotion.slack }
        if Arcane.active { return ArcanePanelMotion.slack }
        if Hud.active { return StarkPanelMotion.slack }
        if Eclipse.active { return EclipsePanelMotion.slack }
        if Abyss.active { return AbyssPanelMotion.slack }
        return nil
    }

    /// The current theme's timeline, if it has one and motion isn't reduced.
    static func make(screen: NSScreen) -> PanelTimeline? {
        guard !Motion.reduced else { return nil }
        if Neon.active { return NeonPanelMotion(screen: screen) }
        if Arcane.active { return ArcanePanelMotion(screen: screen) }
        if Hud.active { return StarkPanelMotion(screen: screen) }
        if Eclipse.active { return EclipsePanelMotion(screen: screen) }
        if Abyss.active { return AbyssPanelMotion(screen: screen) }
        return nil
    }
}
