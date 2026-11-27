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
    /// The bright line a CRT collapses to as it switches off.
    private let collapse = CAGradientLayer()

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

        collapse.colors = [
            Neon.cyan.withAlphaComponent(0).cgColor,
            NSColor.white.cgColor,
            Neon.magenta.withAlphaComponent(0).cgColor,
        ]
        collapse.startPoint = CGPoint(x: 0.5, y: 0)
        collapse.endPoint = CGPoint(x: 0.5, y: 1)
        collapse.opacity = 0
        collapse.shadowColor = Neon.cyan.cgColor
        collapse.shadowRadius = 8
        collapse.shadowOpacity = 1
        collapse.shadowOffset = .zero
        layer?.addSublayer(collapse)

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
        collapse.bounds = CGRect(x: 0, y: 0, width: size.width, height: 6)
        collapse.position = CGPoint(x: size.width / 2, y: size.height / 2)
        collapse.shadowPath = CGPath(rect: collapse.bounds.insetBy(dx: 0, dy: 2), transform: nil)
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

        // While it's open, a bad patch of tape now and then: two slices tear
        // across for a moment. Small layers, so it costs only their strips.
        for (index, slice) in slices.enumerated() where index == 2 || index == 5 {
            let tear = CAKeyframeAnimation(keyPath: "opacity")
            tear.values = [0, 0, 0.9, 0, 0.7, 0, 0]
            tear.keyTimes = [0, 0.9, 0.905, 0.915, 0.92, 0.93, 1]
            tear.duration = index == 2 ? 7.3 : 9.1
            tear.beginTime = CACurrentMediaTime() + 0.5
            tear.repeatCount = .infinity
            slice.add(tear, forKey: "tear")
        }
    }

    /// Switching off like an old CRT: the tubes stutter out, the tape tears,
    /// and the picture collapses into a bright line across the middle that
    /// shrinks to nothing — over the panel's own close, not instead of it.
    func powerOff() {
        let now = CACurrentMediaTime()
        for (index, tube) in [tubeA, tubeB].enumerated() {
            let out = CAKeyframeAnimation(keyPath: "opacity")
            out.values = [1, 0.2, 1, 0.1, 0.6, 0]
            out.keyTimes = [0, 0.15, 0.3, 0.5, 0.65, 1]
            out.duration = 0.28
            out.beginTime = now + Double(index) * 0.03
            out.fillMode = .forwards
            out.isRemovedOnCompletion = false
            tube.add(out, forKey: "strike")
        }
        for (index, slice) in slices.enumerated() {
            slice.removeAnimation(forKey: "tear")
            let flash = CAKeyframeAnimation(keyPath: "opacity")
            flash.values = [0, 1, 0, 0.8, 0]
            flash.keyTimes = [0, 0.25, 0.5, 0.7, 1]
            flash.duration = 0.2
            flash.beginTime = now + Double(index % 4) * 0.025
            slice.add(flash, forKey: "flash")
        }
        let line = CAAnimationGroup()
        let fade = CAKeyframeAnimation(keyPath: "opacity")
        fade.values = [0, 1, 1, 0]
        fade.keyTimes = [0, 0.15, 0.7, 1]
        let squeeze = CAKeyframeAnimation(keyPath: "transform.scale.x")
        squeeze.values = [1, 1, 0.02]
        squeeze.keyTimes = [0, 0.35, 1]
        squeeze.timingFunctions = [CAMediaTimingFunction(name: .linear), CAMediaTimingFunction(name: .easeIn)]
        line.animations = [fade, squeeze]
        line.duration = 0.3
        line.beginTime = now + 0.04
        collapse.add(line, forKey: "collapse")
    }
}

// MARK: - the frame

/// Over the frame, per display: two neon tubes tracing the opening, magenta
/// and cyan a hair apart, still and baked; and in each corner a short tube
/// that flickers now and then like a failing sign — small layers, so the
/// flicker costs only their corner.
final class NeonFramePanel: FrameOverlayPanel {

    init(screen: NSScreen, inset: CGFloat, radius: CGFloat) {
        super.init(screen: screen)

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
            // The corner signs reach into the opening: behind windows.
            backdrop.root.layer?.addSublayer(tube)

            let flicker = CAKeyframeAnimation(keyPath: "opacity")
            flicker.values = [1, 1, 0.2, 1, 0.4, 1, 1]
            flicker.keyTimes = [0, 0.86, 0.87, 0.89, 0.9, 0.92, 1]
            flicker.duration = 5 + Double(index) * 1.3
            flicker.repeatCount = .infinity
            tube.add(flicker, forKey: "flicker")
        }

