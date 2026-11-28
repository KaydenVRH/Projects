import AppKit
import QuartzCore
import SwiftUI

/// The Abyss theme: the deep sea, lit only by what lives there.
///
/// Where Arcane is spellwork and Eclipse is the sky, Abyss is under the water:
/// black-blue depths, bioluminescent teal and a jellyfish violet. Panels rise
/// out of the deep with their far edge rippling and a flash of
/// bioluminescence, and sink away into a burst of bubbles; the frame glows at
/// its rim with plankton drifting up off it and a pulse of light running round
/// it; the pointer trails bubbles; and switching to the theme dives — the
/// water closes over the screen, light rays sway down, and a sonar ping goes
/// out.
///
/// The always-on pieces move together on the wall clock (`gustPeriod`) and are
/// still in between, so the displays rest.
public enum Abyss {
    public private(set) static var active = false
    /// Bioluminescent teal (the theme's accent).
    static var teal = NSColor(red: 0.18, green: 0.9, blue: 0.84, alpha: 1)
    /// Jellyfish violet (the highlight).
    static var jelly = NSColor(red: 0.55, green: 0.48, blue: 1, alpha: 1)
    /// Plankton green (the theme's `mid`).
    static var plankton = NSColor(red: 0.36, green: 1, blue: 0.72, alpha: 1)
    static let deep = NSColor(red: 0.01, green: 0.05, blue: 0.09, alpha: 1)

    /// How deep the far edge of a resting panel rolls (see `PanelStyle.ripple`).
    static let panelRipple: CGFloat = 2.5
    /// How often the deep stirs; shared so every display rests at once.
    static let gustPeriod: TimeInterval = 20

    public static func isAbyss(_ theme: Theme) -> Bool {
        theme.effects == "abyss" || theme.name?.lowercased() == "abyss"
    }

    /// Follow the theme; true when the dive has just begun.
    @discardableResult
    static func update(for theme: Theme) -> Bool {
        let was = active
        active = isAbyss(theme)
        if let colour = theme.accent?.nsColor { teal = colour }
        if let colour = theme.highlight?.nsColor { jelly = colour }
        if let colour = theme.mid?.nsColor { plankton = colour }
        return active && !was
    }

    // MARK: images

    /// A bubble: a clear sphere — a thin bright rim, a faint body and a
    /// highlight up and to the left.
    static let bubble: CGImage = {
        let size: CGFloat = 32
        let ctx = CGContext(
            data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )!
        let c = CGPoint(x: size / 2, y: size / 2)
        let r = size / 2 - 2
        let body = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                              colors: [NSColor.white.withAlphaComponent(0.02).cgColor,
                                       NSColor.white.withAlphaComponent(0.22).cgColor] as CFArray,
                              locations: [0.55, 1])!
        ctx.drawRadialGradient(body, startCenter: c, startRadius: 0, endCenter: c, endRadius: r, options: [])
        ctx.setStrokeColor(NSColor.white.withAlphaComponent(0.85).cgColor)
        ctx.setLineWidth(1.4)
        ctx.strokeEllipse(in: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2))
        ctx.setFillColor(NSColor.white.withAlphaComponent(0.9).cgColor)
        ctx.fillEllipse(in: CGRect(x: c.x - r * 0.55, y: c.y + r * 0.2, width: r * 0.4, height: r * 0.28))
        return ctx.makeImage()!
    }()

    /// Bubbles, tinted with the water's light, rising.
    static func bubbles(rate: Float, velocity: CGFloat, lifetime: Float, scale: CGFloat) -> [CAEmitterCell] {
        [(teal, Float(1)), (jelly, Float(0.35)), (NSColor.white, Float(0.25))].map { colour, share in
            let cell = Arcane.cell(bubble, colour: colour, birthRate: rate * share, lifetime: lifetime,
                                   velocity: velocity, scale: scale)
            cell.emissionLongitude = .pi / 2
            cell.emissionRange = .pi / 8
            cell.yAcceleration = velocity * 0.6
            cell.xAcceleration = 0
            cell.spin = 0
            cell.spinRange = 0
            cell.alphaSpeed = -0.5 / lifetime
            cell.scaleSpeed = scale * 0.1
            return cell
        }
    }

    /// Plankton: soft specks of light drifting up, slowly.
    static func specks(rate: Float, velocity: CGFloat, lifetime: Float, scale: CGFloat) -> [CAEmitterCell] {
        [(teal, Float(1)), (plankton, Float(0.6)), (jelly, Float(0.4))].map { colour, share in
            let cell = Arcane.cell(Arcane.mote, colour: colour, birthRate: rate * share, lifetime: lifetime,
                                   velocity: velocity, scale: scale)
            cell.yAcceleration = 2
            cell.xAcceleration = 0
            cell.emissionRange = .pi / 3
            cell.spin = 0
            return cell
        }
    }
}

