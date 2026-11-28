import AppKit
import QuartzCore
import SwiftUI

/// The Eclipse theme: the sky at totality.
///
/// Where Arcane is spellwork — circles, runes, sparks — Eclipse is the sky
/// itself: midnight, moonlight silver and the sun's gold. Every panel has a
/// crescent bitten out of a corner by a passing moon; panels open waxing and
/// close waning, flaring a corona at totality; the frame is starlit, with an
/// eclipse low in one corner and the odd shooting star; the pointer sheds star
/// dust; and switching to the theme plays a total eclipse across every display.
///
/// Like the other flagships, everything always-on is still between moments: the
/// stars twinkle, the corona flares and the shooting stars fall together on the
/// wall clock (`gustPeriod`), so the displays rest in between.
public enum Eclipse {
    public private(set) static var active = false
    /// The sun's gold (the theme's accent).
    static var gold = NSColor(red: 0.91, green: 0.77, blue: 0.42, alpha: 1)
    /// Moonlight (the highlight).
    static var silver = NSColor(red: 0.78, green: 0.83, blue: 0.96, alpha: 1)
    /// The corona's warm fringe (the theme's `mid`).
    static var corona = NSColor(red: 1, green: 0.62, blue: 0.37, alpha: 1)
    /// The night (the moon's face).
    static let night = NSColor(red: 0.02, green: 0.03, blue: 0.08, alpha: 1)

    /// The radius of the moon that bites a panel's corner (see `PanelStyle.bite`).
    static let panelBite: CGFloat = 32
    /// How often the sky stirs. Shared with the other surfaces so every display
    /// rests at once.
    static let gustPeriod: TimeInterval = 20

    public static func isEclipse(_ theme: Theme) -> Bool {
        theme.effects == "eclipse" || theme.name?.lowercased() == "eclipse"
    }

    /// Follow the theme; true when the eclipse has just come on.
    @discardableResult
    static func update(for theme: Theme) -> Bool {
        let was = active
        active = isEclipse(theme)
        if let colour = theme.accent?.nsColor { gold = colour }
        if let colour = theme.highlight?.nsColor { silver = colour }
        if let colour = theme.mid?.nsColor { corona = colour }
        coronaCache = nil
        return active && !was
    }

    // MARK: images

    private static var coronaCache: CGImage?

    /// The corona: a thin ring of white-gold light round a dark hole, its
    /// streamers fraying outward. The ring sits at `coronaRing` of the image's
    /// size, so a layer `r / coronaRing` across puts the ring at radius `r`.
    static var coronaImage: CGImage {
        if let coronaCache { return coronaCache }
        let image = makeCorona(size: 360, gold: gold, fringe: corona)
        coronaCache = image
        return image
    }

    /// Where the ring sits in `coronaImage`, as a fraction of its width.
    static let coronaRing: CGFloat = 0.3

    private static func makeCorona(size: CGFloat, gold: NSColor, fringe: NSColor) -> CGImage {
        let ctx = CGContext(
            data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )!
        let c = CGPoint(x: size / 2, y: size / 2)
        let ring = size * coronaRing

        // streamers: thin rays of uneven length, brightest at the ring
        ctx.setLineCap(.round)
        for index in 0..<72 {
            // a cheap, repeatable scatter
            let hash = Double((index * 7919 + 104_729) % 1000) / 1000
            let angle = Double(index) / 72 * 2 * .pi + hash * 0.06
            let length = ring * CGFloat(1.12 + 0.55 * hash * hash)
            let from = CGPoint(x: c.x + cos(angle) * ring * 0.98, y: c.y + sin(angle) * ring * 0.98)
            let to = CGPoint(x: c.x + cos(angle) * length, y: c.y + sin(angle) * length)
            let colours = [fringe.withAlphaComponent(0.55).cgColor, fringe.withAlphaComponent(0).cgColor] as CFArray
            let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colours, locations: [0, 1])!
            ctx.saveGState()
            ctx.setLineWidth(index % 3 == 0 ? 2.2 : 1.1)
            ctx.move(to: from)
            ctx.addLine(to: to)
            ctx.replacePathWithStrokedPath()
            ctx.clip()
            ctx.drawLinearGradient(gradient, start: from, end: to, options: [])
            ctx.restoreGState()
        }

