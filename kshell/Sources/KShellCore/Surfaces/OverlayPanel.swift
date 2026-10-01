import AppKit
import QuartzCore
import SwiftUI

/// The screen edge an overlay panel is anchored to — the edge it appears to
/// extend out of (and slides in from).
public enum OverlayEdge: Sendable {
    /// Rises out of the bottom edge of the screen (top corners rounded).
    case bottom
    /// Slides in from the left edge of the screen (trailing corners rounded).
    case leading
    /// Slides in from the right edge of the screen (leading corners rounded).
    case trailing
    /// Drops down from the top edge — or, with a margin, from under the bar —
    /// so it reads as extending from the bar (bottom corners rounded).
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

/// A reusable, key-capable panel that slides in from a screen edge.
///
/// The slide is stepped frame by frame rather than handed to Core Animation, and
/// its content is masked to the panel's outline. Both are deliberate: when the
/// frame is on, this panel's glass lives on the frame's surface (see
/// `SurfaceGlass`), and the window server renders that blur from model geometry
/// — so an animating view frame would not move it. Stepping keeps the glass and
/// the content locked together.
public final class OverlayPanel: NSPanel {
    static let showDuration: TimeInterval = 0.32
    static let hideDuration: TimeInterval = 0.20

    private let shownFrame: NSRect
    private let edge: OverlayEdge
    private let size: NSSize
    private let style: PanelStyle?
    private let makeGlass: ((NSScreen) -> SurfaceGlass?)?
    private let targetScreen: NSScreen

    private weak var host: NSView?
    private var glass: SurfaceGlass?
    private var ticker: DispatchSourceTimer?
    private var isShown = false
    private var offset: CGSize = .zero

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
        // The window always sits at its final spot; only the content slides.
        self.shownFrame = NSRect(x: x, y: y, width: size.width, height: size.height)
        self.targetScreen = screen
        self.edge = edge
        self.size = size
        self.style = style
        self.makeGlass = glass

        super.init(
            contentRect: shownFrame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        // Above the surface holder and the bar's content window.
        level = NSWindow.Level(rawValue: NSWindow.Level.statusBar.rawValue + 2)
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

        // The content lives in a container masked to the panel's outline, so the
        // tint and the panel's glass agree about where the edges are.
        let container = NSView(frame: NSRect(origin: .zero, size: size))
        container.wantsLayer = true
        if let style {
            let mask = CAShapeLayer()
            mask.fillColor = NSColor.black.cgColor
            mask.path = PanelGeometry.path(in: CGRect(origin: .zero, size: size), style: style)
            container.layer?.mask = mask
        }

        let host = NSHostingView(rootView: content)
        host.frame = NSRect(origin: .zero, size: size)
        host.autoresizingMask = []
        container.addSubview(host)
        self.host = host
        contentView = container
    }

    public override var canBecomeKey: Bool { true }
    public override var canBecomeMain: Bool { false }

    public override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    public var isPresented: Bool { isShown }

    /// Where the panel waits before sliding in — past the edge it comes from.
    private var hiddenOffset: CGSize {
        switch edge {
        case .bottom: return CGSize(width: 0, height: -size.height)
        case .leading: return CGSize(width: -size.width, height: 0)
        case .trailing: return CGSize(width: size.width, height: 0)
        case .top: return CGSize(width: 0, height: size.height)
        }
    }

    public func show(animated: Bool) {
        guard !isShown else { return }
        isShown = true

        // Take a piece of the frame's glass, if the frame is providing it.
        glass = makeGlass?(targetScreen)
        glass?.setResting(shownFrame, style: style)

        // Lay the content out flat, then slide it in.
        offset = animated ? hiddenOffset : .zero
        applyOffset()
        NSApp.activate(ignoringOtherApps: true)
        makeKeyAndOrderFront(nil)
        makeKey()

        guard animated else { return }
        // Slide on the next run loop turn: an animation started in the same turn
        // the window is first ordered front is skipped.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isShown else { return }
            self.slide(to: .zero, duration: Self.showDuration, opening: true)
        }
    }

    public func hide(animated: Bool) {
        guard isShown else { return }
        isShown = false

        guard animated else {
            finishHide()
            return
        }
        slide(to: hiddenOffset, duration: Self.hideDuration, opening: false)
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.hideDuration + 0.03) { [weak self] in
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
    }

    /// Step the panel from where it is to `target`.
    private func slide(to target: CGSize, duration: TimeInterval, opening: Bool) {
        ticker?.cancel()
        let from = offset
        let start = CACurrentMediaTime()
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 1.0 / 60.0)
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            let t = min(1, max(0, (CACurrentMediaTime() - start) / duration))
            let eased = SlideEasing.value(t, opening: opening)
            self.offset = CGSize(
                width: from.width + (target.width - from.width) * eased,
                height: from.height + (target.height - from.height) * eased
            )
            self.applyOffset()
            if t >= 1 {
                self.ticker?.cancel()
                self.ticker = nil
            }
        }
        ticker = timer
        timer.resume()
    }

    private func applyOffset() {
        host?.frame = NSRect(x: offset.width, y: offset.height, width: size.width, height: size.height)
        glass?.setOffset(offset)
    }
}
