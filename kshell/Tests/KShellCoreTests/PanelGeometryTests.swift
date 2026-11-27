import XCTest
@testable import KShellCore

final class PanelGeometryTests: XCTestCase {
    private func elements(_ path: CGPath) -> [CGPathElementType] {
        var types: [CGPathElementType] = []
        path.applyWithBlock { types.append($0.pointee.type) }
        return types
    }

    /// Neon's outline is all straight lines: cut corners, and flanges where
    /// the rounded panel has fillets.
    func testCutOutlineIsStraight() {
        let rect = CGRect(x: 0, y: 0, width: 400, height: 300)
        let style = PanelStyle(radius: 22, filletRadius: 22, attachment: .bottom, cut: 26)
        let path = PanelGeometry.path(in: rect, style: style)
        let types = elements(path)
        XCTAssertFalse(types.contains(.addCurveToPoint))
        XCTAssertFalse(types.contains(.addQuadCurveToPoint))
        // the same footprint as the rounded outline, flanges included
        let rounded = PanelGeometry.path(in: rect, style: PanelStyle(radius: 22, filletRadius: 22, attachment: .bottom))
        XCTAssertEqual(path.boundingBox.minX, rounded.boundingBox.minX, accuracy: 0.001)
        XCTAssertEqual(path.boundingBox.maxX, rounded.boundingBox.maxX, accuracy: 0.001)
        XCTAssertEqual(path.boundingBox.minY, rounded.boundingBox.minY, accuracy: 0.001)
        XCTAssertEqual(path.boundingBox.maxY, rounded.boundingBox.maxY, accuracy: 0.001)
    }

    /// The big cut on the corner away from the frame's leading side, the small
    /// one on the other free corner; attached corners stay square.
    func testCutSizes() {
        let rect = CGRect(x: 0, y: 0, width: 400, height: 300)
        let style = PanelStyle(radius: 22, filletRadius: 0, attachment: .bottom, cut: 30)
        let path = PanelGeometry.path(in: rect, style: style)
        XCTAssertFalse(path.contains(CGPoint(x: 2, y: 298)), "top-leading: the big cut")
        XCTAssertTrue(path.contains(CGPoint(x: 398, y: 285)), "top-trailing: a small cut misses this")
        XCTAssertFalse(path.contains(CGPoint(x: 398, y: 298)))
        XCTAssertTrue(path.contains(CGPoint(x: 1, y: 1)), "attached corners stay square")
        XCTAssertTrue(path.contains(CGPoint(x: 399, y: 1)))
    }

    /// A panel squashed to a line (a Neon close) still gets a sane outline.
    func testTinyRectangles() {
        let style = PanelStyle(radius: 22, filletRadius: 0, attachment: .none, cut: 26)
        let path = PanelGeometry.path(in: CGRect(x: 0, y: 0, width: 300, height: 2), style: style)
        XCTAssertFalse(path.boundingBox.isEmpty)
        XCTAssertLessThanOrEqual(path.boundingBox.height, 2)
    }

    /// Arcane's corners are scooped: the corner point itself is outside the
    /// panel, the edge beside it isn't, and the attached corners stay square.
    func testNotchedCorners() {
        let rect = CGRect(x: 0, y: 0, width: 400, height: 300)
        let style = PanelStyle(radius: 22, filletRadius: 0, attachment: .bottom, notch: 18)
        let path = PanelGeometry.path(in: rect, style: style)
        XCTAssertFalse(path.contains(CGPoint(x: 3, y: 297)), "inside the top-leading scoop")
        XCTAssertFalse(path.contains(CGPoint(x: 397, y: 297)), "inside the top-trailing scoop")
        XCTAssertTrue(path.contains(CGPoint(x: 15, y: 290)), "a scoop is a quarter circle, not a cut")
        XCTAssertTrue(path.contains(CGPoint(x: 200, y: 299)))
        XCTAssertTrue(path.contains(CGPoint(x: 1, y: 1)), "attached corners stay square")
    }
}
