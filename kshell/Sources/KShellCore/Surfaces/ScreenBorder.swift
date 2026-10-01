import AppKit
import QuartzCore

/// The surface holder for a display: the frame drawn inside the screen's edges,
/// and the glass that the frame, the bar and the sheet hanging off the bar are
/// cut from.
///
/// The frame and the bar are one vibrancy view masked to their shapes; the sheet
/// has its own vibrancy view *in the same window* (two material views in one
/// window render alike, unlike two in different windows) so that it can be
/// slid — its position animated — instead of revealed.
///
/// That distinction matters: an `NSVisualEffectView`'s blur does not follow an
/// animated layer mask, it only honours a static shape. Revealing by animating
/// the mask made the sheet's glass snap out at full size the instant it opened,
/// leaving a bare "background sheet" under the content as it slid. Sliding the
/// view moves the blur with it, so the glass and the content travel together.
///
/// Decoration only as far as the mouse is concerned: the window ignores mouse
/// events, so the bar's own window (and the launchers) keep handling clicks.
public final class ScreenBorderPanel: NSPanel {
    /// Screen rectangle minus the rounded opening, plus the bar: where the
    /// frame's and the bar's glass is.
    private let surfaceMask = CAShapeLayer()
    /// The frame alone — the part that carries its own tint.
    private let tintMask = CAShapeLayer()
    /// Clips the sheet's glass to below the bar, so it can wait behind it.
    private let sheetClip = NSView()
    private let sheetGlass = NSVisualEffectView()
    private let sheetMask = CAShapeLayer()

    private let config: BorderConfig
    private let screenFrame: NSRect
    /// Glass for launcher panels, built with the window (an effect view added to
    /// a window that is already on screen is unreliable).
    private var panelGlasses: [SurfaceGlass] = []
    /// The display this frame belongs to.
    public let borderScreen: NSScreen
    private var barFrame: NSRect
    private var sheetResting: NSRect

    public init(screen: NSScreen, config: BorderConfig, barFrame: NSRect, sheetResting: NSRect) {
        self.config = config
        self.borderScreen = screen
        self.screenFrame = screen.frame
        self.barFrame = barFrame
        self.sheetResting = sheetResting

        super.init(
            contentRect: screen.frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        isFloatingPanel = true
        // Same level as the bar used to be: above ordinary windows and the menu
        // bar, below the bar's content window and the launchers.
        level = .statusBar
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        isMovable = false
        hidesOnDeactivate = false
        // Deliberately no `.fullScreenAuxiliary`: the shell gets out of the way
        // of full-screen apps rather than drawing over them.
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]

        let root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        root.wantsLayer = true

        if config.blur {
            let glass = NSView(frame: root.bounds)
            glass.wantsLayer = true
            glass.autoresizingMask = [.width, .height]
            surfaceMask.fillRule = .evenOdd
            surfaceMask.fillColor = NSColor.black.cgColor
            glass.layer?.mask = surfaceMask

            let effect = NSVisualEffectView(frame: glass.bounds)
            effect.material = .hudWindow
            effect.blendingMode = .behindWindow
            effect.state = .active
            effect.autoresizingMask = [.width, .height]
            glass.addSubview(effect)
            root.addSubview(glass)

            // The sheet's glass: same window, same material, and it slides.
            sheetClip.wantsLayer = true
            sheetClip.layer?.masksToBounds = true
            sheetGlass.material = .hudWindow
            sheetGlass.blendingMode = .behindWindow
            sheetGlass.state = .active
            sheetMask.fillColor = NSColor.black.cgColor
            sheetGlass.wantsLayer = true
            sheetGlass.layer?.mask = sheetMask
            sheetClip.addSubview(sheetGlass)
            root.addSubview(sheetClip)
        }

        let tint = CALayer()
        tint.frame = root.bounds
        tint.backgroundColor = config.color.nsColor.cgColor
        tintMask.fillRule = .evenOdd
        tintMask.fillColor = NSColor.black.cgColor
        tint.mask = tintMask
        root.layer?.addSublayer(tint)

        contentView = root
        if config.blur {
            panelGlasses = (0..<2).map { _ in
                SurfaceGlass(in: root, screenFrame: screen.frame, color: config.color.nsColor)
            }
        }
        refresh()
        setSheetOffset(sheetClip.frame.height)
    }