// MARK: - menus

/// Over an Abyss menu: a softly glowing teal rim that breathes like something
/// alive, plankton drifting up from its foot, bands of caustic light wandering
/// across it, and a burst of bubbles as it opens.
final class AbyssChromeView: NSView, ThemeChrome {
    private let rim = CAShapeLayer()
    private let plankton = CAEmitterLayer()
    private let bubbles = CAEmitterLayer()
    private let caustics = CAGradientLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true

        caustics.colors = [
            NSColor.white.withAlphaComponent(0).cgColor,
            Abyss.teal.withAlphaComponent(0.07).cgColor,
            NSColor.white.withAlphaComponent(0).cgColor,
            Abyss.jelly.withAlphaComponent(0.05).cgColor,
            NSColor.white.withAlphaComponent(0).cgColor,
        ]
        caustics.locations = [0, 0.18, 0.36, 0.62, 0.85]
        caustics.startPoint = CGPoint(x: 0.1, y: 1)
        caustics.endPoint = CGPoint(x: 0.9, y: 0)
        layer?.addSublayer(caustics)

        plankton.emitterShape = .line
        plankton.renderMode = .additive
        plankton.emitterCells = Abyss.specks(rate: 2, velocity: 14, lifetime: 5, scale: 0.22)
        layer?.addSublayer(plankton)

        bubbles.emitterShape = .line
        bubbles.renderMode = .additive
        bubbles.birthRate = 0
        bubbles.emitterCells = Abyss.bubbles(rate: 40, velocity: 70, lifetime: 1.4, scale: 0.32)
        layer?.addSublayer(bubbles)

        rim.fillColor = nil
        rim.strokeColor = Abyss.teal.withAlphaComponent(0.55).cgColor
        rim.lineWidth = 1.2
        rim.shadowColor = Abyss.teal.cgColor
        rim.shadowRadius = 9
        rim.shadowOpacity = 1
        rim.shadowOffset = .zero
        layer?.addSublayer(rim)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func fit(size: NSSize, style: PanelStyle?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let bounds = CGRect(origin: .zero, size: size)
        rim.frame = bounds
        rim.path = style.map { PanelGeometry.path(in: bounds.insetBy(dx: 0.6, dy: 0.6), style: $0) }
            ?? CGPath(roundedRect: bounds.insetBy(dx: 0.6, dy: 0.6), cornerWidth: 20, cornerHeight: 20, transform: nil)
        rim.bakeGlow(shape: rim.path, width: rim.lineWidth)
        plankton.frame = bounds
        plankton.emitterPosition = CGPoint(x: size.width / 2, y: 0)
        plankton.emitterSize = CGSize(width: size.width * 0.9, height: 1)
        bubbles.frame = bounds
        bubbles.emitterPosition = CGPoint(x: size.width / 2, y: 4)
        bubbles.emitterSize = CGSize(width: size.width * 0.8, height: 1)
        caustics.frame = bounds
        CATransaction.commit()
    }

    func powerOn() {
        Arcane.burst(bubbles, strength: 1, for: 0.35)
        rim.add(breathing(from: 0.5, period: 3.4), forKey: "breathe")
        let wander = CABasicAnimation(keyPath: "locations")
        wander.fromValue = [0, 0.18, 0.36, 0.62, 0.85]
        wander.toValue = [0.12, 0.34, 0.5, 0.74, 1]
        wander.duration = 6
        wander.autoreverses = true
        wander.repeatCount = .infinity
        wander.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        caustics.add(wander, forKey: "wander")
    }
}

