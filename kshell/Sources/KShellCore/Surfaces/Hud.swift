import AppKit
import QuartzCore
import SwiftUI

/// The Stark theme's HUD: effects only that theme gets.
///
/// With Stark on, the shell stops looking like frosted glass and starts looking
/// like a projection. Menus power on with a flicker and draw their corner
/// brackets in, carry scanlines and a slow scan band, and take a targeting
/// reticle for their selection instead of a filled card. The frame grows
/// glowing corner brackets with a light that runs round the screen's edge, the
/// bar gets a ruled baseline with a sweep, and switching to the theme plays a
/// short boot sequence across every display.
///
/// Everything that animates continuously here is a Core Animation animation, so
/// it runs in the render server rather than costing the shell a redraw per
/// frame.
public enum Hud {
    /// Whether the HUD is on: the active theme is Stark, or says `effects = "hud"`.
    public private(set) static var active = false
    /// The theme's accent and its brightest colour, for the AppKit layers.
    static var accent: NSColor = NSColor(red: 0.22, green: 0.85, blue: 1, alpha: 1)
    static var ice: NSColor = NSColor(red: 0.9, green: 0.97, blue: 1, alpha: 1)
    static var alert: NSColor = NSColor(red: 1, green: 0.3, blue: 0.24, alpha: 1)

    public static func isHud(_ theme: Theme) -> Bool {
        theme.effects == "hud" || theme.name?.lowercased() == "stark"
    }

    /// Follow the theme. Returns true when the HUD has just switched on, which
    /// is when the boot sequence plays.
    @discardableResult
    static func update(for theme: Theme) -> Bool {
        let was = active
        active = isHud(theme)
        if let colour = theme.accent?.nsColor { accent = colour }
        if let colour = theme.highlight?.nsColor { ice = colour }
        if let colour = theme.mid?.nsColor { alert = colour }
        return active && !was
    }
}

// MARK: - menus

/// Laid over a menu's content: corner brackets that draw themselves in, a
/// glowing trace of the outline, scanlines, and a scan band that drifts down
/// the panel. It ignores the mouse.
final class HudChromeView: NSView {
    private let outline = CAShapeLayer()
    private let brackets = CAShapeLayer()
    private let scanlines = CAReplicatorLayer()
    private let scanline = CALayer()
    private let sweep = CAGradientLayer()
    private let ticks = CAShapeLayer()
    private var style: PanelStyle?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true

        scanline.backgroundColor = NSColor.white.withAlphaComponent(0.035).cgColor
        scanlines.addSublayer(scanline)
        scanlines.instanceTransform = CATransform3DMakeTranslation(0, 3, 0)
        layer?.addSublayer(scanlines)

        sweep.colors = [
            Hud.accent.withAlphaComponent(0).cgColor,
            Hud.accent.withAlphaComponent(0.10).cgColor,
            Hud.accent.withAlphaComponent(0).cgColor,
        ]
        sweep.startPoint = CGPoint(x: 0.5, y: 0)
        sweep.endPoint = CGPoint(x: 0.5, y: 1)
        layer?.addSublayer(sweep)

        outline.fillColor = nil
        outline.strokeColor = Hud.accent.withAlphaComponent(0.45).cgColor
        outline.lineWidth = 1
        outline.shadowColor = Hud.accent.cgColor
        outline.shadowRadius = 4
        outline.shadowOpacity = 0.8
        outline.shadowOffset = .zero
        layer?.addSublayer(outline)

        ticks.fillColor = nil
        ticks.strokeColor = Hud.accent.withAlphaComponent(0.5).cgColor
        ticks.lineWidth = 1
        layer?.addSublayer(ticks)

