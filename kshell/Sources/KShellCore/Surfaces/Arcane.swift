import AppKit
import QuartzCore
import SwiftUI

/// The Arcane theme's magic: effects only that theme gets.
///
/// Where the Stark HUD is hard light and brackets, Arcane is glow and
/// particles. Sparks trail the pointer, menus open in a burst of them and keep a
/// few motes drifting up inside, the frame breathes violet with motes rising
/// off its edges, the bar twinkles, and switching to the theme casts a magic
/// circle across every display.
///
/// The particles are `CAEmitterLayer`s: the render server simulates and draws
/// them, so a screen full of sparks costs the shell almost nothing.
public enum Arcane {
    public private(set) static var active = false
    static var violet = NSColor(red: 0.71, green: 0.42, blue: 1, alpha: 1)
    static var rose = NSColor(red: 1, green: 0.48, blue: 0.85, alpha: 1)
    static var spark = NSColor(red: 0.49, green: 0.94, blue: 1, alpha: 1)

    /// How deep a panel's free corners are scooped (see `PanelStyle.notch`).
    static let panelNotch: CGFloat = 18
    /// How often the always-on motes and sparkles gust. They gust together, on
    /// the wall clock, so the displays can rest between gusts.
    static let gustPeriod: TimeInterval = 20

    public static func isArcane(_ theme: Theme) -> Bool {
        theme.effects == "arcane" || theme.name?.lowercased() == "arcane"
    }

    /// Follow the theme; true when the magic has just switched on.
    @discardableResult
    static func update(for theme: Theme) -> Bool {
        let was = active
        active = isArcane(theme)
        if let colour = theme.accent?.nsColor { violet = colour }
        if let colour = theme.highlight?.nsColor { rose = colour }
        if let colour = theme.mid?.nsColor { spark = colour }
        return active && !was
    }

    // MARK: particle images

