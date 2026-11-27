import AppKit
import QuartzCore

/// How an Arcane panel comes and goes: summoned, then dispelled.
///
/// Opening: a summoning circle — two rings of runes and a star between them —
/// spins open where the panel will stand, motes of light streaming into it.
/// The panel materialises out of its heart as an orb and swells, unfurling into
/// its own scooped shape a little past its size and back; only once it is whole
/// does it latch onto the frame. Closing: it implodes back into an orb inside a
/// tightening circle, which bursts into sparks.
///
/// Unlike the jelly and Neon's extrusion, an Arcane panel isn't grown out of
/// its edge: it appears in place, out of the middle of where it will be.
final class ArcanePanelMotion: PanelTimeline {
    struct Frame: Equatable {
        /// 0…1 (and a little past) of the panel's size, about its centre.
        var size: CGFloat = 0
        /// 1 an orb, 0 the panel's own shape.
        var round: CGFloat = 1
        /// The circle's radius, as a fraction of the panel's half-width.
        var ring: CGFloat = 0
        var ringAlpha: CGFloat = 0
        /// Radians the circle has turned.
        var spin: CGFloat = 0
        /// Motes streaming into the circle.
        var gather = false
        /// The orb of light it implodes into, 0…1.
        var orb: CGFloat = 0
        var bloom: CGFloat = 0
        /// Whole, and fastened to the frame (its fillets drawn).
        var latched = false
        var closing = false
        /// Bumped each time the sparks should burst.
        var bursts = 0
    }

    static let swell = Spring(response: 0.42, damping: 0.66)
    /// The panel is gone by 0.32 s; the rest is the sparks flying.
    static let closeDuration: Double = 0.95
    /// Room for the swell past full size and the circle.
    static let slack: (out: CGFloat, side: CGFloat) = (40, 40)
    var slack: (out: CGFloat, side: CGFloat) { Self.slack }

    private(set) var frame = Frame()
    var onStep: (() -> Void)?
    var onSettle: ((Bool) -> Void)?
    private let arcaneLights = ArcanePanelLights()
    var lights: CALayer { arcaneLights.layer }

    private var clock: FrameClock?
    private var opening = false
    private var t: Double = 0
    private var size = SpringValue(0)
    private var arrivedAt: Double?
    private var closeFrom = Frame()
    /// The panel's resting rectangle, from the last layout — the circle is
    /// sized to it.
    private var full: NSRect = .zero

    init(screen: NSScreen) {
        clock = FrameClock(screen: screen) { [weak self] dt in self?.step(dt) ?? false }
    }

    deinit { clock?.stop() }

    func open() {
        opening = true
        t = 0
        arrivedAt = nil
        size = SpringValue(0)
        size.target = 1
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
        arcaneLights.clear()
    }

    private static func clamp(_ x: Double) -> Double { min(1, max(0, x)) }
    private static func easeOut(_ x: Double) -> Double { 1 - pow(1 - clamp(x), 3) }
    private static func easeIn(_ x: Double) -> Double { pow(clamp(x), 3) }
    private static func smooth(_ x: Double) -> Double { let c = clamp(x); return c * c * (3 - 2 * c) }

