import AppKit
import QuartzCore
import SwiftUI

/// A reusable, key-capable overlay panel that slides up from below the screen.
/// This is the base for popups and the app launcher.
public final class OverlayPanel: NSPanel {
    private let shownFrame: NSRect
    private let hiddenFrame: NSRect
    private var isShown = false

    public init(
        screen: NSScreen,
        size: NSSize,
        bottomMargin: CGFloat,
        blur: Bool,
        cornerRadius: CGFloat,
        content: AnyView
    ) {
        let x = screen.frame.midX - size.width / 2
        let shown = NSRect(x: x, y: screen.frame.minY + bottomMargin, width: size.width, height: size.height)
        let hidden = NSRect(x: x, y: screen.frame.minY - size.height - 24, width: size.width, height: size.height)
        self.shownFrame = shown
        self.hiddenFrame = hidden

        super.init(
            contentRect: hidden,
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

        let container = NSView(frame: NSRect(origin: .zero, size: size))
        container.wantsLayer = true
        container.layer?.cornerRadius = cornerRadius
        container.layer?.masksToBounds = cornerRadius > 0

        if blur {
            let effect = NSVisualEffectView(frame: container.bounds)
            effect.autoresizingMask = [.width, .height]
            effect.material = .hudWindow
            effect.blendingMode = .behindWindow
            effect.state = .active
            container.addSubview(effect)
        }

        let host = NSHostingView(rootView: content)
        host.frame = container.bounds
        host.autoresizingMask = [.width, .height]
        container.addSubview(host)

        contentView = container
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
        setFrame(hiddenFrame, display: false)
        NSApp.activate(ignoringOtherApps: true)
        makeKeyAndOrderFront(nil)

        guard animated else {
            setFrame(shownFrame, display: true)
            return
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.26
            context.timingFunction = CAMediaTimingFunction(controlPoints: 0.16, 1.0, 0.3, 1.0)
            animator().setFrame(shownFrame, display: true)
        }
    }

    public func hide(animated: Bool) {
        guard isShown else { return }
        isShown = false

        guard animated else {
            orderOut(nil)
            return
        }
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.18
            context.timingFunction = CAMediaTimingFunction(name: .easeIn)
            animator().setFrame(hiddenFrame, display: true)
        }, completionHandler: { [weak self] in
            self?.orderOut(nil)
        })
    }

    public func toggle(animated: Bool) {
        isShown ? hide(animated: animated) : show(animated: animated)
    }
}