        // the ring itself: white at its inner edge, gold, then a long soft fall
        let colours = [
            NSColor.clear.cgColor,
            NSColor.white.cgColor,
            gold.cgColor,
            gold.withAlphaComponent(0.35).cgColor,
            fringe.withAlphaComponent(0.1).cgColor,
            NSColor.clear.cgColor,
        ] as CFArray
        let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colours,
                                  locations: [0, 0.035, 0.09, 0.22, 0.5, 1])!
        ctx.drawRadialGradient(gradient, startCenter: c, startRadius: ring * 0.96,
                               endCenter: c, endRadius: size / 2, options: [])
        return ctx.makeImage()!
    }

    /// Star dust: Arcane's twinkle and mote, in moonlight and gold.
    static func stars(rate: Float, velocity: CGFloat, lifetime: Float, scale: CGFloat) -> [CAEmitterCell] {
        [
            Arcane.cell(Arcane.twinkle, colour: silver, birthRate: rate, lifetime: lifetime, velocity: velocity, scale: scale),
            Arcane.cell(Arcane.twinkle, colour: gold, birthRate: rate * 0.5, lifetime: lifetime * 0.8,
                        velocity: velocity, scale: scale * 0.8),
            Arcane.cell(Arcane.mote, colour: .white, birthRate: rate * 0.6, lifetime: lifetime * 0.6,
                        velocity: velocity * 0.5, scale: scale * 0.5),
        ].map { cell in
            cell.yAcceleration = 0
            cell.spin = 0.2
            cell.spinRange = 0.6
            return cell
        }
    }

    /// A layer showing the corona with its ring at `radius`, centred on `centre`.
    static func coronaLayer(radius: CGFloat, centre: CGPoint) -> CALayer {
        let layer = CALayer()
        layer.contents = coronaImage
        let side = radius / coronaRing
        layer.bounds = CGRect(x: 0, y: 0, width: side, height: side)
        layer.position = centre
        return layer
    }
}

// MARK: - menus

/// Over an Eclipse menu: a moonlit hairline outline with a gold glow; the
/// corona peeking round the moon that bit its corner; and a small
/// constellation beside it that draws itself in as the menu opens, one of its
/// stars catching the light now and then.
final class EclipseChromeView: NSView, ThemeChrome {
    private let outline = CAShapeLayer()
    private let corona = CALayer()
    private let lines = CAShapeLayer()
    private let dots = CAShapeLayer()
    private let glint = CALayer()
    private var points: [CGPoint] = []
    private var twinkling: Recurring?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true

        corona.contents = Eclipse.coronaImage
        corona.opacity = 0
        layer?.addSublayer(corona)

        lines.fillColor = nil
        lines.strokeColor = Eclipse.silver.withAlphaComponent(0.18).cgColor
        lines.lineWidth = 0.7
        lines.strokeEnd = 0
        layer?.addSublayer(lines)

        dots.fillColor = Eclipse.silver.withAlphaComponent(0.6).cgColor
        dots.shadowColor = Eclipse.silver.cgColor
        dots.shadowRadius = 3
        dots.shadowOpacity = 1
        dots.shadowOffset = .zero
        dots.opacity = 0
        layer?.addSublayer(dots)

        glint.contents = Arcane.twinkle
        glint.bounds = CGRect(x: 0, y: 0, width: 18, height: 18)
        glint.opacity = 0
        layer?.addSublayer(glint)

