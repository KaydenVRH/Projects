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

    // MARK: springs

    private func run(_ spring: Spring, from: Double, to: Double, seconds: Double = 2) -> [SpringValue] {
        var value = SpringValue(from)
        value.target = to
        var frames: [SpringValue] = []
        for _ in 0..<Int(seconds * 120) {
            value.advance(by: 1.0 / 120, spring: spring)
            frames.append(value)
        }
        return frames
    }

    func testOpeningBouncesPastAndSettles() {
        let frames = run(.panelOutward, from: 0, to: 1)
        let peak = frames.map(\.value).max() ?? 0
        // A visible bounce, but within what the window's slack covers.
        XCTAssertGreaterThan(peak, 1.02)
        XCTAssertLessThanOrEqual(peak - 1, Spring.panelOutward.overshoot * 1.4)
        XCTAssertTrue(frames.last?.isSettled ?? false)
    }

    func testClosingSpringDoesNotOvershoot() {
        let frames = run(.panelClose, from: 1, to: 0)
        XCTAssertGreaterThanOrEqual(frames.map(\.value).min() ?? -1, -0.0001)
        // Gone well inside half a second.
        XCTAssertLessThan(frames[Int(0.3 * 120)].value, 0.01)
    }

    func testSpringIsFrameRateIndependent() {
        var slow = SpringValue(0)
        var fast = SpringValue(0)
        slow.target = 1
        fast.target = 1
        for _ in 0..<30 { slow.advance(by: 1.0 / 60, spring: .panelOutward) }
        for _ in 0..<60 { fast.advance(by: 1.0 / 120, spring: .panelOutward) }
        XCTAssertEqual(slow.value, fast.value, accuracy: 0.001)
    }

    func testReversingKeepsMomentum() {
        // Half-way open, then told to close: it carries on outward for a moment
        // before turning round, instead of snapping back.
        var value = SpringValue(0)
        value.target = 1
        for _ in 0..<10 { value.advance(by: 1.0 / 120, spring: .panelOutward) }
        let before = value.value
        value.target = 0
        value.advance(by: 1.0 / 120, spring: .panelClose)
        XCTAssertGreaterThan(value.value, before)
    }
}
