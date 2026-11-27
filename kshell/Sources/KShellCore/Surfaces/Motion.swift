import AppKit
import QuartzCore
import SwiftUI

/// The shell's motion, in one place: the springs the stepped panels run on, the
/// display-synced clock that steps them, and the SwiftUI springs the content
/// inside them uses — so a panel and its rows move with the same character.
///
/// The panels are springs rather than fixed-length curves. A spring carries its
/// velocity, so a panel that is told to close while it is still opening turns
/// around smoothly instead of restarting a curve from rest, and it lets a panel
/// arrive with a little bounce past its resting size before settling.
public struct Spring: Sendable, Equatable {
    /// Seconds for one undamped oscillation: lower is quicker.
    public var response: Double
    /// 1 is critically damped (no bounce); lower bounces more.
    public var damping: Double

    public init(response: Double, damping: Double) {
        self.response = response
        self.damping = damping
    }

    var stiffness: Double { pow(2 * .pi / response, 2) }
    var friction: Double { 4 * .pi * damping / response }

    /// How far past the target a step from rest travels, as a fraction of the
    /// distance — what the window's slack has to cover.
    public var overshoot: Double {
        guard damping < 1 else { return 0 }
        return exp(-damping * .pi / sqrt(1 - damping * damping))
    }

    /// Reaching out from the edge. Loosely damped: past the moment it arrives
    /// this spring is the panel's jelly wobble (see `PanelPose`), and a few
    /// decaying swings are what make it read as jelly rather than a bump.
    public static let panelOutward = Spring(response: 0.34, damping: 0.4)
    /// Widening along the edge: a touch quicker and calmer, so a panel spreads
    /// first and then reaches out — the stretch that makes it read as liquid.
    public static let panelAlong = Spring(response: 0.38, damping: 0.82)
    /// Folding away: no bounce, it just goes.
    public static let panelClose = Spring(response: 0.26, damping: 1)
}

/// One value moving under a spring.
///
/// Integrated in fixed one-millisecond slices, so it moves the same on a 60 Hz
/// display, a 120 Hz one, or after a hitch.
public struct SpringValue: Sendable, Equatable {
    public var value: Double
    public var velocity: Double = 0
    public var target: Double

    public init(_ value: Double) {
        self.value = value
        self.target = value
    }

    public mutating func advance(by dt: Double, spring: Spring) {
        // A long hitch (a sleeping display) shouldn't fling the value.
        var remaining = min(max(dt, 0), 0.05)
        let slice = 0.001
        while remaining > 0 {
            let h = min(slice, remaining)
            let force = -spring.stiffness * (value - target) - spring.friction * velocity
            velocity += force * h
            value += velocity * h
            remaining -= h
        }
    }

    public var isSettled: Bool {
        abs(value - target) < 0.0006 && abs(velocity) < 0.01
    }

    public mutating func settle() {
        value = target
        velocity = 0
    }
}

/// Calls back on every refresh of a screen, at its own rate (60–120 Hz).
///
/// A `CADisplayLink` rather than a timer: it fires in step with the display, so
/// a stepped panel never lands two updates in one frame and none in the next —
/// which is what made the timer-driven morph look faintly uneven.
final class FrameClock: NSObject {
    private var link: CADisplayLink?
    private var last: CFTimeInterval = 0
    private let screen: NSScreen
    /// Given the seconds since the last frame; returns false to stop.
    private let tick: (Double) -> Bool

    init(screen: NSScreen, tick: @escaping (Double) -> Bool) {
        self.screen = screen
        self.tick = tick
    }

    var isRunning: Bool { link != nil }

    func start() {
        guard link == nil else { return }
        last = CACurrentMediaTime()
        let link = screen.displayLink(target: self, selector: #selector(step(_:)))
        let fps = Float(max(60, min(screen.maximumFramesPerSecond, 120)))
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 60, maximum: fps, preferred: fps)
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }

    @objc private func step(_ link: CADisplayLink) {
        let now = CACurrentMediaTime()
        let dt = now - last
        last = now
        if !tick(dt) { stop() }
    }
}

/// Where a panel is in its morph, as `PanelMotion` hands it out each frame.
///
/// On the way out the panel simply grows (`reach` 0 → 1). Once it has arrived,
/// the outward spring's swing about 1 stops being growth and becomes `wobble`:
/// squash and stretch around the resting size, the panel getting taller and
/// thinner, then shorter and wider, a few times over. That swing is drawn at a
/// fixed depth in points (`Jelly`), not scaled by the panel's size, so a tall
/// panel doesn't fling itself a hundred points past its rest.
struct PanelPose: Equatable {
    /// 0…1 along the edge.
    var along: CGFloat
    /// 0…1 out from the edge.
    var reach: CGFloat
    /// The jelly swing once arrived: positive is stretched out from the edge
    /// (and squeezed along it), negative squashed.
    var wobble: CGFloat