        outline.fillColor = nil
        outline.strokeColor = Eclipse.silver.withAlphaComponent(0.45).cgColor
        outline.lineWidth = 1
        outline.shadowColor = Eclipse.gold.cgColor
        outline.shadowRadius = 6
        outline.shadowOpacity = 0.9
        outline.shadowOffset = .zero
        layer?.addSublayer(outline)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func fit(size: NSSize, style: PanelStyle?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let bounds = CGRect(origin: .zero, size: size)
        outline.frame = bounds
        outline.path = style.map { PanelGeometry.path(in: bounds.insetBy(dx: 0.5, dy: 0.5), style: $0) }
            ?? CGPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), cornerWidth: 16, cornerHeight: 16, transform: nil)
        outline.bakeGlow(shape: outline.path, width: outline.lineWidth)

        // The constellation sits by the bitten corner (or the top-leading one).
        let corner = style?.bigCut ?? .topLeading
        let origin = PanelGeometry.cornerPoint(corner, in: bounds)
        let sx: CGFloat = origin.x <= bounds.midX ? 1 : -1
        let sy: CGFloat = origin.y >= bounds.midY ? -1 : 1
        let pattern: [(CGFloat, CGFloat)] = [(58, 16), (88, 34), (124, 24), (150, 48), (118, 62), (176, 30)]
        points = pattern
            .map { CGPoint(x: origin.x + sx * $0.0, y: origin.y + sy * $0.1) }
            .filter { bounds.insetBy(dx: 6, dy: 6).contains($0) }
        let path = CGMutablePath()
        if points.count >= 2 {
            path.addLines(between: Array(points.prefix(5)))
            if points.count == 6 { path.move(to: points[2]); path.addLine(to: points[5]) }
        }
        lines.frame = bounds
        lines.path = path
        let dotPath = CGMutablePath()
        for (index, point) in points.enumerated() {
            let r: CGFloat = index % 2 == 0 ? 1.5 : 1.1
            dotPath.addEllipse(in: CGRect(x: point.x - r, y: point.y - r, width: r * 2, height: r * 2))
        }
        dots.frame = bounds
        dots.path = dotPath
        dots.shadowPath = dotPath

        // the corona round the moon that bit the corner
        if let style, let moon = PanelGeometry.biteMoon(in: bounds, style: style) {
            let side = moon.radius * 1.04 / Eclipse.coronaRing
            corona.bounds = CGRect(x: 0, y: 0, width: side, height: side)
            corona.position = moon.centre
            corona.isHidden = false
        } else {
            corona.isHidden = true
        }
    }

    func powerOn() {
        let reduced = Motion.reduced
        corona.opacity = 0.85
        dots.opacity = 1
        lines.strokeEnd = 1
        guard !reduced else { return }

        let rise = CABasicAnimation(keyPath: "opacity")
        rise.fromValue = 0
        rise.toValue = 0.85
        rise.duration = 0.9
        rise.timingFunction = CAMediaTimingFunction(name: .easeOut)
        corona.add(rise, forKey: "rise")
        let swell = CABasicAnimation(keyPath: "transform.scale")
        swell.fromValue = 0.7
        swell.toValue = 1
        swell.duration = 0.9
        swell.timingFunction = CAMediaTimingFunction(controlPoints: 0.2, 0.9, 0.3, 1)
        corona.add(swell, forKey: "swell")

        let draw = CABasicAnimation(keyPath: "strokeEnd")
        draw.fromValue = 0
        draw.toValue = 1
        draw.beginTime = CACurrentMediaTime() + 0.25
        draw.duration = 0.9
        draw.fillMode = .backwards
        draw.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        lines.add(draw, forKey: "draw")
        let appear = CABasicAnimation(keyPath: "opacity")
        appear.fromValue = 0
        appear.toValue = 1
        appear.beginTime = CACurrentMediaTime() + 0.2
        appear.duration = 0.5
        appear.fillMode = .backwards
        dots.add(appear, forKey: "appear")

        // now and then one star catches the light
        twinkling = Recurring(every: 5.5, after: 1.6) { [weak self] in self?.twinkle() }
    }

    func powerOff() {
        twinkling = nil
    }

    private func twinkle() {
        guard !points.isEmpty else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        glint.position = points[Int.random(in: 0..<points.count)]
        CATransaction.commit()
        let flash = CAKeyframeAnimation(keyPath: "opacity")
        flash.values = [0, 1, 0.3, 0.9, 0]
        flash.keyTimes = [0, 0.2, 0.45, 0.6, 1]
        flash.duration = 0.9
        glint.add(flash, forKey: "twinkle")
        let turn = CABasicAnimation(keyPath: "transform.rotation")
        turn.fromValue = 0
        turn.toValue = CGFloat.pi / 2
        turn.duration = 0.9
        glint.add(turn, forKey: "turn")
    }
}

// MARK: - the frame

/// Over the frame, per display: a moonlit edge with a gold glow, baked and
/// still; behind the windows, stars scattered along the edges of the opening,
/// an eclipse low in the trailing corner, and now and then a shooting star.
/// The stars twinkle, the corona flares and the star falls together, every
/// `gustPeriod`, and nothing moves in between.
final class EclipseFramePanel: FrameOverlayPanel {
    private var recurring: [Recurring] = []