    /// A soft round mote.
    static let mote: CGImage = image(size: 24) { ctx, size in
        let colours = [NSColor.white.cgColor, NSColor.white.withAlphaComponent(0.35).cgColor, NSColor.clear.cgColor]
        let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors(colours), locations: [0, 0.25, 1])!
        let centre = CGPoint(x: size / 2, y: size / 2)
        ctx.drawRadialGradient(gradient, startCenter: centre, startRadius: 0, endCenter: centre, endRadius: size / 2, options: [])
    }

    /// A four-point twinkle with a bright heart.
    static let twinkle: CGImage = image(size: 40) { ctx, size in
        let c = size / 2
        let gradient = CGGradient(
            colorsSpace: CGColorSpaceCreateDeviceRGB(),
            colors: colors([NSColor.white.cgColor, NSColor.clear.cgColor]),
            locations: [0, 1]
        )!
        ctx.drawRadialGradient(gradient, startCenter: CGPoint(x: c, y: c), startRadius: 0,
                               endCenter: CGPoint(x: c, y: c), endRadius: size * 0.18, options: [])
        // two thin diamonds make the cross
        ctx.setFillColor(NSColor.white.withAlphaComponent(0.9).cgColor)
        for horizontal in [true, false] {
            let long = size / 2, thin = size * 0.045
            ctx.beginPath()
            if horizontal {
                ctx.move(to: CGPoint(x: c - long, y: c))
                ctx.addLine(to: CGPoint(x: c, y: c + thin))
                ctx.addLine(to: CGPoint(x: c + long, y: c))
                ctx.addLine(to: CGPoint(x: c, y: c - thin))
            } else {
                ctx.move(to: CGPoint(x: c, y: c - long))
                ctx.addLine(to: CGPoint(x: c + thin, y: c))
                ctx.addLine(to: CGPoint(x: c, y: c + long))
                ctx.addLine(to: CGPoint(x: c - thin, y: c))
            }
            ctx.closePath()
            ctx.fillPath()
        }
    }

    private static func colors(_ list: [CGColor]) -> CFArray { list as CFArray }

    private static func image(size: CGFloat, draw: (CGContext, CGFloat) -> Void) -> CGImage {
        let ctx = CGContext(
            data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )!
        draw(ctx, size)
        return ctx.makeImage()!
    }

    /// A particle: `twinkle` for the sparkles, `mote` for the soft dust.
    static func cell(
        _ image: CGImage,
        colour: NSColor,
        birthRate: Float,
        lifetime: Float,
        velocity: CGFloat,
        scale: CGFloat
    ) -> CAEmitterCell {
        let cell = CAEmitterCell()
        cell.contents = image
        cell.color = colour.cgColor
        cell.birthRate = birthRate
        cell.lifetime = lifetime
        cell.lifetimeRange = lifetime * 0.4
        cell.velocity = velocity
        cell.velocityRange = velocity * 0.6
        cell.emissionLongitude = .pi / 2
        cell.emissionRange = .pi / 5
        cell.yAcceleration = 10
        cell.scale = scale
        cell.scaleRange = scale * 0.6
        cell.scaleSpeed = -scale * 0.15
        cell.alphaSpeed = -1 / lifetime
        cell.spin = 0.6
        cell.spinRange = 2
        cell.redRange = 0.08
        cell.blueRange = 0.08
        return cell
    }

    /// The three particles every Arcane emitter mixes: violet motes, rose
    /// twinkles, the odd spectral spark.
    static func cells(rate: Float, velocity: CGFloat = 24, lifetime: Float = 3.2, scale: CGFloat = 0.32) -> [CAEmitterCell] {
        [
            cell(mote, colour: violet, birthRate: rate, lifetime: lifetime, velocity: velocity, scale: scale * 0.7),
            cell(twinkle, colour: rose, birthRate: rate * 0.45, lifetime: lifetime * 0.8, velocity: velocity, scale: scale),
            cell(twinkle, colour: spark, birthRate: rate * 0.15, lifetime: lifetime * 0.7, velocity: velocity * 1.2, scale: scale * 0.8),
        ]
    }

    /// Open a burst: a spray of sparks from `emitter` for a moment.
    static func burst(_ emitter: CAEmitterLayer, strength: Float = 14, for duration: TimeInterval = 0.22) {
        let resting = emitter.birthRate
        emitter.beginTime = CACurrentMediaTime()
        emitter.birthRate = strength
        DispatchQueue.main.asyncAfter(deadline: .now() + duration) { emitter.birthRate = resting }
    }
}

extension CALayer {
    /// Make a glowing layer cheap to composite: give its shadow the exact shape
    /// (so Core Animation needn't render the layer offscreen to find one) and
    /// cache the result as a bitmap. Glows breathe by animating opacity, which a
    /// cached bitmap can do for free — animating the shadow's radius instead
    /// re-blurred a screen-sized outline every frame and pinned the GPU.
    func bakeGlow(shape: CGPath?, width: CGFloat) {
        if let shape {
            shadowPath = shape.copy(strokingWithWidth: max(width, 1), lineCap: .round, lineJoin: .round, miterLimit: 4)
        }
        shouldRasterize = true
        rasterizationScale = NSScreen.main?.backingScaleFactor ?? 2
    }
}

/// A slow breath of opacity, for a baked glow.
func breathing(from low: Float, period: CFTimeInterval) -> CABasicAnimation {
    let breathe = CABasicAnimation(keyPath: "opacity")
    breathe.fromValue = low
    breathe.toValue = 1
    breathe.duration = period
    breathe.autoreverses = true
    breathe.repeatCount = .infinity
    breathe.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
    return breathe
}

/// A menu's chrome, for whichever theme draws one over its content.
protocol ThemeChrome: NSView {
    func fit(size: NSSize, style: PanelStyle?)
    func powerOn()
    /// The panel has started closing. Only Neon does anything with it.
    func powerOff()
}