    /// Glass for a launcher panel to sit on, or nil when the blur is off.
    ///
    /// A panel's glass is its own view in this window rather than part of the
    /// surface mask: both render identically here (one window, one material),
    /// but a view's *position* can be stepped frame by frame and the blur
    /// follows, whereas a masked backdrop is updated lazily and lags the content
    /// — which showed up as a panel floating during its slide and snapping into
    /// place at the end.
    public func makeGlass() -> SurfaceGlass? {
        guard config.blur else { return nil }
        if let free = panelGlasses.first(where: { !$0.isInUse }) {
            return free
        }
        if let oldest = panelGlasses.first {
            return oldest
        }
        return nil
    }

    /// A fresh piece of this window's glass for a launcher panel to sit on, so
    /// the panel's material is the very same vibrancy view the frame and bar are
    /// cut from rather than a near-match in its own window.
    /// The bar's visible rectangle, in screen coordinates. The glass under the
    /// bar follows it (the bar moves when it auto-hides).
    public func setBarFrame(_ rect: NSRect) {
        barFrame = rect
        refresh()
    }

    /// Where the sheet rests when it is open. Published by the bar, which owns
    /// the geometry.
    public func configureSheet(_ resting: NSRect) {
        sheetResting = resting
        refresh()
        setSheetOffset(sheetClip.frame.height)
    }

    /// Move the sheet's glass. `offset` is how far it still is from resting,
    /// measured away from the bar — the same number the content is offset by.
    ///
    /// The caller drives this frame by frame rather than with an animation: the
    /// window server renders an effect view's blur from the layer's model
    /// geometry, so an animation on the view is not what the blur follows.
    public func setSheetOffset(_ offset: CGFloat) {
        let resting = NSRect(origin: .zero, size: sheetClip.frame.size)
        sheetGlass.frame = resting.offsetBy(dx: 0, dy: offset)
    }

    // MARK: - geometry (window coordinates; the window is the screen)

    private func local(_ rect: NSRect) -> NSRect {
        NSRect(
            x: rect.minX - screenFrame.minX,
            y: rect.minY - screenFrame.minY,
            width: rect.width,
            height: rect.height
        )
    }

    /// Inset by the frame's thickness on every side: the opening the screen's
    /// content sits in, rounded so the frame thickens into its corners.
    private func opening(in bounds: NSRect) -> NSRect {
        bounds.insetBy(
            dx: CGFloat(config.inset + config.thickness),
            dy: CGFloat(config.inset + config.thickness)
        )
    }

    private func refresh() {
        let bounds = contentView?.bounds ?? NSRect(origin: .zero, size: screenFrame.size)

        // The frame's ring, which carries the frame's tint.
        let frame = CGMutablePath()
        frame.addRect(bounds)
        frame.addPath(ScreenBorderPanel.roundedRect(
            opening(in: bounds),
            radius: CGFloat(config.radius)
        ))

        tintMask.path = frame

        let surface = CGMutablePath()
        surface.addRect(bounds)
        surface.addPath(ScreenBorderPanel.roundedRect(
            opening(in: bounds),
            radius: CGFloat(config.radius)
        ))
        surface.addPath(ScreenBorderPanel.roundedRect(
            local(barFrame),
            radius: CGFloat(config.radius)
        ))
        surfaceMask.path = surface

        // The sheet's glass is clipped to the region below the bar and masked
        // with the sheet's own outward corners.
        let sheet = local(sheetResting)
        let bar = local(barFrame)
        sheetClip.frame = NSRect(
            x: sheet.minX,
            y: sheet.minY,
            width: sheet.width,
            height: max(bar.minY - sheet.minY, 1)
        )
        sheetMask.path = ScreenBorderPanel.roundedRect(
            NSRect(origin: .zero, size: sheetClip.frame.size),
            radius: SheetShape.defaultRadius,
            corners: sheetEdgeIsTop ? .bottom : .top
        )
    }

    /// The bar hangs above the sheet for a top bar, below it for a bottom bar.
    private var sheetEdgeIsTop: Bool {
        sheetResting.midY < barFrame.midY
    }

    private enum Corner {
        case top, bottom, all
    }

