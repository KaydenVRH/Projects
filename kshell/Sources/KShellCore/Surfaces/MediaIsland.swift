import AppKit
import Combine
import QuartzCore
import SwiftUI

/// The media centre, as a dynamic island: a pill that grows out of the bar.
///
/// It is not a sheet sliding out of the bar's window. The island is its own
/// panel, anchored flush under the bar, whose rectangle and outline are stepped
/// frame by frame from a small pill to the full centre — widening about its
/// own centre and growing away from the bar, so it reads as being pushed out of
/// the bar rather than sliding out from under it.
///
/// The glass is a piece of the *frame's* surface (the same vibrancy view the
/// frame, the bar and the launchers are cut from), so the island is the same
/// material as everything it touches. Its outline is stepped too, which is why
/// the glass is a view here rather than part of the frame's mask: a view's
/// geometry is what the window server renders the blur from.
public final class MediaIslandPanel: NSPanel {
    private let targetScreen: NSScreen
    /// Where the island rests when fully open, in screen coordinates.
    private let expanded: NSRect
    /// Where it starts — a pill sharing the expanded rect's attachment edge and
    /// centre, so interpolating the two grows it out of the bar symmetrically.
    private let pill: NSRect
    private let radius: CGFloat
    private let style: PanelStyle?
    private let makeGlass: ((NSScreen) -> SurfaceGlass?)?

    private let container = NSView()
    private let host: NSHostingView<MediaIslandView>
    private let island: MediaIslandState
    private var glass: SurfaceGlass?
    private var localGlass: NSVisualEffectView?
    private var mask = CAShapeLayer()
    /// The Stark HUD's brackets and scanlines.
    private var chrome: ThemeChrome?
    private lazy var motion = PanelMotion(screen: targetScreen)
    /// Which way is away from the bar: the island's bounce goes that way, so
    /// the window carries slack on that side while it moves.
    private let growsDown: Bool
    private var windowOrigin: CGPoint
    private var activationObserver: NSObjectProtocol?
    private var isShown = false

