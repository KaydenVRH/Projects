import XCTest
@testable import KShellCore

/// The shell's motion: the easing the stepped panels use, and the window slack
/// that lets the outline overshoot without being clipped.
final class MotionTests: XCTestCase {
    private func sample(_ opening: Bool, steps: Int = 400) -> [CGFloat] {
        (0...steps).map { SlideEasing.value(CGFloat($0) / CGFloat(steps), opening: opening) }
    }

    func testOpeningStopsAtItsRestingSize() {
        let values = sample(true)
        // No bounce past the resting size: the panel arrives and stays there.
        XCTAssertLessThanOrEqual(values.max() ?? 0, 1.0001)
        XCTAssertEqual(values.first ?? -1, 0, accuracy: 0.0001)
        XCTAssertEqual(values.last ?? -1, 1, accuracy: 0.0001)
        // ...and it is monotonic, so it never shrinks back on the way in.
        for index in 1..<values.count {
            XCTAssertGreaterThanOrEqual(values[index], values[index - 1] - 0.0001)
        }
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


}