    init(screen: NSScreen, inset: CGFloat, radius: CGFloat) {
        super.init(screen: screen)

        let root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        root.wantsLayer = true
        contentView = root
        let bounds = root.bounds
        let opening = bounds.insetBy(dx: inset, dy: inset)
        let path = CGPath(roundedRect: opening, cornerWidth: radius, cornerHeight: radius, transform: nil)

        let edge = CAShapeLayer()
        edge.frame = bounds
        edge.path = path
        edge.fillColor = nil
        edge.strokeColor = Eclipse.silver.withAlphaComponent(0.5).cgColor
        edge.lineWidth = 1
        edge.shadowColor = Eclipse.gold.cgColor
        edge.shadowRadius = 8
        edge.shadowOpacity = 0.9
        edge.shadowOffset = .zero
        edge.bakeGlow(shape: path, width: 1)
        root.layer?.addSublayer(edge)

        let sky = backdrop.root.layer
        let period = Eclipse.gustPeriod
        let next = Recurring.untilNext(period)

        // a scatter of faint stars along the edges, baked into one layer
        let field = CAShapeLayer()
        field.frame = bounds
        let dots = CGMutablePath()
        var bright: [CGPoint] = []
        for index in 0..<46 {
            let a = Double((index * 7919 + 31) % 997) / 997
            let b = Double((index * 104_729 + 17) % 991) / 991
            // which edge, and how far in from it
            let band = 70.0 * b * b
            let point: CGPoint
            switch index % 4 {
            case 0: point = CGPoint(x: opening.minX + opening.width * a, y: opening.maxY - 40 - band)
            case 1: point = CGPoint(x: opening.minX + opening.width * a, y: opening.minY + 14 + band)
            case 2: point = CGPoint(x: opening.minX + 14 + band, y: opening.minY + opening.height * a)
            default: point = CGPoint(x: opening.maxX - 14 - band, y: opening.minY + opening.height * a)
            }
            let r: CGFloat = index % 7 == 0 ? 1.4 : (index % 3 == 0 ? 1 : 0.7)
            dots.addEllipse(in: CGRect(x: point.x - r, y: point.y - r, width: r * 2, height: r * 2))
            if index % 7 == 0 { bright.append(point) }
        }
        field.path = dots
        field.fillColor = Eclipse.silver.withAlphaComponent(0.7).cgColor
        field.shadowColor = Eclipse.silver.cgColor
        field.shadowRadius = 2
        field.shadowOpacity = 0.8
        field.shadowOffset = .zero
        field.shadowPath = dots
        field.shouldRasterize = true
        field.rasterizationScale = screen.backingScaleFactor
        sky?.addSublayer(field)

        // the brightest few twinkle, one after another, when the sky stirs
        for (index, point) in bright.enumerated() {
            let star = CALayer()
            star.contents = Arcane.twinkle
            star.bounds = CGRect(x: 0, y: 0, width: 16, height: 16)
            star.position = point
            star.opacity = 0.35
            sky?.addSublayer(star)
            recurring.append(Recurring(every: period, after: next + Double(index) * 0.45) { [weak star] in
                let flash = CAKeyframeAnimation(keyPath: "opacity")
                flash.values = [0.35, 1, 0.5, 0.9, 0.35]
                flash.keyTimes = [0, 0.25, 0.5, 0.7, 1]
                flash.duration = 1.4
                star?.add(flash, forKey: "twinkle")
            })
        }

        // the eclipse, low in the trailing corner
        let centre = CGPoint(x: opening.maxX - 96, y: opening.minY + 96)
        let disc: CGFloat = 30
        let corona = Eclipse.coronaLayer(radius: disc + 1, centre: centre)
        corona.opacity = 0.75
        sky?.addSublayer(corona)
        let moon = CAShapeLayer()
        let moonPath = CGPath(ellipseIn: CGRect(x: centre.x - disc, y: centre.y - disc, width: disc * 2, height: disc * 2),
                              transform: nil)
        moon.frame = bounds
        moon.path = moonPath
        moon.fillColor = Eclipse.night.cgColor
        moon.strokeColor = Eclipse.silver.withAlphaComponent(0.25).cgColor
        moon.lineWidth = 0.8
        sky?.addSublayer(moon)
        recurring.append(Recurring(every: period, after: next) { [weak corona] in
            let flare = CAKeyframeAnimation(keyPath: "opacity")
            flare.values = [0.75, 1, 0.75]
            flare.keyTimes = [0, 0.3, 1]
            flare.duration = 2.6
            corona?.add(flare, forKey: "flare")
            let swell = CAKeyframeAnimation(keyPath: "transform.scale")
            swell.values = [1, 1.12, 1]
            swell.keyTimes = [0, 0.3, 1]
            swell.duration = 2.6
            swell.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            corona?.add(swell, forKey: "swell")
        })

        // a shooting star across the top every other stir
        let streak = CAGradientLayer()
        streak.colors = [NSColor.white.withAlphaComponent(0).cgColor, Eclipse.silver.withAlphaComponent(0.6).cgColor,
                         NSColor.white.cgColor]
        streak.locations = [0, 0.8, 1]
        streak.startPoint = CGPoint(x: 0, y: 0.5)
        streak.endPoint = CGPoint(x: 1, y: 0.5)
        streak.bounds = CGRect(x: 0, y: 0, width: 150, height: 1.6)
        streak.cornerRadius = 0.8
        streak.opacity = 0
        streak.setAffineTransform(CGAffineTransform(rotationAngle: -0.32))
        sky?.addSublayer(streak)
        let from = CGPoint(x: opening.minX + opening.width * 0.25, y: opening.maxY - 70)
        let to = CGPoint(x: from.x + 420, y: from.y - 140)
        recurring.append(Recurring(every: period * 2, after: Recurring.untilNext(period * 2) + 0.8) { [weak streak] in
            let move = CABasicAnimation(keyPath: "position")
            move.fromValue = NSValue(point: from)
            move.toValue = NSValue(point: to)
            move.duration = 0.85
            move.timingFunction = CAMediaTimingFunction(name: .easeIn)
            let fade = CAKeyframeAnimation(keyPath: "opacity")
            fade.values = [0, 1, 0.8, 0]
            fade.keyTimes = [0, 0.15, 0.7, 1]
            fade.duration = 0.85
            let group = CAAnimationGroup()
            group.animations = [move, fade]
            group.duration = 0.85
            streak?.add(group, forKey: "fall")
        })
    }
}

