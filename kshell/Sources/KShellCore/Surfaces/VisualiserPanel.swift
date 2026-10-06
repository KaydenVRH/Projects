import AppKit
import Combine
import QuartzCore

/// The desktop audio visualiser: a strip of cava-style bars that rises while
/// music plays and fades away when it stops.
///
/// It is decoration only — the window ignores the mouse — and it carries no
/// material view, so its fade can be an ordinary window alpha animation rather
/// than one of the shell's stepped slides.
///
/// `layer` decides where it sits: `background` puts it on the desktop behind
/// windows, `overlay` just below the frame's surface (above ordinary windows).
public final class VisualiserPanel: NSPanel {
    /// The display this strip belongs to.
    public private(set) var visualiserScreen: NSScreen
    private let bars = VisualiserBarsView(frame: .zero)
    private var config = VisualiserConfig()
    private var observer: AnyCancellable?
    private var shown = false

    /// `overlay` just under the frame's surface (above ordinary windows), or the
    /// desktop itself for `background`.
    private static func level(for config: VisualiserConfig) -> NSWindow.Level {
        config.layer == "background"
            ? NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.desktopIconWindow)) + 1)
            : NSWindow.Level(rawValue: NSWindow.Level.statusBar.rawValue - 1)
    }

    /// Where the strip sits: the screen's width inside the frame's opening, at
    /// the bottom edge.
    private static func frame(
        for screen: NSScreen,
        config: VisualiserConfig,
        margin: CGFloat
    ) -> NSRect {
        let inset = CGFloat(config.inset)
        return NSRect(
            x: screen.frame.minX + margin + inset,
            y: screen.frame.minY + margin + inset,
            width: max(screen.frame.width - 2 * (margin + inset), 1),
            height: max(CGFloat(config.height), 8)
        )
    }

    public init(
        screen: NSScreen,
        config: VisualiserConfig,
        theme: Theme,
        margin: CGFloat
    ) {
        self.visualiserScreen = screen
        self.config = config

        super.init(
            contentRect: VisualiserPanel.frame(for: screen, config: config, margin: margin),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        level = VisualiserPanel.level(for: config)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        isMovable = false
        hidesOnDeactivate = false
        // No `.fullScreenAuxiliary`: full-screen apps should cover it.
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

        bars.frame = NSRect(origin: .zero, size: frame.size)
        bars.autoresizingMask = [.width, .height]
        bars.apply(config: config, theme: theme)
        contentView = bars

        alphaValue = 0

        // The levels are pushed straight into the view: no SwiftUI update hop
        // between the tap and the pixels, and AppKit coalesces the redraws to the
        // display's refresh rate.
        observer = MusicPlayer.shared.spectrum.$bands
            .receive(on: RunLoop.main)
            .sink { [weak self] values in self?.bars.setBands(values) }
    }

    public override var canBecomeKey: Bool { false }
    public override var canBecomeMain: Bool { false }

    public override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    /// Re-point the strip at a display, or re-colour it, without rebuilding the
    /// window — a theme change then never makes it blink.
    public func apply(config: VisualiserConfig, theme: Theme, margin: CGFloat, screen: NSScreen) {
        self.config = config
        visualiserScreen = screen
        level = VisualiserPanel.level(for: config)
        setFrame(VisualiserPanel.frame(for: screen, config: config, margin: margin), display: true)
        bars.frame = NSRect(origin: .zero, size: frame.size)
        bars.apply(config: config, theme: theme)
    }

    /// Fade the strip in or out. The bars keep falling behind the fade, so a
    /// pause drains rather than snaps.
    public func setPlaying(_ playing: Bool, animated: Bool = true) {
        if playing == shown, animated { return }
        shown = playing
        let alpha: CGFloat = playing ? 1 : 0
        guard animated else {
            alphaValue = alpha
            if playing { orderFrontRegardless() } else { orderOut(nil) }
            return
        }
        if playing { orderFrontRegardless() }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = max(0.05, config.fade)
            context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            animator().alphaValue = alpha
        } completionHandler: { [weak self] in
            guard let self, !playing else { return }
            self.orderOut(nil)
        }
    }

    public func present() {
        if shown { orderFrontRegardless() }
    }

    public func dismiss() {
        orderOut(nil)
    }
}

// MARK: - the bars