    static let closed = PanelPose(along: 0, reach: 0, wobble: 0)
}

/// How a pose's wobble becomes a shape.
enum Jelly {
    /// Points of stretch per unit of wobble, for a panel that travels `travel`
    /// points out from its edge. With the outward spring's ~30 % first swing
    /// that is a stretch of up to ~28 pt, then ~9 pt the other way, then ~3.
    static func depth(travel: CGFloat) -> CGFloat { min(90, travel * 0.4) }
    /// How much narrower a panel gets per point it stretches — the volume it
    /// keeps, which is what makes the swing read as squish.
    static let squeeze: CGFloat = 1.15
    /// The room a window needs for the jelly: out from the edge, and on each
    /// side along it (the squashed swing is wider than the panel).
    static func slack(travel: CGFloat) -> (out: CGFloat, side: CGFloat) {
        let peak = depth(travel: travel) * CGFloat(Spring.panelOutward.overshoot)
        return (ceil(peak * 1.15) + 4, ceil(peak * CGFloat(Spring.panelOutward.overshoot) * squeeze) + 6)
    }

    /// `rest` deformed by `wobble`: stretched (or squashed) away from `edge`,
    /// squeezed (or bulged) along it about its centre.
    static func shape(_ rest: NSRect, edge: OverlayEdge, wobble: CGFloat, travel: CGFloat) -> NSRect {
        guard wobble != 0 else { return rest }
        let stretch = wobble * depth(travel: travel)
        let across = -stretch * squeeze
        var rect = rest
        switch edge {
        case .bottom, .top:
            rect.size.height += stretch
            if edge == .top { rect.origin.y -= stretch }
            rect.size.width += across
            rect.origin.x -= across / 2
        case .leading, .trailing:
            rect.size.width += stretch
            if edge == .trailing { rect.origin.x -= stretch }
            rect.size.height += across
            rect.origin.y -= across / 2
        }
        return rect
    }
}

/// The two springs a panel's morph is made of — along the edge it is anchored to
/// and out from it — and the clock that steps them.
final class PanelMotion {
    private(set) var along = SpringValue(0)
    private(set) var outward = SpringValue(0)
    private var opening = false
    /// Whether the panel has reached its size on this opening: from then on the
    /// outward spring's swing is jelly, not growth.
    private var arrived = false
    private var clock: FrameClock?

    /// Each frame, where the panel is.
    var onStep: ((PanelPose) -> Void)?
    /// Once both springs are at rest; the argument is whether it is open.
    var onSettle: ((Bool) -> Void)?

    init(screen: NSScreen) {
        clock = FrameClock(screen: screen) { [weak self] dt in
            self?.step(dt) ?? false
        }
    }

    deinit { clock?.stop() }

    var isMoving: Bool { clock?.isRunning ?? false }

    var pose: PanelPose {
        if arrived {
            return PanelPose(along: along.value, reach: 1, wobble: outward.value - 1)
        }
        return PanelPose(along: along.value, reach: min(1, outward.value), wobble: 0)
    }

    func animate(open: Bool) {
        opening = open
        along.target = open ? 1 : 0
        outward.target = open ? 1 : 0
        if Motion.reduced {
            jump(open: open)
            return
        }
        clock?.start()
    }

    func jump(open: Bool) {
        opening = open
        along = SpringValue(open ? 1 : 0)
        outward = SpringValue(open ? 1 : 0)
        arrived = open
        clock?.stop()
        onStep?(pose)
        onSettle?(open)
    }

    private func step(_ dt: Double) -> Bool {
        along.advance(by: dt, spring: opening ? .panelAlong : .panelClose)
        outward.advance(by: dt, spring: opening ? .panelOutward : .panelClose)
        // Closing never goes past the pill; the pill is where it disappears.
        if !opening {
            along.value = max(0, along.value)
            outward.value = max(0, outward.value)
        }
        // Widening along the edge never overshoots on its own; the jelly's
        // squash is what moves the sides once the panel has arrived.
        along.value = min(1, along.value)
        if opening, outward.value >= 1 { arrived = true }
        if !opening, outward.value < 1 { arrived = false }
        // At rest once the wobble is under half a point: the last swings of a
        // loose spring take ages to die out and nobody can see them.
        let still = abs(outward.value - outward.target) < 0.004 && abs(outward.velocity) < 0.06
        // Closing is done once the panel is all but the pill: the last
        // fraction of a point of a critically damped spring takes as long as
        // the rest of the close, and it is exactly the bit that looked stuck.
        let gone = !opening && outward.value < 0.03 && along.value < 0.06
        let done = gone || (along.isSettled && still)
        if done {
            along.settle()
            outward.settle()
        }
        onStep?(pose)
        if done { onSettle?(opening) }
        return !done
    }
}

