import AppKit
import QuartzCore

/// How an Eclipse panel comes and goes: it waxes, and it wanes.
///
/// Opening: the panel is there from the start but hidden behind the moon,
/// only a corona of light showing round where it will be. A diamond-ring flash
/// breaks out on the far side, and the moon slides off toward the bitten
/// corner, the panel waxing in behind a gold edge of sunlight — the moon's
/// limb — as the corona fades into the day. Closing: the moon slides back
/// across it; at totality the corona flares and the panel bursts into stars.
///
/// The moon is `PanelStyle.shade`: the outline itself has the disc taken out
/// of it, so the glass, the content and the chrome all wax and wane together.
final class EclipsePanelMotion: PanelTimeline {
    struct Frame: Equatable {
        /// The moon across the panel, 1 covering it, 0 gone.
        var shade: CGFloat = 1
        /// The panel's size, about its centre.
        var size: CGFloat = 0.94
        /// The corona round the hidden panel, 0…1, and its ring's radius as a
        /// fraction of the panel's half-side.
        var corona: CGFloat = 0
        var coronaScale: CGFloat = 0.5
        /// The diamond ring's flash, 0…1.
        var flash: CGFloat = 0
        /// The moon's lit edge, 0…1.
        var limb: CGFloat = 0
        var bloom: CGFloat = 0
        var latched = false
        var closing = false
        var bursts = 0
    }

    static let settle = Spring(response: 0.4, damping: 0.72)
    static let closeDuration: Double = 0.9
    static let slack: (out: CGFloat, side: CGFloat) = (36, 36)
    var slack: (out: CGFloat, side: CGFloat) { Self.slack }

    private(set) var frame = Frame()
    var onStep: (() -> Void)?
    var onSettle: ((Bool) -> Void)?
    private let eclipseLights = EclipsePanelLights()
    var lights: CALayer { eclipseLights.layer }

    private var clock: FrameClock?
    private var opening = false
    private var t: Double = 0
    private var size = SpringValue(0.94)
    private var closeFrom = Frame()
    private var full: NSRect = .zero
    private var style = PanelStyle()

    init(screen: NSScreen) {
        clock = FrameClock(screen: screen) { [weak self] dt in self?.step(dt) ?? false }
    }

    deinit { clock?.stop() }

    func open() {
        opening = true
        t = 0
        size = SpringValue(0.94)
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
        eclipseLights.clear()
    }

    private static func clamp(_ x: Double) -> Double { min(1, max(0, x)) }
    private static func easeOut(_ x: Double) -> Double { 1 - pow(1 - clamp(x), 3) }
    private static func easeIn(_ x: Double) -> Double { pow(clamp(x), 3) }
    private static func smooth(_ x: Double) -> Double { let c = clamp(x); return c * c * (3 - 2 * c) }
    private static func bell(_ x: Double) -> Double { sin(.pi * clamp(x)) }