    /// - Parameters:
    ///   - barFrame: the bar's visible rectangle on screen; the island hangs off
    ///     its inner edge.
    ///   - edge: which edge the bar is on, so the island grows away from it.
    ///   - align: where along the bar it sits.
    ///   - margin: the frame's thickness, so it stays inside the opening.
    public init(
        screen: NSScreen,
        barFrame: NSRect,
        edge: BarEdge,
        size: NSSize,
        pillSize: NSSize,
        align: OverlayAlign,
        paddingX: CGFloat,
        radius: CGFloat,
        viewModel: BarViewModel,
        monitor: MediaMonitor,
        style: PanelStyle?,
        glass: ((NSScreen) -> SurfaceGlass?)?,
        onClose: @escaping () -> Void
    ) {
        targetScreen = screen
        self.radius = radius
        self.style = style
        makeGlass = glass

        // Along the bar; the pill shares the centre so the growth is symmetric.
        let width = size.width
        let x: CGFloat
        switch align {
        case .leading: x = barFrame.minX + paddingX
        case .trailing: x = barFrame.maxX - paddingX - width
        case .center: x = barFrame.midX - width / 2
        }
        let centreX = x + width / 2

        // Away from the bar: down from a top bar, up from a bottom one.
        let full: NSRect
        let small: NSRect
        switch edge {
        case .top:
            full = NSRect(x: x, y: barFrame.minY - size.height, width: width, height: size.height)
            small = NSRect(
                x: centreX - pillSize.width / 2,
                y: barFrame.minY - pillSize.height,
                width: pillSize.width,
                height: pillSize.height
            )
        case .bottom:
            full = NSRect(x: x, y: barFrame.maxY, width: width, height: size.height)
            small = NSRect(
                x: centreX - pillSize.width / 2,
                y: barFrame.maxY,
                width: pillSize.width,
                height: pillSize.height
            )
        }
        expanded = full
        pill = small
        growsDown = edge == .top
        windowOrigin = full.origin

        island = MediaIslandState()
        host = NSHostingView(rootView: MediaIslandView(
            viewModel: viewModel,
            monitor: monitor,
            island: island,
            fullSize: size,
            pillSize: pillSize,
            onClose: onClose
        ))

        super.init(
            contentRect: full,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        // Above the frame's surface, alongside the launchers.
        level = WindowLevel.overlay
        // Light themes need a light appearance, or the material keeps the dark
        // system blur under a light tint.
        appearance = viewModel.appearance.background.vibrancyAppearance
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        isMovable = false
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        collectionBehavior = [.canJoinAllSpaces, .ignoresCycle]

        // The container is what actually morphs: the window stays at the open
        // rectangle and the island grows inside it.
        container.wantsLayer = true
        mask.fillColor = NSColor.black.cgColor
        container.layer?.mask = mask

        host.autoresizingMask = [.width, .height]
        container.addSubview(host)
        // The trademark themes draw their own chrome over the content.
        let chrome: ThemeChrome? = Hud.active ? HudChromeView(frame: .zero)
            : Arcane.active ? ArcaneChromeView(frame: .zero)
            : Neon.active ? NeonChromeView(frame: .zero) : nil
        if let chrome {
            container.addSubview(chrome)
            self.chrome = chrome
        }
        contentView = container

        // Without the frame there is no shared surface to borrow, so the island
        // draws its own material exactly as the launchers do.
        if style == nil, viewModel.appearance.blur {
            let effect = NSVisualEffectView()
            effect.material = .hudWindow
            effect.blendingMode = .behindWindow
            effect.state = .active
            effect.wantsLayer = true
            container.addSubview(effect, positioned: .below, relativeTo: host)
            localGlass = effect
        }

        motion.onStep = { [weak self] pose in
            self?.apply(pose)
        }
        motion.onSettle = { [weak self] open in
            guard let self else { return }
            if open { self.setSlack(false) } else if !self.isShown { self.finishHide() }
        }
        apply(.closed)
    }

    private var travel: CGFloat { expanded.height - pill.height }
    private var jellyEdge: OverlayEdge { growsDown ? .top : .bottom }

    /// The open rectangle plus room for the jelly: away from the bar, and a
    /// little either side.
    private var slackFrame: NSRect {
        let room = Jelly.slack(travel: travel)
        var frame = expanded.insetBy(dx: -room.side, dy: 0)
        frame.size.height += room.out
        if growsDown { frame.origin.y -= room.out }
        return frame
    }

    /// Room to bounce while moving; exactly the island at rest, so no strip
    /// of invisible window is left over the app below.
    private func setSlack(_ on: Bool) {
        let target = on ? slackFrame : expanded
        guard frame != target else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        setFrame(target, display: false)
        windowOrigin = target.origin
        apply(motion.pose)
        CATransaction.commit()
    }

    public override var canBecomeKey: Bool { true }
    public override var canBecomeMain: Bool { false }

    public override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    // MARK: - showing

    public func show(animated: Bool) {
        guard !isShown else { return }
        if activationObserver == nil {
            activationObserver = NotificationCenter.default.addObserver(
                forName: NSApplication.didBecomeActiveNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                guard let self, self.isShown else { return }
                self.makeKeyAndOrderFront(nil)
                self.makeKey()
            }
        }
        isShown = true

        glass = makeGlass?(targetScreen)
        if !isVisible { apply(.closed) }
        setSlack(animated)

        Frontmost.claim()
        // Activation is asynchronous — see OverlayPanel.takeKeyboard. A panel
        // ordered front before the app is active never becomes key, and then
        // takes no input at all.
        makeKeyAndOrderFront(nil)
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isShown else { return }
            self.makeKeyAndOrderFront(nil)
            self.makeKey()
        }

        guard animated else {
            motion.jump(open: true)
            return
        }
        // Started on the next turn: an animation begun in the same turn the
        // window is first ordered front gets skipped.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isShown else { return }
            self.motion.animate(open: true)
            if let chrome = self.chrome {
                chrome.powerOn()
                if Hud.active { HudChromeView.flicker(self.host.layer) }
                if Neon.active {
                    Neon.strike(self.host.layer)
                    Neon.jolt(self.host.layer)
                }
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
        setSlack(true)
        motion.animate(open: false)
        chrome?.powerOff()
        if Neon.active { Neon.jolt(host.layer) }
        // The spring hides it when it settles; this covers a display that
        // stopped ticking half-way.
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
            guard let self, !self.isShown, self.isVisible else { return }
            self.finishHide()
        }
    }

