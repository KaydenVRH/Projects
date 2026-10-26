import AppKit
import QuartzCore
import SwiftUI

/// The screen edge an overlay panel is anchored to — the edge it grows out of.
public enum OverlayEdge: Sendable {
    /// Rises out of the bottom edge of the screen (top corners rounded).
    case bottom
    /// Grows out of the left edge of the screen (trailing corners rounded).
    case leading
    /// Grows out of the right edge of the screen (leading corners rounded).
    case trailing
    /// Drops out of the top edge — or, with a margin, from under the bar — so it
    /// reads as extending from the bar (bottom corners rounded).
    case top

    /// The corners a sheet on this edge rounds convexly.
    var sheetCorners: SheetShape.Corners {
        switch self {
        case .bottom: return .top
        case .leading: return .trailing
        case .trailing: return .leading
        case .top: return .bottom
        }
    }
}

/// Where a panel sits along the screen edge it is anchored to.
public enum OverlayAlign: Sendable {
    case leading, center, trailing
}

/// A reusable, key-capable panel that grows out of a screen edge.
///
/// Opening is a fluid morph rather than a slide: the panel starts as a small
/// pill resting on the edge it is anchored to and swells into its full size —
/// widening about its centre and reaching away from the edge — with its outline
/// and its glass travelling with it. The content is laid out at its final size
/// and anchored to that same edge, so the growing outline *reveals* it instead
/// of reflowing it.
///
/// The morph is stepped frame by frame rather than handed to Core Animation.
/// That is deliberate: when the frame is on, this panel's glass lives on the
/// frame's surface (see `SurfaceGlass`), and the window server renders that blur
/// from model geometry — so an animating view frame or mask would not move it.
/// Stepping keeps the glass and the content locked together.
public final class OverlayPanel: NSPanel {
    static let showDuration: TimeInterval = 0.42
    static let hideDuration: TimeInterval = 0.26
    /// How far the pill reaches along the edge, and out from it.
    private static let seedLength: CGFloat = 300
    private static let seedDepth: CGFloat = 44

    private let shownFrame: NSRect
    private let edge: OverlayEdge
    private let size: NSSize
    private let style: PanelStyle?
    private let makeGlass: ((NSScreen) -> SurfaceGlass?)?
    private let targetScreen: NSScreen
    /// The outline to step while the frame is off: same rounding, no fillets.
    private let fallbackStyle: PanelStyle

    private let container = NSView()
    private let mask = CAShapeLayer()
    private weak var host: NSView?
    private var glass: SurfaceGlass?
    private var ticker: DispatchSourceTimer?
    private var isShown = false

    public init(
        screen: NSScreen,
        size: NSSize,
        edge: OverlayEdge = .bottom,
        margin: CGFloat = 0,
        align: OverlayAlign = .center,
        appearance: BarConfig = BarConfig(),
        style: PanelStyle? = nil,
        glass: ((NSScreen) -> SurfaceGlass?)? = nil,
        content: AnyView
    ) {
        let screenFrame = screen.frame
        // Line a top-anchored panel up with the bar's content.
        let inset = CGFloat(appearance.paddingX)
        func alignedX(_ width: CGFloat) -> CGFloat {
            switch align {
            case .leading: return screenFrame.minX + inset
            case .trailing: return screenFrame.maxX - inset - width
            case .center: return screenFrame.midX - width / 2
            }
        }
        let x: CGFloat
        let y: CGFloat
        switch edge {
        case .bottom:
            x = alignedX(size.width)
            y = screenFrame.minY + margin
        case .top:
            // `margin` is the distance from the screen's top edge (the bar's
            // height), so the panel's top sits flush against the bar's bottom.
            x = alignedX(size.width)
            y = screenFrame.maxY - margin - size.height
        case .leading:
            x = screenFrame.minX + margin
            y = screenFrame.midY - size.height / 2
        case .trailing:
            x = screenFrame.maxX - margin - size.width
            y = screenFrame.midY - size.height / 2
        }
        // The window always sits at its final spot; only the content grows.
        self.shownFrame = NSRect(x: x, y: y, width: size.width, height: size.height)
        self.targetScreen = screen
        self.edge = edge
        self.size = size
        self.style = style
        self.makeGlass = glass
        self.fallbackStyle = style
            ?? PanelStyle(radius: SheetShape.defaultRadius, filletRadius: 0, attachment: .none)

        super.init(
            contentRect: shownFrame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        // Light themes need a light appearance, or their panels keep the dark
        // system material under a light tint and the content's adaptive colours
        // stay pale on a pale surface.
        self.appearance = appearance.background.vibrancyAppearance
        // Above the surface holder and the bar's content window.
        level = WindowLevel.overlay
        isOpaque = false
        backgroundColor = .clear
        // The media centre has no shadow, and these should read as the same
        // surface as it does — a shadow made them look like a separate window.
        hasShadow = false
        isMovable = false
        isMovableByWindowBackground = false
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        // No `.fullScreenAuxiliary`: the panels step aside for full-screen apps.
        collectionBehavior = [.canJoinAllSpaces, .ignoresCycle]

        // The container is what morphs — the window stays put — and it is masked
        // to the current outline so the content is revealed by the growing shape.
        container.wantsLayer = true
        mask.fillColor = NSColor.black.cgColor
        container.layer?.mask = mask

        let host = NSHostingView(rootView: content)
        host.frame = NSRect(origin: .zero, size: size)
        host.autoresizingMask = []
        container.addSubview(host)
        self.host = host
        contentView = container

        apply(progress: 0)
    }

    public override var canBecomeKey: Bool { true }
    public override var canBecomeMain: Bool { false }

    public override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    public var isPresented: Bool { isShown }

    /// The pill the panel starts as: resting on the anchored edge, centred along
    /// it, so growing it out is symmetric.
    private var seedFrame: NSRect {
        switch edge {
        case .bottom:
            let length = min(size.width, Self.seedLength)
            return NSRect(x: shownFrame.midX - length / 2, y: shownFrame.minY,
                          width: length, height: min(size.height, Self.seedDepth))
        case .top:
            let length = min(size.width, Self.seedLength)
            let depth = min(size.height, Self.seedDepth)
            return NSRect(x: shownFrame.midX - length / 2, y: shownFrame.maxY - depth,
                          width: length, height: depth)
        case .leading:
            let length = min(size.height, Self.seedLength)
            return NSRect(x: shownFrame.minX, y: shownFrame.midY - length / 2,
                          width: min(size.width, Self.seedDepth), height: length)
        case .trailing:
            let length = min(size.height, Self.seedLength)
            let depth = min(size.width, Self.seedDepth)
            return NSRect(x: shownFrame.maxX - depth, y: shownFrame.midY - length / 2,
                          width: depth, height: length)
        }
    }

    public func show(animated: Bool) {
        guard !isShown else { return }
        isShown = true

        // Take a piece of the frame's glass, if the frame is providing it.
        glass = makeGlass?(targetScreen)
        apply(progress: 0)

        Frontmost.claim()
        makeKeyAndOrderFront(nil)
        makeKey()

        guard animated else {
            apply(progress: 1)
            return
        }
        // Grow on the next run loop turn: a morph started in the same turn the
        // window is first ordered front is skipped.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isShown else { return }
            self.morph(to: 1, duration: Self.showDuration, opening: true)
        }
    }