public enum Motion {
    /// The user asked macOS to reduce motion: panels appear in place.
    static var reduced: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }

    /// When a panel last started to open, so its rows can cascade in with it —
    /// and only then, not every time one scrolls back into view.
    static var lastReveal: CFTimeInterval = 0

    /// Start a fresh cascade: a panel opening, or a list swapping its contents.
    public static func reveal() { lastReveal = CACurrentMediaTime() }

    /// 0…1 ease for fading content in over part of a morph.
    static func ramp(_ value: CGFloat, from: CGFloat, to: CGFloat) -> CGFloat {
        let t = min(1, max(0, (value - from) / (to - from)))
        return t * t * (3 - 2 * t)
    }
}

// MARK: - SwiftUI

public extension Animation {
    /// The armed row or tab moving: lively, one small bounce.
    static let kSelect = Animation.spring(response: 0.34, dampingFraction: 0.64)
    /// The pointer over something: quick and springy.
    static let kHover = Animation.spring(response: 0.26, dampingFraction: 0.58)
    /// Things arriving, leaving and rearranging.
    static let kLayout = Animation.spring(response: 0.42, dampingFraction: 0.72)
    /// Something worth noticing — a new song, a play press.
    static let kBouncy = Animation.spring(response: 0.46, dampingFraction: 0.52)
}

/// Rows that arrive one after another, rising out of a blur, when their panel
/// opens or their list is swapped — the cascade the Linux shells do.
struct Cascade: ViewModifier {
    let index: Int
    /// Rows past this many arrive with the last of them, not later still.
    var cap = 14
    var distance: CGFloat = 16
    /// The side the rows come in from: the edge their panel grows out of.
    var from: Edge = .bottom

    @State private var shown = false
    /// Neon: the row's sideways glitch and flicker as it arrives.
    @State private var glitchX: CGFloat = 0
    @State private var flicker: Double = 1

    private var start: CGSize {
        switch from {
        case .bottom: return CGSize(width: 0, height: distance)
        case .top: return CGSize(width: 0, height: -distance)
        case .leading: return CGSize(width: -distance * 2, height: 0)
        case .trailing: return CGSize(width: distance * 2, height: 0)
        }
    }

    func body(content: Content) -> some View {
        if Neon.active {
            neonBody(content)
        } else {
            content
                .opacity(shown ? 1 : 0)
                .offset(shown ? .zero : start)
                .scaleEffect(shown ? 1 : 0.94)
                .blur(radius: shown ? 0 : 6)
                .onAppear {
                    // A row scrolled into view long after the panel opened is just
                    // there; only a fresh panel or list cascades.
                    let since = CACurrentMediaTime() - Motion.lastReveal
                    guard since < 0.6, !Motion.reduced else {
                        shown = true
                        return
                    }
                    let delay = 0.05 + Double(min(index, cap)) * 0.028
                    withAnimation(.kLayout.delay(delay)) { shown = true }
                }
        }
    }

    /// Neon: no easing at all. The row cuts in a little to one side (they
    /// alternate), flickers, and snaps into place in hard steps — a signal
    /// locking on, not something floating up.
    private func neonBody(_ content: Content) -> some View {
        content
            .opacity(shown ? flicker : 0)
            .offset(x: glitchX)
            .onAppear {
                let since = CACurrentMediaTime() - Motion.lastReveal
                guard since < 0.6, !Motion.reduced else {
                    shown = true
                    return
                }
                let side: CGFloat = index % 2 == 0 ? 1 : -1
                let delay = 0.12 + Double(min(index, cap)) * 0.022
                let steps: [(Double, CGFloat, Double)] = [
                    (0, 14 * side, 0.5), (0.03, -6 * side, 0.15), (0.06, 3 * side, 0.9), (0.09, 0, 0.6), (0.12, 0, 1),
                ]
                for (at, x, alpha) in steps {
                    DispatchQueue.main.asyncAfter(deadline: .now() + delay + at) {
                        shown = true
                        glitchX = x
                        flicker = alpha
                    }
                }
            }
    }
}

extension View {
    /// Cascade in as row `index` of a list that just opened.
    func cascade(_ index: Int, distance: CGFloat = 16, from edge: Edge = .bottom) -> some View {
        modifier(Cascade(index: index, distance: distance, from: edge))
    }

    /// A springy press: shrinks under the pointer and bounces back on release.
    func pressBounce() -> some View {
        buttonStyle(PressBounceStyle())
    }
}

struct PressBounceStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.86 : 1)
            .animation(configuration.isPressed ? .kHover : .kBouncy, value: configuration.isPressed)
    }
}
