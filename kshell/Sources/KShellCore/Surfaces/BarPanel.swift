import AppKit
import QuartzCore
import SwiftUI

/// A decorationless, non-activating panel pinned to an edge of one display.
/// This is the macOS analog of Quickshell's `PanelWindow`. Supports auto-hide
/// by sliding off the screen edge.
public final class BarPanel: NSPanel {
    public let targetScreen: NSScreen
    public let autohideEnabled: Bool
    public let revealZone: CGFloat
    public let hideDelay: TimeInterval
    public private(set) var isRevealed: Bool

    private let shownFrame: NSRect
    private let shownWindowFrame: NSRect
    private let hiddenWindowFrame: NSRect

    /// Slack kept beyond the screen edge so the reveal's overshoot cannot lift
    /// the bar off the edge (which reads as the bar floating for a moment).
    static let revealBleed: CGFloat = 16

    /// The full window frame for a visible bar frame: the bar itself plus slack
    /// past the screen edge.
    static func windowFrame(visible: NSRect, edge: BarEdge) -> NSRect {
        switch edge {
        case .top:
            return NSRect(x: visible.minX, y: visible.minY,
                          width: visible.width, height: visible.height + revealBleed)
        case .bottom:
            return NSRect(x: visible.minX, y: visible.minY - revealBleed,
                          width: visible.width, height: visible.height + revealBleed)
        }
    }

    public init(screen: NSScreen, appearance: BarConfig, viewModel: BarViewModel) {
        self.targetScreen = screen
        self.autohideEnabled = appearance.autohide
        self.revealZone = CGFloat(appearance.autohideZone)
        self.hideDelay = appearance.autohideDelay

        let shown = BarPanel.frame(for: screen, appearance: appearance)
        let hidden = BarPanel.hiddenFrame(for: screen, appearance: appearance, shown: shown)
        self.shownFrame = shown
        self.shownWindowFrame = BarPanel.windowFrame(visible: shown, edge: appearance.edge)
        self.hiddenWindowFrame = BarPanel.windowFrame(visible: hidden, edge: appearance.edge)
        self.isRevealed = !appearance.autohide

        super.init(
            contentRect: appearance.autohide ? self.hiddenWindowFrame : self.shownWindowFrame,
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
        level = .statusBar
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        isMovable = false
        isMovableByWindowBackground = false
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        isFloatingPanel = true
        ignoresMouseEvents = false
        collectionBehavior = [
            .canJoinAllSpaces,
            .stationary,
            .fullScreenAuxiliary,
            .ignoresCycle,
        ]

        // The window is taller than the bar by `revealBleed`; the bar itself sits
        // at the on-screen end of it.
        let windowSize = NSSize(width: size.width, height: size.height + Self.revealBleed)
        let barFrame = NSRect(
            x: 0,
            y: appearance.edge == .top ? 0 : Self.revealBleed,
            width: size.width,
            height: size.height
        )
        let container = NSView(frame: NSRect(origin: .zero, size: windowSize))
        container.wantsLayer = true
        container.layer?.cornerRadius = appearance.cornerRadius
        container.layer?.masksToBounds = appearance.cornerRadius > 0

        if appearance.blur {
            let effect = NSVisualEffectView(frame: barFrame)
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

        contentView = container
    }

    public func present() {
        orderFrontRegardless()
    }

    public func dismiss() {
        orderOut(nil)
    }

    public func setRevealed(_ revealed: Bool, animated: Bool) {
        isRevealed = revealed
        let target = revealed ? shownWindowFrame : hiddenWindowFrame
        guard frame != target else { return }
        guard animated else {
            setFrame(target, display: true)
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
}
