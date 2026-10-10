import XCTest
@testable import KShellCore

final class QueueTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("kshell-queue-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    /// A track whose file really exists, so loading keeps it.
    private func track(_ name: String) -> MusicTrack {
        let url = directory.appendingPathComponent("\(name).m4a")
        FileManager.default.createFile(atPath: url.path, contents: Data())
        return MusicTrack(path: url.path, title: name)
    }

    private var queueFile: URL { directory.appendingPathComponent("queue.json") }

    func testAddKeepsOrderAndDedupes() {
        let queue = MusicQueue(file: queueFile)
        XCTAssertTrue(queue.add(track("c")))
        XCTAssertTrue(queue.add(track("a")))
        XCTAssertTrue(queue.add(track("b")))
        // a second press of the same song is a no-op
        XCTAssertFalse(queue.add(track("a")))
        XCTAssertEqual(queue.tracks.map(\.title), ["c", "a", "b"])
        XCTAssertEqual(queue.count, 3)
    }

    func testMoveSlidesAndClamps() {
        let queue = MusicQueue(file: queueFile)
        for name in ["a", "b", "c"] { queue.add(track(name)) }

        queue.move(0, by: 1)
        XCTAssertEqual(queue.tracks.map(\.title), ["b", "a", "c"])

        // at the ends it stays put rather than wrapping
        queue.move(0, by: -1)
        XCTAssertEqual(queue.tracks.map(\.title), ["b", "a", "c"])
        queue.move(2, by: 5)
        XCTAssertEqual(queue.tracks.map(\.title), ["b", "a", "c"])
    }

    func testRemove() {
        let queue = MusicQueue(file: queueFile)
        let a = track("a"), b = track("b")
        queue.add(a); queue.add(b)

        queue.remove(a)
        XCTAssertEqual(queue.tracks.map(\.title), ["b"])
        queue.remove(at: 0)
        XCTAssertTrue(queue.isEmpty)
    }

    func testKeptBetweenRuns() {
        let a = track("a"), b = track("b")
        let queue = MusicQueue(file: queueFile)
        queue.add(a); queue.add(b)

        let reread = MusicQueue(file: queueFile)
        XCTAssertEqual(reread.tracks.map(\.title), ["a", "b"])
    }

    func testDeletedFilesAreDropped() throws {
        let a = track("a"), b = track("b")
        let queue = MusicQueue(file: queueFile)
        queue.add(a); queue.add(b)

        try FileManager.default.removeItem(at: a.url)
        let reread = MusicQueue(file: queueFile)
        XCTAssertEqual(reread.tracks.map(\.title), ["b"])
    }

    func testClear() {
        let queue = MusicQueue(file: queueFile)
        queue.add(track("a"))
        queue.clear()
        XCTAssertTrue(queue.isEmpty)
        XCTAssertTrue(MusicQueue(file: queueFile).isEmpty)
    }
}
