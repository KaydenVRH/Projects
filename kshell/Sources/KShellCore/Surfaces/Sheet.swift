import AppKit
import SwiftUI

/// A blur that samples what's behind the window (the frosted sheet look).
struct VisualEffectBackground: NSViewRepresentable {
    var material: NSVisualEffectView.Material = .hudWindow

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = .behindWindow
        view.state = .active
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {
        nsView.material = material
    }
}

/// A rounded rectangle that only rounds the corners along the screen edge it is
/// anchored to — so a panel reads as extending out of that edge (a sheet rising
/// from the bottom, or a sidebar sliding in from the left).
public struct SheetShape: Shape {
    /// Which corners to round.
    public struct Corners: OptionSet, Sendable {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }

        public static let topLeading = Corners(rawValue: 1 << 0)
        public static let topTrailing = Corners(rawValue: 1 << 1)
        public static let bottomLeading = Corners(rawValue: 1 << 2)
        public static let bottomTrailing = Corners(rawValue: 1 << 3)

        /// Rounds the top corners — a sheet rising from the bottom edge.
        public static let top: Corners = [.topLeading, .topTrailing]
        /// Rounds the trailing corners — a sheet sliding in from the left edge.
        public static let trailing: Corners = [.topTrailing, .bottomTrailing]
        /// Rounds the leading corners — a sheet sliding in from the right edge.
        public static let leading: Corners = [.topLeading, .bottomLeading]
        /// Rounds the bottom corners — a sheet dropping from the top edge.
        public static let bottom: Corners = [.bottomLeading, .bottomTrailing]
        public static let all: Corners = [.topLeading, .topTrailing, .bottomLeading, .bottomTrailing]
    }

    public var radius: CGFloat
    public var corners: Corners

    public init(radius: CGFloat = 22, corners: Corners = .top) {
        self.radius = radius
        self.corners = corners
    }

    public func path(in rect: CGRect) -> Path {
        UnevenRoundedRectangle(
            topLeadingRadius: corners.contains(.topLeading) ? radius : 0,
            bottomLeadingRadius: corners.contains(.bottomLeading) ? radius : 0,
            bottomTrailingRadius: corners.contains(.bottomTrailing) ? radius : 0,
            topTrailingRadius: corners.contains(.topTrailing) ? radius : 0,
            style: .continuous
        )
        .path(in: rect)
    }
}
