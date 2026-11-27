import AppKit
import CoreGraphics
import SwiftUI

/// Which side of a panel sits against the frame's inner edge.
public enum PanelAttachment: Equatable, Sendable {
    case none
    case top
    case bottom
    case leading
    case trailing
}

public extension OverlayEdge {
    /// The side of a panel that ends up against the frame when it is anchored to
    /// this screen edge.
    var attachment: PanelAttachment {
        switch self {
        case .top: return .top
        case .bottom: return .bottom
        case .leading: return .leading
        case .trailing: return .trailing
        }
    }
}

/// A panel's outline: which corners are rounded the usual (convex) way, and
/// which curve *into* the frame so the panel reads as attached to it.
public struct PanelStyle: Equatable, Sendable {
    public var radius: CGFloat
    /// How far the concave corners reach into the frame.
    public var filletRadius: CGFloat
    /// The side sitting against the frame, if any.
    public var attachment: PanelAttachment
    /// Neon: cut the free corners off straight instead of rounding them — the
    /// corner leading away from the frame by this much, the other by under half
    /// — and meet the frame with straight flanges instead of curved fillets.
    /// 0 is the usual rounded panel.
    public var cut: CGFloat
    /// Arcane: scoop the free corners out — a concave quarter-circle of this
    /// radius, like an ornate charm — instead of rounding them. The corners
    /// that meet the frame keep their curved fillets. 0 is the usual panel.
    public var notch: CGFloat
    /// Stark: a HUD readout — the free corners chamfered by this much, evenly,
    /// and a raised tab in the middle of the edge furthest from the frame;
    /// straight flanges where it meets the frame. 0 is the usual panel.
    public var hud: CGFloat
    /// How much of the panel is there, 0…1, as interlaced scanlines: below 1
    /// the outline is cut into horizontal bands that thin toward nothing — a
    /// hologram resolving or derezzing. 1 is solid.
    public var interlace: CGFloat

    public init(
        radius: CGFloat = SheetShape.defaultRadius,
        filletRadius: CGFloat = 22,
        attachment: PanelAttachment = .none,
        cut: CGFloat = 0,
        notch: CGFloat = 0,
        hud: CGFloat = 0,
        interlace: CGFloat = 1
    ) {
        self.radius = radius
        self.filletRadius = filletRadius
        self.attachment = attachment
        self.cut = cut
        self.notch = notch
        self.hud = hud
        self.interlace = interlace
    }

    /// The big cut's corner: the one a reader's eye lands on first. The other
    /// free corner gets the small cut.
    var bigCut: SheetShape.Corners {
        switch attachment {
        case .none, .bottom: return .topLeading
        case .top: return .bottomTrailing
        case .leading: return .topTrailing
        case .trailing: return .topLeading
        }
    }

    /// Corners rounded convexly: everything except the attached side.
    public var convexCorners: SheetShape.Corners {
        switch attachment {
        case .none, .top: return .all
        case .bottom: return .top
        case .leading: return .trailing
        case .trailing: return .leading
        }
    }

    /// The corners on the attached side, which get a concave fillet.
    public var filletCorners: SheetShape.Corners {
        switch attachment {
        case .none, .top: return []
        case .bottom: return .bottom
        case .leading: return .leading
        case .trailing: return .trailing
        }
    }
}

/// The panel's outline as a path.
///
/// Built in AppKit's coordinates (y grows upward, so the visual top is `maxY`);
/// `PanelShape` mirrors it for SwiftUI. Each fillet is the little wedge between
/// the panel's edge and the frame's edge, bounded by an arc tangent to both, so
/// the panel's material flows into the frame instead of stopping at a corner.
public enum PanelGeometry {
    public static func path(in rect: CGRect, style: PanelStyle) -> CGPath {
        guard style.interlace < 1 else { return solidPath(in: rect, style: style) }
        // A hologram resolving: the outline, kept only where the scanlines are.
        guard style.interlace > 0.002, rect.width > 0, rect.height > 0 else { return CGMutablePath() }
        var solid = style
        solid.interlace = 1
        let outline = solidPath(in: rect, style: solid)
        let pitch: CGFloat = 5
        let band = pitch * min(1, style.interlace)
        let bands = CGMutablePath()
        let bounds = outline.boundingBox
        var y = bounds.minY
        while y < bounds.maxY {
            bands.addRect(CGRect(x: bounds.minX, y: y, width: bounds.width, height: band))
            y += pitch
        }
        return outline.intersection(bands)
    }

