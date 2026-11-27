import AppKit
import QuartzCore

/// How a Stark panel comes and goes: projected, then derezzed.
///
/// Opening: four corner brackets slam in from outside and lock onto where the
/// panel will be, a wireframe of its outline traces itself round, and the
/// panel resolves inside it as a hologram — interlaced scanlines thickening
/// into a solid surface while a scan beam sweeps down it and the content
/// flickers on. A readout confirms the link. Closing runs it backwards: the
/// picture thins back into scanlines and is gone, the wireframe erases itself
/// and the brackets let go.
///
/// The panel never moves or grows — a projection appears where it's aimed.
final class StarkPanelMotion: PanelTimeline {
    struct Frame: Equatable {
        /// How much of the hologram is there (`PanelStyle.interlace`).
        var interlace: CGFloat = 0
        /// The content's flicker as it comes on, 0…1.
        var signal: CGFloat = 0
        /// How far the brackets still are from their corners, in points.
        var spread: CGFloat = 60
        var brackets: CGFloat = 0
        /// The wireframe: drawn up to `wireEnd`, erased up to `wireStart`.
        var wireStart: CGFloat = 0
        var wireEnd: CGFloat = 0
        var wire: CGFloat = 0
        /// The scan beam, 0 at the far edge to 1 at the frame, and its light.
        var beam: CGFloat = 0
        var beamAlpha: CGFloat = 0
        var readout: CGFloat = 0
        /// The flash of the outline as the link locks.
        var flash: CGFloat = 0
        var closing = false
    }

    static let closeDuration: Double = 0.42
    /// Room for the brackets coming in from outside.
    static let slack: (out: CGFloat, side: CGFloat) = (68, 68)
    var slack: (out: CGFloat, side: CGFloat) { Self.slack }

    private(set) var frame = Frame()
    var onStep: (() -> Void)?
    var onSettle: ((Bool) -> Void)?
    private let starkLights = StarkPanelLights()
    var lights: CALayer { starkLights.layer }

    private var clock: FrameClock?
    private var opening = false
    private var t: Double = 0
    private var closeFrom = Frame()
    private var outline = PanelStyle()

    init(screen: NSScreen) {
        clock = FrameClock(screen: screen) { [weak self] dt in self?.step(dt) ?? false }
    }

    deinit { clock?.stop() }

    func open() {
        opening = true
        t = 0
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
        starkLights.clear()
    }

    private static func clamp(_ x: Double) -> Double { min(1, max(0, x)) }
    private static func easeOut(_ x: Double) -> Double { 1 - pow(1 - clamp(x), 3) }
    private static func easeIn(_ x: Double) -> Double { pow(clamp(x), 2) }
    /// Past the target and back, for the brackets slamming home.
    private static func backOut(_ x: Double) -> Double {
        let c = clamp(x), s = 2.2
        return 1 + (s + 1) * pow(c - 1, 3) + s * pow(c - 1, 2)
    }