// MARK: - the frame

/// Over the frame, per display: a teal rim glowing from the deep, warming to
/// violet along the bottom, baked and still; behind the windows, plankton
/// drifting up off the bottom edge and the sides. When the deep stirs, the
/// plankton gusts and a pulse of bioluminescence runs once round the frame.
final class AbyssFramePanel: FrameOverlayPanel {
    private var recurring: [Recurring] = []

    init(screen: NSScreen, inset: CGFloat, radius: CGFloat) {
        super.init(screen: screen)

        let root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        root.wantsLayer = true
        contentView = root
        let bounds = root.bounds
        let opening = bounds.insetBy(dx: inset, dy: inset)
        let path = CGPath(roundedRect: opening, cornerWidth: radius, cornerHeight: radius, transform: nil)

        // the rim (see `ArcaneFramePanel` on why these are baked and still)
        for (index, colour) in [Abyss.teal, Abyss.jelly].enumerated() {
            let edge = CAShapeLayer()
            edge.frame = bounds
            edge.path = path
            edge.fillColor = nil
            edge.strokeColor = colour.withAlphaComponent(0.55).cgColor
            edge.lineWidth = 1.2
            edge.shadowColor = colour.cgColor
            edge.shadowRadius = 10
            edge.shadowOpacity = 1
            edge.shadowOffset = .zero
            edge.bakeGlow(shape: path, width: 1.2)
            if index == 1 {
                let fade = CAGradientLayer()
                fade.frame = bounds
                fade.colors = [NSColor.black.cgColor, NSColor.clear.cgColor]
                fade.startPoint = CGPoint(x: 0.5, y: 0)
                fade.endPoint = CGPoint(x: 0.5, y: 0.4)
                edge.mask = fade
            }
            root.layer?.addSublayer(edge)
        }

        // the pulse: a short run of light going once round the rim
        let pulse = CAShapeLayer()
        let pulseGlow = CAShapeLayer()
        for (shape, width, colour) in [(pulseGlow, CGFloat(10), Abyss.teal.withAlphaComponent(0.22)),
                                       (pulse, CGFloat(2.2), Abyss.plankton)] {
            shape.frame = bounds
            shape.path = path
            shape.fillColor = nil
            shape.strokeColor = colour.cgColor
            shape.lineWidth = width
            shape.lineCap = .round
            shape.strokeStart = 0
            shape.strokeEnd = 0
            root.layer?.addSublayer(shape)
        }

        let period = Abyss.gustPeriod
        let next = Recurring.untilNext(period)
        recurring.append(Recurring(every: period, after: next) { [weak pulse, weak pulseGlow] in
            let run = CAKeyframeAnimation(keyPath: "strokeStart")
            run.values = [0, 0.92]
            let runEnd = CAKeyframeAnimation(keyPath: "strokeEnd")
            runEnd.values = [0.08, 1]
            let fade = CAKeyframeAnimation(keyPath: "opacity")
            fade.values = [0, 1, 1, 0]
            fade.keyTimes = [0, 0.08, 0.85, 1]
            let group = CAAnimationGroup()
            group.animations = [run, runEnd, fade]
            group.duration = 4.5
            group.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            pulse?.add(group, forKey: "pulse")
            pulseGlow?.add(group, forKey: "pulse")
        })

        // plankton off the bottom and up the sides, behind the windows
        let floor = CAEmitterLayer()
        floor.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 300)
        floor.masksToBounds = true
        floor.emitterShape = .line
        floor.emitterPosition = CGPoint(x: bounds.midX, y: inset)
        floor.emitterSize = CGSize(width: opening.width, height: 1)
        floor.renderMode = .additive
        floor.emitterCells = Abyss.specks(rate: 2.4, velocity: 16, lifetime: 6, scale: 0.26)
        backdrop.root.layer?.addSublayer(floor)
        recurring.append(floor.gusting(every: period, for: 4))

