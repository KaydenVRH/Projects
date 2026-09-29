import AppKit
import SwiftUI

/// The screen edge an overlay panel is anchored to — the edge it appears to
/// extend out of (and slides in from).
public enum OverlayEdge: Sendable {
    /// Rises out of the bottom edge of the screen (top corners rounded).
    case bottom
    /// Slides in from the left edge of the screen (trailing corners rounded).
    case leading

    /// The corners a sheet on this edge should round.
    var sheetCorners: SheetShape.Corners {
        switch self {
        case .bottom: return .top
        case .leading: return .trailing
        }
    }
}

/// Drives the slide of a sheet's content inside its (stationary) window.
final class OverlayPresentation: ObservableObject {
    /// Whether the sheet is slid into view.
    @Published var presented = false
    /// `false` places the sheet without animating (an instant show/hide).
    @Published var animated = true
}

/// Hosts a sheet's content inside a window that never leaves its screen: the
/// content slides in from (and back out to) the anchored edge while the window
/// stays put. The window clips the content, so nothing can ever leak onto a
/// neighbouring display the way an off-screen window frame would.
private struct SheetHost: View {
    @ObservedObject var presentation: OverlayPresentation
    let edge: OverlayEdge
    let size: CGSize
    let content: AnyView

    private var hiddenOffset: CGSize {
        guard !presentation.presented else { return .zero }
        switch edge {
        case .bottom: return CGSize(width: 0, height: size.height)
        case .leading: return CGSize(width: -size.width, height: 0)
        }
    }

    private var animation: Animation? {
        guard presentation.animated else { return nil }
        return presentation.presented
            // A gentle ease-out: the travel has to be spread across the whole
            // duration or the sheet reads as a pop rather than a slide.
            ? .timingCurve(0.215, 0.61, 0.355, 1.0, duration: OverlayPanel.showDuration)
            : .timingCurve(0.4, 0.0, 1.0, 1.0, duration: OverlayPanel.hideDuration)
    }

    var body: some View {
        content
            .frame(width: size.width, height: size.height)
            .offset(hiddenOffset)
            .animation(animation, value: presentation.presented)
            .clipped()
    }
}

/// A reusable, key-capable overlay panel that slides in from a screen edge.
/// This is the base for popups and the launchers.
public final class OverlayPanel: NSPanel {
    static let showDuration: TimeInterval = 0.30
    static let hideDuration: TimeInterval = 0.18

    private let shownFrame: NSRect
    private let presentation = OverlayPresentation()
    private var isShown = false

    public init(
        screen: NSScreen,
        size: NSSize,
        edge: OverlayEdge = .bottom,
        margin: CGFloat = 0,
        content: AnyView
    ) {
        let screenFrame = screen.frame
        let x: CGFloat
        let y: CGFloat
        switch edge {
        case .bottom:
            x = screenFrame.midX - size.width / 2
            y = screenFrame.minY + margin
        case .leading:
            x = screenFrame.minX + margin
            y = screenFrame.midY - size.height / 2
        }
        // The window always sits at its final spot; only the content slides.
        self.shownFrame = NSRect(x: x, y: y, width: size.width, height: size.height)

        super.init(
            contentRect: shownFrame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        level = .statusBar + 1
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        isMovable = false
        hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]

        let host = NSHostingView(rootView: SheetHost(
            presentation: presentation,
            edge: edge,
            size: size,
            content: content
        ))
        host.frame = NSRect(origin: .zero, size: size)
        host.autoresizingMask = [.width, .height]
        contentView = host
    }

    public override var canBecomeKey: Bool { true }
    public override var canBecomeMain: Bool { false }

    public override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    public var isPresented: Bool { isShown }

    public func show(animated: Bool) {
        guard !isShown else { return }
        isShown = true

        // Lay the window out at its final position with the content slid out, so
        // the first frame the window server composites is the hidden one.
        presentation.animated = animated
        presentation.presented = false
        setFrame(shownFrame, display: true)

        // Force the accessory app to the front and make this panel key so it can
        // receive keystrokes immediately.
        NSApp.activate(ignoringOtherApps: true)
        makeKeyAndOrderFront(nil)
        makeKey()

        guard animated else {
            presentation.presented = true
            return
        }
        // Slide in on the next run loop turn: animating in the same turn that the
        // window is first ordered front is skipped by the window server.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isShown else { return }
            self.presentation.presented = true
        }
    }

    public func hide(animated: Bool) {
        guard isShown else { return }
        isShown = false

        presentation.animated = animated
        presentation.presented = false

        guard animated else {
            orderOut(nil)
            return
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.hideDuration + 0.02) { [weak self] in
            guard let self, !self.isShown else { return }
            self.orderOut(nil)
        }
    }

    public func toggle(animated: Bool) {
        isShown ? hide(animated: animated) : show(animated: animated)
    }
}
