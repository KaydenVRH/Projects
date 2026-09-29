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

    func testScriptFilter() {
        let scripts = [
            ScriptItem(id: "/s/a.sh", name: "a", path: "/s/a.sh"),
            ScriptItem(id: "/s/theme-switch.sh", name: "theme-switch", path: "/s/theme-switch.sh"),
        ]
        XCTAssertEqual(ScriptCatalog.filter(scripts, query: "theme").map(\.name), ["theme-switch"])
        XCTAssertEqual(ScriptCatalog.filter(scripts, query: "").count, 2)
    }

    func testWallpaperCatalogFiltersExtensions() throws {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("kshell-wp-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        FileManager.default.createFile(atPath: directory.appendingPathComponent("a.png").path, contents: Data())
        FileManager.default.createFile(atPath: directory.appendingPathComponent("b.mp4").path, contents: Data())
        FileManager.default.createFile(atPath: directory.appendingPathComponent("c.txt").path, contents: Data())

        let items = WallpaperCatalog.load(directories: [directory])
        XCTAssertEqual(items.map(\.name).sorted(), ["a", "b"])
        XCTAssertEqual(items.first { $0.name == "b" }?.isVideo, true)
        XCTAssertEqual(items.first { $0.name == "a" }?.isVideo, false)
    }
}
