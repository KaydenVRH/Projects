import AppKit
import QuartzCore
import SwiftUI

/// The bar's root view.
///
/// The window is taller than the bar: besides the slack used for the auto-hide
/// reveal it also carries the *sheet* area below the bar (the media centre), so
/// a sheet is drawn with the bar's own vibrancy material rather than a second
/// material view that can never match it. The mask keeps that area invisible
/// until a sheet is open, and this view only claims hit-testing where something
/// is actually visible — so clicks pass through the reserved space.
final class BarRootView: NSView {
    /// Regions (this view's coordinates) that should receive mouse events.
    var visibleRects: [NSRect] = []

    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = superview.map { convert(point, from: $0) } ?? point
        guard visibleRects.contains(where: { $0.contains(local) }) else { return nil }
        return super.hitTest(point)
    }
}

/// A decorationless, non-activating panel pinned to an edge of one display.
/// This is the macOS analog of Quickshell's `PanelWindow`. Supports auto-hide
/// by sliding off the screen edge, and sheets that slide down out of the bar.
public final class BarPanel: NSPanel {
    public let targetScreen: NSScreen
    public let autohideEnabled: Bool
    public let revealZone: CGFloat
    public let hideDelay: TimeInterval
    public private(set) var isRevealed: Bool

    private let shownFrame: NSRect
    /// The bar's frame while hidden. The window's frame is derived from whichever
    /// of these applies, plus however much sheet is actually in use.
    private let hiddenFrame: NSRect

    /// Slack kept beyond the screen edge so the reveal's overshoot cannot lift
    /// the bar off the edge (which reads as the bar floating for a moment).
    static let revealBleed: CGFloat = 16
    /// How much room the sheet area takes in the *container*. The container keeps
    /// this size at all times so the bar's and the sheet's coordinates never
    /// move; the window around it grows only as far as the open sheet reaches.
    static let sheetAllowance: CGFloat = 380
    /// Corner radius on a sheet's free corners.
    static let sheetRadius: CGFloat = 22
    /// Height the sheet collapses to when closed. Not zero: the mask always
    /// carries the sheet subpath so its structure never changes (see `maskPath`).
    static let sheetClosedHeight: CGFloat = 0.25

    // Sheet state.
    private var sheetEdge: BarEdge = .top
    private var barSize = NSSize.zero
    private var barRadius: CGFloat = 0
    private var barInset: CGFloat = 0
    private var sheetAlign: OverlayAlign = .leading
    private var sheetWidth: CGFloat = 460
    private var sheetContentHeight: CGFloat = 306
    private var sheetHeight: CGFloat = 0
    /// The fixed-size view holding the bar and the sheet area. The window clips
    /// whatever part of it is not in use.
    private weak var container: BarRootView?
    /// Steps the sheet's slide (content and glass together) frame by frame.
    private var sheetTicker: DispatchSourceTimer?
    /// The display's surface holder, when it is providing the glass for this bar
    /// (and the sheet hanging off it). The ShellController wires this up.
    public weak var surface: ScreenBorderPanel?
    /// `true` when the glass comes from the surface holder rather than this
    /// window's own material view. Keeps the bar and the frame one material.
    public var usesSharedSurface = false
    private let maskLayer = CAShapeLayer()
    /// Holds the sheet's content and clips it to the region *below* the bar. The
    /// content slides down out of the bar, so without this clip it would ride up
    /// over the bar while the sheet is still on its way out.
    private let sheetClip = NSView()
    private weak var sheetView: NSView?

    /// The bar's window frame: the bar itself, slack past the screen edge, and
    /// room for the sheet that is *currently* open — and no more.
    ///
    /// That last part matters. A transparent window still swallows clicks, and
    /// `hitTest` returning nil does not pass them through, so reserving room for
    /// a closed sheet would leave a dead strip on screen.
    static func windowFrame(visible: NSRect, edge: BarEdge, sheetHeight: CGFloat) -> NSRect {
        switch edge {
        case .top:
            return NSRect(x: visible.minX, y: visible.minY - sheetHeight,
                          width: visible.width,
                          height: visible.height + revealBleed + sheetHeight)
        case .bottom:
            return NSRect(x: visible.minX, y: visible.minY - revealBleed,
                          width: visible.width,
                          height: visible.height + revealBleed + sheetHeight)
        }
    }