    private static func roundedRect(
        _ rect: NSRect,
        radius: CGFloat,
        corners: Corner = .all
    ) -> CGPath {
        let path = CGMutablePath()
        guard rect.width > 0, rect.height > 0 else { return path }
        let r = max(0, min(radius, min(rect.width, rect.height) / 2))
        let topR = corners == .bottom ? 0 : r
        let bottomR = corners == .top ? 0 : r

        path.move(to: CGPoint(x: rect.minX, y: rect.minY + bottomR))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - topR))
        path.addArc(
            tangent1End: CGPoint(x: rect.minX, y: rect.maxY),
            tangent2End: CGPoint(x: rect.minX + topR, y: rect.maxY),
            radius: topR
        )
        path.addLine(to: CGPoint(x: rect.maxX - topR, y: rect.maxY))
        path.addArc(
            tangent1End: CGPoint(x: rect.maxX, y: rect.maxY),
            tangent2End: CGPoint(x: rect.maxX, y: rect.maxY - topR),
            radius: topR
        )
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + bottomR))
        path.addArc(
            tangent1End: CGPoint(x: rect.maxX, y: rect.minY),
            tangent2End: CGPoint(x: rect.maxX - bottomR, y: rect.minY),
            radius: bottomR
        )
        path.addLine(to: CGPoint(x: rect.minX + bottomR, y: rect.minY))
        path.addArc(
            tangent1End: CGPoint(x: rect.minX, y: rect.minY),
            tangent2End: CGPoint(x: rect.minX, y: rect.minY + bottomR),
            radius: bottomR
        )
        path.closeSubpath()
        return path
    }
}

/// A launcher panel's glass: the same vibrancy view and tint the frame is made
/// of, in the frame's own window — so a panel meets the frame seamlessly — but
/// as a view that can be slid frame by frame, outline and fillets included.
public final class SurfaceGlass {
    private let clip = NSView()
    private let glass = NSVisualEffectView()
    private let tint = CALayer()
    private let mask = CAShapeLayer()
    private let screenFrame: NSRect
    private var resting: NSRect = .zero
    private var localResting: NSRect = .zero
    private(set) var isInUse = false

    fileprivate init(in parent: NSView, screenFrame: NSRect, color: NSColor) {
        self.screenFrame = screenFrame
        clip.wantsLayer = true
        clip.autoresizingMask = []

        mask.fillColor = NSColor.black.cgColor
        clip.layer?.mask = mask

        glass.material = .hudWindow
        glass.blendingMode = .behindWindow
        glass.state = .active
        glass.autoresizingMask = [.width, .height]
        glass.wantsLayer = true
        clip.addSubview(glass)

        tint.backgroundColor = color.cgColor
        clip.layer?.addSublayer(tint)

        park()
        parent.addSubview(clip)
    }

    /// Where the panel rests, in screen coordinates, and its outline. The clip is
    /// grown past the panel on the side that meets the frame so the concave
    /// fillets have room to be drawn.
    public func setResting(_ rect: NSRect, style: PanelStyle?) {
        isInUse = true
        resting = rect
        let expansion = CGFloat(style?.filletRadius ?? 0)
        var local = NSRect(
            x: rect.minX - screenFrame.minX,
            y: rect.minY - screenFrame.minY,
            width: rect.width,
            height: rect.height
        )
        // The fillets sit at the two ends of the edge that meets the frame, so
        // the clip grows *sideways* for a top/bottom panel and *vertically* for
        // a side one — along the frame, not away from it.
        switch style?.attachment ?? .none {
        case .none: break
        case .top, .bottom:
            local.origin.x -= expansion
            local.size.width += expansion * 2
        case .leading, .trailing:
            local.origin.y -= expansion
            local.size.height += expansion * 2
        }
        localResting = local
        clip.frame = local
        glass.frame = NSRect(origin: .zero, size: local.size)
        tint.frame = NSRect(origin: .zero, size: local.size)

        let panel = CGRect(
            x: rect.minX - screenFrame.minX - local.origin.x,
            y: rect.minY - screenFrame.minY - local.origin.y,
            width: rect.width,
            height: rect.height
        )
        mask.path = style.map { PanelGeometry.path(in: panel, style: $0) }
            ?? CGPath(rect: panel, transform: nil)
    }

    /// How far the panel still is from resting, along its slide.
    public func setOffset(_ offset: CGSize) {
        clip.frame.origin = CGPoint(
            x: localResting.minX + offset.width,
            y: localResting.minY + offset.height
        )
    }

    /// Clear of the screen, ready for the next panel.
    public func park() {
        isInUse = false
        clip.frame = NSRect(x: screenFrame.maxX + 50, y: screenFrame.minY, width: 1, height: 1)
    }
}
