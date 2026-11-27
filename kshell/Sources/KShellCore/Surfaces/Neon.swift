import AppKit
import QuartzCore
import SwiftUI

/// The Neon theme's look: effects only that theme gets.
///
/// Stark is a cold hologram and Arcane is glow and particles; Neon is a
/// cyberpunk city on tape. Menus strike up like neon tubes and jolt through a
/// glitch as they open, their outline doubled magenta and cyan like a misaligned
/// tape, with scanlines and a tracking band rolling through. Shapes lean
/// forward, titles split into colour fringes, and switching to the theme glitches
/// it in across every display.
///
/// Like the others it keeps every continuous animation off screen-sized
/// layers: the window server recomposites whatever a changing layer covers.
public enum Neon {
    public private(set) static var active = false
    static var magenta = NSColor(red: 1, green: 0.18, blue: 0.59, alpha: 1)
    static var cyan = NSColor(red: 0, green: 0.94, blue: 1, alpha: 1)
    static var yellow = NSColor(red: 1, green: 0.82, blue: 0.25, alpha: 1)

    public static func isNeon(_ theme: Theme) -> Bool {
        theme.effects == "neon" || theme.name?.lowercased() == "neon"
    }

    @discardableResult
    static func update(for theme: Theme) -> Bool {
        let was = active
        active = isNeon(theme)
        if let colour = theme.accent?.nsColor { magenta = colour }
        if let colour = theme.highlight?.nsColor { cyan = colour }
        if let colour = theme.mid?.nsColor { yellow = colour }
        return active && !was
    }

    /// A neon tube switching on: dark, a stutter, half-light, then lit.
    static func strike(_ layer: CALayer?, delay: CFTimeInterval = 0) {
        let strike = CAKeyframeAnimation(keyPath: "opacity")
        strike.values = [0, 1, 0.1, 0.9, 0.3, 0.3, 1, 0.7, 1]
        strike.keyTimes = [0, 0.08, 0.14, 0.24, 0.3, 0.46, 0.54, 0.62, 1]
        strike.duration = 0.55
        strike.beginTime = CACurrentMediaTime() + delay
        strike.fillMode = .backwards
        layer?.add(strike, forKey: "strike")
    }

    /// A glitch: the layer jolts sideways a few times and settles.
    static func jolt(_ layer: CALayer?) {
        let jolt = CAKeyframeAnimation(keyPath: "transform.translation.x")
        jolt.values = [0, -9, 6, 0, 0, 4, -3, 0]
        jolt.keyTimes = [0, 0.1, 0.18, 0.26, 0.55, 0.62, 0.7, 1]
        jolt.duration = 0.45
        layer?.add(jolt, forKey: "jolt")
    }
}

// MARK: - menus

/// Over a Neon menu: the outline doubled — magenta, and cyan a couple of points
/// off it — striking up like tubes; glitch slices that flash across it as it
/// opens; scanlines; and a tracking band that rolls down it now and then.
final class NeonChromeView: NSView, ThemeChrome {
    private let tubeA = CAShapeLayer()
    private let tubeB = CAShapeLayer()
    private let scanlines = CAReplicatorLayer()
    private let scanline = CALayer()
    private let tracking = CAGradientLayer()
    private var slices: [CALayer] = []

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true

        scanline.backgroundColor = NSColor.black.withAlphaComponent(0.16).cgColor
        scanlines.addSublayer(scanline)
        scanlines.instanceTransform = CATransform3DMakeTranslation(0, 3, 0)
        layer?.addSublayer(scanlines)

        tracking.colors = [
            NSColor.white.withAlphaComponent(0).cgColor,
            Neon.cyan.withAlphaComponent(0.09).cgColor,
            NSColor.white.withAlphaComponent(0.05).cgColor,
            Neon.magenta.withAlphaComponent(0.09).cgColor,
            NSColor.white.withAlphaComponent(0).cgColor,
        ]
        tracking.startPoint = CGPoint(x: 0.5, y: 1)
        tracking.endPoint = CGPoint(x: 0.5, y: 0)
        tracking.locations = [-0.3, -0.25, -0.2, -0.15, -0.1]
        layer?.addSublayer(tracking)