        for x in [opening.minX, opening.maxX] {
            let side = CAEmitterLayer()
            let strip: CGFloat = 90
            side.frame = CGRect(x: x - strip / 2, y: 0, width: strip, height: bounds.height)
            side.masksToBounds = true
            side.emitterShape = .line
            side.emitterPosition = CGPoint(x: strip / 2, y: bounds.midY)
            side.emitterSize = CGSize(width: 1, height: opening.height * 0.9)
            side.renderMode = .additive
            side.emitterCells = Abyss.bubbles(rate: 0.9, velocity: 18, lifetime: 4, scale: 0.22)
            backdrop.root.layer?.addSublayer(side)
            recurring.append(side.gusting(every: period, for: 3))
        }
    }
}

// MARK: - the bar

/// A teal-to-violet glow line along the bar's foot; when the deep stirs a
/// pulse of light runs along it and a few bubbles rise through the bar.
struct AbyssBarTrim: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { AbyssBarTrimView() }
    func updateNSView(_ view: NSView, context: Context) {}
}

final class AbyssBarTrimView: NSView {
    private let glow = CAGradientLayer()
    private let pulse = CAGradientLayer()
    private let bubbles = CAEmitterLayer()
    private var recurring: [Recurring] = []

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        glow.colors = [
            Abyss.teal.withAlphaComponent(0).cgColor,
            Abyss.teal.withAlphaComponent(0.75).cgColor,
            Abyss.jelly.withAlphaComponent(0.7).cgColor,
            Abyss.jelly.withAlphaComponent(0).cgColor,
        ]
        glow.locations = [0, 0.3, 0.7, 1]
        glow.startPoint = CGPoint(x: 0, y: 0.5)
        glow.endPoint = CGPoint(x: 1, y: 0.5)
        glow.shadowColor = Abyss.teal.cgColor
        glow.shadowRadius = 5
        glow.shadowOpacity = 1
        glow.shadowOffset = .zero
        layer?.addSublayer(glow)

        pulse.colors = [
            Abyss.plankton.withAlphaComponent(0).cgColor,
            NSColor.white.withAlphaComponent(0.95).cgColor,
            Abyss.plankton.withAlphaComponent(0).cgColor,
        ]
        pulse.startPoint = CGPoint(x: 0, y: 0.5)
        pulse.endPoint = CGPoint(x: 1, y: 0.5)
        pulse.opacity = 0
        layer?.addSublayer(pulse)

        bubbles.emitterShape = .line
        bubbles.renderMode = .additive
        bubbles.emitterCells = Abyss.bubbles(rate: 1.4, velocity: 7, lifetime: 2.6, scale: 0.18)
        layer?.addSublayer(bubbles)

        let period = Abyss.gustPeriod
        recurring = [
            bubbles.gusting(every: period, for: 2.5),
            Recurring(every: period, after: Recurring.untilNext(period) + 0.2) { [weak self] in self?.run() },
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
        pulse.frame = CGRect(x: -80, y: 0, width: 160, height: 1.5)
        bubbles.frame = bounds
        bubbles.emitterPosition = CGPoint(x: bounds.midX, y: 1)
        bubbles.emitterSize = CGSize(width: bounds.width * 0.95, height: 1)
        CATransaction.commit()
    }

    private func run() {
        let move = CABasicAnimation(keyPath: "position.x")
        move.fromValue = -80
        move.toValue = bounds.width + 80
        move.duration = 2.6
        move.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        let fade = CAKeyframeAnimation(keyPath: "opacity")
        fade.values = [0, 1, 1, 0]
        fade.keyTimes = [0, 0.1, 0.9, 1]
        fade.duration = 2.6
        let group = CAAnimationGroup()
        group.animations = [move, fade]
        group.duration = 2.6
        pulse.add(group, forKey: "run")
    }
}

// MARK: - the dive

/// Switching to Abyss dives: the water closes over every display from the top
/// down, shafts of light sway in from the surface, a sonar ping rings out from
/// the middle three times, and a column of bubbles rushes up — then the water
/// clears.
enum AbyssDive {
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
            let host = NSHostingView(rootView: AbyssDiveView(
                teal: Color(nsColor: Abyss.teal),
                jelly: Color(nsColor: Abyss.jelly),
                plankton: Color(nsColor: Abyss.plankton)
            ))
            host.frame = NSRect(origin: .zero, size: screen.frame.size)
            let root = NSView(frame: host.frame)
            root.wantsLayer = true
            root.addSubview(host)