/// Draws the bars with CoreGraphics, as few passes over the whole strip rather
/// than one per bar: a single path holds every bar, so the gradient fill, the
/// glow and the reflection are three draws in total, however many bars there
/// are. The levels arrive at the tap's rate, which is the animation — there is
/// nothing on a timer here.
final class VisualiserBarsView: NSView {
    private var config = VisualiserConfig()
    private var base: NSColor = .white
    private var tip: NSColor = .white
    private var bands: [Float] = []

    func apply(config: VisualiserConfig, theme: Theme) {
        self.config = config
        base = theme.accent?.nsColor ?? .controlAccentColor
        tip = theme.highlight?.nsColor ?? base
        needsDisplay = true
    }

    func setBands(_ values: [Float]) {
        bands = values
        needsDisplay = true
    }

    override var isOpaque: Bool { false }
    /// Bottom-left origin: bars grow up from the baseline, reflections below it.
    override var isFlipped: Bool { false }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        let size = bounds.size
        let values = bands
        guard !values.isEmpty, size.width > 0, size.height > 0 else { return }

        // Centred: every band is drawn twice, mirrored about the middle, so the
        // two halves move as one symmetric shape. Sweep: low to high, left to
        // right.
        let centred = config.layout == "centered"
        let slots = centred ? values.count * 2 : values.count
        let gap = CGFloat(config.gap)
        let barWidth = max((size.width - gap * CGFloat(slots - 1)) / CGFloat(slots), 1)
        let baselineFraction: CGFloat = config.mirror > 0 ? 0.22 : 0
        let baseline = size.height * baselineFraction
        let maxBar = size.height * (1 - baselineFraction)
        let radius = min(barWidth / 2, 3)

        /// Which band a slot draws: the innermost slot is the lowest band.
        func band(_ slot: Int) -> Int {
            guard centred else { return slot }
            return slot < values.count ? values.count - 1 - slot : slot - values.count
        }

        let bars = CGMutablePath()
        let reflections = CGMutablePath()
        for slot in 0..<slots {
            let level = CGFloat(values[band(slot)])
            let height = max(maxBar * level, 2)
            let x = CGFloat(slot) * (barWidth + gap)
            bars.addPath(CGPath(
                roundedRect: CGRect(x: x, y: baseline, width: barWidth, height: height),
                cornerWidth: radius, cornerHeight: radius, transform: nil
            ))
            guard config.mirror > 0 else { continue }
            let mirrored = height * CGFloat(config.mirror)
            reflections.addPath(CGPath(
                roundedRect: CGRect(x: x, y: baseline - mirrored, width: barWidth, height: mirrored),
                cornerWidth: radius, cornerHeight: radius, transform: nil
            ))
        }

        ctx.saveGState()
        ctx.setAlpha(CGFloat(config.opacity))
        ctx.setShouldAntialias(true)

        // Glow: a couple of soft outlines around the whole path. A blurred shadow
        // pass looks marginally softer but costs several times as much per frame,
        // and at this size the two are indistinguishable.
        for (width, alpha) in [(9.0, 0.05), (4.0, 0.09)] {
            ctx.setStrokeColor(base.withAlphaComponent(alpha).cgColor)
            ctx.setLineWidth(width)
            ctx.addPath(bars)
            ctx.strokePath()
        }

        // Reflections, fading away from the baseline.
        if config.mirror > 0, let gradient = gradient([
            tip.withAlphaComponent(0.30), base.withAlphaComponent(0.04),
        ], locations: [0, 1]) {
            ctx.saveGState()
            ctx.addPath(reflections)
            ctx.clip()
            ctx.drawLinearGradient(
                gradient,
                start: CGPoint(x: 0, y: baseline),
                end: CGPoint(x: 0, y: baseline - maxBar * CGFloat(config.mirror)),
                options: []
            )
            ctx.restoreGState()
        }

        // The bars themselves: the theme's accent along the body, running to the
        // highlight at the tips, so a pink→cyan theme doesn't blend through grey.
        if let gradient = gradient([
            base.withAlphaComponent(0.30), base, tip,
        ], locations: [0, 0.55, 1]) {
            ctx.saveGState()
            ctx.addPath(bars)
            ctx.clip()
            ctx.drawLinearGradient(
                gradient,
                start: CGPoint(x: 0, y: baseline),
                end: CGPoint(x: 0, y: baseline + maxBar),
                options: []
            )
            ctx.restoreGState()
        }

        ctx.restoreGState()
    }

    private func gradient(_ colors: [NSColor], locations: [CGFloat]) -> CGGradient? {
        CGGradient(
            colorsSpace: CGColorSpaceCreateDeviceRGB(),
            colors: colors.map(\.cgColor) as CFArray,
            locations: locations
        )
    }
}
