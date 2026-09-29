import AppKit
import XCTest
@testable import KShellCore

final class LauncherTests: XCTestCase {
    private func app(_ name: String) -> AppItem {
        AppItem(
            id: "/Applications/\(name).app",
            name: name,
            url: URL(fileURLWithPath: "/Applications/\(name).app"),
            icon: NSImage()
        )
    }

    func testFilterEmptyReturnsAll() {
        let apps = [app("Safari"), app("Notes")]
        XCTAssertEqual(AppCatalog.filter(apps, query: "").count, 2)
    }

    func testFilterIsCaseInsensitive() {
        let apps = [app("Safari"), app("Notes")]
        XCTAssertEqual(AppCatalog.filter(apps, query: "saf").map(\.name), ["Safari"])
        XCTAssertEqual(AppCatalog.filter(apps, query: "NOTES").map(\.name), ["Notes"])
    }

    func testFilterNoMatch() {
        XCTAssertTrue(AppCatalog.filter([app("Safari")], query: "zzz").isEmpty)
    }
}