    private func step(_ dt: Double) -> Bool {
        t += dt
        var f = Frame()
        var done = false
        if opening {
            // targeting: the brackets slam in
            f.spread = CGFloat(60 * (1 - Self.backOut(t / 0.2)))
            f.brackets = CGFloat(Self.clamp(t / 0.06) * (1 - Self.clamp((t - 0.62) / 0.18)))
            // the wireframe traces round, then fades once the surface is solid
            f.wireEnd = CGFloat(Self.easeOut((t - 0.05) / 0.24))
            f.wire = CGFloat(Self.clamp((t - 0.05) / 0.05) * (1 - Self.clamp((t - 0.5) / 0.2)))
            // the hologram resolves, scanline by scanline
            f.interlace = CGFloat(Self.easeIn((t - 0.14) / 0.3))
            f.beam = CGFloat(Self.clamp((t - 0.14) / 0.3))
            f.beamAlpha = CGFloat(Self.clamp((t - 0.14) / 0.04) * (1 - Self.clamp((t - 0.42) / 0.06)))
            // the content flickers on as the signal comes in
            let flicker = Int(t * 60) % 5 == 0 ? 0.45 : 1.0
            f.signal = CGFloat(Self.clamp((t - 0.22) / 0.2) * (t < 0.46 ? flicker : 1))
            f.flash = CGFloat(Self.clamp((t - 0.44) / 0.03) * (1 - Self.clamp((t - 0.47) / 0.2)))
            f.readout = CGFloat(Self.clamp((t - 0.3) / 0.05) * (1 - Self.clamp((t - 0.85) / 0.15)))
            done = t >= 1.0
            if done { f = Frame(interlace: 1, signal: 1, spread: 0) }
        } else {
            f.closing = true
            let from = closeFrom
            // derez: the picture thins back into scanlines, flickering
            let thin = Self.easeIn(t / 0.24)
            f.interlace = from.interlace * CGFloat(1 - thin)
            let flicker = Int(t * 70) % 3 == 0 ? 0.4 : 1.0
            f.signal = from.signal * CGFloat((1 - Self.clamp(t / 0.2)) * flicker)
            // the wireframe holds the shape for a moment, then erases itself
            f.wire = CGFloat(Self.clamp(t / 0.04) * (1 - Self.clamp((t - 0.34) / 0.08)))
            f.wireEnd = 1
            f.wireStart = CGFloat(Self.easeIn((t - 0.12) / 0.26))
            // the brackets let go
            f.brackets = CGFloat(Self.clamp(t / 0.04) * (1 - Self.clamp((t - 0.22) / 0.14)))
            f.spread = CGFloat(48 * Self.easeIn((t - 0.08) / 0.3))
            f.beam = CGFloat(1 - Self.clamp(t / 0.24))
            f.beamAlpha = CGFloat(Self.clamp(t / 0.03) * (1 - Self.clamp((t - 0.22) / 0.04)))
            done = t >= Self.closeDuration
            if done { f = Frame(spread: 60) }
        }
        frame = f
        onStep?()
        if done { onSettle?(opening) }
        return !done
    }

    func layout(full: NSRect, edge: OverlayEdge, style: PanelStyle) -> PanelTimelineLayout {
        outline = style
        var projected = style
        projected.interlace = frame.interlace
        return PanelTimelineLayout(
            rect: full,
            style: projected,
            squash: false,
            hostAlpha: frame.signal,
            glassFade: frame.interlace > 0 ? 1 : 0,
            chromeAlpha: frame.signal,
            bloom: frame.flash * 0.18,
            bloomColour: Hud.accent
        )
    }

    func placeLights(rect: CGRect, edge: OverlayEdge) {
        starkLights.place(frame, rect: rect, style: outline, edge: edge)
    }
}

/// The light of a Stark projection: the corner brackets, the wireframe, the
/// scan beam and the readout. Drawn over the panel, unclipped.
final class StarkPanelLights {
    let layer = CALayer()
    private let brackets = CAShapeLayer()
    private let wire = CAShapeLayer()
    private let beam = CAGradientLayer()
    private let readout = CATextLayer()
    private var wiredRect: CGRect = .null
    private var wiredStyle: PanelStyle?

    init() {
        layer.masksToBounds = false
        for shape in [brackets, wire] {
            shape.fillColor = nil
            shape.strokeColor = Hud.accent.cgColor
            shape.shadowColor = Hud.accent.cgColor
            shape.shadowRadius = 5
            shape.shadowOpacity = 1
            shape.shadowOffset = .zero
            shape.opacity = 0
            shape.lineCap = .square
            layer.addSublayer(shape)
        }
        brackets.lineWidth = 2.2
        brackets.strokeColor = Hud.ice.cgColor
        wire.lineWidth = 1.3

        beam.colors = [
            Hud.accent.withAlphaComponent(0).cgColor,
            Hud.ice.cgColor,
            Hud.accent.withAlphaComponent(0).cgColor,
        ]
        beam.startPoint = CGPoint(x: 0, y: 0.5)
        beam.endPoint = CGPoint(x: 1, y: 0.5)
        beam.shadowColor = Hud.accent.cgColor
        beam.shadowRadius = 8
        beam.shadowOpacity = 1
        beam.shadowOffset = .zero
        beam.opacity = 0
        layer.addSublayer(beam)

        readout.string = "◢ LINK ESTABLISHED"
        readout.font = NSFont.monospacedSystemFont(ofSize: 9, weight: .semibold)
        readout.fontSize = 9
        readout.foregroundColor = Hud.accent.cgColor
        readout.contentsScale = NSScreen.main?.backingScaleFactor ?? 2
        readout.alignmentMode = .left
        readout.opacity = 0
        readout.shadowColor = Hud.accent.cgColor
        readout.shadowRadius = 4
        readout.shadowOpacity = 0.9
        readout.shadowOffset = .zero
        layer.addSublayer(readout)
    }

