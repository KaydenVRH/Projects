import AppKit
import QuartzCore

/// How a Neon panel comes and goes — a timeline, not jelly.
///
/// Opening: a line ignites along the edge, spreading from its centre; the panel
/// extrudes out of it on a snappy spring with a scan head riding its leading
/// edge; it lands with a few hard digital steps sideways, the way a signal
/// locks on. Closing is an old CRT switching off: the picture brightens as it
/// squashes into a line through its middle, the line pinches into a dot, and
/// the dot flashes out.
///
/// Stepped every display frame like `PanelMotion`, for the same reason: the
/// panel's glass lives in the frame's window and follows model geometry only.
final class NeonPanelMotion {
    struct Frame: Equatable {
        /// 0…1 along the edge.
        var length: CGFloat = 0
        /// 0…1 out from it (a little past 1 as it lands).
        var depth: CGFloat = 0
        /// Points sideways, along the edge — the lock-on jitter.
        var jitter: CGFloat = 0
        /// Closing: 0…1 squashed toward the centre line.
        var collapse: CGFloat = 0
        /// Closing: 0…1 the line pinched toward a dot.
        var shrink: CGFloat = 0
        /// The scan head on the leading edge, 0…1.
        var head: CGFloat = 0
        /// The CRT line's brightness, 0…1.
        var flare: CGFloat = 0
        /// The last dot's brightness, 0…1.
        var dot: CGFloat = 0
        /// How white the picture runs as it collapses, 0…1.
        var bloom: CGFloat = 0

        var closing: Bool { collapse > 0 || shrink > 0 }
    }

    static let extrude = Spring(response: 0.3, damping: 0.62)
    static let closeDuration: Double = 0.36

    private(set) var frame = Frame()
    var onStep: ((Frame) -> Void)?
    /// Once it has landed (true) or gone (false).
    var onSettle: ((Bool) -> Void)?

    private var clock: FrameClock?
    private var opening = false
    private var t: Double = 0
    private var depth = SpringValue(0)
    private var arrivedAt: Double?
    private var closeFrom = Frame()

    init(screen: NSScreen) {
        clock = FrameClock(screen: screen) { [weak self] dt in self?.step(dt) ?? false }
    }

    deinit { clock?.stop() }

    func open() {
        opening = true
        t = 0
        arrivedAt = nil
        depth = SpringValue(0)
        depth.target = 1
        frame = Frame()
        clock?.start()
    }

    func close() {
        opening = false
        t = 0
        closeFrom = frame
        closeFrom.jitter = 0
        closeFrom.head = 0
        clock?.start()
    }

    func reset() {
        clock?.stop()
        frame = Frame()
    }

    private static func clamp(_ x: Double) -> Double { min(1, max(0, x)) }
    private static func easeOut(_ x: Double) -> Double { 1 - pow(1 - clamp(x), 3) }
    private static func easeIn(_ x: Double) -> Double { pow(clamp(x), 3) }

    private func step(_ dt: Double) -> Bool {
        t += dt
        var f: Frame
        var done = false
        if opening {
            f = Frame()
            // ignite: the line spreads from the middle of the edge
            f.length = CGFloat(0.03 + 0.97 * Self.easeOut(t / 0.15))
            // extrude, a beat after it ignites
            if t > 0.07 { depth.advance(by: dt, spring: Self.extrude) }
            f.depth = CGFloat(max(0, depth.value))
            if arrivedAt == nil, depth.value >= 1 { arrivedAt = t }
            if let arrived = arrivedAt {
                let since = t - arrived
                f.head = CGFloat(max(0, 1 - since / 0.22))
                // locking on: hard steps, no easing between them
                let steps: [(Double, CGFloat)] = [(0, 7), (0.035, -5), (0.07, 3), (0.1, -1.5), (0.13, 0)]
                f.jitter = steps.last { since >= $0.0 }?.1 ?? 0
                let still = abs(depth.value - 1) < 0.003 && abs(depth.velocity) < 0.05
                done = since > 0.2 && still
            } else {
                f.head = CGFloat(Self.clamp(t / 0.06))
            }
            if done {
                depth.settle()
                f = Frame(length: 1, depth: 1)
            }
        } else {
            f = closeFrom
            f.collapse = CGFloat(Self.easeIn(t / 0.14))
            f.shrink = CGFloat(Self.easeIn((t - 0.12) / 0.12))
            f.bloom = CGFloat(Self.clamp(t / 0.12)) * (1 - f.shrink)
            f.flare = CGFloat(Self.clamp(t / 0.1) * (1 - Self.clamp((t - 0.22) / 0.06)))
            f.dot = CGFloat(sin(.pi * Self.clamp((t - 0.19) / 0.17)))
            done = t >= Self.closeDuration
            if done { f = Frame() }
        }
        frame = f
        onStep?(f)
        if done { onSettle?(opening) }
        return !done
    }
}