extension ThemeChrome {
    func powerOff() {}
}

extension HudChromeView: ThemeChrome {}

// MARK: - menus

/// Over an Arcane menu: a soft violet outline that breathes, motes drifting up
/// from its foot, a slow shimmer across it, and a burst of sparks off its
/// outline as it opens.
final class ArcaneChromeView: NSView, ThemeChrome {
    private let outline = CAShapeLayer()
    private let motes = CAEmitterLayer()
    private let burst = CAEmitterLayer()
    private let shimmer = CAGradientLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true

        shimmer.colors = [
            NSColor.white.withAlphaComponent(0).cgColor,
            Arcane.rose.withAlphaComponent(0.07).cgColor,
            NSColor.white.withAlphaComponent(0).cgColor,
        ]
        shimmer.startPoint = CGPoint(x: 0, y: 0.3)
        shimmer.endPoint = CGPoint(x: 1, y: 0.7)
        shimmer.locations = [-0.4, -0.2, 0.0]
        layer?.addSublayer(shimmer)

        motes.emitterShape = .line
        motes.renderMode = .additive
        motes.emitterCells = Arcane.cells(rate: 2.2, velocity: 22, lifetime: 4, scale: 0.26)
        layer?.addSublayer(motes)

        burst.emitterShape = .rectangle
        burst.emitterMode = .outline
        burst.renderMode = .additive
        burst.birthRate = 0
        let cells = Arcane.cells(rate: 30, velocity: 60, lifetime: 1.1, scale: 0.36)
        for cell in cells { cell.emissionRange = .pi * 2; cell.yAcceleration = 25 }
        burst.emitterCells = cells
        layer?.addSublayer(burst)

        outline.fillColor = nil
        outline.strokeColor = Arcane.violet.withAlphaComponent(0.55).cgColor
        outline.lineWidth = 1.2
        outline.shadowColor = Arcane.violet.cgColor
        outline.shadowRadius = 8
        outline.shadowOpacity = 1
        outline.shadowOffset = .zero
        layer?.addSublayer(outline)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func fit(size: NSSize, style: PanelStyle?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let bounds = CGRect(origin: .zero, size: size)
        outline.frame = bounds
        outline.path = style.map { PanelGeometry.path(in: bounds.insetBy(dx: 0.6, dy: 0.6), style: $0) }
            ?? CGPath(roundedRect: bounds.insetBy(dx: 0.6, dy: 0.6), cornerWidth: 16, cornerHeight: 16, transform: nil)
        outline.bakeGlow(shape: outline.path, width: outline.lineWidth)
        motes.frame = bounds
        motes.emitterPosition = CGPoint(x: size.width / 2, y: 0)
        motes.emitterSize = CGSize(width: size.width * 0.9, height: 1)
        burst.frame = bounds
        burst.emitterPosition = CGPoint(x: size.width / 2, y: size.height / 2)
        burst.emitterSize = size
        shimmer.frame = bounds
        CATransaction.commit()
    }

    func powerOn() {
        Arcane.burst(burst, strength: 1, for: 0.25)

        outline.add(breathing(from: 0.45, period: 2.2), forKey: "breathe")

        // The band travels in the gradient's own 0…1 space, so it crosses the
        // whole menu whatever size the menu is — measuring it in points here
        // took the size of the pill the menu opens from, and the band stopped
        // part-way across and snapped back.
        let glide = CABasicAnimation(keyPath: "locations")
        glide.fromValue = [-0.4, -0.2, 0.0]
        glide.toValue = [1.0, 1.2, 1.4]
        glide.duration = 3.2
        glide.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        let group = CAAnimationGroup()
        group.animations = [glide]
        group.duration = 7
        group.repeatCount = .infinity
        shimmer.add(group, forKey: "shimmer")
    }
}

// MARK: - the frame