    private func finishHide() {
        guard isVisible else { return }
        if let activationObserver { NotificationCenter.default.removeObserver(activationObserver) }
        activationObserver = nil
        glass?.park()
        glass = nil
        orderOut(nil)
        motion.jump(open: false)
        setSlack(false)
        Frontmost.handBack()
    }

    /// Put the island, its outline and its glass at `pose`: widening along the
    /// bar, dropping away from it, then wobbling like jelly once it is open.
    ///
    /// Everything — the window's content, the outline and the borrowed glass —
    /// is set to its model value each tick. Nothing is handed to Core Animation:
    /// the window server renders an effect view's blur from the layer's model
    /// geometry, so an animated frame would leave the glass behind.
    private func apply(_ pose: PanelPose) {
        let width = pill.width + (expanded.width - pill.width) * pose.along
        let height = pill.height + (expanded.height - pill.height) * pose.reach
        // Hung off the bar: a top bar's island keeps its top edge, a bottom
        // bar's its bottom edge.
        let rect = Jelly.shape(NSRect(
            x: expanded.midX - width / 2,
            y: growsDown ? expanded.maxY - height : expanded.minY,
            width: width,
            height: height
        ), edge: jellyEdge, wobble: pose.wobble, travel: travel)
        // The pill's radius is already the panel's: a 44pt pill at radius 22 is
        // a capsule, and the same radius at full height is the panel's corner.
        let outline = PanelStyle(radius: radius, filletRadius: 0, attachment: .none)

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let local = NSRect(
            x: rect.minX - windowOrigin.x,
            y: rect.minY - windowOrigin.y,
            width: rect.width,
            height: rect.height
        )
        container.frame = local
        host.frame = NSRect(origin: .zero, size: local.size)
        // While it wobbles the content is squished with it rather than
        // reflowed: laid out at the open size, drawn into the jelly.
        if pose.wobble != 0 { host.bounds = NSRect(origin: .zero, size: expanded.size) }
        localGlass?.frame = NSRect(origin: .zero, size: local.size)

        let shape = CGMutablePath()
        shape.addPath(PanelGeometry.path(in: NSRect(origin: .zero, size: local.size), style: outline))
        mask.path = shape

        // The content crossfades from the pill to the centre on the way out;
        // the bounce past open is geometry only.
        island.progress = pose.reach
        if let chrome {
            chrome.frame = NSRect(origin: .zero, size: local.size)
            chrome.fit(size: local.size, style: outline)
            chrome.alphaValue = Motion.ramp(pose.reach, from: 0.3, to: 0.8)
        }
        glass?.setResting(rect, style: outline)
        glass?.setFade(Motion.ramp(pose.reach, from: 0, to: 0.3))
    }
}

/// The island's own progress, so its content can crossfade between the pill
/// layout and the full centre while the geometry is stepped underneath it.
public final class MediaIslandState: ObservableObject {
    @Published public var progress: CGFloat = 0
}

// MARK: - content

/// The island's content: a compact row that fits the pill, crossfading into the
/// full media centre as it opens.
struct MediaIslandView: View {
    @ObservedObject var viewModel: BarViewModel
    @ObservedObject var monitor: MediaMonitor
    @ObservedObject var island: MediaIslandState
    let fullSize: NSSize
    let pillSize: NSSize
    let onClose: () -> Void

