import AppKit
import QuartzCore

/// How an Abyss panel comes and goes: it rises out of the deep, and sinks.
///
/// Opening: the panel surfaces from below where it will rest — small and dim
/// at first, as if far down — with a flash of bioluminescence and a stream of
/// bubbles rising off it, its far edge rolling in a wave that calms as it
/// settles. When it arrives a ring of light pulses out from it, the way a
/// jellyfish glows. Closing: it sinks, darkening, its edge churning, and
/// bursts into bubbles that rise where it was.
final class AbyssPanelMotion: PanelTimeline {
    struct Frame: Equatable {
        /// How far below its rest the panel is, in points.
        var depth: CGFloat = 40
        /// Its size, about its centre.
        var size: CGFloat = 0.9
        /// The wave on its far edge: how deep, and where in its cycle.
        var ripple: CGFloat = 0
        var phase: CGFloat = 0
        var bloom: CGFloat = 0
        /// Bubbles streaming off it.
        var bubbling = false
        /// The arrival pulse, 0…1 (0 none).
        var pulse: CGFloat = 0
        var latched = false
        var closing = false
        var bursts = 0
    }

    static let rise = Spring(response: 0.5, damping: 0.74)
    static let depth: CGFloat = 40
    static let closeDuration: Double = 1.05
    static let slack: (out: CGFloat, side: CGFloat) = (48, 48)
    var slack: (out: CGFloat, side: CGFloat) { Self.slack }

    private(set) var frame = Frame()
    var onStep: (() -> Void)?
    var onSettle: ((Bool) -> Void)?
    private let abyssLights = AbyssPanelLights()
    var lights: CALayer { abyssLights.layer }

    private var clock: FrameClock?
    private var opening = false
    private var t: Double = 0
    /// 1 at the bottom, 0 at rest.
    private var sunk = SpringValue(1)
    private var arrivedAt: Double?
    private var closeFrom = Frame()
    private var full: NSRect = .zero

    init(screen: NSScreen) {
        clock = FrameClock(screen: screen) { [weak self] dt in self?.step(dt) ?? false }
    }

    deinit { clock?.stop() }

    func open() {
        opening = true
        t = 0
        arrivedAt = nil
        sunk = SpringValue(1)
        sunk.target = 0
        frame = Frame()
        clock?.start()
    }

    func close() {
        opening = false
        t = 0
        closeFrom = frame
        clock?.start()
    }

    func reset() {
        clock?.stop()
        frame = Frame()
        abyssLights.clear()
    }

    private static func clamp(_ x: Double) -> Double { min(1, max(0, x)) }
    private static func easeIn(_ x: Double) -> Double { pow(clamp(x), 2.4) }
    private static func smooth(_ x: Double) -> Double { let c = clamp(x); return c * c * (3 - 2 * c) }
    private static func bell(_ x: Double) -> Double { sin(.pi * clamp(x)) }

    private func step(_ dt: Double) -> Bool {
        t += dt
        var f = frame
        var done = false
        if opening {
            f.closing = false
            sunk.advance(by: dt, spring: Self.rise)
            f.depth = Self.depth * CGFloat(sunk.value)
            f.size = CGFloat(1 - 0.1 * max(0, sunk.value))
            // the wave calms as it settles
            f.ripple = Abyss.panelRipple + CGFloat(10 * exp(-3.4 * t))
            f.phase += CGFloat(dt * (4 + 6 * exp(-2 * t)))
            f.bloom = CGFloat(0.5 * (1 - Self.smooth(t / 0.5)))
            f.bubbling = t < 0.45
            if arrivedAt == nil, sunk.value < 0.02 {
                arrivedAt = t
                f.latched = true
            }
            if let arrived = arrivedAt {
                f.pulse = CGFloat(Self.clamp((t - arrived) / 0.75))
                done = t - arrived > 0.75 && sunk.isSettled
            }
            if done {
                sunk.settle()
                f = Frame(depth: 0, size: 1, ripple: Abyss.panelRipple, phase: f.phase, latched: true)
            }
        } else {
            f = closeFrom
            f.closing = true
            f.latched = false
            f.bubbling = false
            f.pulse = 0
            let fall = Self.easeIn(t / 0.4)
            f.depth = closeFrom.depth + (Self.depth + 6) * CGFloat(fall)
            f.size = closeFrom.size * CGFloat(1 - 0.1 * fall)
            f.ripple = Abyss.panelRipple + CGFloat(8 * Self.bell(t / 0.42))
            f.phase += CGFloat(dt * 11)
            f.bloom = CGFloat(0.3 * Self.bell(t / 0.3))
            f.bursts = closeFrom.bursts + 1
            done = t >= Self.closeDuration
            if done { f = Frame() }
        }
        frame = f
        onStep?()
        if done { onSettle?(opening) }
        return !done
    }