    private func step(_ dt: Double) -> Bool {
        t += dt
        var f = frame
        var done = false
        if opening {
            f.closing = false
            // the circle spins open, fast at first
            f.ring = CGFloat(Self.easeOut(t / 0.3))
            f.spin += CGFloat(dt * (7 - 5 * Double(min(1, f.size))))
            f.gather = t < 0.3
            // the panel swells out of it, a beat later
            if t > 0.13 { size.advance(by: dt, spring: Self.swell) }
            f.size = CGFloat(max(0, size.value))
            f.round = CGFloat(1 - Self.smooth(Double(f.size) * 1.08))
            // the circle fades as the panel outgrows it
            f.ringAlpha = CGFloat(Self.clamp(t / 0.08) * (1 - Self.smooth((Double(f.size) - 0.55) / 0.4)))
            if arrivedAt == nil, size.value >= 1 {
                arrivedAt = t
                f.latched = true
            }
            if let arrived = arrivedAt {
                let still = abs(size.value - 1) < 0.003 && abs(size.velocity) < 0.05
                done = t - arrived > 0.12 && still
            }
            if done {
                size.settle()
                f = Frame(size: 1, round: 0, latched: true)
            }
        } else {
            f = closeFrom
            f.closing = true
            f.latched = false
            f.gather = false
            // imploding into an orb
            let fall = Self.easeIn(t / 0.3)
            f.size = closeFrom.size * CGFloat(1 - fall)
            f.round = CGFloat(max(Double(closeFrom.round), Self.easeOut(t / 0.2)))
            // the circle comes back and tightens round it, spinning harder
            f.ringAlpha = CGFloat(Self.clamp(t / 0.1) * (1 - Self.clamp((t - 0.3) / 0.08)))
            f.ring = CGFloat(1 - 0.85 * Self.easeIn(t / 0.32))
            f.spin += CGFloat(dt * (4 + 14 * Self.clamp(t / 0.3)))
            f.bloom = CGFloat(Self.clamp(t / 0.25) * 0.6 * (1 - Self.clamp((t - 0.28) / 0.04)))
            f.orb = CGFloat(sin(.pi * Self.clamp((t - 0.18) / 0.3)))
            // and bursts
            f.bursts = closeFrom.bursts + (t >= 0.3 ? 1 : 0)
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
        let s = max(0.015, frame.size)
        let side = min(full.width, full.height)
        let shape = 1 - frame.round
        // an orb at first: both sides start from the same small circle and
        // take on the panel's proportions as it unfurls
        let width = s * (side + (full.width - side) * shape)
        let height = s * (side + (full.height - side) * shape)
        let rect = NSRect(x: full.midX - width / 2, y: full.midY - height / 2, width: width, height: height)

        // One corner value slides from round (an orb) through square to the
        // scoop, so the shape never pops from one kind of corner to the other.
        let corner = Double(min(width, height) / 2) * Double(frame.round) - Double(style.notch) * Double(shape)
        let outline = PanelStyle(
            radius: CGFloat(max(0, corner)),
            filletRadius: frame.latched ? style.filletRadius : 0,
            attachment: frame.latched ? style.attachment : .none,
            notch: CGFloat(max(0, -corner))
        )
        let gone = frame.closing ? CGFloat(1 - Self.clamp((t - 0.24) / 0.08)) : 1
        return PanelTimelineLayout(
            rect: rect,
            style: outline,
            squash: true,
            hostAlpha: frame.closing
                ? CGFloat(1 - Self.easeIn(t / 0.3) * 0.6) * gone
                : Motion.ramp(frame.size, from: 0.35, to: 0.9),
            glassFade: frame.closing ? gone : Motion.ramp(frame.size, from: 0, to: 0.2),
            chromeAlpha: frame.closing ? gone : Motion.ramp(frame.size, from: 0.5, to: 1),
            bloom: frame.bloom,
            bloomColour: Arcane.violet
        )
    }

    func placeLights(rect: CGRect, edge: OverlayEdge) {
        let side = min(full.width, full.height)
        arcaneLights.place(frame, centre: CGPoint(x: rect.midX, y: rect.midY), radius: side * 0.42)
    }
}

/// The circle, the motes and the sparks of an Arcane panel's coming and going.
///
/// The circle is drawn once at a fixed size and scaled and turned by its
/// transform each frame, so nothing is re-stroked or re-blurred as it moves.
final class ArcanePanelLights {
    let layer = CALayer()
    private let circle = CALayer()
    private let outer = CAShapeLayer()
    private let inner = CAShapeLayer()
    private let star = CAShapeLayer()
    private let gather = CAEmitterLayer()
    private let burst = CAEmitterLayer()
    private let orb = CAGradientLayer()
    private var lastBursts = 0
    /// The size the circle is drawn at; it is scaled from this.
    private static let unit: CGFloat = 100