            // the bubbles, rushing up from the bottom
            let rush = CAEmitterLayer()
            rush.frame = root.bounds
            rush.emitterShape = .line
            rush.emitterPosition = CGPoint(x: root.bounds.midX, y: -10)
            rush.emitterSize = CGSize(width: root.bounds.width * 0.7, height: 1)
            rush.renderMode = .additive
            rush.birthRate = 0
            rush.emitterCells = Abyss.bubbles(rate: 140, velocity: 300, lifetime: 3.4, scale: 0.5)
            root.layer?.addSublayer(rush)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                Arcane.burst(rush, strength: 1, for: 0.9)
            }

            panel.contentView = root
            panel.orderFrontRegardless()
            return panel
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 4) {
            for panel in panels { panel.orderOut(nil) }
            panels.removeAll()
        }
    }
}

private struct AbyssDiveView: View {
    let teal: Color
    let jelly: Color
    let plankton: Color

    @State private var water: CGFloat = 0
    @State private var sway = false
    @State private var pings = [false, false, false]
    @State private var gone = false

    var body: some View {
        GeometryReader { geo in
            ZStack {
                // the water closing over, top first
                LinearGradient(colors: [Color(red: 0.0, green: 0.12, blue: 0.2).opacity(0.75),
                                        Color(red: 0.0, green: 0.03, blue: 0.08).opacity(0.9)],
                               startPoint: .top, endPoint: .bottom)
                    .mask(alignment: .top) {
                        Rectangle().frame(height: geo.size.height * water)
                    }
                    .opacity(gone ? 0 : 1)

                // shafts of light from the surface
                ForEach(0..<6, id: \.self) { index in
                    let x = geo.size.width * (0.12 + 0.16 * CGFloat(index))
                    LinearGradient(colors: [teal.opacity(0.28), teal.opacity(0)], startPoint: .top, endPoint: .bottom)
                        .frame(width: CGFloat(50 + (index * 37) % 70), height: geo.size.height * 0.85)
                        .blur(radius: 14)
                        .rotationEffect(.degrees((sway ? 9 : -6) + Double(index % 3) * 4), anchor: .top)
                        .position(x: x, y: geo.size.height * 0.42)
                        .blendMode(.plusLighter)
                        .opacity(gone ? 0 : water * 0.9)
                }

                // the ping
                ForEach(0..<3, id: \.self) { index in
                    Circle()
                        .stroke(index == 1 ? jelly : teal, lineWidth: 2)
                        .shadow(color: teal, radius: 8)
                        .frame(width: 120, height: 120)
                        .scaleEffect(pings[index] ? 9 : 0.2)
                        .opacity(pings[index] ? 0 : 0.95)
                        .position(x: geo.size.width / 2, y: geo.size.height / 2)
                }
                Circle()
                    .fill(RadialGradient(colors: [.white, teal, teal.opacity(0)], center: .center,
                                         startRadius: 1, endRadius: 40))
                    .frame(width: 80, height: 80)
                    .scaleEffect(pings[0] ? 1 : 0.3)
                    .opacity(gone ? 0 : (water > 0.5 ? 1 : 0))
                    .position(x: geo.size.width / 2, y: geo.size.height / 2)
                    .blendMode(.plusLighter)
            }
        }
        .ignoresSafeArea()
        .onAppear {
            withAnimation(.easeInOut(duration: 0.7)) { water = 1 }
            withAnimation(.easeInOut(duration: 2.6)) { sway = true }
            for index in 0..<3 {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.65 + Double(index) * 0.55) {
                    withAnimation(.easeOut(duration: 1.6)) { pings[index] = true }
                }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.8) {
                withAnimation(.easeIn(duration: 1)) { gone = true }
            }
        }
    }
}

// MARK: - SwiftUI pieces

/// A bubble: a clear sphere with a bright rim and a highlight; `lit`, it
/// glows with the creature inside it.
struct AbyssBubble: View {
    let tint: Color
    var lit = false