    private func step(_ dt: Double) -> Bool {
        t += dt
        var f = frame
        var done = false
        if opening {
            f.closing = false
            // the corona round the hidden panel, then fading into the day
            f.corona = CGFloat(Self.clamp(t / 0.12) * (1 - Self.smooth((t - 0.22) / 0.4)))
            f.coronaScale = CGFloat(0.55 + 0.55 * Self.easeOut(t / 0.65))
            // the diamond ring, just before the moon moves
            f.flash = CGFloat(Self.bell((t - 0.08) / 0.24))
            // the moon slides off, the panel waxing in behind it
            let reveal = Self.easeOut((t - 0.14) / 0.55)
            f.shade = CGFloat(1 - reveal)
            f.limb = CGFloat(Self.bell(reveal * 0.98 + 0.02) * Self.clamp(t / 0.16))
            if t > 0.1 { size.advance(by: dt, spring: Self.settle) }
            f.size = CGFloat(size.value)
            f.bloom = f.flash * 0.22
            f.latched = f.shade < 0.01
            done = f.latched && t > 0.72 && size.isSettled
            if done {
                size.settle()
                f = Frame(shade: 0, size: 1, latched: true)
            }
        } else {
            f = closeFrom
            f.closing = true
            f.latched = false
            f.flash = 0
            // the moon slides back over it
            let cover = Self.easeIn(t / 0.36)
            f.shade = CGFloat(max(Double(closeFrom.shade), cover))
            f.limb = CGFloat(Self.bell(cover))
            f.size = closeFrom.size * CGFloat(1 - 0.05 * cover)
            // totality: the corona flares, and the stars burst
            let flare = Self.clamp((t - 0.3) / 0.5)
            f.corona = CGFloat(Self.bell(flare))
            f.coronaScale = CGFloat(0.6 + 0.75 * Self.easeOut(flare))
            f.bloom = CGFloat(Self.bell(t / 0.36) * 0.35)
            f.bursts = closeFrom.bursts + (t >= 0.33 ? 1 : 0)
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
        let rect = NSRect(x: full.midX - full.width * s / 2, y: full.midY - full.height * s / 2,
                          width: full.width * s, height: full.height * s)
        var outline = style
        outline.shade = frame.shade
        // Fillets only once whole and fastened; the attachment stays, so the
        // moon keeps one direction all the way through.
        if !frame.latched { outline.filletRadius = 0 }
        self.style = outline
        let shown = 1 - frame.shade
        let gone = frame.closing ? CGFloat(1 - Self.clamp((t - 0.34) / 0.06)) : 1
        return PanelTimelineLayout(
            rect: rect,
            style: outline,
            squash: false,
            hostAlpha: frame.closing ? gone : Motion.ramp(shown, from: 0.02, to: 0.5),
            glassFade: frame.closing ? gone : Motion.ramp(shown, from: 0, to: 0.12),
            chromeAlpha: frame.closing ? gone : Motion.ramp(shown, from: 0.2, to: 0.8),
            bloom: frame.bloom,
            bloomColour: Eclipse.gold
        )
    }

    func placeLights(rect: CGRect, edge: OverlayEdge) {
        let side = min(full.width, full.height)
        eclipseLights.place(frame, rect: rect, style: style, side: side)
    }
}

/// The light of an Eclipse panel's coming and going: the corona round it, the
/// diamond ring, the gold limb of the moon crossing it, and the stars it
/// bursts into.
///
/// The limb stays inside the panel, so it rides in the panel's window. The
/// corona, the diamond and the stars reach far past the panel — the closing
/// flare is twice its size — and its window only has a little slack round it,
/// which cut them off square. They're drawn in a click-through window over
/// the whole screen instead, ordered in only while there is light in it.
final class EclipsePanelLights {
    let layer = CALayer()
    private var sky: NSPanel?
    private let skyLayer = CALayer()
    /// Bumped whenever the sky is wanted again, so a pending put-away of an
    /// earlier one knows to stand down.
    private var skyStamp = 0
    private var puttingAway = false
    private let corona = CALayer()
    private let diamond = CALayer()
    /// The limb, kept to the panel's own outline.
    private let limbBox = CALayer()
    private let limbMask = CAShapeLayer()
    private let limb = CAShapeLayer()
    private let limbGlow = CAShapeLayer()
    private let burst = CAEmitterLayer()
    private var lastBursts = 0
    private var moonRadius: CGFloat = 0
    /// The size the corona layer is drawn at; it is scaled from this.
    private static let unit: CGFloat = 400

    init() {
        layer.masksToBounds = false
        corona.contents = Eclipse.coronaImage
        corona.bounds = CGRect(x: 0, y: 0, width: Self.unit, height: Self.unit)
        corona.opacity = 0
        skyLayer.addSublayer(corona)

        limbMask.fillColor = NSColor.black.cgColor
        limbBox.mask = limbMask
        for (shape, width, alpha) in [(limbGlow, CGFloat(10), CGFloat(0.22)), (limb, CGFloat(2), CGFloat(1))] {
            shape.fillColor = nil
            shape.strokeColor = Eclipse.gold.withAlphaComponent(alpha).cgColor
            shape.lineWidth = width
            limbBox.addSublayer(shape)
        }
        limbBox.opacity = 0
        layer.addSublayer(limbBox)

        diamond.contents = Arcane.twinkle
        diamond.bounds = CGRect(x: 0, y: 0, width: 90, height: 90)
        diamond.opacity = 0
        skyLayer.addSublayer(diamond)

        burst.emitterShape = .circle
        burst.emitterMode = .outline
        burst.renderMode = .additive
        burst.birthRate = 0
        burst.emitterCells = Eclipse.stars(rate: 260, velocity: 150, lifetime: 0.9, scale: 0.4).map { cell in
            cell.emissionRange = .pi * 2
            cell.velocityRange = 90
            return cell
        }
        skyLayer.addSublayer(burst)
    }

    /// The window the panel is in, to find where on screen the light goes.
    private var panelWindow: NSWindow? {
        (layer.superlayer?.delegate as? NSView)?.window
    }