    @State private var artwork: NSImage?
    @FocusState private var focused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }
    private var state: MediaState { monitor.state }

    private var expandedAlpha: Double { Double(min(max((island.progress - 0.42) / 0.45, 0), 1)) }
    private var compactAlpha: Double { Double(min(max(1 - island.progress / 0.55, 0), 1)) }

    var body: some View {
        ZStack {
            content
                .frame(width: fullSize.width, height: fullSize.height)
                .opacity(expandedAlpha)
            pill
                .frame(width: pillSize.width, height: pillSize.height)
                .opacity(compactAlpha)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.space) { monitor.control(.playPause); return .handled }
        .onKeyPress(.rightArrow) { monitor.control(.next); return .handled }
        .onKeyPress(.leftArrow) { monitor.control(.previous); return .handled }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear {
            loadArtwork(state.artworkPath)
            refocus()
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
        .onChange(of: state) { _, new in loadArtwork(new.artworkPath) }
    }

    // MARK: - the pill

    private var pill: some View {
        HStack(spacing: 10) {
            thumbnail(size: 26)
            VStack(alignment: .leading, spacing: 1) {
                Text(state.hasTrack ? state.title : "Nothing playing")
                    .font(.custom(bar.fontFamily, size: 12))
                    .foregroundColor(foreground)
                    .lineLimit(1)
                Text(state.hasTrack ? state.artist : "kshell library")
                    .font(.custom(bar.fontFamily, size: 10))
                    .foregroundColor(dim)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if state.hasTrack {
                Equaliser(playing: state.isPlaying, color: tint)
            }
        }
        .padding(.horizontal, 12)
        .overlay(alignment: .bottom) {
            if state.hasTrack {
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule().fill(Color.white.opacity(0.10))
                        Capsule().fill(tint).frame(width: max(2, geo.size.width * state.progress))
                    }
                }
                .frame(height: 2)
                .padding(.horizontal, 12)
                .padding(.bottom, 5)
            }
        }
    }

    // MARK: - the full centre

    private var content: some View {
        VStack(spacing: 0) {
            if state.hasTrack {
                track
            } else {
                idle
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(bar.background.color.opacity(style == nil ? 1 : 0))
    }

    /// The cover's colour, as the music panel takes it.
    private var tint: Color {
        ArtworkCache.shared.tint(state.artworkPath).map(Color.init(nsColor:)) ?? accent
    }

    private var track: some View {
        VStack(spacing: 14) {
            HStack(alignment: .center, spacing: 16) {
                thumbnail(size: 112, artworkOnly: true)
                    .shadow(color: tint.opacity(state.isPlaying ? 0.55 : 0.2), radius: 18, y: 6)
                    .scaleEffect(state.isPlaying ? 1 : 0.92)
                    .animation(.kBouncy, value: state.isPlaying)
                VStack(alignment: .leading, spacing: 4) {
                    Text(state.source == "kshell" ? "KSHELL LIBRARY" : "NOW PLAYING")
                        .font(.system(size: 9, weight: .bold))
                        .tracking(1.3)
                        .foregroundColor(tint)
                    Text(state.title)
                        .font(.custom(bar.fontFamily, size: 18))
                        .foregroundColor(foreground)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                        .id("t-\(state.title)")
                        .transition(.blurSwap)
                    if !state.artist.isEmpty {
                        Text(state.album.isEmpty ? state.artist : "\(state.artist) · \(state.album)")
                            .font(.custom(bar.fontFamily, size: 12.5))
                            .foregroundColor(foreground.opacity(0.7))
                            .lineLimit(1)
                            .id("a-\(state.artist)\(state.album)")
                            .transition(.blurSwap)
                    }
                }
                .animation(.kBouncy, value: state.title)
                Spacer(minLength: 0)
            }

            VStack(spacing: 4) {
                WaveLine(progress: state.progress, playing: state.isPlaying, tint: tint, dim: dim)
                HStack {
                    Text(clock(state.position))
                    Spacer()
                    Text(state.duration > 0 ? "-" + clock(max(0, state.duration - state.position)) : "--:--")
                }
                .font(.custom(bar.fontFamily, size: 10))
                .foregroundColor(dim)
                .monospacedDigit()
            }

            HStack(spacing: 22) {
                SkipButton(symbol: "backward.fill", colour: foreground) { monitor.control(.previous) }
                PlayButton(playing: state.isPlaying, tint: tint, tintColour: ArtworkCache.shared.tint(state.artworkPath), hud: Hud.isHud(theme), arcane: Arcane.isArcane(theme), neon: Neon.isNeon(theme)) {
                    monitor.control(.playPause)
                }
                SkipButton(symbol: "forward.fill", colour: foreground) { monitor.control(.next) }
            }
            .frame(maxWidth: .infinity)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 16)
        .background {
            AmbientArt(path: state.artworkPath, tint: tint, playing: state.isPlaying)
                .opacity(0.9)
        }
        .clipped()
    }

    private var idle: some View {
        KitEmpty(
            icon: "music.note.list",
            title: "Nothing playing",
            detail: "Start something in the music panel",
            palette: KitPalette(theme: theme, bar: bar)
        )
    }

    /// The one piece both layouts share, so the artwork is continuous across the
    /// morph rather than a second image fading in over the first.
    private func thumbnail(size: CGFloat, artworkOnly: Bool = false) -> some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.24)
                .fill(Color.white.opacity(0.06))
            if let artwork {
                Image(nsImage: artwork)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
            } else {
                Image(systemName: "music.note")
                    .font(.system(size: size * 0.34))
                    .foregroundColor(dim.opacity(0.6))
            }
        }
        .frame(width: size, height: size)
        .clipped()
        .clipShape(RoundedRectangle(cornerRadius: size * 0.24))
        .overlay(
            RoundedRectangle(cornerRadius: size * 0.24)
                .stroke(Color.white.opacity(0.10), lineWidth: 1)
        )
        .shadow(color: artworkOnly ? .black.opacity(0.45) : .clear, radius: 12, y: 5)
    }

    // MARK: - helpers

    private var style: PanelStyle? { viewModel.panelStyle(for: .top) }

    private func clock(_ seconds: Double) -> String {
        guard seconds.isFinite, seconds >= 0 else { return "0:00" }
        let total = Int(seconds.rounded())
        let h = total / 3600, m = (total % 3600) / 60, s = total % 60
        return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
    }

    private func loadArtwork(_ path: String?) {
        guard let path, !path.isEmpty else {
            artwork = nil
            return
        }
        artwork = NSImage(contentsOfFile: path)
    }

    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }
}