        for (tube, colour) in [(tubeA, Neon.magenta), (tubeB, Neon.cyan)] {
            tube.fillColor = nil
            tube.strokeColor = colour.withAlphaComponent(0.9).cgColor
            tube.lineWidth = 1.4
            tube.shadowColor = colour.cgColor
            tube.shadowRadius = 6
            tube.shadowOpacity = 1
            tube.shadowOffset = .zero
            layer?.addSublayer(tube)
        }

        for index in 0..<7 {
            let slice = CALayer()
            slice.backgroundColor = (index % 2 == 0 ? Neon.magenta : Neon.cyan).withAlphaComponent(0.55).cgColor
            slice.opacity = 0
            layer?.addSublayer(slice)
            slices.append(slice)
        }
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func fit(size: NSSize, style: PanelStyle?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let bounds = CGRect(origin: .zero, size: size)
        let path = style.map { PanelGeometry.path(in: bounds.insetBy(dx: 1.5, dy: 1.5), style: $0) }
            ?? CGPath(roundedRect: bounds.insetBy(dx: 1.5, dy: 1.5), cornerWidth: 12, cornerHeight: 12, transform: nil)
        tubeA.frame = bounds
        tubeA.path = path
        tubeA.bakeGlow(shape: path, width: 1.4)
        // the cyan tube sits a little off the magenta one, like a misregistered tape
        tubeB.frame = bounds.offsetBy(dx: 2, dy: -1.5)
        tubeB.path = path
        tubeB.bakeGlow(shape: path, width: 1.4)
        scanline.frame = CGRect(x: 0, y: 0, width: size.width, height: 1)
        scanlines.frame = bounds
        scanlines.instanceCount = Int(size.height / 3) + 1
        tracking.frame = bounds
        for (index, slice) in slices.enumerated() {
            let y = CGFloat((index * 37 + 13) % 100) / 100 * size.height
            let width = size.width * CGFloat(0.25 + Double((index * 53) % 60) / 100)
            let x = CGFloat((index * 29) % 50) / 100 * (size.width - width)
            slice.frame = CGRect(x: x, y: y, width: width, height: CGFloat(2 + index % 3 * 3))
        }
        CATransaction.commit()
    }

    func powerOn() {
        Neon.strike(tubeA)
        Neon.strike(tubeB, delay: 0.06)

        // glitch slices flashing across as it opens
        for (index, slice) in slices.enumerated() {
            let flash = CAKeyframeAnimation(keyPath: "opacity")
            flash.values = [0, 1, 0, 1, 0]
            flash.keyTimes = [0, 0.2, 0.45, 0.6, 1]
            flash.duration = 0.32
            flash.beginTime = CACurrentMediaTime() + Double(index) * 0.035
            slice.add(flash, forKey: "flash")
            let slide = CAKeyframeAnimation(keyPath: "transform.translation.x")
            slide.values = [0, CGFloat(index % 2 == 0 ? 30 : -24), CGFloat(index % 2 == 0 ? -12 : 16)]
            slide.duration = 0.32
            slide.beginTime = flash.beginTime
            slice.add(slide, forKey: "slide")
        }

        // the tracking band rolls down every few seconds
        let roll = CABasicAnimation(keyPath: "locations")
        roll.fromValue = [-0.3, -0.25, -0.2, -0.15, -0.1]
        roll.toValue = [1.1, 1.15, 1.2, 1.25, 1.3]
        roll.duration = 1.8
        roll.timingFunction = CAMediaTimingFunction(name: .linear)
        let group = CAAnimationGroup()
        group.animations = [roll]
        group.duration = 6.5
        group.beginTime = CACurrentMediaTime() + 1.2
        group.repeatCount = .infinity
        tracking.add(group, forKey: "roll")
    }
}

// MARK: - the frame