/// Over the frame, per display: the opening's edge glowing violet and
/// breathing, and motes rising off the bottom edge and drifting up the sides.
final class ArcaneFramePanel: FrameOverlayPanel {
    private var recurring: [Recurring] = []

    init(screen: NSScreen, inset: CGFloat, radius: CGFloat) {
        super.init(screen: screen)

        let root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        root.wantsLayer = true
        contentView = root
        let bounds = root.bounds
        let opening = bounds.insetBy(dx: inset, dy: inset)

        // The edge glows violet, warming to rose along the bottom, baked once and
        // left still. These layers span the whole screen, and anything that
        // changes on a screen-sized layer — even its opacity — makes the window
        // server recomposite the whole screen, glass and all, every frame. The
        // life comes from the particles, whose layers are kept to thin strips.
        let path = CGPath(roundedRect: opening, cornerWidth: radius, cornerHeight: radius, transform: nil)
        for (index, colour) in [Arcane.violet, Arcane.rose].enumerated() {
            let edge = CAShapeLayer()
            edge.frame = bounds
            edge.path = path
            edge.fillColor = nil
            edge.strokeColor = colour.withAlphaComponent(0.6).cgColor
            edge.lineWidth = 1.2
            edge.shadowColor = colour.cgColor
            edge.shadowRadius = 10
            edge.shadowOpacity = 1
            edge.shadowOffset = .zero
            edge.bakeGlow(shape: path, width: 1.2)
            if index == 1 {
                // rose only along the bottom, fading out up the sides
                let fade = CAGradientLayer()
                fade.frame = bounds
                fade.colors = [NSColor.black.cgColor, NSColor.clear.cgColor]
                fade.startPoint = CGPoint(x: 0.5, y: 0)
                fade.endPoint = CGPoint(x: 0.5, y: 0.45)
                edge.mask = fade
            }
            root.layer?.addSublayer(edge)
        }

        // motes rising off the bottom edge, and a few drifting up each side
        // Each emitter's layer is only the strip its particles live in, so the
        // screen is redrawn there and nowhere else.
        let floor = CAEmitterLayer()
        floor.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 260)
        floor.masksToBounds = true
        floor.emitterShape = .line
        floor.emitterPosition = CGPoint(x: bounds.midX, y: inset)
        floor.emitterSize = CGSize(width: opening.width, height: 1)
        floor.renderMode = .additive
        floor.emitterCells = Arcane.cells(rate: 1.6, velocity: 30, lifetime: 5, scale: 0.3)
        // The motes drift up into the opening, so they rise behind windows.
        backdrop.root.layer?.addSublayer(floor)
        recurring.append(floor.gusting(every: Arcane.gustPeriod, for: 3.5))

        for x in [opening.minX, opening.maxX] {
            let side = CAEmitterLayer()
            let strip: CGFloat = 90
            side.frame = CGRect(x: x - strip / 2, y: 0, width: strip, height: bounds.height)
            side.masksToBounds = true
            side.emitterShape = .line
            side.emitterPosition = CGPoint(x: strip / 2, y: bounds.midY)
            side.emitterSize = CGSize(width: 1, height: opening.height * 0.9)
            side.renderMode = .additive
            side.emitterCells = Arcane.cells(rate: 0.45, velocity: 14, lifetime: 4, scale: 0.26)
            backdrop.root.layer?.addSublayer(side)
            recurring.append(side.gusting(every: Arcane.gustPeriod, for: 3.5))
        }
    }
}

// MARK: - the pointer

/// Something drawn after the pointer, one per theme that has one.
protocol CursorTrail: AnyObject {
    func stop()
    func setHidden(_ hidden: Bool, on screen: NSScreen)
}