    public func hide(animated: Bool) {
        guard isShown else { return }
        isShown = false

        guard animated else {
            finishHide()
            return
        }
        morph(to: 0, duration: Self.hideDuration, opening: false)
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.hideDuration + 0.04) { [weak self] in
            guard let self, !self.isShown else { return }
            self.finishHide()
        }
    }

    public func toggle(animated: Bool) {
        isShown ? hide(animated: animated) : show(animated: animated)
    }

    private func finishHide() {
        ticker?.cancel()
        ticker = nil
        glass?.park()
        glass = nil
        orderOut(nil)
        // The keyboard came with us; give it back now the panel is gone.
        Frontmost.handBack()
    }

    /// Step the panel from where it is to `progress` (0 = the pill, 1 = open).
    private func morph(to target: CGFloat, duration: TimeInterval, opening: Bool) {
        ticker?.cancel()
        let from = progress
        let start = CACurrentMediaTime()
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 1.0 / 60.0)
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            let t = min(1, max(0, (CACurrentMediaTime() - start) / duration))
            self.apply(progress: from + (target - from) * SlideEasing.value(t, opening: opening))
            if t >= 1 {
                self.ticker?.cancel()
                self.ticker = nil
            }
        }
        ticker = timer
        timer.resume()
    }

    private var progress: CGFloat = 0

    /// Put the panel, its outline, its content and its glass at `progress`.
    ///
    /// Everything is set to its model value each tick rather than animated: the
    /// window server renders an effect view's blur from the layer's model
    /// geometry, so an animated frame or mask would leave the glass behind.
    private func apply(progress value: CGFloat) {
        progress = value
        let seed = seedFrame
        let rect = NSRect(
            x: seed.minX + (shownFrame.minX - seed.minX) * value,
            y: seed.minY + (shownFrame.minY - seed.minY) * value,
            width: seed.width + (shownFrame.width - seed.width) * value,
            height: seed.height + (shownFrame.height - seed.height) * value
        )
        let style = self.style ?? fallbackStyle

        // The container is the window's frame, less the shown frame's origin.
        let local = NSRect(
            x: rect.minX - shownFrame.minX,
            y: rect.minY - shownFrame.minY,
            width: rect.width,
            height: rect.height
        )
        container.frame = local

        let shape = CGMutablePath()
        shape.addPath(PanelGeometry.path(
            in: NSRect(origin: .zero, size: local.size),
            style: style
        ))
        mask.path = shape

        // The content keeps its final size and stays pinned to the anchored
        // edge, so the growing outline reveals it. It is centred along the edge,
        // which is the axis the pill grows about.
        if let host {
            var origin = CGPoint.zero
            switch edge {
            case .bottom: origin = CGPoint(x: (local.width - size.width) / 2, y: 0)
            case .top: origin = CGPoint(x: (local.width - size.width) / 2, y: local.height - size.height)
            case .leading: origin = CGPoint(x: 0, y: (local.height - size.height) / 2)
            case .trailing: origin = CGPoint(x: local.width - size.width, y: (local.height - size.height) / 2)
            }
            host.frame = NSRect(origin: origin, size: size)
        }

        glass?.setResting(rect, style: style)
    }
}
