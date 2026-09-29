import XCTest
@testable import KShellCore

final class EventTests: XCTestCase {
    func testFocusedWorkspaceFromKeyValue() {
        XCTAssertEqual(WorkspacesMonitor.focused(from: "FOCUSED_WORKSPACE=3"), "3")
    }

    func testFocusedWorkspaceFromBareId() {
        XCTAssertEqual(WorkspacesMonitor.focused(from: "7"), "7")
    }

    func testFocusedWorkspaceIgnoresJunk() {
        XCTAssertNil(WorkspacesMonitor.focused(from: "hello world"))
        XCTAssertNil(WorkspacesMonitor.focused(from: ""))
    }

    func testEventBusRoundTrip() {
        let expectation = expectation(description: "event delivered")
        let token = EventBus.observe("unit_test") { payload in
            XCTAssertEqual(payload, "hello")
            expectation.fulfill()
        }
        EventBus.post("unit_test", payload: "hello")
        wait(for: [expectation], timeout: 1)
        EventBus.unobserve(token)
    }
}