/// Particles shed by the pointer as it moves — Arcane's sparks, Eclipse's
/// star dust, Abyss's bubbles — stopping when it rests. One click-through
/// window per display; the pointer is sampled at the display's rate (no input
/// monitoring, so no permissions).
final class ParticleCursorTrail: CursorTrail {
    private var panels: [(NSPanel, CAEmitterLayer)] = []
    private var timer: DispatchSourceTimer?
    private var last = NSPoint(x: -1, y: -1)
    private var resting = true
    private var restStamp = UUID()
    /// Displays a full-screen app has taken over.
    private var hiddenScreens: [NSRect] = []

    /// `cells` makes one display's emitter cells; the emitter's birth rate
    /// scales them with the pointer's speed.
    init(cells makeCells: () -> [CAEmitterCell]) {
        panels = NSScreen.screens.map { screen in
            let panel = NSPanel(contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel],
                                backing: .buffered, defer: false)
            panel.level = WindowLevel.overlay
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.ignoresMouseEvents = true
            panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle, .fullScreenAuxiliary]
            let root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
            root.wantsLayer = true
            panel.contentView = root

            let emitter = CAEmitterLayer()
            emitter.frame = root.bounds
            emitter.emitterShape = .point
            emitter.renderMode = .additive
            emitter.birthRate = 0
            emitter.emitterCells = makeCells()
            root.layer?.addSublayer(emitter)
            return (panel, emitter)
        }
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 1.0 / 60)
        timer.setEventHandler { [weak self] in self?.follow() }
        timer.resume()
        self.timer = timer
    }

    private func follow() {
        let mouse = NSEvent.mouseLocation
        let moved = hypot(mouse.x - last.x, mouse.y - last.y)
        last = mouse
        // Resting: stop shedding, and once the last sparks have faded take the
        // windows off screen — a screen-sized window, even an empty one, costs
        // the window server a composite every frame something beneath it moves.
        if moved < 0.5 {
            if resting { return }
            resting = true
            for (_, emitter) in panels { emitter.birthRate = 0 }
            let stamp = UUID()
            restStamp = stamp
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) { [weak self] in
                guard let self, self.resting, self.restStamp == stamp else { return }
                for (panel, _) in self.panels { panel.orderOut(nil) }
            }
            return
        }
        if resting {
            for (panel, _) in panels where !hiddenScreens.contains(panel.frame) { panel.orderFrontRegardless() }
        }
        resting = false
        for (panel, emitter) in panels {
            let frame = panel.frame
            guard frame.contains(mouse) else {
                emitter.birthRate = 0
                continue
            }
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            emitter.emitterPosition = CGPoint(x: mouse.x - frame.minX, y: mouse.y - frame.minY)
            CATransaction.commit()
            // The faster it moves, the more it sheds; resting, it sheds nothing.
            emitter.birthRate = Float(min(1, moved / 25))
        }
    }

    func stop() {
        timer?.cancel()
        timer = nil
        for (panel, _) in panels { panel.orderOut(nil) }
        panels.removeAll()
    }

    /// Get out of the way of a full-screen app on `screen`, or come back.
    func setHidden(_ hidden: Bool, on screen: NSScreen) {
        hiddenScreens.removeAll { $0 == screen.frame }
        if hidden { hiddenScreens.append(screen.frame) }
        for (panel, _) in panels where panel.frame == screen.frame && hidden {
            panel.orderOut(nil)
        }
    }
}

// MARK: - the bar

/// Sparkles that wink in along the bar and float up off it, over a soft glow
/// line that drifts from violet to rose.
struct ArcaneBarTrim: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { ArcaneBarTrimView() }
    func updateNSView(_ view: NSView, context: Context) {}
}

