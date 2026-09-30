import XCTest
@testable import KShellCore

final class WorkspacesTests: XCTestCase {
    func testParseSplitsFocusedAndOccupied() {
        let snapshot = WorkspacesMonitor.parse("""
        F:3
        1
        3
        3
        7
        """)
        XCTAssertEqual(snapshot.focused, "3")
        XCTAssertEqual(snapshot.occupied, ["1", "3", "7"])
    }

    func testUnknownOutputShowsEverything() {
        let snapshot = WorkspacesMonitor.parse("")
        XCTAssertTrue(snapshot.isUnknown)
        // aerospace unavailable → don't blank the bar.
        XCTAssertTrue(snapshot.shows("5"))
    }

    func testShowsFocusedWorkspaceEvenWhenEmpty() {
        let snapshot = WorkspacesMonitor.parse("F:2\n1")
        XCTAssertTrue(snapshot.shows("1"))
        XCTAssertTrue(snapshot.shows("2"))
        XCTAssertFalse(snapshot.shows("3"))
    }

    func testSnapshotIgnoresWorkspaceOrder() {
        XCTAssertEqual(WorkspacesMonitor.parse("F:1\n1\n2"), WorkspacesMonitor.parse("F:1\n2\n1"))
    }
}
