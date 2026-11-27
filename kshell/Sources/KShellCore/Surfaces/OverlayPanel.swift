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
/// The morph is two springs (see `PanelMotion`): one widening the pill along
/// its edge, one reaching it out — the outward one bounces a little past the
/// resting size and settles back. A spring keeps its velocity, so reversing a
/// panel half-way turns it around instead of restarting it.
///
/// The morph is stepped frame by frame rather than handed to Core Animation.
/// That is deliberate: when the frame is on, this panel's glass lives on the
/// frame's surface (see `SurfaceGlass`), and the window server renders that blur
/// from model geometry — so an animating view frame or mask would not move it.
/// Stepping keeps the glass and the content locked together.
///
/// The bounce needs room past the panel, so while it moves the window carries
/// some slack on the side it grows toward; once it is at rest the window
/// shrinks back to exactly the panel, so no invisible margin is left to swallow
/// clicks meant for the app behind it.
public final class OverlayPanel: NSPanel {
    /// Roughly how long a panel takes to fold away, for callers that wait on it.
    static let hideDuration: TimeInterval = 0.34
    /// How far the content drifts in behind the growing outline.
    private static let contentDrift: CGFloat = 26
    /// How far the pill reaches along the edge, and out from it.
    private static let seedLength: CGFloat = 300
    private static let seedDepth: CGFloat = 44

    private let shownFrame: NSRect
    private let edge: OverlayEdge
    private let size: NSSize
    private let style: PanelStyle?
    private let makeGlass: ((NSScreen) -> SurfaceGlass?)?
    private let targetScreen: NSScreen
    /// Whether the panel takes the keyboard. Launchers do; a notification popup
    /// must not, or it would interrupt whatever is being typed when it appears.
    private let takesKeyboard: Bool
    /// The outline to step while the frame is off: same rounding, no fillets.
    private let fallbackStyle: PanelStyle

    /// The window's content: the panel (`container`, masked to its outline) and,
    /// over it unmasked, the light its theme's timeline throws.
    private let root = NSView()
    private let container = NSView()
    private let mask = CAShapeLayer()
    /// Neon and Arcane panels come and go on their own timeline (see
    /// `PanelTimeline`); made when the panel opens, so it follows the theme.
    private var timeline: PanelTimeline?
    /// A wash of colour over the content, inside the outline (a CRT running
    /// white as it collapses, a spell glowing as it implodes).
    private let bloom = CALayer()
    private weak var host: NSView?
    /// The Stark HUD's brackets and scanlines, over the content.
    private var chrome: ThemeChrome?
    private var glass: SurfaceGlass?
    private lazy var motion = PanelMotion(screen: targetScreen)
    /// The window's origin on screen: the shown frame's, or the slack frame's
    /// while the panel is moving.
    private var windowOrigin: CGPoint
    private var activationObserver: NSObjectProtocol?
    public private(set) var isShown = false

    public init(
        screen: NSScreen,
        size: NSSize,
        edge: OverlayEdge = .bottom,
        margin: CGFloat = 0,
        /// Where a left/right-anchored panel sits vertically: `.leading` is the
        /// top of the screen, `.trailing` the bottom.
        vertical: OverlayAlign = .center,
        /// The distance from the top/bottom edge when `vertical` is not centred.
        /// Defaults to `margin`.
        verticalMargin: CGFloat? = nil,
        align: OverlayAlign = .center,
        appearance: BarConfig = BarConfig(),
        style: PanelStyle? = nil,
        glass: ((NSScreen) -> SurfaceGlass?)? = nil,
        takesKeyboard: Bool = true,
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
            y = Self.verticalY(vertical, inset: verticalMargin ?? margin, size: size, screen: screenFrame)
        case .trailing:
            x = screenFrame.maxX - margin - size.width
            y = Self.verticalY(vertical, inset: verticalMargin ?? margin, size: size, screen: screenFrame)
        }
        // The window always sits at its final spot; only the content grows.
        self.shownFrame = NSRect(x: x, y: y, width: size.width, height: size.height)
        self.windowOrigin = shownFrame.origin
        self.targetScreen = screen
        self.edge = edge
        self.size = size
        self.style = style
        self.makeGlass = glass
        self.takesKeyboard = takesKeyboard
        self.fallbackStyle = style
            ?? PanelStyle(radius: SheetShape.defaultRadius, filletRadius: 0, attachment: .none,
                          cut: Neon.active ? Neon.panelCut : 0,
                          notch: Arcane.active ? Arcane.panelNotch : 0)