final class ArcaneBarTrimView: NSView {
    private let glow = CAGradientLayer()
    private let sparkles = CAEmitterLayer()
    private var recurring: [Recurring] = []

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        glow.colors = [
            Arcane.violet.withAlphaComponent(0).cgColor,
            Arcane.violet.withAlphaComponent(0.7).cgColor,
            Arcane.rose.withAlphaComponent(0.7).cgColor,
            Arcane.rose.withAlphaComponent(0).cgColor,
        ]
        glow.locations = [0, 0.3, 0.7, 1]
        glow.startPoint = CGPoint(x: 0, y: 0.5)
        glow.endPoint = CGPoint(x: 1, y: 0.5)
        glow.shadowColor = Arcane.violet.cgColor
        glow.shadowRadius = 5
        glow.shadowOpacity = 1
        glow.shadowOffset = .zero
        layer?.addSublayer(glow)

        sparkles.emitterShape = .line
        sparkles.renderMode = .additive
        let cells = Arcane.cells(rate: 1.6, velocity: 6, lifetime: 2.4, scale: 0.2)
        for cell in cells { cell.yAcceleration = 3 }
        sparkles.emitterCells = cells
        layer?.addSublayer(sparkles)
        // the glow drifts and the sparkles rise in a gust now and then, in step
        // with the frame's motes, and the bar rests in between
        recurring = [
            sparkles.gusting(every: Arcane.gustPeriod, for: 2.5),
            Recurring(every: Arcane.gustPeriod, after: Recurring.untilNext(Arcane.gustPeriod)) { [weak self] in
                self?.drift()
            },
        ]
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        glow.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 1)
        glow.shadowPath = CGPath(rect: glow.bounds, transform: nil)
        sparkles.frame = bounds
        sparkles.emitterPosition = CGPoint(x: bounds.midX, y: 2)
        sparkles.emitterSize = CGSize(width: bounds.width * 0.95, height: 1)
        CATransaction.commit()

    }

    private func drift() {
        let drift = CABasicAnimation(keyPath: "locations")
        drift.fromValue = [0, 0.3, 0.7, 1]
        drift.toValue = [0, 0.1, 0.45, 1]
        drift.duration = 3.5
        drift.autoreverses = true
        drift.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        glow.add(drift, forKey: "drift")
    }
}

// MARK: - SwiftUI pieces

/// A star polygon {n/k} — the shape at the heart of a magic circle.
struct StarPolygon: Shape {
    var points = 7
    var skip = 3

    func path(in rect: CGRect) -> Path {
        let centre = CGPoint(x: rect.midX, y: rect.midY)
        let radius = min(rect.width, rect.height) / 2
        var path = Path()
        var index = 0
        for step in 0...points {
            let angle = Double(index) / Double(points) * 2 * .pi - .pi / 2
            let point = CGPoint(x: centre.x + cos(angle) * radius, y: centre.y + sin(angle) * radius)
            if step == 0 { path.move(to: point) } else { path.addLine(to: point) }
            index = (index + skip) % points
        }
        path.closeSubpath()
        return path
    }
}

/// A small magic circle: a ring, a dashed rune ring turning one way, a star
/// turning the other, glowing.
struct MagicCircle: View {
    let violet: Color
    let rose: Color
    var lineWidth: CGFloat = 1.2
    var period: Double = 14

    var body: some View {
        ZStack {
            // The glow is a still, blurred ring under the moving parts: a shadow
            // on the spinning layers would be re-rendered every frame.
            Circle().stroke(violet.opacity(0.7), lineWidth: lineWidth * 4).blur(radius: 5)
            Circle().stroke(violet.opacity(0.85), lineWidth: lineWidth)
            SpinningRing(colour: rose.opacity(0.8), dash: [1.5, 3, 5, 3], width: lineWidth * 1.6, period: period)
                .padding(lineWidth * 4)
            SpinningRing(colour: violet.opacity(0.5), dash: [1, 2], width: lineWidth, period: period * 0.7, clockwise: false)
                .padding(lineWidth * 8)
            Spinning(period: period * 1.6, clockwise: false) {
                StarPolygon(points: 7, skip: 3)
                    .stroke(violet.opacity(0.75), lineWidth: lineWidth)
                    .padding(lineWidth * 8)
            }
        }
    }
}