// MARK: - the bar

/// A moonlit hairline along the bar's foot, warming to gold in the middle,
/// with a star sliding along it when the sky stirs.
struct EclipseBarTrim: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { EclipseBarTrimView() }
    func updateNSView(_ view: NSView, context: Context) {}
}

final class EclipseBarTrimView: NSView {
    private let line = CAGradientLayer()
    private let star = CALayer()
    private var gliding: Recurring?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        line.colors = [
            Eclipse.silver.withAlphaComponent(0).cgColor,
            Eclipse.silver.withAlphaComponent(0.55).cgColor,
            Eclipse.gold.withAlphaComponent(0.9).cgColor,
            Eclipse.silver.withAlphaComponent(0.55).cgColor,
            Eclipse.silver.withAlphaComponent(0).cgColor,
        ]
        line.locations = [0, 0.25, 0.5, 0.75, 1]
        line.startPoint = CGPoint(x: 0, y: 0.5)
        line.endPoint = CGPoint(x: 1, y: 0.5)
        line.shadowColor = Eclipse.gold.cgColor
        line.shadowRadius = 4
        line.shadowOpacity = 0.8
        line.shadowOffset = .zero
        layer?.addSublayer(line)

        star.contents = Arcane.twinkle
        star.bounds = CGRect(x: 0, y: 0, width: 12, height: 12)
        star.opacity = 0
        layer?.addSublayer(star)
        gliding = Recurring(every: Eclipse.gustPeriod, after: Recurring.untilNext(Eclipse.gustPeriod) + 0.3) { [weak self] in
            self?.glide()
        }
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        line.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 1)
        line.shadowPath = CGPath(rect: line.bounds, transform: nil)
        star.position = CGPoint(x: -10, y: 0.5)
        CATransaction.commit()
    }

    private func glide() {
        let move = CABasicAnimation(keyPath: "position.x")
        move.fromValue = bounds.width * 0.1
        move.toValue = bounds.width * 0.9
        move.duration = 2.4
        move.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        let fade = CAKeyframeAnimation(keyPath: "opacity")
        fade.values = [0, 1, 1, 0]
        fade.keyTimes = [0, 0.15, 0.85, 1]
        fade.duration = 2.4
        let turn = CABasicAnimation(keyPath: "transform.rotation")
        turn.fromValue = 0
        turn.toValue = -CGFloat.pi
        turn.duration = 2.4
        let group = CAAnimationGroup()
        group.animations = [move, fade, turn]
        group.duration = 2.4
        star.add(group, forKey: "glide")
    }
}