    var body: some View {
        GeometryReader { geo in
            let side = min(geo.size.width, geo.size.height)
            ZStack {
                Circle()
                    .fill(RadialGradient(colors: [tint.opacity(lit ? 0.6 : 0.04), tint.opacity(lit ? 0.3 : 0.12)],
                                         center: .center, startRadius: 0, endRadius: side / 2))
                Circle()
                    .stroke(LinearGradient(colors: [.white.opacity(0.7), tint.opacity(0.35)],
                                           startPoint: .topLeading, endPoint: .bottomTrailing),
                            lineWidth: 1)
                Ellipse()
                    .fill(Color.white.opacity(0.6))
                    .frame(width: side * 0.24, height: side * 0.13)
                    .rotationEffect(.degrees(-35))
                    .offset(x: -side * 0.2, y: -side * 0.22)
            }
            .frame(width: side, height: side)
            .position(x: geo.size.width / 2, y: geo.size.height / 2)
        }
    }
}

/// A sonar ping: a ring going out and fading, again and again.
struct SonarPing: View {
    let colour: Color
    var period: Double = 2.4

    @State private var out = false

    var body: some View {
        Circle()
            .stroke(colour, lineWidth: 1.2)
            .scaleEffect(out ? 1.5 : 0.85)
            .opacity(out ? 0 : 0.8)
            .onAppear {
                guard !Motion.reduced else { return }
                withAnimation(.easeOut(duration: period).repeatForever(autoreverses: false)) { out = true }
            }
            .allowsHitTesting(false)
    }
}

/// A few bubbles rising through their parent, over and over.
struct RisingBubbles: View {
    let colours: [Color]
    var count = 5

    var body: some View {
        GeometryReader { geo in
            ForEach(0..<count, id: \.self) { index in
                RisingBubble(colour: colours[index % colours.count], delay: Double(index) * 0.53,
                             size: CGFloat(4 + (index * 3) % 5), travel: geo.size.height)
                    .position(x: geo.size.width * CGFloat((index * 37 + 13) % 100) / 100, y: geo.size.height)
            }
        }
        .allowsHitTesting(false)
    }
}

private struct RisingBubble: View {
    let colour: Color
    let delay: Double
    let size: CGFloat
    let travel: CGFloat

    @State private var risen = false

    var body: some View {
        Circle()
            .stroke(colour.opacity(0.8), lineWidth: 0.8)
            .background(Circle().fill(colour.opacity(0.15)))
            .frame(width: size, height: size)
            .offset(x: risen ? 3 : -2, y: risen ? -travel : 0)
            .opacity(risen ? 0 : 0.9)
            .onAppear {
                guard !Motion.reduced else { return }
                withAnimation(.easeIn(duration: 2.4).repeatForever(autoreverses: false).delay(delay)) { risen = true }
            }
    }
}

/// A gentle wave across its width, for underlines.
struct TideLine: Shape {
    var amplitude: CGFloat = 2
    var wavelength: CGFloat = 26

    func path(in rect: CGRect) -> Path {
        var path = Path()
        let mid = rect.midY
        path.move(to: CGPoint(x: rect.minX, y: mid))
        var x = rect.minX
        while x <= rect.maxX {
            path.addLine(to: CGPoint(x: x, y: mid + amplitude * sin(2 * .pi * (x - rect.minX) / wavelength)))
            x += 2
        }
        return path
    }
}

// MARK: - the kit

/// The header: the icon in a glowing bubble that pings, rising into place; the
/// title in soft rounded letters lit from within; a wave drawn in under it;
/// bubbles drifting up behind.
struct AbyssHeader<Trailing: View>: View {
    let icon: String
    let title: String
    var subtitle: String?
    let palette: KitPalette
    let trailing: Trailing