/// Turns its content forever.
struct Spinning<Content: View>: View {
    var period: Double = 10
    var clockwise = true
    @ViewBuilder let content: Content

    @State private var turned = false

    var body: some View {
        content
            .rotationEffect(.degrees(turned ? (clockwise ? 360 : -360) : 0))
            .onAppear {
                withAnimation(.linear(duration: period).repeatForever(autoreverses: false)) { turned = true }
            }
    }
}

/// A few sparkles twinkling at random around their parent.
struct Twinkles: View {
    let colours: [Color]
    var count = 6

    var body: some View {
        GeometryReader { geo in
            ForEach(0..<count, id: \.self) { index in
                Twinkle(colour: colours[index % colours.count], delay: Double(index) * 0.37)
                    .position(
                        x: geo.size.width * CGFloat((index * 37 + 11) % 100) / 100,
                        y: geo.size.height * CGFloat((index * 61 + 23) % 100) / 100
                    )
            }
        }
        .allowsHitTesting(false)
    }
}

private struct Twinkle: View {
    let colour: Color
    let delay: Double

    @State private var lit = false

    var body: some View {
        Image(systemName: "sparkle")
            .font(.system(size: 8))
            .foregroundColor(colour)
            .shadow(color: colour, radius: 3)
            .scaleEffect(lit ? 1 : 0.2)
            .opacity(lit ? 1 : 0)
            .rotationEffect(.degrees(lit ? 45 : 0))
            .onAppear {
                withAnimation(.easeInOut(duration: 1.1).repeatForever(autoreverses: true).delay(delay)) { lit = true }
            }
    }
}

// MARK: - the casting

/// Switching to Arcane casts a magic circle across every display: it draws
/// itself — ring, runes, star — flares, bursts into sparks, and fades.
enum ArcaneCast {
    private static var panels: [NSPanel] = []

    static func play() {
        guard panels.isEmpty, !Motion.reduced else { return }
        panels = NSScreen.screens.map { screen in
            let panel = NSPanel(contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel],
                                backing: .buffered, defer: false)
            panel.level = WindowLevel.overlay
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.ignoresMouseEvents = true
            panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]
            let host = NSHostingView(rootView: ArcaneCastView(
                violet: Color(nsColor: Arcane.violet),
                rose: Color(nsColor: Arcane.rose),
                spark: Color(nsColor: Arcane.spark)
            ))
            host.frame = NSRect(origin: .zero, size: screen.frame.size)
            let root = NSView(frame: host.frame)
            root.wantsLayer = true
            root.addSubview(host)

            // the burst, timed to the flare
            let burst = CAEmitterLayer()
            burst.frame = root.bounds
            burst.emitterShape = .circle
            burst.emitterMode = .outline
            burst.emitterPosition = CGPoint(x: root.bounds.midX, y: root.bounds.midY)
            burst.emitterSize = CGSize(width: 340, height: 340)
            burst.renderMode = .additive
            burst.birthRate = 0
            let cells = Arcane.cells(rate: 160, velocity: 260, lifetime: 1.8, scale: 0.5)
            for cell in cells { cell.emissionRange = .pi * 2; cell.yAcceleration = 60 }
            burst.emitterCells = cells
            root.layer?.addSublayer(burst)
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.35) {
                Arcane.burst(burst, strength: 1, for: 0.3)
            }

            panel.contentView = root
            panel.orderFrontRegardless()
            return panel
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.4) {
            for panel in panels { panel.orderOut(nil) }
            panels.removeAll()
        }
    }
}

private struct ArcaneCastView: View {
    let violet: Color
    let rose: Color
    let spark: Color

    @State private var drawn: CGFloat = 0
    @State private var flare = false
    @State private var gone = false