        brackets.fillColor = nil
        brackets.strokeColor = Hud.ice.cgColor
        brackets.lineWidth = 2
        brackets.lineCap = .square
        brackets.shadowColor = Hud.accent.cgColor
        brackets.shadowRadius = 6
        brackets.shadowOpacity = 1
        brackets.shadowOffset = .zero
        layer?.addSublayer(brackets)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// Fit the chrome to the panel's outline at `size`.
    func fit(size: NSSize, style: PanelStyle?) {
        self.style = style
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let bounds = CGRect(origin: .zero, size: size)
        let path = style.map { PanelGeometry.path(in: bounds.insetBy(dx: 0.5, dy: 0.5), style: $0) }
            ?? CGPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), cornerWidth: 14, cornerHeight: 14, transform: nil)
        outline.path = path
        outline.frame = bounds
        brackets.frame = bounds
        brackets.path = Self.bracketPath(in: bounds.insetBy(dx: 9, dy: 9), arm: 18)
        ticks.frame = bounds
        ticks.path = Self.tickPath(in: bounds)
        scanline.frame = CGRect(x: 0, y: 0, width: size.width, height: 1)
        scanlines.frame = bounds
        scanlines.instanceCount = Int(size.height / 3) + 1
        sweep.frame = CGRect(x: 0, y: 0, width: size.width, height: 90)
        CATransaction.commit()
    }

    /// The opening flourish: brackets and outline draw in, the scan band starts
    /// its slow fall.
    func powerOn() {
        let draw = CABasicAnimation(keyPath: "strokeEnd")
        draw.fromValue = 0
        draw.toValue = 1
        draw.duration = 0.55
        draw.timingFunction = CAMediaTimingFunction(controlPoints: 0.2, 0.9, 0.3, 1)
        brackets.add(draw, forKey: "draw")
        let trace = draw.copy() as! CABasicAnimation
        trace.duration = 0.8
        outline.add(trace, forKey: "draw")

        let fall = CABasicAnimation(keyPath: "position.y")
        fall.fromValue = bounds.height + 60
        fall.toValue = -60
        fall.duration = 4.5
        fall.repeatCount = .infinity
        sweep.add(fall, forKey: "fall")

        let breathe = CABasicAnimation(keyPath: "shadowRadius")
        breathe.fromValue = 4
        breathe.toValue = 9
        breathe.duration = 1.6
        breathe.autoreverses = true
        breathe.repeatCount = .infinity
        brackets.add(breathe, forKey: "breathe")
    }

    /// A hologram switching on: the content stutters up to full strength.
    static func flicker(_ layer: CALayer?) {
        let flicker = CAKeyframeAnimation(keyPath: "opacity")
        flicker.values = [0, 0.9, 0.15, 1, 0.45, 1, 0.8, 1]
        flicker.keyTimes = [0, 0.12, 0.2, 0.34, 0.42, 0.6, 0.72, 1]
        flicker.duration = 0.42
        layer?.add(flicker, forKey: "flicker")
    }

    static func bracketPath(in rect: CGRect, arm: CGFloat) -> CGPath {
        let path = CGMutablePath()
        let corners: [(CGPoint, CGFloat, CGFloat)] = [
            (CGPoint(x: rect.minX, y: rect.minY), 1, 1),
            (CGPoint(x: rect.maxX, y: rect.minY), -1, 1),
            (CGPoint(x: rect.minX, y: rect.maxY), 1, -1),
            (CGPoint(x: rect.maxX, y: rect.maxY), -1, -1),
        ]
        for (corner, dx, dy) in corners {
            path.move(to: CGPoint(x: corner.x + dx * arm, y: corner.y))
            path.addLine(to: corner)
            path.addLine(to: CGPoint(x: corner.x, y: corner.y + dy * arm))
        }
        return path
    }

    /// A ruler along the top edge, centred: short ticks, a long one every fifth.
    static func tickPath(in rect: CGRect) -> CGPath {
        let path = CGMutablePath()
        let count = 21
        let spacing: CGFloat = 7
        let start = rect.midX - CGFloat(count - 1) / 2 * spacing
        for index in 0..<count {
            let x = start + CGFloat(index) * spacing
            let long = index % 5 == 0
            path.move(to: CGPoint(x: x, y: rect.maxY - 3))
            path.addLine(to: CGPoint(x: x, y: rect.maxY - (long ? 10 : 6)))
        }
        return path
    }
}

// MARK: - the frame

/// Over the frame, per display: a glowing trace of the frame's inner opening,
/// bright brackets in its four corners, and a light that runs round it. Click
/// through, like the frame itself.
final class HudFramePanel: NSPanel {
    let frameScreen: NSScreen
    private let trace = CAShapeLayer()
    private let runner = CAShapeLayer()
    private let corners = CAShapeLayer()

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

        let opening = root.bounds.insetBy(dx: inset, dy: inset)
        let path = CGPath(roundedRect: opening, cornerWidth: radius, cornerHeight: radius, transform: nil)