    /// The full-screen window for the light, over the panel's screen.
    private func skyPanel(over screen: NSScreen) -> NSPanel {
        if let sky, sky.frame == screen.frame { return sky }
        sky?.orderOut(nil)
        let panel = NSPanel(contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel],
                            backing: .buffered, defer: false)
        // just above the panels, so the flare lies over the panel it leaves
        panel.level = NSWindow.Level(rawValue: WindowLevel.overlay.rawValue + 1)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]
        let root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        root.wantsLayer = true
        skyLayer.removeFromSuperlayer()
        skyLayer.frame = root.bounds
        root.layer?.addSublayer(skyLayer)
        panel.contentView = root
        sky = panel
        return panel
    }

    /// Show the sky while there's light, and put it away once the last star
    /// has faded.
    private func setSkyLit(_ lit: Bool) {
        guard let sky else { return }
        if lit {
            skyStamp += 1
            puttingAway = false
            if !sky.isVisible { sky.orderFrontRegardless() }
            return
        }
        guard sky.isVisible, !puttingAway else { return }
        puttingAway = true
        let stamp = skyStamp
        // the stars outlive the frame that flung them; the closure keeps the
        // window alive past this object, too
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) { [weak self, sky] in
            guard self == nil || self?.skyStamp == stamp else { return }
            self?.puttingAway = false
            sky.orderOut(nil)
        }
    }

    func place(_ frame: EclipsePanelMotion.Frame, rect: CGRect, style: PanelStyle, side: CGFloat) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        // where the panel is on the screen, in the sky's coordinates
        var skyRect = rect
        if let window = panelWindow, let screen = window.screen ?? NSScreen.main {
            _ = skyPanel(over: screen)
            skyRect = rect.offsetBy(dx: window.frame.minX - screen.frame.minX,
                                    dy: window.frame.minY - screen.frame.minY)
        }
        let centre = CGPoint(x: skyRect.midX, y: skyRect.midY)
        let ring = max(1, side / 2 * frame.coronaScale)
        corona.position = centre
        corona.opacity = Float(frame.corona)
        let scale = ring / Eclipse.coronaRing / Self.unit
        corona.transform = CATransform3DMakeScale(scale, scale, 1)

        // the diamond: on the corona's ring, on the side the panel waxes from
        let out = PanelGeometry.diagonal(of: style.bigCut, in: skyRect)
        diamond.position = CGPoint(x: centre.x - out.x * ring, y: centre.y - out.y * ring)
        diamond.opacity = Float(frame.flash)
        let flare = 0.5 + 0.9 * frame.flash
        diamond.transform = CATransform3DMakeScale(flare, flare, 1)

        // the limb: the moon's edge, lit where it crosses the panel
        if frame.limb > 0.01, let moon = PanelGeometry.moon(in: rect, style: style) {
            // just round the panel: the limb only shows inside it, and a mask
            // the size of the screen would be rendered offscreen every frame
            limbBox.frame = rect.insetBy(dx: -8, dy: -8)
            var clear = style
            clear.shade = 0
            let origin = limbBox.frame.origin
            let local = rect.offsetBy(dx: -origin.x, dy: -origin.y)
            limbMask.frame = limbBox.bounds
            limbMask.path = PanelGeometry.path(in: local, style: clear)
            if abs(moon.radius - moonRadius) > 0.5 {
                moonRadius = moon.radius
                let circle = CGPath(ellipseIn: CGRect(x: -moon.radius, y: -moon.radius,
                                                      width: moon.radius * 2, height: moon.radius * 2), transform: nil)
                for shape in [limb, limbGlow] {
                    shape.bounds = .zero
                    shape.path = circle
                }
            }
            let at = CGPoint(x: moon.centre.x - origin.x, y: moon.centre.y - origin.y)
            limb.position = at
            limbGlow.position = at
            limbBox.opacity = Float(frame.limb)
        } else {
            limbBox.opacity = 0
        }

        burst.emitterPosition = centre
        burst.emitterSize = CGSize(width: ring * 1.6, height: ring * 1.6)
        // A burst is a moment of birth rate (see `ArcanePanelLights`).
        let flinging = frame.bursts > lastBursts
        if flinging {
            burst.birthRate = 1
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) { [burst] in burst.birthRate = 0 }
        }
        lastBursts = frame.bursts
        setSkyLit(flinging || frame.corona > 0.002 || frame.flash > 0.002)
    }

    func clear() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        corona.opacity = 0
        diamond.opacity = 0
        limbBox.opacity = 0
        burst.birthRate = 0
        CATransaction.commit()
        lastBursts = 0
        setSkyLit(false)
    }
}