    var body: some View {
        ZStack {
            Color.black.opacity(gone ? 0 : 0.35)
            ZStack {
                Circle().trim(from: 0, to: drawn).stroke(violet, lineWidth: 2.5).frame(width: 420, height: 420)
                Circle().trim(from: 0, to: drawn)
                    .stroke(rose, style: StrokeStyle(lineWidth: 6, dash: [2, 5, 9, 5]))
                    .frame(width: 380, height: 380)
                    .rotationEffect(.degrees(Double(drawn) * 120))
                Circle().trim(from: 0, to: drawn).stroke(violet.opacity(0.7), lineWidth: 1.5).frame(width: 330, height: 330)
                StarPolygon(points: 7, skip: 3).trim(from: 0, to: drawn)
                    .stroke(violet, lineWidth: 2).frame(width: 320, height: 320)
                    .rotationEffect(.degrees(-Double(drawn) * 50))
                StarPolygon(points: 6, skip: 2).trim(from: 0, to: drawn)
                    .stroke(rose.opacity(0.8), lineWidth: 1.5).frame(width: 190, height: 190)
                    .rotationEffect(.degrees(Double(drawn) * 80))
                Circle().fill(Color.white).frame(width: 18, height: 18)
                    .shadow(color: violet, radius: 20).shadow(color: rose, radius: 40)
                    .scaleEffect(flare ? 6 : (drawn > 0.5 ? 1 : 0))
                    .opacity(flare ? 0 : 1)
            }
            .shadow(color: violet, radius: 12)
            .shadow(color: rose.opacity(0.6), radius: 30)
            .scaleEffect(gone ? 1.5 : (flare ? 1.08 : 1))
            .opacity(gone ? 0 : 1)
            .blur(radius: gone ? 10 : 0)
        }
        .onAppear {
            withAnimation(.easeInOut(duration: 1.3)) { drawn = 1 }
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.35) {
                withAnimation(.easeOut(duration: 0.5)) { flare = true }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.2) {
                withAnimation(.easeIn(duration: 0.9)) { gone = true }
            }
        }
    }
}

// MARK: - ink

/// Writes the text in as if by a glowing pen: it appears left to right behind
/// a bright nib, the freshest ink still glowing. Played when it appears, a
/// beat after its panel starts to open.
struct InkWrite: ViewModifier {
    let nib: Color
    var delay: Double = 0.28
    var duration: Double = 0.55

    @State private var written: CGFloat = 0

    func body(content: Content) -> some View {
        content
            .modifier(InkMask(written: written, nib: nib))
            .onAppear {
                guard !Motion.reduced else {
                    written = 1
                    return
                }
                written = 0
                withAnimation(.easeInOut(duration: duration).delay(delay)) { written = 1 }
            }
    }
}

/// The animatable part of `InkWrite`: the mask and the nib at `written`.
private struct InkMask: ViewModifier, Animatable {
    var written: CGFloat
    let nib: Color

    var animatableData: CGFloat {
        get { written }
        set { written = newValue }
    }

    func body(content: Content) -> some View {
        // the edge is soft: a little ink ahead of the nib, fading in
        let edge: CGFloat = 0.08
        let at = written * (1 + edge)
        content
            .mask {
                LinearGradient(
                    stops: [
                        .init(color: .black, location: max(0, at - edge)),
                        .init(color: .clear, location: min(1, max(0.0001, at))),
                    ],
                    startPoint: .leading,
                    endPoint: .trailing
                )
            }
            .overlay(alignment: .leading) {
                GeometryReader { geo in
                    Circle()
                        .fill(RadialGradient(colors: [.white, nib, nib.opacity(0)], center: .center,
                                             startRadius: 0, endRadius: 7))
                        .frame(width: 14, height: 14)
                        .position(x: geo.size.width * min(1, written), y: geo.size.height * 0.55)
                        .opacity(written > 0.01 && written < 0.99 ? 1 : 0)
                        .blendMode(.plusLighter)
                }
                .allowsHitTesting(false)
            }
    }
}