        trace.path = path
        trace.fillColor = nil
        trace.strokeColor = Hud.accent.withAlphaComponent(0.35).cgColor
        trace.lineWidth = 1
        trace.shadowColor = Hud.accent.cgColor
        trace.shadowRadius = 5
        trace.shadowOpacity = 0.9
        trace.shadowOffset = .zero
        root.layer?.addSublayer(trace)

        // A short bright dash that runs the whole way round every few seconds.
        runner.path = path
        runner.fillColor = nil
        runner.strokeColor = Hud.ice.cgColor
        runner.lineWidth = 2
        runner.lineCap = .round
        runner.shadowColor = Hud.accent.cgColor
        runner.shadowRadius = 8
        runner.shadowOpacity = 1
        runner.shadowOffset = .zero
        runner.strokeStart = 0
        runner.strokeEnd = 0.04
        root.layer?.addSublayer(runner)

        corners.path = HudChromeView.bracketPath(in: opening.insetBy(dx: 10, dy: 10), arm: 34)
        corners.fillColor = nil
        corners.strokeColor = Hud.ice.cgColor
        corners.lineWidth = 2
        corners.lineCap = .square
        corners.shadowColor = Hud.accent.cgColor
        corners.shadowRadius = 7
        corners.shadowOpacity = 1
        corners.shadowOffset = .zero
        root.layer?.addSublayer(corners)

        animate()
    }

    private func animate() {
        let run = CAKeyframeAnimation(keyPath: "strokeStart")
        run.values = [0, 0.96]
        let runEnd = CAKeyframeAnimation(keyPath: "strokeEnd")
        runEnd.values = [0.04, 1]
        let group = CAAnimationGroup()
        group.animations = [run, runEnd]
        group.duration = 7
        group.repeatCount = .infinity
        group.timingFunction = CAMediaTimingFunction(name: .linear)
        runner.add(group, forKey: "run")

        let pulse = CABasicAnimation(keyPath: "opacity")
        pulse.fromValue = 0.55
        pulse.toValue = 1
        pulse.duration = 1.8
        pulse.autoreverses = true
        pulse.repeatCount = .infinity
        pulse.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        corners.add(pulse, forKey: "pulse")

        let draw = CABasicAnimation(keyPath: "strokeEnd")
        draw.fromValue = 0
        draw.toValue = 1
        draw.duration = 1.2
        draw.timingFunction = CAMediaTimingFunction(controlPoints: 0.2, 0.9, 0.3, 1)
        trace.add(draw, forKey: "draw")
    }
}

// MARK: - the bar

/// The bar's HUD trim: a ruled baseline that fades out at both ends, ticks
/// along it, and a light that sweeps the length of the bar now and then. Core
/// Animation runs the sweep, so the bar is not redrawn for it.
struct HudBarTrim: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = HudBarTrimView()
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {}
}

final class HudBarTrimView: NSView {
    private let baseline = CAGradientLayer()
    private let ticks = CAShapeLayer()
    private let sweep = CAGradientLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true

        baseline.colors = [
            Hud.accent.withAlphaComponent(0).cgColor,
            Hud.accent.withAlphaComponent(0.7).cgColor,
            Hud.accent.withAlphaComponent(0.7).cgColor,
            Hud.accent.withAlphaComponent(0).cgColor,
        ]
        baseline.locations = [0, 0.15, 0.85, 1]
        baseline.startPoint = CGPoint(x: 0, y: 0.5)
        baseline.endPoint = CGPoint(x: 1, y: 0.5)
        baseline.shadowColor = Hud.accent.cgColor
        baseline.shadowRadius = 3
        baseline.shadowOpacity = 1
        baseline.shadowOffset = .zero
        layer?.addSublayer(baseline)

        ticks.strokeColor = Hud.accent.withAlphaComponent(0.35).cgColor
        ticks.lineWidth = 1
        layer?.addSublayer(ticks)