    init() {
        layer.masksToBounds = false
        let u = Self.unit
        circle.bounds = CGRect(x: 0, y: 0, width: u * 2, height: u * 2)
        circle.opacity = 0

        func ring(_ shape: CAShapeLayer, radius: CGFloat, colour: NSColor, width: CGFloat, dashes: [NSNumber]) {
            let path = CGPath(ellipseIn: CGRect(x: u - radius, y: u - radius, width: radius * 2, height: radius * 2), transform: nil)
            shape.frame = circle.bounds
            shape.path = path
            shape.fillColor = nil
            shape.strokeColor = colour.cgColor
            shape.lineWidth = width
            shape.lineDashPattern = dashes
            shape.shadowColor = colour.cgColor
            shape.shadowRadius = 5
            shape.shadowOpacity = 1
            shape.shadowOffset = .zero
            shape.shadowPath = path.copy(strokingWithWidth: width + 1, lineCap: .round, lineJoin: .round, miterLimit: 4)
            circle.addSublayer(shape)
        }
        // runes round the rim: long and short marks
        ring(outer, radius: u * 0.96, colour: Arcane.rose, width: 2.2, dashes: [2, 5, 12, 5, 4, 5])
        ring(inner, radius: u * 0.74, colour: Arcane.violet, width: 1.2, dashes: [1, 3])

        // a six-pointed star between them
        let starPath = CGMutablePath()
        for offset in [0.0, Double.pi / 3] {
            for k in 0..<3 {
                let a = offset + Double(k) * 2 * .pi / 3 + .pi / 2
                let p = CGPoint(x: u + u * 0.72 * CGFloat(cos(a)), y: u + u * 0.72 * CGFloat(sin(a)))
                if k == 0 { starPath.move(to: p) } else { starPath.addLine(to: p) }
            }
            starPath.closeSubpath()
        }
        star.frame = circle.bounds
        star.path = starPath
        star.fillColor = nil
        star.strokeColor = Arcane.violet.withAlphaComponent(0.7).cgColor
        star.lineWidth = 1
        star.shadowColor = Arcane.violet.cgColor
        star.shadowRadius = 4
        star.shadowOpacity = 1
        star.shadowOffset = .zero
        star.shadowPath = starPath.copy(strokingWithWidth: 2, lineCap: .round, lineJoin: .round, miterLimit: 4)
        circle.addSublayer(star)
        layer.addSublayer(circle)

        // motes streaming in from a ring outside the circle
        gather.emitterShape = .circle
        gather.emitterMode = .outline
        gather.renderMode = .additive
        gather.birthRate = 0
        gather.emitterCells = [Arcane.mote, Arcane.twinkle].enumerated().map { index, image in
            let cell = Arcane.cell(image, colour: index == 0 ? Arcane.violet : Arcane.rose,
                                   birthRate: 90, lifetime: 0.4, velocity: 0, scale: index == 0 ? 0.24 : 0.3)
            cell.name = "in\(index)"
            cell.yAcceleration = 0
            cell.emissionRange = 0
            cell.velocityRange = 0
            return cell
        }
        layer.addSublayer(gather)

        // sparks flung out when it bursts
        burst.emitterShape = .point
        burst.renderMode = .additive
        burst.birthRate = 0
        burst.emitterCells = Arcane.cells(rate: 300, velocity: 190, lifetime: 0.8, scale: 0.45).map { cell in
            cell.emissionRange = .pi * 2
            cell.yAcceleration = -60
            return cell
        }
        layer.addSublayer(burst)

        orb.type = .radial
        orb.colors = [
            NSColor.white.cgColor,
            Arcane.rose.withAlphaComponent(0.85).cgColor,
            Arcane.violet.withAlphaComponent(0.35).cgColor,
            Arcane.violet.withAlphaComponent(0).cgColor,
        ]
        orb.locations = [0, 0.25, 0.55, 1]
        orb.startPoint = CGPoint(x: 0.5, y: 0.5)
        orb.endPoint = CGPoint(x: 1, y: 1)
        orb.opacity = 0
        layer.addSublayer(orb)
    }

    func place(_ frame: ArcanePanelMotion.Frame, centre: CGPoint, radius: CGFloat) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        let r = max(1, radius * frame.ring)
        circle.position = centre
        circle.opacity = Float(frame.ringAlpha)
        circle.transform = CATransform3DMakeScale(r / Self.unit, r / Self.unit, 1)
        outer.transform = CATransform3DMakeRotation(frame.spin, 0, 0, 1)
        inner.transform = CATransform3DMakeRotation(-frame.spin * 1.4, 0, 0, 1)
        star.transform = CATransform3DMakeRotation(frame.spin * 0.5, 0, 0, 1)

        // the motes start a little outside the circle and fall into its heart
        let reach = r * 1.5
        gather.emitterPosition = centre
        gather.emitterSize = CGSize(width: reach * 2, height: reach * 2)
        // attached cells only change through the emitter's key paths
        for index in 0..<2 {
            gather.setValue(-reach / 0.4, forKeyPath: "emitterCells.in\(index).velocity")
        }
        gather.birthRate = frame.gather ? 1 : 0

        burst.emitterPosition = centre
        // A burst is a moment of birth rate. Not `Arcane.burst`: that restarts
        // the emitter, which clears whatever it has already flung.
        if frame.bursts > lastBursts {
            burst.birthRate = 1
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.07) { [burst] in burst.birthRate = 0 }
        }
        lastBursts = frame.bursts

        orb.opacity = Float(frame.orb)
        let size = 24 + 70 * frame.orb
        orb.frame = CGRect(x: centre.x - size / 2, y: centre.y - size / 2, width: size, height: size)
    }

    func clear() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        circle.opacity = 0
        gather.birthRate = 0
        burst.birthRate = 0
        orb.opacity = 0
        CATransaction.commit()
        lastBursts = 0
    }
}