// MARK: - totality

/// Switching to Eclipse plays a total eclipse across every display: the
/// screen dims under a blazing sun, the moon slides across it, a diamond ring
/// flashes on its last edge, and at totality the corona blooms and the stars
/// come out — then the day returns.
enum EclipseTotality {
    private static var panels: [NSPanel] = []

    static func play() {
        guard panels.isEmpty, !Motion.reduced else { return }
        let corona = Eclipse.coronaImage
        panels = NSScreen.screens.map { screen in
            let panel = NSPanel(contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel],
                                backing: .buffered, defer: false)
            panel.level = WindowLevel.overlay
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.ignoresMouseEvents = true
            panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]
            let host = NSHostingView(rootView: EclipseTotalityView(
                gold: Color(nsColor: Eclipse.gold),
                silver: Color(nsColor: Eclipse.silver),
                fringe: Color(nsColor: Eclipse.corona),
                corona: corona
            ))
            host.frame = NSRect(origin: .zero, size: screen.frame.size)
            panel.contentView = host
            panel.orderFrontRegardless()
            return panel
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.9) {
            for panel in panels { panel.orderOut(nil) }
            panels.removeAll()
        }
    }
}

private struct EclipseTotalityView: View {
    let gold: Color
    let silver: Color
    let fringe: Color
    let corona: CGImage

    @State private var covered: CGFloat = 0
    @State private var diamond = false
    @State private var total = false
    @State private var gone = false

    private let sun: CGFloat = 210

    var body: some View {
        ZStack {
            Color(red: 0.01, green: 0.02, blue: 0.06).opacity(gone ? 0 : (total ? 0.62 : 0.3))
            if total {
                Twinkles(colours: [silver, gold, .white], count: 18)
                    .opacity(gone ? 0 : 0.9)
                    .transition(.opacity)
            }
            ZStack {
                Image(decorative: corona, scale: 1)
                    .resizable()
                    .frame(width: sun / 2 / Eclipse.coronaRing * 1.02, height: sun / 2 / Eclipse.coronaRing * 1.02)
                    .scaleEffect(total ? 1.18 : 0.92)
                    .opacity(total ? 1 : 0)
                    .rotationEffect(.degrees(total ? 8 : 0))
                Circle()
                    .fill(RadialGradient(colors: [.white, gold, fringe.opacity(0.9)], center: .center,
                                         startRadius: 4, endRadius: sun / 2))
                    .frame(width: sun, height: sun)
                    .shadow(color: gold, radius: 40)
                    .shadow(color: fringe.opacity(0.6), radius: 90)
                    .opacity(total ? 0 : 1)
                Circle()
                    .fill(Color(nsColor: Eclipse.night))
                    .overlay(Circle().stroke(silver.opacity(total ? 0.35 : 0), lineWidth: 1))
                    .frame(width: sun + 4, height: sun + 4)
                    .offset(x: (1 - covered) * -(sun + 60), y: (1 - covered) * 24)
                // the diamond ring: the last bead of sunlight on the edge
                Image(systemName: "sparkle")
                    .font(.system(size: 58, weight: .ultraLight))
                    .foregroundColor(.white)
                    .shadow(color: gold, radius: 16)
                    .shadow(color: .white, radius: 30)
                    .scaleEffect(diamond ? 1.4 : 0.2)
                    .opacity(diamond ? 1 : 0)
                    .offset(x: sun / 2 - 6, y: -12)
            }
            .scaleEffect(gone ? 1.25 : 1)
            .opacity(gone ? 0 : 1)
            .blur(radius: gone ? 12 : 0)
        }
        .ignoresSafeArea()
        .onAppear {
            withAnimation(.easeOut(duration: 1.45)) { covered = 1 }
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.05) {
                withAnimation(.easeOut(duration: 0.2)) { diamond = true }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                    withAnimation(.easeIn(duration: 0.25)) { diamond = false }
                }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.42) {
                withAnimation(.easeOut(duration: 0.8)) { total = true }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.75) {
                withAnimation(.easeIn(duration: 1)) { gone = true }
            }
        }
    }
}