        sweep.colors = [
            Hud.ice.withAlphaComponent(0).cgColor,
            Hud.ice.withAlphaComponent(0.22).cgColor,
            Hud.ice.withAlphaComponent(0).cgColor,
        ]
        sweep.startPoint = CGPoint(x: 0, y: 0.5)
        sweep.endPoint = CGPoint(x: 1, y: 0.5)
        layer?.addSublayer(sweep)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let width = bounds.width
        baseline.frame = CGRect(x: 0, y: 0, width: width, height: 1)
        ticks.frame = bounds
        let path = CGMutablePath()
        var x: CGFloat = 24
        var index = 0
        while x < width - 24 {
            let long = index % 8 == 0
            path.move(to: CGPoint(x: x, y: 1))
            path.addLine(to: CGPoint(x: x, y: long ? 5 : 3))
            x += 12
            index += 1
        }
        ticks.path = path
        sweep.frame = CGRect(x: -220, y: 0, width: 220, height: bounds.height)
        CATransaction.commit()

        sweep.removeAnimation(forKey: "sweep")
        let run = CABasicAnimation(keyPath: "position.x")
        run.fromValue = -110
        run.toValue = width + 110
        run.duration = 2.6
        run.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        let group = CAAnimationGroup()
        group.animations = [run]
        group.duration = 9
        group.repeatCount = .infinity
        sweep.add(group, forKey: "sweep")
    }
}

// MARK: - SwiftUI pieces

/// Four corner brackets around a rectangle.
struct BracketShape: Shape {
    var arm: CGFloat = 10

    func path(in rect: CGRect) -> Path {
        Path(HudChromeView.bracketPath(in: rect, arm: min(arm, rect.width / 2, rect.height / 2)))
    }
}

/// A rectangle with its corners cut off — the HUD's panel shape.
struct ChamferShape: InsettableShape {
    var cut: CGFloat = 8
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: inset, dy: inset)
        let c = min(cut, r.width / 2, r.height / 2)
        var path = Path()
        path.move(to: CGPoint(x: r.minX + c, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX - c, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX, y: r.minY + c))
        path.addLine(to: CGPoint(x: r.maxX, y: r.maxY - c))
        path.addLine(to: CGPoint(x: r.maxX - c, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX + c, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX, y: r.maxY - c))
        path.addLine(to: CGPoint(x: r.minX, y: r.minY + c))
        path.closeSubpath()
        return path
    }

    func inset(by amount: CGFloat) -> ChamferShape {
        var copy = self
        copy.inset += amount
        return copy
    }
}

/// A flat-topped hexagon.
struct HexagonShape: InsettableShape {
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: inset, dy: inset)
        let w = r.width, h = r.height
        var path = Path()
        path.move(to: CGPoint(x: r.minX + w * 0.25, y: r.minY))
        path.addLine(to: CGPoint(x: r.minX + w * 0.75, y: r.minY))
        path.addLine(to: CGPoint(x: r.maxX, y: r.midY))
        path.addLine(to: CGPoint(x: r.minX + w * 0.75, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX + w * 0.25, y: r.maxY))
        path.addLine(to: CGPoint(x: r.minX, y: r.midY))
        path.closeSubpath()
        _ = h
        return path
    }

    func inset(by amount: CGFloat) -> HexagonShape {
        var copy = self
        copy.inset += amount
        return copy
    }
}

/// A dashed ring that turns forever.
struct SpinningRing: View {
    let colour: Color
    var dash: [CGFloat] = [6, 4]
    var width: CGFloat = 1.2
    var period: Double = 8
    var clockwise = true

    @State private var turned = false

    var body: some View {
        Circle()
            .stroke(colour, style: StrokeStyle(lineWidth: width, dash: dash))
            .rotationEffect(.degrees(turned ? (clockwise ? 360 : -360) : 0))
            .onAppear {
                withAnimation(.linear(duration: period).repeatForever(autoreverses: false)) { turned = true }
            }
    }
}

/// Text that types itself out, with a block cursor while it does.
struct TypewriterText: View {
    let text: String
    var delay: Double = 0
    var speed: Double = 0.022

    @State private var shown = 0

    var body: some View {
        Text(String(text.prefix(shown)) + (shown < text.count ? "▍" : ""))
            .onAppear {
                for index in 0...text.count {
                    DispatchQueue.main.asyncAfter(deadline: .now() + delay + Double(index) * speed) {
                        shown = index
                    }
                }
            }
    }
}

// MARK: - boot sequence