    private static func solidPath(in rect: CGRect, style: PanelStyle) -> CGPath {
        if style.hud > 0 { return hudPath(in: rect, style: style) }
        if style.cut > 0 { return cutPath(in: rect, style: style) }
        if style.notch > 0 { return notchPath(in: rect, style: style) }
        let radius = max(0, min(style.radius, min(rect.width, rect.height) / 2))
        let fillet = max(0, min(style.filletRadius, min(rect.width, rect.height) / 2))
        let convex = style.convexCorners

        let topLeading = convex.contains(.topLeading) ? radius : 0
        let topTrailing = convex.contains(.topTrailing) ? radius : 0
        let bottomLeading = convex.contains(.bottomLeading) ? radius : 0
        let bottomTrailing = convex.contains(.bottomTrailing) ? radius : 0

        let path = CGMutablePath()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY + bottomLeading))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - topLeading))
        path.addArc(
            tangent1End: CGPoint(x: rect.minX, y: rect.maxY),
            tangent2End: CGPoint(x: rect.minX + topLeading, y: rect.maxY),
            radius: topLeading
        )
        path.addLine(to: CGPoint(x: rect.maxX - topTrailing, y: rect.maxY))
        path.addArc(
            tangent1End: CGPoint(x: rect.maxX, y: rect.maxY),
            tangent2End: CGPoint(x: rect.maxX, y: rect.maxY - topTrailing),
            radius: topTrailing
        )
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + bottomTrailing))
        path.addArc(
            tangent1End: CGPoint(x: rect.maxX, y: rect.minY),
            tangent2End: CGPoint(x: rect.maxX - bottomTrailing, y: rect.minY),
            radius: bottomTrailing
        )
        path.addLine(to: CGPoint(x: rect.minX + bottomLeading, y: rect.minY))
        path.addArc(
            tangent1End: CGPoint(x: rect.minX, y: rect.minY),
            tangent2End: CGPoint(x: rect.minX, y: rect.minY + bottomLeading),
            radius: bottomLeading
        )
        path.closeSubpath()

        guard fillet > 0 else { return path }
        for corner in corners(of: style.filletCorners) {
            path.addPath(filletPath(corner, in: rect, radius: fillet, attachment: style.attachment))
        }
        return path
    }

    private enum Corner {
        case topLeading, topTrailing, bottomLeading, bottomTrailing
    }

    /// The Arcane outline: each free corner scooped out with a concave arc,
    /// attached ones meeting the frame in the usual curved fillets.
    private static func notchPath(in rect: CGRect, style: PanelStyle) -> CGPath {
        let limit = min(rect.width, rect.height) / 2
        let convex = style.convexCorners
        func size(_ corner: SheetShape.Corners) -> CGFloat {
            convex.contains(corner) ? max(0, min(style.notch, limit)) : 0
        }
        let tl = size(.topLeading), tr = size(.topTrailing)
        let bl = size(.bottomLeading), br = size(.bottomTrailing)

        let path = CGMutablePath()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY + bl))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - tl))
        // each scoop is a quarter circle centred on the corner itself, so it
        // bites into the panel
        if tl > 0 {
            path.addArc(center: CGPoint(x: rect.minX, y: rect.maxY), radius: tl,
                        startAngle: -.pi / 2, endAngle: 0, clockwise: false)
        }
        path.addLine(to: CGPoint(x: rect.maxX - tr, y: rect.maxY))
        if tr > 0 {
            path.addArc(center: CGPoint(x: rect.maxX, y: rect.maxY), radius: tr,
                        startAngle: .pi, endAngle: .pi * 1.5, clockwise: false)
        }
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + br))
        if br > 0 {
            path.addArc(center: CGPoint(x: rect.maxX, y: rect.minY), radius: br,
                        startAngle: .pi / 2, endAngle: .pi, clockwise: false)
        }
        path.addLine(to: CGPoint(x: rect.minX + bl, y: rect.minY))
        if bl > 0 {
            path.addArc(center: CGPoint(x: rect.minX, y: rect.minY), radius: bl,
                        startAngle: 0, endAngle: .pi / 2, clockwise: false)
        }
        path.closeSubpath()

        let fillet = max(0, min(style.filletRadius, limit))
        guard fillet > 0 else { return path }
        for corner in corners(of: style.filletCorners) {
            path.addPath(filletPath(corner, in: rect, radius: fillet, attachment: style.attachment))
        }
        return path
    }

    /// The Neon outline: free corners cut off straight (one big, one small),
    /// attached ones meeting the frame in straight 45° flanges.
    private static func cutPath(in rect: CGRect, style: PanelStyle) -> CGPath {
        let limit = min(rect.width, rect.height) / 2
        let convex = style.convexCorners
        func cut(_ corner: SheetShape.Corners) -> CGFloat {
            guard convex.contains(corner) else { return 0 }
            let size = style.bigCut.contains(corner) ? style.cut : style.cut * 0.4
            return max(0, min(size, limit))
        }
        let tl = cut(.topLeading), tr = cut(.topTrailing)
        let bl = cut(.bottomLeading), br = cut(.bottomTrailing)

        let path = CGMutablePath()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY + bl))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - tl))
        path.addLine(to: CGPoint(x: rect.minX + tl, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.maxX - tr, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - tr))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + br))
        path.addLine(to: CGPoint(x: rect.maxX - br, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.minX + bl, y: rect.minY))
        path.closeSubpath()

        addFlanges(to: path, in: rect, style: style)
        return path
    }

    /// The Stark outline, drawn as if the panel sat on the bottom edge — the
    /// far edge on top, its middle third raised as a tab — then turned to the
    /// side it really sits on.
    private static func hudPath(in rect: CGRect, style: PanelStyle) -> CGPath {
        let vertical = style.attachment == .leading || style.attachment == .trailing
        let w = vertical ? rect.height : rect.width     // along the frame
        let h = vertical ? rect.width : rect.height     // out from it
        let c = max(0, min(style.hud, min(w, h) / 3))
        // the tab stands this proud of the rest of the far edge
        let rise = min(5, h / 6)
        let tabL = w * 0.34, tabR = w * 0.66
        let floating = style.attachment == .none
        var points: [CGPoint] = []
        if floating {
            points += [CGPoint(x: c, y: 0), CGPoint(x: 0, y: c)]
        } else {
            points.append(CGPoint(x: 0, y: 0))
        }
        points += [
            CGPoint(x: 0, y: h - rise - c),
            CGPoint(x: c, y: h - rise),
            CGPoint(x: tabL - rise, y: h - rise),
            CGPoint(x: tabL, y: h),
            CGPoint(x: tabR, y: h),
            CGPoint(x: tabR + rise, y: h - rise),
            CGPoint(x: w - c, y: h - rise),
            CGPoint(x: w, y: h - rise - c),
        ]
        if floating {
            points += [CGPoint(x: w, y: c), CGPoint(x: w - c, y: 0)]
        } else {
            points.append(CGPoint(x: w, y: 0))
        }
        // (x along the frame, y away from it) → the real rectangle
        func place(_ p: CGPoint) -> CGPoint {
            switch style.attachment {
            case .none, .bottom: return CGPoint(x: rect.minX + p.x, y: rect.minY + p.y)
            case .top: return CGPoint(x: rect.minX + p.x, y: rect.maxY - p.y)
            case .leading: return CGPoint(x: rect.minX + p.y, y: rect.minY + p.x)
            case .trailing: return CGPoint(x: rect.maxX - p.y, y: rect.minY + p.x)
            }
        }
        let path = CGMutablePath()
        path.addLines(between: points.map(place))
        path.closeSubpath()
        addFlanges(to: path, in: rect, style: style)
        return path
    }

    /// Straight 45° flanges where the panel meets the frame: the fillets'
    /// wedges with their arcs pulled straight.
    private static func addFlanges(to path: CGMutablePath, in rect: CGRect, style: PanelStyle) {
        let flange = max(0, min(style.filletRadius, min(rect.width, rect.height) / 2))
        guard flange > 0 else { return }
        for corner in corners(of: style.filletCorners) {
            let wedge = filletPath(corner, in: rect, radius: flange, attachment: style.attachment)
            var points: [CGPoint] = []
            wedge.applyWithBlock { element in
                let e = element.pointee
                switch e.type {
                case .moveToPoint, .addLineToPoint: points.append(e.points[0])
                case .addQuadCurveToPoint: points.append(e.points[1])
                case .addCurveToPoint: points.append(e.points[2])
                default: break
                }
            }
            guard points.count >= 3 else { continue }
            let corner = points[0], outward = points[1], along = points[points.count - 1]
            path.move(to: corner)
            path.addLine(to: outward)
            path.addLine(to: along)
            path.closeSubpath()
        }
    }

    private static func corners(of set: SheetShape.Corners) -> [Corner] {
        var result: [Corner] = []
        if set.contains(.topLeading) { result.append(.topLeading) }
        if set.contains(.topTrailing) { result.append(.topTrailing) }
        if set.contains(.bottomLeading) { result.append(.bottomLeading) }
        if set.contains(.bottomTrailing) { result.append(.bottomTrailing) }
        return result
    }

    /// The wedge at one corner: out from the panel along the frame, and along the
    /// panel's edge to the other side of the corner, joined by a concave arc.
    private static func filletPath(
        _ corner: Corner,
        in rect: CGRect,
        radius: CGFloat,
        attachment: PanelAttachment
    ) -> CGPath {
        let path = CGMutablePath()
        let cornerPoint: CGPoint
        let outward: CGPoint
        let along: CGPoint
        switch (attachment, corner) {
        case (.bottom, .bottomLeading):
            cornerPoint = CGPoint(x: rect.minX, y: rect.minY)
            outward = CGPoint(x: rect.minX - radius, y: rect.minY)
            along = CGPoint(x: rect.minX, y: rect.minY + radius)
        case (.bottom, .bottomTrailing):
            cornerPoint = CGPoint(x: rect.maxX, y: rect.minY)
            outward = CGPoint(x: rect.maxX + radius, y: rect.minY)
            along = CGPoint(x: rect.maxX, y: rect.minY + radius)
        case (.leading, .topLeading):
            cornerPoint = CGPoint(x: rect.minX, y: rect.maxY)
            outward = CGPoint(x: rect.minX, y: rect.maxY + radius)
            along = CGPoint(x: rect.minX + radius, y: rect.maxY)
        case (.leading, .bottomLeading):
            cornerPoint = CGPoint(x: rect.minX, y: rect.minY)
            outward = CGPoint(x: rect.minX, y: rect.minY - radius)
            along = CGPoint(x: rect.minX + radius, y: rect.minY)
        case (.trailing, .topTrailing):
            cornerPoint = CGPoint(x: rect.maxX, y: rect.maxY)
            outward = CGPoint(x: rect.maxX, y: rect.maxY + radius)
            along = CGPoint(x: rect.maxX - radius, y: rect.maxY)
        case (.trailing, .bottomTrailing):
            cornerPoint = CGPoint(x: rect.maxX, y: rect.minY)
            outward = CGPoint(x: rect.maxX, y: rect.minY - radius)
            along = CGPoint(x: rect.maxX - radius, y: rect.minY)
        default:
            return path
        }
        path.move(to: cornerPoint)
        path.addLine(to: outward)
        path.addArc(tangent1End: cornerPoint, tangent2End: along, radius: radius)
        path.closeSubpath()
        return path
    }
}