/// Over the frame, per display: two neon tubes tracing the opening, magenta
/// and cyan a hair apart, still and baked; and in each corner a short tube
/// that flickers now and then like a failing sign — small layers, so the
/// flicker costs only their corner.
final class NeonFramePanel: NSPanel {
    let frameScreen: NSScreen

    init(screen: NSScreen, inset: CGFloat, radius: CGFloat) {
        frameScreen = screen
        super.init(contentRect: screen.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        level = WindowLevel.surface
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        isMovable = false
        hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

        let root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        root.wantsLayer = true
        contentView = root
        let bounds = root.bounds
        let opening = bounds.insetBy(dx: inset, dy: inset)
        let path = CGPath(roundedRect: opening, cornerWidth: radius, cornerHeight: radius, transform: nil)

        for (index, colour) in [Neon.magenta, Neon.cyan].enumerated() {
            let tube = CAShapeLayer()
            tube.frame = bounds.offsetBy(dx: index == 0 ? 0 : 1.5, dy: index == 0 ? 0 : -1)
            tube.path = path
            tube.fillColor = nil
            tube.strokeColor = colour.withAlphaComponent(index == 0 ? 0.85 : 0.55).cgColor
            tube.lineWidth = 1.3
            tube.shadowColor = colour.cgColor
            tube.shadowRadius = 7
            tube.shadowOpacity = 1
            tube.shadowOffset = .zero
            tube.bakeGlow(shape: path, width: 1.3)
            root.layer?.addSublayer(tube)
        }

        // corner tubes: an angled stroke in each corner, flickering out of step
        let arm: CGFloat = 46
        let corners: [(CGPoint, CGFloat, CGFloat)] = [
            (CGPoint(x: opening.minX + 12, y: opening.minY + 12), 1, 1),
            (CGPoint(x: opening.maxX - 12, y: opening.minY + 12), -1, 1),
            (CGPoint(x: opening.minX + 12, y: opening.maxY - 12), 1, -1),
            (CGPoint(x: opening.maxX - 12, y: opening.maxY - 12), -1, -1),
        ]
        for (index, (corner, dx, dy)) in corners.enumerated() {
            let box = CGRect(x: min(corner.x, corner.x + dx * arm) - 10, y: min(corner.y, corner.y + dy * arm) - 10,
                             width: arm + 20, height: arm + 20)
            let tube = CAShapeLayer()
            tube.frame = box
            let local = CGPoint(x: corner.x - box.minX, y: corner.y - box.minY)
            let shape = CGMutablePath()
            shape.move(to: CGPoint(x: local.x + dx * arm, y: local.y))
            shape.addLine(to: CGPoint(x: local.x + dx * 10, y: local.y))
            shape.addLine(to: CGPoint(x: local.x, y: local.y + dy * 10))
            shape.addLine(to: CGPoint(x: local.x, y: local.y + dy * arm))
            tube.path = shape
            tube.fillColor = nil
            let colour = index % 2 == 0 ? Neon.cyan : Neon.yellow
            tube.strokeColor = colour.cgColor
            tube.lineWidth = 2
            tube.lineCap = .round
            tube.shadowColor = colour.cgColor
            tube.shadowRadius = 6
            tube.shadowOpacity = 1
            tube.shadowOffset = .zero
            tube.bakeGlow(shape: shape, width: 2)
            root.layer?.addSublayer(tube)

            let flicker = CAKeyframeAnimation(keyPath: "opacity")
            flicker.values = [1, 1, 0.2, 1, 0.4, 1, 1]
            flicker.keyTimes = [0, 0.86, 0.87, 0.89, 0.9, 0.92, 1]
            flicker.duration = 5 + Double(index) * 1.3
            flicker.repeatCount = .infinity
            tube.add(flicker, forKey: "flicker")
        }
    }
}

// MARK: - the bar

/// The bar's Neon trim: a tube along its foot running magenta into cyan, and a
/// glint that runs along it every so often.
struct NeonBarTrim: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { NeonBarTrimView() }
    func updateNSView(_ view: NSView, context: Context) {}
}