        // A popup that doesn't take the keyboard must not activate the app when
        // clicked, or a stray notification would steal the front from a terminal.
        var styleMask: NSWindow.StyleMask = [.borderless]
        if !takesKeyboard { styleMask.insert(.nonactivatingPanel) }

        super.init(
            contentRect: shownFrame,
            styleMask: styleMask,
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
        // A keyboard panel must become key on a click anywhere in it. With this
        // left on, clicking the body of a launcher never made the panel key, so
        // nothing inside it could take focus.
        becomesKeyOnlyIfNeeded = !takesKeyboard
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
        // The trademark themes draw their own chrome over the content.
        let chrome: ThemeChrome? = Hud.active ? HudChromeView(frame: .zero)
            : Arcane.active ? ArcaneChromeView(frame: .zero)
            : Neon.active ? NeonChromeView(frame: .zero) : nil
        if let chrome {
            container.addSubview(chrome)
            self.chrome = chrome
        }
        bloom.backgroundColor = NSColor.white.cgColor
        bloom.opacity = 0
        container.layer?.addSublayer(bloom)
        root.wantsLayer = true
        root.addSubview(container)
        contentView = root

        motion.onStep = { [weak self] pose in
            self?.apply(pose)
        }

        motion.onSettle = { [weak self] open in
            guard let self else { return }
            if open {
                // At rest: the window is exactly the panel again.
                self.setSlack(false)
            } else if !self.isShown {
                self.finishHide() // a no-op once it is already gone
            }
        }
        apply(.closed)
    }

    /// How far the panel travels out from its edge.
    private var travel: CGFloat {
        (edge == .top || edge == .bottom)
            ? size.height - seedFrame.height
            : size.width - seedFrame.width
    }

    /// The window while the panel moves: the shown frame, plus room for the
    /// jelly — out from the edge, and a little on both sides along it.
    private var slackFrame: NSRect {
        var room = Jelly.slack(travel: travel)
        if let extra = timeline?.slack ?? PanelTimelines.slack {
            room = (max(room.out, extra.out), max(room.side, extra.side))
        }
        var frame = shownFrame
        switch edge {
        case .bottom, .top:
            frame.size.height += room.out
            if edge == .top { frame.origin.y -= room.out }
            frame = frame.insetBy(dx: -room.side, dy: 0)
        case .leading, .trailing:
            frame.size.width += room.out
            if edge == .trailing { frame.origin.x -= room.out }
            frame = frame.insetBy(dx: 0, dy: -room.side)
        }
        return frame
    }

    /// Give the window room to bounce, or take it away again.
    private func setSlack(_ on: Bool) {
        let target = on ? slackFrame : shownFrame
        guard frame != target else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        setFrame(target, display: false)
        windowOrigin = target.origin
        if timeline != nil { applyTimeline() } else { apply(motion.pose) }
        CATransaction.commit()
    }