// MARK: - the controller

/// Opens and closes one island on the display the bar is on. The media monitor
/// only polls while it is open.
public final class MediaIsland {
    private let viewModel: BarViewModel
    private let monitor: MediaMonitor
    private let align: OverlayAlign
    private let bar: () -> BarPanel?
    private var panel: MediaIslandPanel?
    private var isOpen = false

    public init(
        viewModel: BarViewModel,
        monitor: MediaMonitor,
        align: OverlayAlign = .leading,
        bar: @escaping () -> BarPanel?
    ) {
        self.viewModel = viewModel
        self.monitor = monitor
        self.align = align
        self.bar = bar
    }

    public var visible: Bool { isOpen }

    public func show(animated: Bool = true) {
        guard !isOpen, let bar = bar() else { return }
        isOpen = true
        monitor.start()

        let appearance = viewModel.appearance
        let panel = MediaIslandPanel(
            screen: bar.targetScreen,
            // The bar's real rectangle: the config's appearance has not had the
            // frame's margins folded in, so measuring from it put the island
            // inside the bar.
            barFrame: bar.visibleFrame,
            edge: appearance.edge,
            size: MediaIsland.expandedSize,
            pillSize: MediaIsland.pillSize,
            align: align,
            paddingX: CGFloat(appearance.paddingX),
            radius: CGFloat(viewModel.panelSurface?.radius ?? 22),
            viewModel: viewModel,
            monitor: monitor,
            style: viewModel.panelStyle(for: .top),
            glass: viewModel.makeGlass,
            onClose: { [weak self] in self?.hide() }
        )
        self.panel = panel
        panel.show(animated: animated)
    }

    public func hide(animated: Bool = true) {
        guard isOpen else { return }
        isOpen = false
        monitor.stop()
        let current = panel
        panel = nil
        current?.hide(animated: animated)
    }

    public func toggle() { isOpen ? hide() : show() }

    /// The fully-open island, and the pill it grows out of.
    static let expandedSize = NSSize(width: 460, height: 300)
    static let pillSize = NSSize(width: 260, height: 44)
}