        addSigns(in: opening, scale: screen.backingScaleFactor)
    }

    /// Street signs on the desktop, behind the windows: a vertical katakana
    /// sign down the right side — ネオン街, "neon town" — in a cyan tube box,
    /// one character of it buzzing now and then like a failing tube; and a
    /// little "Open 24H" in script low on the left. Each is a small baked
    /// layer, so only its own few hundred points ever flicker.
    private func addSigns(in opening: CGRect, scale: CGFloat) {
        guard opening.height > 420, opening.width > 600 else { return }
        let root = backdrop.root.layer

        // the vertical sign
        let glyphs = ["ネ", "オ", "ン", "街"]
        let font = NSFont(name: "HiraginoSans-W6", size: 30) ?? .boldSystemFont(ofSize: 30)
        let cell: CGFloat = 40
        let box = CGRect(x: opening.maxX - 78, y: opening.midY - cell * CGFloat(glyphs.count) / 2 - 14,
                         width: 50, height: cell * CGFloat(glyphs.count) + 28)
        let frame = CAShapeLayer()
        frame.frame = box.insetBy(dx: -12, dy: -12)
        let outline = CGPath(roundedRect: CGRect(x: 12, y: 12, width: box.width, height: box.height),
                             cornerWidth: 7, cornerHeight: 7, transform: nil)
        frame.path = outline
        frame.fillColor = NSColor.black.withAlphaComponent(0.35).cgColor
        frame.strokeColor = Neon.cyan.withAlphaComponent(0.9).cgColor
        frame.lineWidth = 1.6
        frame.shadowColor = Neon.cyan.cgColor
        frame.shadowRadius = 7
        frame.shadowOpacity = 1
        frame.shadowOffset = .zero
        frame.bakeGlow(shape: outline, width: 1.6)
        root?.addSublayer(frame)

        for (index, glyph) in glyphs.enumerated() {
            let text = Self.glowText(glyph, font: font, core: NSColor(red: 1, green: 0.82, blue: 0.93, alpha: 1),
                                     glow: Neon.magenta, scale: scale)
            text.position = CGPoint(x: box.midX, y: box.maxY - 14 - cell * (CGFloat(index) + 0.5))
            root?.addSublayer(text)
            // the old tube: buzzing out and back now and then
            if index == 3 {
                let buzz = CAKeyframeAnimation(keyPath: "opacity")
                buzz.values = [1, 1, 0.15, 0.9, 0.1, 0.6, 0.2, 1, 1]
                buzz.keyTimes = [0, 0.78, 0.785, 0.8, 0.81, 0.83, 0.845, 0.86, 1]
                buzz.duration = 6.7
                buzz.repeatCount = .infinity
                text.add(buzz, forKey: "buzz")
            }
        }

        // "Open 24H"
        let script = NSFont(name: "SnellRoundhand-Black", size: 34) ?? .boldSystemFont(ofSize: 28)
        let open = Self.glowText("Open", font: script, core: NSColor(red: 1, green: 0.8, blue: 0.92, alpha: 1),
                                 glow: Neon.magenta, scale: scale)
        open.position = CGPoint(x: opening.minX + 120, y: opening.minY + 52)
        root?.addSublayer(open)
        let hours = Self.glowText("24H", font: .monospacedSystemFont(ofSize: 13, weight: .heavy),
                                  core: NSColor(red: 1, green: 0.95, blue: 0.75, alpha: 1), glow: Neon.yellow, scale: scale)
        hours.position = CGPoint(x: open.frame.maxX + hours.bounds.width / 2 + 4, y: open.position.y - 6)
        root?.addSublayer(hours)
        let blink = CAKeyframeAnimation(keyPath: "opacity")
        blink.values = [1, 1, 0.1, 1, 0.1, 1, 1]
        blink.keyTimes = [0, 0.9, 0.91, 0.93, 0.94, 0.96, 1]
        blink.duration = 9.3
        blink.repeatCount = .infinity
        hours.add(blink, forKey: "blink")
    }

    /// Lettering like a lit tube: a pale core with the tube's colour glowing
    /// round it, baked.
    private static func glowText(_ string: String, font: NSFont, core: NSColor, glow: NSColor, scale: CGFloat) -> CATextLayer {
        let layer = CATextLayer()
        let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: core]
        layer.string = NSAttributedString(string: string, attributes: attributes)
        let size = (string as NSString).size(withAttributes: attributes)
        layer.bounds = CGRect(x: 0, y: 0, width: ceil(size.width) + 6, height: ceil(size.height) + 4)
        layer.alignmentMode = .center
        layer.contentsScale = scale
        layer.shadowColor = glow.cgColor
        layer.shadowRadius = 6
        layer.shadowOpacity = 1
        layer.shadowOffset = .zero
        layer.shouldRasterize = true
        layer.rasterizationScale = scale
        return layer
    }
}

