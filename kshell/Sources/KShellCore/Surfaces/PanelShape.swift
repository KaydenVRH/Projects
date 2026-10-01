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

    public init(
        radius: CGFloat = SheetShape.defaultRadius,
        filletRadius: CGFloat = 22,
        attachment: PanelAttachment = .none
    ) {
        self.radius = radius
        self.filletRadius = filletRadius
        self.attachment = attachment
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
    public static func value(_ t: CGFloat, opening: Bool) -> CGFloat {
        let c1x: CGFloat = opening ? 0.22 : 0.42
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