    /// The vertical position for a left/right-anchored panel.
    private static func verticalY(
        _ vertical: OverlayAlign,
        inset: CGFloat,
        size: NSSize,
        screen: NSRect
    ) -> CGFloat {
        switch vertical {
        case .leading: return screen.maxY - inset - size.height
        case .trailing: return screen.minY + inset
        case .center: return screen.midY - size.height / 2
        }
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
        if !isVisible { apply(.closed) }
        setSlack(animated)
        Motion.reveal()

        if takesKeyboard {
            Frontmost.claim()
            takeKeyboard()
        } else {
            // A notification card appears without taking anything: no key
            // window, no app activation, so typing carries on undisturbed.
            orderFrontRegardless()
        }

        guard animated else {
            motion.jump(open: true)
            return
        }
        // Grow on the next run loop turn: a morph started in the same turn the
        // window is first ordered front is skipped.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isShown else { return }
            if let timeline = PanelTimelines.make(screen: self.targetScreen) {
                self.timeline = timeline
                self.root.layer?.addSublayer(timeline.lights)
                timeline.onStep = { [weak self] in self?.applyTimeline() }
                timeline.onSettle = { [weak self] open in
                    guard let self else { return }
                    if open {
                        self.setSlack(false)
                    } else if !self.isShown {
                        self.finishHide()
                    }
                }
                timeline.open()
            } else {
                self.motion.animate(open: true)
            }
            if let chrome = self.chrome {
                chrome.powerOn()
                if Hud.active { HudChromeView.flicker(self.host?.layer) }
                if Neon.active { Neon.strike(self.host?.layer) }
            }
        }
    }

    public func hide(animated: Bool) {
        guard isShown else { return }
        isShown = false

        guard animated else {
            finishHide()
            return
        }
        // Interrupting an opening bounce: it may still be past the panel.
        setSlack(true)
        if let timeline {
            timeline.close()
        } else {
            motion.animate(open: false)
        }
        chrome?.powerOff()
        // The spring hides it when it settles; this only covers a display that
        // stopped ticking (asleep, unplugged) half-way.
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) { [weak self] in
            guard let self, !self.isShown, self.isVisible else { return }
            self.finishHide()
        }
    }

    public func toggle(animated: Bool) {
        isShown ? hide(animated: animated) : show(animated: animated)
    }

    /// Put this panel in front and make it the key window.
    ///
    /// `NSApp.activate` is asynchronous: the `makeKey()` issued in the same turn
    /// is dropped, so the panel appears while the shell is the active app with
    /// no key window at all. Every keystroke then goes nowhere and nothing in
    /// the panel gains focus — it looks alive and takes nothing. Re-assert on
    /// the next turn, and again when activation actually lands.
    private func takeKeyboard() {
        reassertKey()
        if activationObserver == nil {
            activationObserver = NotificationCenter.default.addObserver(
                forName: NSApplication.didBecomeActiveNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.reassertKey()
            }
        }
    }

    /// Order the panel front and make it key, retrying briefly.
    ///
    /// Window ordering can also land a frame late, and a key window that is
    /// asked for before the window is on screen is silently refused — which
    /// leaves an active shell with no key window, the state that beeps at every
    /// keystroke. Keep asking for a few turns until it takes.
    private func reassertKey(attempt: Int = 0) {
        guard isShown, takesKeyboard else { return }
        makeKeyAndOrderFront(nil)
        makeKey()
        guard !isKeyWindow, attempt < 6 else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            self?.reassertKey(attempt: attempt + 1)
        }
    }

    private func finishHide() {
        guard isVisible else { return }
        if let activationObserver { NotificationCenter.default.removeObserver(activationObserver) }
        activationObserver = nil
        glass?.park()
        glass = nil
        orderOut(nil)
        // Back to the pill, off screen, ready for the next opening.
        timeline?.reset()
        timeline?.lights.removeFromSuperlayer()
        timeline = nil
        bloom.opacity = 0
        motion.jump(open: false)
        setSlack(false)
        // The keyboard came with us; give it back now the panel is gone.
        if takesKeyboard { Frontmost.handBack() }
    }

    /// Put the panel where the springs have it: a pill swelling out of the
    /// edge, then jelly once it has arrived.
    private func apply(_ pose: PanelPose) {
        let seed = seedFrame
        func mix(_ a: CGFloat, _ b: CGFloat, _ t: CGFloat) -> CGFloat { a + (b - a) * t }
        let alongX = edge == .top || edge == .bottom
        let tx = alongX ? pose.along : pose.reach
        let ty = alongX ? pose.reach : pose.along
        let grown = NSRect(
            x: mix(seed.minX, shownFrame.minX, tx),
            y: mix(seed.minY, shownFrame.minY, ty),
            width: mix(seed.width, shownFrame.width, tx),
            height: mix(seed.height, shownFrame.height, ty)
        )
        let rect = Jelly.shape(grown, edge: edge, wobble: pose.wobble, travel: travel)
        place(
            rect,
            style: style ?? fallbackStyle,
            squash: pose.wobble != 0,
            drift: (1 - pose.reach) * Self.contentDrift,
            hostAlpha: Motion.ramp(min(pose.reach, pose.along + 0.2), from: 0.1, to: 0.65),
            glassFade: Motion.ramp(pose.reach, from: 0, to: 0.3),
            chromeAlpha: Motion.ramp(pose.reach, from: 0.2, to: 0.7)
        )
    }

    /// Put the panel where its theme's timeline has it.
    private func applyTimeline() {
        guard let timeline else { return }
        let frame = timeline.layout(full: shownFrame, edge: edge, style: style ?? fallbackStyle)
        place(
            frame.rect,
            style: frame.style,
            squash: frame.squash,
            drift: 0,
            hostAlpha: frame.hostAlpha,
            glassFade: frame.glassFade,
            chromeAlpha: frame.chromeAlpha
        )
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        bloom.frame = container.bounds
        bloom.backgroundColor = frame.bloomColour.cgColor
        bloom.opacity = Float(frame.bloom)
        CATransaction.commit()
        timeline.placeLights(rect: frame.rect.offsetBy(dx: -windowOrigin.x, dy: -windowOrigin.y), edge: edge)
    }

    /// Put the panel's outline, content, chrome and glass at `rect`.
    ///
    /// Everything is set to its model value each tick rather than animated: the
    /// window server renders an effect view's blur from the layer's model
    /// geometry, so an animated frame or mask would leave the glass behind.
    private func place(
        _ rect: NSRect,
        style: PanelStyle,
        squash: Bool,
        drift: CGFloat,
        hostAlpha: CGFloat,
        glassFade: CGFloat,
        chromeAlpha: CGFloat
    ) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        // The container is the panel's rect in the window's coordinates.
        let local = NSRect(
            x: rect.minX - windowOrigin.x,
            y: rect.minY - windowOrigin.y,
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

        if let chrome {
            chrome.frame = NSRect(origin: .zero, size: local.size)
            chrome.fit(size: local.size, style: style)
            chrome.alphaValue = chromeAlpha
        }

        guard let host else {
            glass?.setResting(rect, style: style)
            glass?.setFade(glassFade)
            return
        }
        if squash {
            // Wobbling (or a CRT collapsing): the content is squished with the
            // outline — laid out at its own size and drawn into the deformed
            // rectangle.
            host.frame = NSRect(origin: .zero, size: local.size)
            host.bounds = NSRect(origin: .zero, size: size)
            host.alphaValue = hostAlpha
        } else {
            // Growing: the content keeps its final size and stays pinned to the
            // anchored edge, so the growing outline reveals it. It is centred
            // along the edge, which is the axis the pill grows about. It also
            // trails the outline a little — drifting in from the edge and
            // fading up — so it reads as carried out by the panel rather than
            // uncovered by it.
            var origin = CGPoint.zero
            switch edge {
            case .bottom: origin = CGPoint(x: (local.width - size.width) / 2, y: -drift)
            case .top: origin = CGPoint(x: (local.width - size.width) / 2, y: local.height - size.height + drift)
            case .leading: origin = CGPoint(x: -drift, y: (local.height - size.height) / 2)
            case .trailing: origin = CGPoint(x: local.width - size.width + drift, y: (local.height - size.height) / 2)
            }
            host.frame = NSRect(origin: origin, size: size)
            host.bounds = NSRect(origin: .zero, size: size)
            host.alphaValue = hostAlpha
        }

        glass?.setResting(rect, style: style)
        glass?.setFade(glassFade)
    }
}