// MARK: - the pointer

/// A streak of light after the pointer when it moves quickly — magenta, with a
/// cyan copy a hair off it — gone within a fraction of a second. Slow, careful
/// pointing leaves nothing. Like Arcane's sparks, the windows only exist while
/// there is something to draw: a screen-sized window costs a composite every
/// frame anything beneath it moves.
final class NeonCursorTrail: CursorTrail {
    private struct Screen {
        let panel: NSPanel
        let magenta: CAShapeLayer
        let cyan: CAShapeLayer
    }

    private var screens: [Screen] = []
    private var timer: DispatchSourceTimer?
    private var points: [(point: CGPoint, time: CFTimeInterval)] = []
    private var last = NSPoint(x: -1, y: -1)
    private var shown = false
    private var hiddenScreens: [NSRect] = []
    /// How long a point of the streak lasts.
    private let life: CFTimeInterval = 0.16
    /// Pointer speed (points a frame) below which nothing is drawn.
    private let threshold: CGFloat = 7

    init() {
        screens = NSScreen.screens.map { screen in
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
            func streak(_ colour: NSColor, width: CGFloat) -> CAShapeLayer {
                let layer = CAShapeLayer()
                layer.frame = root.bounds
                layer.fillColor = nil
                layer.strokeColor = colour.cgColor
                layer.lineWidth = width
                layer.lineCap = .round
                layer.lineJoin = .round
                layer.shadowColor = colour.cgColor
                layer.shadowRadius = 5
                layer.shadowOpacity = 0.9
                layer.shadowOffset = .zero
                root.layer?.addSublayer(layer)
                return layer
            }
            let cyan = streak(Neon.cyan.withAlphaComponent(0.7), width: 1.6)
            cyan.setAffineTransform(CGAffineTransform(translationX: 2, y: -1.5))
            let magenta = streak(Neon.magenta.withAlphaComponent(0.9), width: 2.4)
            return Screen(panel: panel, magenta: magenta, cyan: cyan)
        }
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 1.0 / 60)
        timer.setEventHandler { [weak self] in self?.follow() }
        timer.resume()
        self.timer = timer
    }

    private func follow() {
        let now = CACurrentMediaTime()
        let mouse = NSEvent.mouseLocation
        let moved = hypot(mouse.x - last.x, mouse.y - last.y)
        last = mouse
        if moved >= threshold {
            points.append((mouse, now))
        } else if !points.isEmpty, moved > 0.5 {
            // slowing down: the streak still follows the tip as it fades
            points.append((mouse, now))
        }
        points.removeAll { now - $0.time > life }
        guard points.count >= 2 else {
            if shown { hide() }
            return
        }
        if !shown {
            shown = true
            for screen in screens where !hiddenScreens.contains(screen.panel.frame) {
                screen.panel.orderFrontRegardless()
            }
        }
        for screen in screens {
            let frame = screen.panel.frame
            let path = CGMutablePath()
            let local = points.filter { frame.contains($0.point) }
                .map { CGPoint(x: $0.point.x - frame.minX, y: $0.point.y - frame.minY) }
            if local.count >= 2 { path.addLines(between: local) }
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            for layer in [screen.magenta, screen.cyan] {
                layer.path = path
                layer.shadowPath = local.count >= 2
                    ? path.copy(strokingWithWidth: 3, lineCap: .round, lineJoin: .round, miterLimit: 4) : nil
            }
            // the streak dims as its newest point ages
            let age = now - (points.last?.time ?? now)
            screen.magenta.opacity = Float(max(0, 1 - age / life))
            screen.cyan.opacity = screen.magenta.opacity * 0.8
            CATransaction.commit()
        }
    }

    private func hide() {
        shown = false
        for screen in screens {
            screen.magenta.path = nil
            screen.cyan.path = nil
            screen.panel.orderOut(nil)
        }
    }

    func stop() {
        timer?.cancel()
        timer = nil
        for screen in screens { screen.panel.orderOut(nil) }
        screens.removeAll()
    }

    func setHidden(_ hidden: Bool, on screen: NSScreen) {
        hiddenScreens.removeAll { $0 == screen.frame }
        if hidden { hiddenScreens.append(screen.frame) }
        for entry in screens where entry.panel.frame == screen.frame && hidden {
            entry.panel.orderOut(nil)
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