    @State private var arrived = false

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                SonarPing(colour: palette.accent.opacity(0.7), period: 2.8)
                    .frame(width: 40, height: 40)
                AbyssBubble(tint: palette.accent, lit: true)
                    .frame(width: 38, height: 38)
                    .shadow(color: palette.accent.opacity(0.8), radius: 8)
                Image(systemName: icon)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundColor(.white)
                    .shadow(color: palette.accent, radius: 5)
                    .symbolEffect(.bounce, value: arrived)
            }
            .frame(width: 44, height: 44)
            .offset(y: arrived ? 0 : 22)
            .opacity(arrived ? 1 : 0)

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 18, weight: .semibold, design: .rounded))
                    .foregroundStyle(LinearGradient(colors: [palette.foreground, palette.accent],
                                                    startPoint: .top, endPoint: .bottom))
                    .shadow(color: palette.accent.opacity(0.75), radius: 8)
                    .offset(y: arrived ? 0 : 12)
                    .opacity(arrived ? 1 : 0)
                if let subtitle {
                    Text("≋ " + subtitle)
                        .font(.system(size: 10.5, design: .rounded))
                        .foregroundColor(palette.foreground.opacity(0.6))
                        .contentTransition(.numericText())
                }
            }
            Spacer(minLength: 8)
            trailing
        }
        .padding(.horizontal, 18)
        .padding(.top, 16)
        .padding(.bottom, 12)
        .background(RisingBubbles(colours: [palette.accent, palette.highlight], count: 5).opacity(0.7))
        .overlay(alignment: .bottom) {
            TideLine(amplitude: 1.6, wavelength: 28)
                .trim(from: 0, to: arrived ? 1 : 0)
                .stroke(LinearGradient(colors: [palette.accent, palette.highlight], startPoint: .leading, endPoint: .trailing),
                        lineWidth: 1.2)
                .frame(height: 6)
                .shadow(color: palette.accent, radius: 4)
                .padding(.horizontal, 18)
        }
        .onAppear {
            withAnimation(.spring(response: 0.6, dampingFraction: 0.7).delay(0.08)) { arrived = true }
        }
    }
}

/// The armed row: a glowing pod of teal light, bubbles rising off its end.
struct AbyssCard: ViewModifier {
    let selected: Bool
    let hovered: Bool
    let palette: KitPalette
    let highlight: Namespace.ID

    func body(content: Content) -> some View {
        content
            .background {
                ZStack {
                    RoundedRectangle(cornerRadius: 20, style: .continuous)
                        .fill(palette.accent.opacity(hovered ? 0.1 : 0.035))
                    if selected {
                        RoundedRectangle(cornerRadius: 20, style: .continuous)
                            .fill(LinearGradient(colors: [palette.accent.opacity(0.3), palette.highlight.opacity(0.14)],
                                                 startPoint: .leading, endPoint: .trailing))
                            .overlay(
                                RoundedRectangle(cornerRadius: 20, style: .continuous)
                                    .stroke(LinearGradient(colors: [palette.accent, palette.highlight.opacity(0.35)],
                                                           startPoint: .leading, endPoint: .trailing),
                                            lineWidth: 1.1)
                            )
                            .shadow(color: palette.accent.opacity(0.6), radius: 14)
                            .overlay(alignment: .trailing) {
                                RisingBubbles(colours: [palette.accent, .white], count: 3)
                                    .frame(width: 26)
                                    .padding(.trailing, 10)
                            }
                            .matchedGeometryEffect(id: "kit-armed", in: highlight)
                    }
                }
            }
            .scaleEffect(selected ? 1.015 : (hovered ? 1.008 : 1))
            .animation(.kSelect, value: selected)
            .animation(.kHover, value: hovered)
    }
}

/// The icon bubble: a bubble at rest; lit from inside and pinging while live.
struct AbyssIconBubble: View {
    let symbol: String
    let active: Bool
    let palette: KitPalette
    let size: CGFloat

    var body: some View {
        ZStack {
            if active {
                SonarPing(colour: palette.accent, period: 2.2)
                    .frame(width: size, height: size)
            }
            AbyssBubble(tint: palette.accent, lit: active)
                .frame(width: size, height: size)
                .shadow(color: palette.accent.opacity(active ? 0.8 : 0), radius: 8)
            Image(systemName: symbol)
                .font(.system(size: size * 0.38, weight: .medium))
                .foregroundColor(active ? .white : palette.foreground.opacity(0.75))
                .symbolEffect(.bounce, value: active)
        }
        .frame(width: size + 8, height: size + 8)
        .animation(.kBouncy, value: active)
    }
}