    /// The window frame for the current reveal state and sheet height.
    private func windowFrame(revealed: Bool, sheetHeight height: CGFloat) -> NSRect {
        BarPanel.windowFrame(visible: revealed ? shownFrame : hiddenFrame,
                             edge: sheetEdge,
                             sheetHeight: height)
    }

    public init(screen: NSScreen, appearance: BarConfig, viewModel: BarViewModel) {
        self.targetScreen = screen
        self.autohideEnabled = appearance.autohide
        self.revealZone = CGFloat(appearance.autohideZone)
        self.hideDelay = appearance.autohideDelay

        let shown = BarPanel.frame(for: screen, appearance: appearance)
        let hidden = BarPanel.hiddenFrame(for: screen, appearance: appearance, shown: shown)
        self.shownFrame = shown
        self.hiddenFrame = hidden
        self.isRevealed = !appearance.autohide

        super.init(
            contentRect: BarPanel.windowFrame(
                visible: appearance.autohide ? hidden : shown,
                edge: appearance.edge,
                sheetHeight: 0
            ),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        configure(
            appearance: appearance,
            viewModel: viewModel,
            size: shown.size,
            notchWidth: BarPanel.notchWidth(for: screen, config: appearance)
        )
    }

    /// The gap (in points) to keep clear in the centre of the bar for the
    /// display's notch. Auto-detected from the screen unless overridden.
    public static func notchWidth(for screen: NSScreen, config: BarConfig) -> CGFloat {
        if let override = config.notchWidth { return CGFloat(override) }
        guard screen.safeAreaInsets.top > 0 else { return 0 }
        let left = screen.auxiliaryTopLeftArea?.width ?? 0
        let right = screen.auxiliaryTopRightArea?.width ?? 0
        let width = screen.frame.width - left - right
        return width > 0 ? width : 0
    }

    static func frame(for screen: NSScreen, appearance: BarConfig) -> NSRect {
        let height = appearance.height
        let width = screen.frame.width - appearance.marginX * 2
        let x = screen.frame.minX + appearance.marginX
        let y: CGFloat
        switch appearance.edge {
        case .top:    y = screen.frame.maxY - height - appearance.marginY
        case .bottom: y = screen.frame.minY + appearance.marginY
        }
        return NSRect(x: x, y: y, width: max(width, 0), height: height)
    }

    /// Frame used while hidden: only `autohide_peek` points remain on screen.
    static func hiddenFrame(for screen: NSScreen, appearance: BarConfig, shown: NSRect) -> NSRect {
        let peek = CGFloat(appearance.autohidePeek)
        let y: CGFloat
        switch appearance.edge {
        case .top:    y = screen.frame.maxY - peek
        case .bottom: y = screen.frame.minY - shown.height + peek
        }
        return NSRect(x: shown.minX, y: y, width: shown.width, height: shown.height)
    }

    private func configure(appearance: BarConfig, viewModel: BarViewModel, size: NSSize, notchWidth: CGFloat) {
        // `isFloatingPanel` forces the level to .floating, so set it first and
        // the level after — otherwise the bar ends up at level 3.
        isFloatingPanel = true
        // One above the surface holder (the glass and the frame), one below the
        // launchers.
        level = NSWindow.Level(rawValue: NSWindow.Level.statusBar.rawValue + 1)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        isMovable = false
        isMovableByWindowBackground = false
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        ignoresMouseEvents = false
        // No `.fullScreenAuxiliary`: the bar steps aside for full-screen apps.
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

        sheetEdge = appearance.edge
        barSize = size
        barRadius = CGFloat(appearance.cornerRadius)
        barInset = CGFloat(appearance.paddingX)

        // The container holds every surface at a fixed size, anchored to the end
        // of the window the bar hangs from, so the bar's and the sheet's
        // coordinates never move. The window around it is only ever as tall as
        // what is actually on screen, and clips the rest — a transparent window
        // still swallows clicks, so leaving room for a closed sheet would put a
        // dead strip over the desktop.
        let containerSize = NSSize(
            width: size.width,
            height: size.height + Self.revealBleed + Self.sheetAllowance
        )
        let root = NSView(frame: NSRect(origin: .zero, size: frame.size))
        root.wantsLayer = true

        let container = BarRootView(frame: NSRect(
            x: 0,
            y: appearance.edge == .top ? root.frame.height - containerSize.height : 0,
            width: containerSize.width,
            height: containerSize.height
        ))
        container.wantsLayer = true
        container.autoresizingMask = appearance.edge == .top ? [.minYMargin] : [.maxYMargin]
        root.addSubview(container)
        self.container = container

        let barFrame = barFrameInContainer

        // When a surface holder is providing the glass this view is left out
        // entirely: two nearby material views never render the same, which is
        // exactly the seam this avoids.
        if appearance.blur && !usesSharedSurface {
            let effect = NSVisualEffectView(frame: NSRect(origin: .zero, size: containerSize))
            effect.autoresizingMask = []
            effect.material = .hudWindow
            effect.blendingMode = .behindWindow
            effect.state = .active
            container.addSubview(effect)
        }

        let host = NSHostingView(rootView: BarView(viewModel: viewModel, notchWidth: notchWidth))
        host.frame = barFrame
        host.autoresizingMask = []
        container.addSubview(host)

        // Everything the sheet draws lives in here, confined to below the bar.
        sheetClip.frame = sheetFrameInContainer(sheetContentHeight)
        sheetClip.wantsLayer = true
        sheetClip.layer?.masksToBounds = true
        sheetClip.autoresizingMask = []
        container.addSubview(sheetClip)

        // The mask shapes both the bar and (when open) the sheet out of the same
        // material.
        // The clip for the bar's own content: static, since the sheet's slide
        // (and the glass that follows it) happens in the surface holder.
        maskLayer.fillRule = .nonZero
        maskLayer.path = maskPath()
        container.layer?.mask = maskLayer
        container.layer?.masksToBounds = false

        contentView = root
        updateHitRegion()
    }

    // MARK: - sheet geometry (container coordinates, origin bottom-left)

    private var barFrameInContainer: NSRect {
        switch sheetEdge {
        case .top:
            return NSRect(x: 0, y: Self.sheetAllowance, width: barSize.width, height: barSize.height)
        case .bottom:
            return NSRect(x: 0, y: Self.revealBleed, width: barSize.width, height: barSize.height)
        }
    }

    /// Where the sheet lives: aligned along the bar, `height` tall. The sheet's
    /// view is laid out at its full size and never re-laid out — the mask is
    /// what reveals it — so the reveal cannot squish the content.
    private func sheetFrameInContainer(_ height: CGFloat) -> NSRect {
        let bar = barFrameInContainer
        // Clear the bar's rounded corners, or the sheet's square shoulder would
        // sit under one and leave a notch.
        let inset = max(barInset, barRadius + 2)
        let width = min(sheetWidth, max(bar.width - inset * 2, 60))
        let x: CGFloat
        switch sheetAlign {
        case .leading:  x = inset
        case .trailing: x = bar.width - inset - width
        case .center:   x = (bar.width - width) / 2
        }
        switch sheetEdge {
        case .top:    return NSRect(x: x, y: bar.minY - height, width: width, height: height)
        case .bottom: return NSRect(x: x, y: bar.maxY, width: width, height: height)
        }
    }

    /// The sheet's view sits inside `sheetClip` at the clip's origin: full size,
    /// no offset. Sliding is applied as an offset from here.
    private var sheetContentRestingFrame: NSRect {
        let sheet = sheetFrameInContainer(sheetContentHeight)
        return NSRect(x: 0, y: 0, width: sheet.width, height: sheetContentHeight)
    }

    /// Size/placement of the sheet, from config.
    public func configureSheet(width: CGFloat, contentHeight: CGFloat, align: OverlayAlign) {
        sheetWidth = width
        sheetContentHeight = contentHeight
        sheetAlign = align
        sheetClip.frame = sheetFrameInContainer(sheetContentHeight)
        if let sheetView {
            sheetView.frame = sheetContentRestingFrame
        }
        maskLayer.path = maskPath()
        publishSurface()
    }

    /// The bar's own shape, plus the sheet's resting area: where the bar's and
    /// the sheet's content are allowed to draw.
    ///
    /// All four of the bar's corners are turned (it floats inside the frame); the
    /// sheet rounds only its outward corners, so where it meets the bar the two
    /// join squarely and read as one surface.
    private func maskPath() -> CGPath {
        let path = CGMutablePath()
        let barIsUppermost = sheetEdge == .top
        // The bar floats inside the frame, so all four of its corners are free
        // and turned. The sheet hangs from it, so only the sheet's outward
        // corners are rounded and they meet the bar's edge squarely.
        path.addPath(BarPanel.roundedPath(
            barFrameInContainer,
            topRadius: barRadius,
            bottomRadius: barRadius
        ))
        path.addPath(BarPanel.roundedPath(
            sheetFrameInContainer(sheetContentHeight),
            topRadius: barIsUppermost ? 0 : SheetShape.defaultRadius,
            bottomRadius: barIsUppermost ? SheetShape.defaultRadius : 0
        ))
        return path
    }

    /// Tell the surface holder where the bar is and where the sheet's glass
    /// rests, so the glass it draws follows both. Public so the ShellController
    /// can hand over a freshly created surface.
    public func publishSurface() {
        guard usesSharedSurface, let surface, let container else { return }
        func onScreen(_ rect: NSRect) -> NSRect {
            convertToScreen(container.convert(rect, to: nil))
        }
        surface.setBarFrame(onScreen(barFrameInContainer))
        surface.configureSheet(onScreen(sheetFrameInContainer(sheetContentHeight)))
    }

    private static func roundedPath(_ rect: NSRect, topRadius: CGFloat, bottomRadius: CGFloat) -> CGPath {
        let path = CGMutablePath()
        let tr = max(0, min(topRadius, rect.height / 2))
        let br = max(0, min(bottomRadius, rect.height / 2))
        path.move(to: CGPoint(x: rect.minX, y: rect.minY + br))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - tr))
        path.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.maxY),
                    tangent2End: CGPoint(x: rect.minX + tr, y: rect.maxY), radius: tr)
        path.addLine(to: CGPoint(x: rect.maxX - tr, y: rect.maxY))
        path.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.maxY),
                    tangent2End: CGPoint(x: rect.maxX, y: rect.maxY - tr), radius: tr)
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + br))
        path.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.minY),
                    tangent2End: CGPoint(x: rect.maxX - br, y: rect.minY), radius: br)
        path.addLine(to: CGPoint(x: rect.minX + br, y: rect.minY))
        path.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.minY),
                    tangent2End: CGPoint(x: rect.minX, y: rect.minY + br), radius: br)
        path.closeSubpath()
        return path
    }

    private func updateHitRegion() {
        var rects = [barFrameInContainer]
        if sheetHeight > 0 { rects.append(sheetFrameInContainer(sheetHeight)) }
        container?.visibleRects = rects
    }

    /// Install/remove the view shown inside the sheet area.
    public func setSheetView(_ view: NSView?) {
        sheetView?.removeFromSuperview()
        sheetView = view
        guard let view else { return }
        sheetClip.frame = sheetFrameInContainer(sheetContentHeight)
        view.frame = sheetContentRestingFrame
        view.autoresizingMask = []
        sheetClip.addSubview(view)
    }

    /// Open (or close, with 0) the sheet. The mask's sheet edge and the content
    /// both travel downward only, from behind the bar to their resting place, so
    /// the sheet reads as one piece sliding down.
    public func setSheetHeight(_ height: CGFloat, animated: Bool) {
        let previous = sheetHeight
        sheetHeight = height
        updateHitRegion()

        // The content keeps its size and only moves, so nothing re-lays out. It
        // starts fully behind the bar (offset by its own height) and slides down
        // into the clip, which is what makes it emerge from under the bar.
        let resting = sheetContentRestingFrame
        // Toward the screen interior is down; away from it is back behind the bar.
        let slide: CGFloat = sheetEdge == .top ? 1 : -1

        let opening = height > previous
        let windowTarget = windowFrame(revealed: isRevealed, sheetHeight: height)
        guard animated, previous != height else {
            sheetTicker?.cancel()
            sheetTicker = nil
            setFrame(windowTarget, display: true)
            sheetView?.frame = resting
            surface?.setSheetOffset(slide * max(0, sheetContentHeight - height))
            return
        }
        // Opening: grow the window first, so the sheet has room to slide into.
        // Closing: shrink only after it has finished collapsing, otherwise the
        // window would clip the content while it is still sliding.
        if opening {
            setFrame(windowTarget, display: true)
        } else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.20 + 0.06) { [weak self] in
                guard let self, self.sheetHeight == height else { return }
                self.setFrame(windowTarget, display: true)
            }
        }

        // One clock drives both the content and the surface's glass, and it sets
        // their model geometry each tick rather than animating them. That is
        // deliberate: the window server renders an effect view's blur from the
        // layer's model geometry, so neither an animated mask nor an animating
        // view frame moves the blur — only per-frame model changes do. Without
        // this the glass snapped out at full size and left a bare sheet sitting
        // under the content as it slid.
        let duration: TimeInterval = opening ? 0.32 : 0.20
        let from = previous
        let to = height
        let start = CACurrentMediaTime()
        sheetTicker?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 1.0 / 60.0)
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            let t = min(1, max(0, (CACurrentMediaTime() - start) / duration))
            let eased = SlideEasing.value(t, opening: opening)
            let current = from + (to - from) * eased
            let offset = max(0, self.sheetContentHeight - current)
            self.sheetView?.frame = resting.offsetBy(dx: 0, dy: slide * offset)
            self.surface?.setSheetOffset(slide * offset)
            if t >= 1 {
                self.sheetTicker?.cancel()
                self.sheetTicker = nil
            }
        }
        sheetTicker = timer
        timer.resume()
    }


    public func present() {
        orderFrontRegardless()
    }

    public func dismiss() {
        orderOut(nil)
    }

    public func setRevealed(_ revealed: Bool, animated: Bool) {
        isRevealed = revealed
        let target = windowFrame(revealed: revealed, sheetHeight: sheetHeight)
        guard frame != target else { return }
        guard animated else {
            setFrame(target, display: true)
            publishSurface()
            return
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = revealed ? 0.28 : 0.18
            // easeOutBack on the way in: the bar overshoots its resting place a
            // little and settles back, instead of stopping dead.
            context.timingFunction = revealed
                ? CAMediaTimingFunction(controlPoints: 0.34, 1.56, 0.64, 1.0)
                : CAMediaTimingFunction(name: .easeIn)
            animator().setFrame(target, display: true)
        }
        // The bar travels with the window, so the glass under it has to travel
        // too, on the same curve.
        publishSurface()
    }

    /// Whether the pointer is currently over this bar's shown area.
    public func containsMouseLocation() -> Bool {
        NSMouseInRect(NSEvent.mouseLocation, shownFrame, false)
    }

    /// Whether the pointer is inside this bar's reveal zone at its screen edge.
    public func mouseInRevealZone() -> Bool {
        let mouse = NSEvent.mouseLocation
        let screen = targetScreen.frame
        guard mouse.x >= screen.minX, mouse.x <= screen.maxX else { return false }
        return mouse.y >= screen.maxY - revealZone
    }

    /// Without this, AppKit clamps the panel below the menu bar / notch to keep
    /// it inside the screen's visible frame. We need it at the very top edge.
    public override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    public override var canBecomeKey: Bool { true }
}
