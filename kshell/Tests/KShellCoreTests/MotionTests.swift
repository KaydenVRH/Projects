import XCTest
@testable import KShellCore

/// The shell's motion: the easing the stepped panels use, and the window slack
/// that lets the outline overshoot without being clipped.
final class MotionTests: XCTestCase {
    private func sample(_ opening: Bool, steps: Int = 400) -> [CGFloat] {
        (0...steps).map { SlideEasing.value(CGFloat($0) / CGFloat(steps), opening: opening) }
    }

    func testOpeningOvershootsThenSettles() {
        let values = sample(true)
        let peak = values.max() ?? 0
        XCTAssertGreaterThan(peak, 1.005, "opening should travel past its resting size")
        XCTAssertLessThan(peak, 1.06, "the overshoot should be a settle, not a bounce off-screen")
        XCTAssertEqual(values.first ?? -1, 0, accuracy: 0.0001)
        XCTAssertEqual(values.last ?? -1, 1, accuracy: 0.0001)
        // ...and it must end up back at rest after the peak, not stay out.
        XCTAssertLessThan(values[values.count * 9 / 10], 1.01)
    }

    func testClosingNeverOvershoots() {
        for value in sample(false) {
            XCTAssertGreaterThanOrEqual(value, -0.0001)
            XCTAssertLessThanOrEqual(value, 1.0001)
        }
    }

    func testOpeningIsFasterThanTheOldCurve() {
        // Half-way through the time, most of the distance is already covered.
        XCTAssertGreaterThan(SlideEasing.value(0.5, opening: true), 0.95)
    }

    /// The window is grown by `overshootAllowance` on the side a panel grows
    /// toward; it has to cover the easing's peak for every panel shape we build.
    func testWindowSlackCoversThePeak() {
        let shapes: [(OverlayEdge, NSSize)] = [
            (.trailing, NSSize(width: 420, height: 700)),   // theme / wallpaper launchers
            (.trailing, NSSize(width: 440, height: 760)),   // notification centre
            (.trailing, NSSize(width: 384, height: 92)),    // a notification card
            (.bottom, NSSize(width: 900, height: 620)),     // app launcher
            (.bottom, NSSize(width: 560, height: 520)),     // script launcher
            (.top, NSSize(width: 460, height: 300)),        // media island
        ]
        let peak = 0.043
        for (edge, size) in shapes {
            let travel = (edge == .top || edge == .bottom) ? size.height : size.width
            let allowance = OverlayPanel.overshootAllowance(for: edge, size: size)
            XCTAssertGreaterThan(
                allowance,
                travel * peak,
                "\(edge) panel of \(Int(size.width))×\(Int(size.height)) would clip its settle"
            )
        }
    }
}