// MARK: - SwiftUI pieces

/// The corona as a SwiftUI image, its ring at `radius`.
struct CoronaGlow: View {
    var radius: CGFloat

    var body: some View {
        Image(decorative: Eclipse.coronaImage, scale: 1)
            .resizable()
            .frame(width: radius / Eclipse.coronaRing, height: radius / Eclipse.coronaRing)
            .allowsHitTesting(false)
    }
}

/// An eclipse: the moon's dark face, rimmed in moonlight, with the corona
/// blazing round it.
struct EclipseDisc: View {
    let size: CGFloat
    let silver: Color

    var body: some View {
        ZStack {
            CoronaGlow(radius: size / 2 + 1)
            Circle()
                .fill(Color(nsColor: Eclipse.night))
                .overlay(Circle().stroke(silver.opacity(0.35), lineWidth: 0.8))
                .frame(width: size, height: size)
        }
    }
}

/// A thin orbit with a planet going round it.
struct OrbitRing: View {
    let colour: Color
    let planet: Color
    var period: Double = 9

    var body: some View {
        GeometryReader { geo in
            let side = min(geo.size.width, geo.size.height)
            ZStack {
                Circle()
                    .stroke(colour.opacity(0.35), style: StrokeStyle(lineWidth: 0.8, dash: [1, 3]))
                Spinning(period: period) {
                    Circle()
                        .fill(planet)
                        .frame(width: 4.5, height: 4.5)
                        .shadow(color: planet, radius: 3)
                        .offset(y: -side / 2)
                }
            }
            .frame(width: side, height: side)
            .position(x: geo.size.width / 2, y: geo.size.height / 2)
        }
    }
}

/// The moon in a phase: lit `silver`, with the night slid across it. 0 is
/// full, 1 is new.
struct MoonPhase: View {
    let silver: Color
    var shadow: CGFloat = 0.55

    var body: some View {
        GeometryReader { geo in
            let side = min(geo.size.width, geo.size.height)
            ZStack {
                Circle().fill(silver)
                Circle()
                    .fill(Color(nsColor: Eclipse.night).opacity(0.92))
                    .offset(x: side * (1 - shadow) * 0.95)
            }
            .frame(width: side, height: side)
            .clipShape(Circle())
            .position(x: geo.size.width / 2, y: geo.size.height / 2)
        }
    }
}

// MARK: - the kit

/// Eclipse's versions of the kit's pieces (see `Kit.swift`).
enum EclipseKit {
    /// The title: tracked serif capitals in moonlight warming to gold.
    static func title(_ text: String, palette: KitPalette) -> some View {
        Text(text.uppercased())
            .font(.system(size: 15, weight: .medium, design: .serif))
            .tracking(3)
            .foregroundStyle(LinearGradient(colors: [palette.foreground, palette.accent],
                                            startPoint: .leading, endPoint: .trailing))
            .shadow(color: palette.accent.opacity(0.55), radius: 7)
    }
}

/// The header: the icon at the centre of an orbit, the title emerging from
/// behind the moon (a shadow sliding off it), a hairline that warms to gold.
struct EclipseHeader<Trailing: View>: View {
    let icon: String
    let title: String
    var subtitle: String?
    let palette: KitPalette
    let trailing: Trailing