    func place(_ frame: StarkPanelMotion.Frame, rect: CGRect, style: PanelStyle, edge: OverlayEdge) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        // the wireframe: the outline itself, without the flanges, traced
        if rect != wiredRect || style != wiredStyle {
            wiredRect = rect
            wiredStyle = style
            var bare = style
            bare.filletRadius = 0
            bare.interlace = 1
            let path = PanelGeometry.path(in: rect.insetBy(dx: 0.5, dy: 0.5), style: bare)
            wire.path = path
            wire.shadowPath = path.copy(strokingWithWidth: 3, lineCap: .square, lineJoin: .miter, miterLimit: 4)
        }
        wire.opacity = Float(frame.wire)
        wire.strokeStart = frame.wireStart
        wire.strokeEnd = max(frame.wireStart, frame.wireEnd)

        // four L brackets, `spread` points out from the corners
        brackets.opacity = Float(frame.brackets)
        if frame.brackets > 0 {
            let r = rect.insetBy(dx: -4 - frame.spread, dy: -4 - frame.spread)
            let arm = min(26, min(rect.width, rect.height) / 5)
            let path = CGMutablePath()
            for (corner, dx, dy) in [
                (CGPoint(x: r.minX, y: r.minY), CGFloat(1), CGFloat(1)),
                (CGPoint(x: r.maxX, y: r.minY), -1, 1),
                (CGPoint(x: r.minX, y: r.maxY), 1, -1),
                (CGPoint(x: r.maxX, y: r.maxY), -1, -1),
            ] {
                path.move(to: CGPoint(x: corner.x + dx * arm, y: corner.y))
                path.addLine(to: corner)
                path.addLine(to: CGPoint(x: corner.x, y: corner.y + dy * arm))
            }
            brackets.path = path
            brackets.shadowPath = path.copy(strokingWithWidth: 4, lineCap: .square, lineJoin: .miter, miterLimit: 4)
        }

        // the scan beam sweeps from the far edge to the frame
        beam.opacity = Float(frame.beamAlpha)
        if frame.beamAlpha > 0 {
            let thickness: CGFloat = 2
            switch edge {
            case .bottom:
                beam.frame = CGRect(x: rect.minX, y: rect.maxY - rect.height * frame.beam, width: rect.width, height: thickness)
            case .top:
                beam.frame = CGRect(x: rect.minX, y: rect.minY + rect.height * frame.beam, width: rect.width, height: thickness)
            case .leading:
                beam.frame = CGRect(x: rect.maxX - rect.width * frame.beam, y: rect.minY, width: thickness, height: rect.height)
            case .trailing:
                beam.frame = CGRect(x: rect.minX + rect.width * frame.beam, y: rect.minY, width: thickness, height: rect.height)
            }
            let vertical = edge == .leading || edge == .trailing
            beam.startPoint = vertical ? CGPoint(x: 0.5, y: 0) : CGPoint(x: 0, y: 0.5)
            beam.endPoint = vertical ? CGPoint(x: 0.5, y: 1) : CGPoint(x: 1, y: 0.5)
            beam.shadowPath = CGPath(rect: beam.bounds, transform: nil)
        }

        // the readout, just outside the panel's far corner
        readout.opacity = Float(frame.readout)
        if frame.readout > 0 {
            let size = CGSize(width: 130, height: 12)
            let origin: CGPoint
            switch edge {
            case .bottom: origin = CGPoint(x: rect.minX + 4, y: rect.maxY + 6)
            case .top: origin = CGPoint(x: rect.minX + 4, y: rect.minY - 6 - size.height)
            case .leading: origin = CGPoint(x: rect.maxX + 8, y: rect.maxY - size.height)
            case .trailing: origin = CGPoint(x: rect.minX + 4, y: rect.maxY + 6)
            }
            readout.frame = CGRect(origin: origin, size: size)
        }
    }

    func clear() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        for item in [brackets, wire, beam, readout] as [CALayer] { item.opacity = 0 }
        CATransaction.commit()
    }
}
