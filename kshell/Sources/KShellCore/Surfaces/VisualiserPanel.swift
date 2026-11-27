import AppKit
import Combine
import QuartzCore
import SwiftUI

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
            : WindowLevel.visualiser
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

/// The bars, drawn by the GPU.
///
/// One gradient layer is masked to a single path holding every bar, so a new
/// set of levels is one path handed to Core Animation — the render server
/// rasterises and composites it. Drawing them with CoreGraphics in `draw(_:)`
/// (as this used to) filled, stroked and clipped every bar on the main thread
/// for every update, on every screen, which is what made the whole shell lag
/// while music played.
final class VisualiserBarsView: NSView {
    private var config = VisualiserConfig()
    private var base: NSColor = .white
    private var tip: NSColor = .white

    private let fill = CAGradientLayer()
    private let barsMask = CAShapeLayer()
    private let reflection = CAGradientLayer()
    private let reflectionMask = CAShapeLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.masksToBounds = true
        fill.mask = barsMask
        reflection.mask = reflectionMask
        layer?.addSublayer(reflection)
        layer?.addSublayer(fill)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override var isOpaque: Bool { false }
    /// Bottom-left origin: bars grow up from the baseline, reflections below it.
    override var isFlipped: Bool { false }
    override var wantsUpdateLayer: Bool { true }

    func apply(config: VisualiserConfig, theme: Theme) {
        self.config = config
        base = theme.accent?.nsColor ?? .controlAccentColor
        tip = theme.highlight?.nsColor ?? base
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layer?.opacity = Float(config.opacity)
        // The theme's accent along the body, running to the highlight at the
        // tips, so a pink→cyan theme doesn't blend through grey.
        fill.colors = [base.withAlphaComponent(0.30).cgColor, base.cgColor, tip.cgColor]
        fill.locations = [0, 0.55, 1]
        fill.startPoint = CGPoint(x: 0.5, y: 0)
        fill.endPoint = CGPoint(x: 0.5, y: 1)
        reflection.colors = [tip.withAlphaComponent(0.30).cgColor, base.withAlphaComponent(0.04).cgColor]
        reflection.startPoint = CGPoint(x: 0.5, y: 1)
        reflection.endPoint = CGPoint(x: 0.5, y: 0)
        reflection.isHidden = config.mirror <= 0
        CATransaction.commit()
        layoutLayers()
    }

    func setBands(_ values: [Float]) {
        rebuild(values)
    }

    override func layout() {
        super.layout()
        layoutLayers()
    }

    private func layoutLayers() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let size = bounds.size
        let baseline = size.height * (config.mirror > 0 ? 0.22 : 0)
        fill.frame = CGRect(x: 0, y: baseline, width: size.width, height: size.height - baseline)
        barsMask.frame = fill.bounds
        reflection.frame = CGRect(x: 0, y: 0, width: size.width, height: baseline)
        reflectionMask.frame = reflection.bounds
        CATransaction.commit()
    }

    private func rebuild(_ values: [Float]) {
        let size = bounds.size
        guard !values.isEmpty, size.width > 0, size.height > 0 else { return }

        // Centred: every band is drawn twice, mirrored about the middle, so the
        // two halves move as one symmetric shape. Sweep: low to high, left to
        // right.
        let centred = config.layout == "centered"
        let slots = centred ? values.count * 2 : values.count
        let gap = CGFloat(config.gap)
        let barWidth = max((size.width - gap * CGFloat(slots - 1)) / CGFloat(slots), 1)
        let baseline = size.height * (config.mirror > 0 ? 0.22 : 0)
        let maxBar = size.height - baseline
        let radius = min(barWidth / 2, 3)

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
            bars.addRoundedRect(in: CGRect(x: x, y: 0, width: barWidth, height: height),
                                cornerWidth: radius, cornerHeight: radius)
            if config.mirror > 0 {
                let mirrored = min(height * CGFloat(config.mirror), baseline)
                reflections.addRoundedRect(in: CGRect(x: x, y: baseline - mirrored, width: barWidth, height: mirrored),
                                           cornerWidth: min(radius, mirrored / 2), cornerHeight: min(radius, mirrored / 2))
            }
        }

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        barsMask.path = bars
        if config.mirror > 0 { reflectionMask.path = reflections }
        CATransaction.commit()
    }
}

// MARK: - in the bar

/// The visualiser drawn inside the bar, behind its widgets: the same bars as
/// the screen strip, rising from the bar's bottom edge, kept faint so the text
/// on top stays readable. Levels go straight into the view as they arrive, as
/// they do for the strip, so SwiftUI never re-renders the bar per frame.
struct BarVisualiser: NSViewRepresentable {
    let config: VisualiserConfig
    let theme: Theme

    func makeNSView(context: Context) -> VisualiserBarsView {
        let view = VisualiserBarsView(frame: .zero)
        view.wantsLayer = true
        view.alphaValue = MusicPlayer.shared.isPlaying ? 1 : 0
        context.coordinator.attach(view)
        return view
    }

    func updateNSView(_ view: VisualiserBarsView, context: Context) {
        var inBar = config
        inBar.mirror = 0
        inBar.opacity = config.barOpacity
        view.apply(config: inBar, theme: theme)
        context.coordinator.fade = config.fade
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        var fade: Double = 0.5
        private var observers: [AnyCancellable] = []

        func attach(_ view: VisualiserBarsView) {
            observers = [
                MusicPlayer.shared.spectrum.$bands
                    .receive(on: RunLoop.main)
                    .sink { [weak view] values in view?.setBands(values) },
                // Rise with the music and sink away when it stops; the bars
                // keep draining behind the fade.
                MusicPlayer.shared.$isPlaying
                    .removeDuplicates()
                    .receive(on: RunLoop.main)
                    .sink { [weak view, weak self] playing in
                        guard let view else { return }
                        NSAnimationContext.runAnimationGroup { context in
                            context.duration = max(0.05, self?.fade ?? 0.5)
                            context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                            view.animator().alphaValue = playing ? 1 : 0
                        }
                    },
            ]
        }
    }
}