/// The panel outline as a SwiftUI shape, so a panel's content agrees with its
/// glass about where the edges and fillets are.
public struct PanelShape: Shape {
    public var style: PanelStyle

    public init(style: PanelStyle) {
        self.style = style
    }

    public func path(in rect: CGRect) -> Path {
        let path = PanelGeometry.path(in: rect, style: style)
        // AppKit's outline has y growing upward; SwiftUI's space has it growing
        // downward, so mirror it about the halfway line.
        let flip = CGAffineTransform(1, 0, 0, -1, 0, rect.height)
        return Path(path.copy(using: [flip]) ?? path)
    }
}

/// What the launchers need in order to take their glass from the frame.
public struct PanelSurfaceSettings: Equatable, Sendable {
    public var radius: CGFloat
    public var filletRadius: CGFloat

    public init(radius: CGFloat, filletRadius: CGFloat = 22) {
        self.radius = radius
        self.filletRadius = filletRadius
    }
}

/// A launcher panel's glass, as the surface holder needs to know it: where the
/// panel rests on screen, its outline, and how far it still is from resting.
public struct SurfacePanel: Equatable, Sendable {
    public var rect: NSRect
    public var style: PanelStyle
    public var offset: CGSize

    public init(rect: NSRect, style: PanelStyle, offset: CGSize) {
        self.rect = rect
        self.style = style
        self.offset = offset
    }
}