    @State private var arrived = false

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                OrbitRing(colour: palette.highlight, planet: palette.accent, period: 9)
                    .frame(width: 44, height: 44)
                Image(systemName: icon)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(LinearGradient(colors: [palette.accent, palette.highlight],
                                                    startPoint: .top, endPoint: .bottom))
                    .shadow(color: palette.accent, radius: 5)
                    .symbolEffect(.bounce, value: arrived)
            }
            .frame(width: 44, height: 44)
            .scaleEffect(arrived ? 1 : 0.4)
            .opacity(arrived ? 1 : 0)

            VStack(alignment: .leading, spacing: 3) {
                EclipseKit.title(title, palette: palette)
                    .modifier(Waxing(revealed: arrived))
                if let subtitle {
                    Text("☾  " + subtitle)
                        .font(.system(size: 10.5, design: .serif).italic())
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
        .overlay(alignment: .bottom) {
            LinearGradient(colors: [.clear, palette.accent.opacity(0.8), palette.highlight.opacity(0.45), .clear],
                           startPoint: .leading, endPoint: .trailing)
                .frame(height: 1)
                .shadow(color: palette.accent.opacity(0.7), radius: 3)
                .padding(.horizontal, 18)
                .scaleEffect(x: arrived ? 1 : 0.1)
                .opacity(arrived ? 1 : 0)
        }
        .onAppear {
            withAnimation(.easeOut(duration: 0.8).delay(0.1)) { arrived = true }
        }
    }
}

/// Reveals its content the way a crescent waxes: a soft shadow edge sliding
/// off it from leading to trailing.
struct Waxing: ViewModifier, Animatable {
    var progress: CGFloat

    init(revealed: Bool) { progress = revealed ? 1 : 0 }

    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }

    func body(content: Content) -> some View {
        content.mask {
            LinearGradient(
                stops: [
                    .init(color: .black, location: max(0, progress * 1.3 - 0.3)),
                    .init(color: .clear, location: min(1, max(0.0001, progress * 1.3))),
                ],
                startPoint: .leading,
                endPoint: .trailing
            )
        }
    }
}

/// The armed row: a gold-lit band in a moonlit hairline, with a crescent
/// marking it.
struct EclipseCard: ViewModifier {
    let selected: Bool
    let hovered: Bool
    let palette: KitPalette
    let highlight: Namespace.ID

    func body(content: Content) -> some View {
        content
            .background {
                ZStack {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(palette.highlight.opacity(hovered ? 0.07 : 0.025))
                    if selected {
                        ZStack {
                            RoundedRectangle(cornerRadius: 12, style: .continuous)
                                .fill(LinearGradient(colors: [palette.accent.opacity(0.24), palette.highlight.opacity(0.05)],
                                                     startPoint: .leading, endPoint: .trailing))
                            RoundedRectangle(cornerRadius: 12, style: .continuous)
                                .stroke(LinearGradient(colors: [palette.accent.opacity(0.95), palette.highlight.opacity(0.2)],
                                                       startPoint: .topLeading, endPoint: .bottomTrailing),
                                        lineWidth: 1)
                            HStack {
                                MoonPhase(silver: palette.accent, shadow: 0.55)
                                    .frame(width: 9, height: 9)
                                    .shadow(color: palette.accent, radius: 3)
                                    .offset(x: -11)
                                Spacer()
                            }
                        }
                        .shadow(color: palette.accent.opacity(0.35), radius: 10)
                        .matchedGeometryEffect(id: "kit-armed", in: highlight)
                    }
                }
            }
            .animation(.kSelect, value: selected)
            .animation(.kHover, value: hovered)
    }
}

/// The icon bubble: a moon in its phase at rest; eclipsed and blazing with
/// corona while live.
struct EclipseIconBubble: View {
    let symbol: String
    let active: Bool
    let palette: KitPalette
    let size: CGFloat

    var body: some View {
        ZStack {
            if active {
                EclipseDisc(size: size * 0.86, silver: palette.highlight)
                    .transition(.scale(scale: 0.6).combined(with: .opacity))
            } else {
                Circle()
                    .fill(palette.highlight.opacity(0.08))
                    .overlay(Circle().stroke(palette.highlight.opacity(0.3), lineWidth: 1))
                    .frame(width: size, height: size)
            }
            Image(systemName: symbol)
                .font(.system(size: size * 0.36, weight: .medium))
                .foregroundColor(active ? palette.accent : palette.foreground.opacity(0.75))
                .shadow(color: active ? palette.accent : .clear, radius: 4)
                .symbolEffect(.bounce, value: active)
        }
        .frame(width: size + 10, height: size + 10)
        .animation(.kBouncy, value: active)
    }
}