    func layout(full: NSRect, edge: OverlayEdge, style: PanelStyle) -> PanelTimelineLayout {
        self.full = full
        let s = max(0.01, frame.size)
        let width = full.width * s, height = full.height * s
        // deeper is lower on screen, whichever edge the panel rests on
        let rect = NSRect(x: full.midX - width / 2, y: full.midY - height / 2 - frame.depth,
                          width: width, height: height)
        var outline = style
        outline.ripple = frame.ripple
        outline.ripplePhase = frame.phase
        if !frame.latched { outline.filletRadius = 0 }
        let near = 1 - frame.depth / Self.depth
        let gone = frame.closing ? CGFloat(1 - Self.smooth(t / 0.34)) : 1
        return PanelTimelineLayout(
            rect: rect,
            style: outline,
            squash: false,
            hostAlpha: frame.closing ? gone : Motion.ramp(near, from: 0.1, to: 0.75),
            glassFade: frame.closing ? gone : Motion.ramp(near, from: 0, to: 0.35),
            chromeAlpha: frame.closing ? gone : Motion.ramp(near, from: 0.3, to: 0.9),
            bloom: frame.bloom,
            bloomColour: Abyss.teal
        )
    }

    func placeLights(rect: CGRect, edge: OverlayEdge) {
        abyssLights.place(frame, rect: rect)
    }
}

/// The light and bubbles of an Abyss panel's coming and going.
final class AbyssPanelLights {
    let layer = CALayer()
    private let stream = CAEmitterLayer()
    private let burst = CAEmitterLayer()
    private let ring = CAShapeLayer()
    private let ringGlow = CAShapeLayer()
    private var lastBursts = 0
    private var ringRect: CGRect = .null

    init() {
        layer.masksToBounds = false

        stream.emitterShape = .line
        stream.renderMode = .additive
        stream.birthRate = 0
        stream.emitterCells = Abyss.bubbles(rate: 70, velocity: 90, lifetime: 1.1, scale: 0.3)
        layer.addSublayer(stream)

        burst.emitterShape = .rectangle
        burst.emitterMode = .volume
        burst.renderMode = .additive
        burst.birthRate = 0
        burst.emitterCells = Abyss.bubbles(rate: 240, velocity: 110, lifetime: 1.2, scale: 0.36)
        layer.addSublayer(burst)

        for (shape, width, alpha) in [(ringGlow, CGFloat(12), CGFloat(0.2)), (ring, CGFloat(2), CGFloat(0.9))] {
            shape.fillColor = nil
            shape.strokeColor = Abyss.teal.withAlphaComponent(alpha).cgColor
            shape.lineWidth = width
            shape.opacity = 0
            layer.addSublayer(shape)
        }
    }

    func place(_ frame: AbyssPanelMotion.Frame, rect: CGRect) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        stream.emitterPosition = CGPoint(x: rect.midX, y: rect.minY + 6)
        stream.emitterSize = CGSize(width: rect.width * 0.8, height: 1)
        stream.birthRate = frame.bubbling ? 1 : 0

        burst.emitterPosition = CGPoint(x: rect.midX, y: rect.midY)
        burst.emitterSize = rect.size
        if frame.bursts > lastBursts {
            burst.birthRate = 1
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) { [burst] in burst.birthRate = 0 }
        }
        lastBursts = frame.bursts

        // the arrival pulse: a ring of light going out from the panel's edge
        if frame.pulse > 0.001 && frame.pulse < 0.999 {
            if ringRect != rect {
                ringRect = rect
                let path = CGPath(roundedRect: CGRect(origin: .zero, size: rect.size),
                                  cornerWidth: 22, cornerHeight: 22, transform: nil)
                for shape in [ring, ringGlow] {
                    shape.bounds = CGRect(origin: .zero, size: rect.size)
                    shape.path = path
                }
            }
            // out by the same number of points on every side
            let reach = 26 * frame.pulse
            for shape in [ring, ringGlow] {
                shape.position = CGPoint(x: rect.midX, y: rect.midY)
                shape.transform = CATransform3DMakeScale(1 + 2 * reach / max(1, rect.width),
                                                         1 + 2 * reach / max(1, rect.height), 1)
                shape.opacity = Float((1 - frame.pulse) * 0.9)
            }
        } else {
            ring.opacity = 0
            ringGlow.opacity = 0
        }
    }

    func clear() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        stream.birthRate = 0
        burst.birthRate = 0
        ring.opacity = 0
        ringGlow.opacity = 0
        CATransaction.commit()
        lastBursts = 0
    }
}