final class NeonBarTrimView: NSView {
    private let tube = CAGradientLayer()
    private let glint = CAGradientLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = false
        tube.colors = [Neon.magenta.cgColor, Neon.magenta.cgColor, Neon.cyan.cgColor, Neon.cyan.cgColor]
        tube.locations = [0, 0.35, 0.65, 1]
        tube.startPoint = CGPoint(x: 0, y: 0.5)
        tube.endPoint = CGPoint(x: 1, y: 0.5)
        tube.shadowColor = Neon.magenta.cgColor
        tube.shadowRadius = 4
        tube.shadowOpacity = 1
        tube.shadowOffset = .zero
        layer?.addSublayer(tube)

        glint.colors = [
            NSColor.white.withAlphaComponent(0).cgColor,
            NSColor.white.withAlphaComponent(0.9).cgColor,
            NSColor.white.withAlphaComponent(0).cgColor,
        ]
        glint.startPoint = CGPoint(x: 0, y: 0.5)
        glint.endPoint = CGPoint(x: 1, y: 0.5)
        layer?.addSublayer(glint)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        tube.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 1.5)
        tube.shadowPath = CGPath(rect: tube.bounds, transform: nil)
        glint.frame = CGRect(x: -120, y: 0, width: 120, height: 1.5)
        CATransaction.commit()

        glint.removeAnimation(forKey: "run")
        let run = CABasicAnimation(keyPath: "position.x")
        run.fromValue = -60
        run.toValue = bounds.width + 60
        run.duration = 1.6
        run.timingFunction = CAMediaTimingFunction(name: .easeIn)
        let group = CAAnimationGroup()
        group.animations = [run]
        group.duration = 7
        group.repeatCount = .infinity
        glint.add(group, forKey: "run")
    }
}

// MARK: - SwiftUI pieces

/// A rectangle leaning forward, like italic type.
struct SlantShape: InsettableShape {
    var slant: CGFloat = 8
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: inset, dy: inset)
        let s = min(slant, r.width / 3)
        var path = Path()
        path.move(to: CGPoint(x: r.minX + s, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX - s, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX, y: r.maxY))
        path.closeSubpath()
        return path
    }

    func inset(by amount: CGFloat) -> SlantShape {
        var copy = self
        copy.inset += amount
        return copy
    }
}

/// Text split into colour fringes: cyan a little left, magenta a little right,
/// the text itself on top — and, now and then, the fringes jump apart.
struct ChromaText: View {
    let text: String
    let font: Font
    let magenta: Color
    let cyan: Color
    var foreground: Color = .white

    @State private var glitch = false

    var body: some View {
        ZStack {
            Text(text).font(font).foregroundColor(cyan.opacity(0.85)).offset(x: glitch ? -4 : -1.2, y: glitch ? 1 : 0)
            Text(text).font(font).foregroundColor(magenta.opacity(0.85)).offset(x: glitch ? 4 : 1.2)
            Text(text).font(font).foregroundColor(foreground)
        }
        .onAppear { schedule() }
    }

    private func schedule() {
        DispatchQueue.main.asyncAfter(deadline: .now() + Double.random(in: 2.5...6)) {
            withAnimation(.linear(duration: 0.04)) { glitch = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.09) {
                withAnimation(.linear(duration: 0.05)) { glitch = false }
                schedule()
            }
        }
    }
}

/// A synthwave sun: yellow into magenta, stripes cut out of its lower half,
/// drifting down.
struct SynthSun: View {
    let yellow: Color
    let magenta: Color

    @State private var drift: CGFloat = 0