/// The light a Neon panel's motion throws, drawn outside its outline (so it
/// isn't clipped): the scan head along the leading edge as it extrudes, and the
/// CRT's bright line and last dot as it switches off.
final class NeonPanelLights {
    let layer = CALayer()
    private let head = CAGradientLayer()
    private let line = CAGradientLayer()
    private let dot = CALayer()

    init() {
        layer.masksToBounds = false
        for gradient in [head, line] {
            gradient.colors = [
                Neon.cyan.withAlphaComponent(0).cgColor,
                NSColor.white.cgColor,
                NSColor.white.cgColor,
                Neon.magenta.withAlphaComponent(0).cgColor,
            ]
            gradient.locations = [0, 0.2, 0.8, 1]
            gradient.startPoint = CGPoint(x: 0, y: 0.5)
            gradient.endPoint = CGPoint(x: 1, y: 0.5)
            gradient.shadowColor = Neon.cyan.cgColor
            gradient.shadowRadius = 6
            gradient.shadowOpacity = 1
            gradient.shadowOffset = .zero
            gradient.opacity = 0
            layer.addSublayer(gradient)
        }
        dot.backgroundColor = NSColor.white.cgColor
        dot.cornerRadius = 5
        dot.shadowColor = Neon.cyan.cgColor
        dot.shadowRadius = 12
        dot.shadowOpacity = 1
        dot.shadowOffset = .zero
        dot.opacity = 0
        layer.addSublayer(dot)
    }

    /// Place the lights for `frame` round `rect` (in the window's coordinates),
    /// for a panel growing out of `edge`.
    func place(_ frame: NeonPanelMotion.Frame, rect: CGRect, edge: OverlayEdge) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        // the scan head rides the edge furthest from the anchor
        head.opacity = Float(frame.head)
        if frame.head > 0 {
            let thickness: CGFloat = 2
            switch edge {
            case .bottom: head.frame = CGRect(x: rect.minX, y: rect.maxY - thickness, width: rect.width, height: thickness)
            case .top: head.frame = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: thickness)
            case .leading: head.frame = CGRect(x: rect.maxX - thickness, y: rect.minY, width: thickness, height: rect.height)
            case .trailing: head.frame = CGRect(x: rect.minX, y: rect.minY, width: thickness, height: rect.height)
            }
            let vertical = edge == .leading || edge == .trailing
            head.startPoint = vertical ? CGPoint(x: 0.5, y: 0) : CGPoint(x: 0, y: 0.5)
            head.endPoint = vertical ? CGPoint(x: 0.5, y: 1) : CGPoint(x: 1, y: 0.5)
            head.shadowPath = CGPath(rect: head.bounds, transform: nil)
        }

        // the CRT line across the middle, a little wider than what's left
        line.opacity = Float(frame.flare)
        if frame.flare > 0 {
            let width = rect.width + 24 * frame.flare
            line.frame = CGRect(x: rect.midX - width / 2, y: rect.midY - 1.5, width: width, height: 3)
            line.startPoint = CGPoint(x: 0, y: 0.5)
            line.endPoint = CGPoint(x: 1, y: 0.5)
            line.shadowPath = CGPath(rect: line.bounds, transform: nil)
        }

        dot.opacity = Float(frame.dot)
        if frame.dot > 0 {
            let size = 6 + 8 * frame.dot
            dot.frame = CGRect(x: rect.midX - size / 2, y: rect.midY - size / 2, width: size, height: size)
            dot.cornerRadius = size / 2
            dot.shadowPath = CGPath(ellipseIn: dot.bounds, transform: nil)
        }
    }

    func clear() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        head.opacity = 0
        line.opacity = 0
        dot.opacity = 0
        CATransaction.commit()
    }
}