/// Draws a launcher panel's surface.
///
/// While the frame is on, the panel's glass lives on the frame's surface and
/// this only fills the panel's outline with the tint — the panel is masked to
/// the same outline, so the two agree about the edges and the fillets. With the
/// frame off, the panel draws its own material exactly as it used to.
struct PanelSurface: ViewModifier {
    @ObservedObject var viewModel: BarViewModel
    let edge: OverlayEdge
    let fallback: SheetShape

    func body(content: Content) -> some View {
        if viewModel.panelStyle(for: edge) != nil {
            // The frame's surface provides both the glass and the tint; the
            // panel only contributes content. The panel's own window masks it to
            // the same outline, so the two agree about edges and fillets.
            content
        } else {
            content
                .background {
                    ZStack {
                        if viewModel.appearance.blur { VisualEffectBackground() }
                        viewModel.appearance.background.color
                    }
                }
                .clipShape(fallback)
        }
    }
}

/// The shell's slide curve, sampled so a per-frame driver can use the same
/// shape as Core Animation would: strongly decelerating on the way out, crisp on
/// the way back in.
public enum SlideEasing {
    /// A cubic-bezier step for the panels' stepped geometry.
    ///
    /// Both directions stay within 0…1: a panel arrives quickly and stops at its
    /// resting size, with no bounce past it. The springiness lives in the rows
    /// and selections inside the panel instead.
    public static func value(_ t: CGFloat, opening: Bool) -> CGFloat {
        let c1x: CGFloat = opening ? 0.18 : 0.42
        let c1y: CGFloat = opening ? 1.00 : 0.00
        let c2x: CGFloat = opening ? 0.36 : 1.00
        let c2y: CGFloat = opening ? 1.00 : 1.00
        if t <= 0 { return 0 }
        if t >= 1 { return 1 }
        func bezier(_ p1: CGFloat, _ p2: CGFloat, _ x: CGFloat) -> CGFloat {
            let mt = 1 - x
            return 3 * mt * mt * x * p1 + 3 * mt * x * x * p2 + x * x * x
        }
        var low: CGFloat = 0
        var high: CGFloat = 1
        for _ in 0..<24 {
            let mid = (low + high) / 2
            if bezier(c1x, c2x, mid) < t { low = mid } else { high = mid }
        }
        return bezier(c1y, c2y, (low + high) / 2)
    }
}