    var body: some View {
        GeometryReader { geo in
            let size = min(geo.size.width, geo.size.height)
            Circle()
                .fill(LinearGradient(colors: [yellow, magenta], startPoint: .top, endPoint: .bottom))
                .mask {
                    VStack(spacing: 0) {
                        Rectangle().frame(height: size * 0.5)
                        ForEach(0..<7, id: \.self) { index in
                            Rectangle()
                                .frame(height: size * 0.5 / 7 - CGFloat(index) * 0.9 - 1.5)
                                .padding(.bottom, CGFloat(index) * 0.9 + 1.5)
                        }
                    }
                    .offset(y: drift)
                }
                .frame(width: size, height: size)
                .shadow(color: magenta.opacity(0.7), radius: 18)
        }
    }
}

// MARK: - the glitch-in

/// Switching to Neon glitches it in across every display: a dark flash, slices
/// of colour tearing across, a big title in colour fringes, a tape-rewind wipe
/// of scanlines, gone.
enum NeonBoot {
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
            panel.contentView = NSHostingView(rootView: NeonBootView(
                magenta: Color(nsColor: Neon.magenta),
                cyan: Color(nsColor: Neon.cyan),
                yellow: Color(nsColor: Neon.yellow)
            ))
            panel.orderFrontRegardless()
            return panel
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.9) {
            for panel in panels { panel.orderOut(nil) }
            panels.removeAll()
        }
    }
}

private struct NeonBootView: View {
    let magenta: Color
    let cyan: Color
    let yellow: Color

    @State private var start = Date()

    var body: some View {
        TimelineView(.animation) { context in
            let t = context.date.timeIntervalSince(start)
            GeometryReader { geo in
                ZStack {
                    Color.black.opacity(t < 0.12 ? 0.75 : (t < 2.2 ? 0.45 : max(0, 0.45 - (t - 2.2) * 0.8)))

                    // slices tearing across, re-drawn every few frames
                    let step = Int(t * 18)
                    ForEach(0..<9, id: \.self) { index in
                        let seed = Double((index * 73 + step * 31) % 97) / 97
                        let y = seed * geo.size.height
                        let w = geo.size.width * (0.2 + (Double((index * 41 + step * 17) % 60) / 100))
                        let on = t < 0.9 || (t > 2.0 && t < 2.35)
                        Rectangle()
                            .fill(index % 3 == 0 ? magenta : (index % 3 == 1 ? cyan : yellow))
                            .frame(width: w, height: CGFloat(2 + index % 4 * 4))
                            .position(x: CGFloat(Double((index * 61 + step * 13) % 100) / 100) * geo.size.width, y: y)
                            .opacity(on && (index + step) % 3 != 0 ? 0.8 : 0)
                    }

                    VStack(spacing: 10) {
                        ChromaText(text: "KSHELL", font: .system(size: 96, weight: .black).italic(),
                                   magenta: magenta, cyan: cyan)
                            .offset(x: t < 0.9 ? CGFloat(sin(t * 90) * 8) : 0)
                        Text("// NEON ONLINE")
                            .font(.system(size: 16, weight: .bold, design: .monospaced))
                            .tracking(6)
                            .foregroundColor(yellow)
                            .shadow(color: yellow, radius: 8)
                            .opacity(t > 0.5 ? 1 : 0)
                    }
                    .shadow(color: magenta.opacity(0.8), radius: 20)
                    .opacity(t < 0.15 ? 0 : (t < 2.2 ? (Int(t * 30) % 11 == 0 ? 0.3 : 1) : max(0, 1 - (t - 2.2) * 2.5)))
                    .scaleEffect(x: t > 2.2 ? 1 + (t - 2.2) * 2 : 1, y: t > 2.2 ? max(0.02, 1 - (t - 2.2) * 3) : 1)

                    // the rewind wipe: scanlines sweeping up
                    if t > 1.9 {
                        Rectangle()
                            .fill(LinearGradient(colors: [cyan.opacity(0), cyan.opacity(0.5), magenta.opacity(0.5), magenta.opacity(0)],
                                                 startPoint: .top, endPoint: .bottom))
                            .frame(height: 140)
                            .position(x: geo.size.width / 2, y: geo.size.height * (1 - min(1, (t - 1.9) / 0.6)))
                    }
                }
            }
        }
        .onAppear { start = Date() }
    }
}