/// Plays the HUD's start-up across every display when the Stark theme comes
/// on: the screen dims, rings blossom out of the centre, a status log types
/// itself, a scan line sweeps down, and it all fades back to the desktop. Click
/// through the whole time.
enum HudBoot {
    private static var panels: [NSPanel] = []

    static func play() {
        guard panels.isEmpty, !Motion.reduced else { return }
        panels = NSScreen.screens.map { screen in
            let panel = NSPanel(
                contentRect: screen.frame,
                styleMask: [.borderless, .nonactivatingPanel],
                backing: .buffered,
                defer: false
            )
            panel.level = WindowLevel.overlay
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.ignoresMouseEvents = true
            panel.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]
            panel.contentView = NSHostingView(rootView: HudBootView(
                accent: Color(nsColor: Hud.accent),
                ice: Color(nsColor: Hud.ice),
                alert: Color(nsColor: Hud.alert)
            ))
            panel.orderFrontRegardless()
            return panel
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.4) {
            for panel in panels { panel.orderOut(nil) }
            panels.removeAll()
        }
    }
}

private struct HudBootView: View {
    let accent: Color
    let ice: Color
    let alert: Color

    @State private var phase = 0
    @State private var scan: CGFloat = 0

    private let lines = [
        "KSHELL // HUD INITIALISING",
        "› display surfaces ............ OK",
        "› glass + frame ................ OK",
        "› audio spectrum ............... OK",
        "› notification relay ........... OK",
        "› theme  STARK ............. ONLINE",
    ]

    var body: some View {
        GeometryReader { geo in
            ZStack {
                Color.black.opacity(phase == 0 ? 0 : (phase >= 3 ? 0 : 0.55))

                // rings blossoming out of the centre
                ZStack {
                    ForEach(0..<3, id: \.self) { index in
                        Circle()
                            .stroke(accent.opacity(0.7), lineWidth: 1.5)
                            .frame(width: 180 + CGFloat(index) * 120, height: 180 + CGFloat(index) * 120)
                            .scaleEffect(phase >= 1 ? 1 : 0.2)
                            .opacity(phase >= 1 && phase < 3 ? 1 : 0)
                            .animation(.kBouncy.delay(Double(index) * 0.09), value: phase)
                    }
                    SpinningRing(colour: accent, dash: [2, 6], width: 3, period: 3)
                        .frame(width: 140, height: 140)
                    SpinningRing(colour: ice.opacity(0.8), dash: [30, 12], width: 1.5, period: 5, clockwise: false)
                        .frame(width: 520, height: 520)
                    HexagonShape()
                        .stroke(ice, lineWidth: 2)
                        .frame(width: 70, height: 62)
                        .shadow(color: accent, radius: 12)
                    Circle()
                        .fill(ice)
                        .frame(width: 14, height: 14)
                        .shadow(color: accent, radius: 18)
                }
                .opacity(phase >= 1 && phase < 3 ? 1 : 0)
                .scaleEffect(phase >= 3 ? 1.6 : 1)
                .blur(radius: phase >= 3 ? 12 : 0)

                // the status log
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Array(lines.enumerated()), id: \.offset) { index, line in
                        if phase >= 1 {
                            TypewriterText(text: line, delay: 0.15 + Double(index) * 0.22, speed: 0.012)
                                .foregroundColor(index == lines.count - 1 ? ice : accent.opacity(0.85))
                        }
                    }
                }
                .font(.system(size: 13, weight: .medium, design: .monospaced))
                .shadow(color: accent.opacity(0.8), radius: 6)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
                .padding(.leading, 70)
                .padding(.bottom, 90)
                .opacity(phase >= 3 ? 0 : 1)

                // the scan line
                Rectangle()
                    .fill(LinearGradient(colors: [accent.opacity(0), ice.opacity(0.9), accent.opacity(0)],
                                         startPoint: .leading, endPoint: .trailing))
                    .frame(height: 2)
                    .shadow(color: accent, radius: 10)
                    .position(x: geo.size.width / 2, y: scan * geo.size.height)
                    .opacity(phase == 2 ? 1 : 0)
            }
            .frame(width: geo.size.width, height: geo.size.height)
            .animation(.easeInOut(duration: 0.45), value: phase)
        }
        .onAppear {
            phase = 1
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.9) {
                phase = 2
                withAnimation(.easeInOut(duration: 0.7)) { scan = 1 }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.6) { phase = 3 }
        }
    }
}
