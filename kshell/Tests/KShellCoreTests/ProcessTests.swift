import XCTest
@testable import KShellCore

/// Regression tests for the child-process paths.
///
/// The shell's "it went deaf after a while" bug was a serial queue parked on a
/// child that never returned: `runBounded` waited on the pipe, then on the
/// process, with nothing bounding either. These assert the bound holds.
final class ProcessRunnerTests: XCTestCase {

    func testHungCommandIsKilledAndReturns() {
        let start = Date()
        let output = ProcessRunner.runBounded(command: "exec sleep 30", timeout: 0.4)
        let elapsed = Date().timeIntervalSince(start)

        XCTAssertEqual(output, "")
        XCTAssertLessThan(elapsed, 6, "a wedged command must not park the caller")
    }

    func testShellCaptureIsBoundedToo() {
        let start = Date()
        let output = Shell.capture("exec sleep 30", timeout: 0.4)
        let elapsed = Date().timeIntervalSince(start)

        XCTAssertEqual(output, "")
        XCTAssertLessThan(elapsed, 6, "Shell.capture must share the bound")
    }

    func testFastCommandStillReturnsItsOutput() {
        XCTAssertEqual(ProcessRunner.runBounded(command: "echo hello", timeout: 5), "hello\n")
        XCTAssertEqual(Shell.capture("echo  spaced  "), "spaced")
    }
}

final class FullscreenWatchTests: XCTestCase {

    func testParseKeepsOnlyFullscreenWindows() {
        let output = """
        188|false
        118|true
        garbage
        9|true
        |true
        """
        XCTAssertEqual(FullscreenWatch.parse(output), [118, 9])
    }

    func testParseOfFailedQueryIsEmpty() {
        // A timed-out query returns "" — that must read as "nothing full screen"
        // rather than leaving a stale set behind.
        XCTAssertTrue(FullscreenWatch.parse("").isEmpty)
    }
}
