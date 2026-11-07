import XCTest
@testable import KShellCore

final class NotificationTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("kshell-notifications-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private var file: URL { directory.appendingPathComponent("notifications.json") }

    private func note(
        _ title: String,
        date: Date = Date(),
        source: KShellNotification.Source = .kshell,
        id: String? = nil
    ) -> KShellNotification {
        KShellNotification(
            id: id ?? "kshell:" + title,
            source: source,
            app: "kshell",
            title: title,
            date: date
        )
    }

    // MARK: - the notify payload

    func testParseJSONPayload() throws {
        let json = #"{"title":"Build finished","body":"48 tests pass","app":"Terminal","tone":"success","action":"echo done"}"#
        let item = try XCTUnwrap(KShellNotification.parse(payload: json))
        XCTAssertEqual(item.title, "Build finished")
        XCTAssertEqual(item.body, "48 tests pass")
        XCTAssertEqual(item.app, "Terminal")
        XCTAssertEqual(item.tone, .success)
        XCTAssertEqual(item.action, "echo done")
        XCTAssertEqual(item.source, .kshell)
        XCTAssertTrue(item.hasBody)
    }

    /// The body may contain the separator the one-line form uses.
    func testParseJSONKeepsSeparatorsInTheBody() throws {
        let json = #"{"title":"a","body":"left|right"}"#
        let item = try XCTUnwrap(KShellNotification.parse(payload: json))
        XCTAssertEqual(item.body, "left|right")
    }

    func testParsePipePayload() throws {
        let item = try XCTUnwrap(KShellNotification.parse(payload: "Downloaded|Song.m4a|Music"))
        XCTAssertEqual(item.title, "Downloaded")
        XCTAssertEqual(item.body, "Song.m4a")
        XCTAssertEqual(item.app, "Music")
        XCTAssertEqual(item.source, .kshell)
    }

    func testParseRejectsEmptyPayloads() {
        XCTAssertNil(KShellNotification.parse(payload: ""))
        XCTAssertNil(KShellNotification.parse(payload: "   "))
        XCTAssertNil(KShellNotification.parse(payload: #"{"title":"   "}"#))
        XCTAssertNil(KShellNotification.parse(payload: "|"))
        // A body without a title is legitimate — the card shows the body.
        XCTAssertEqual(KShellNotification.parse(payload: #"{"body":"no title"}"#)?.headline, "no title")
    }

    func testHeadlineFallsBackToTheBody() {
        let item = KShellNotification(source: .macos, app: "Mail", title: "", body: "A message\nsecond line")
        XCTAssertEqual(item.headline, "A message")
        XCTAssertTrue(item.hasBody)
    }

    // MARK: - the store

    func testIngestDedupesSortsAndIgnoresRepeats() {
        let store = NotificationStore(file: file)
        let old = Date(timeIntervalSinceReferenceDate: 800_000_000)
        let new = old.addingTimeInterval(60)

        store.ingest([note("older", date: old), note("newer", date: new)], announce: false)
        XCTAssertEqual(store.items.map(\.title), ["newer", "older"])

        // The same id again is not a new notification.
        store.ingest([note("older", date: old)], announce: false)
        XCTAssertEqual(store.items.count, 2)
    }

    func testUnreadCountsOnlyAnnouncedArrivals() {
        let store = NotificationStore(file: file)
        store.ingest([note("history", date: Date(timeIntervalSinceReferenceDate: 800_000_000))], announce: false)
        XCTAssertEqual(store.unread, 0)

        store.ingest([note("news")], announce: true)
        XCTAssertEqual(store.unread, 1)

        store.markAllRead()
        XCTAssertEqual(store.unread, 0)
    }

    func testArrivalCallbackFiresPerItem() {
        let store = NotificationStore(file: file)
        var arrived: [String] = []
        store.onArrive = { arrived.append($0.title) }

        store.ingest([note("one", date: Date())])
        store.ingest([note("two", date: Date().addingTimeInterval(1))])
        // oldest first, so the popups queue in the order they happened
        XCTAssertEqual(arrived, ["one", "two"])
    }

    func testHistoryIsCappedNewestFirst() {
        let store = NotificationStore(file: file)
        store.limit = 3
        let base = Date(timeIntervalSinceReferenceDate: 800_000_000)
        for index in 0..<6 {
            store.ingest([note("n\(index)", date: base.addingTimeInterval(Double(index)))], announce: false)
        }
        XCTAssertEqual(store.items.count, 3)
        XCTAssertEqual(store.items.map(\.title), ["n5", "n4", "n3"])
    }

    func testDismissAndClear() {
        let store = NotificationStore(file: file)
        store.ingest([note("a", date: Date()), note("b", date: Date().addingTimeInterval(1))], announce: false)
        store.dismiss("kshell:a")
        XCTAssertEqual(store.items.map(\.title), ["b"])
        store.clear()
        XCTAssertTrue(store.items.isEmpty)
    }

    func testHistorySurvivesARestart() {
        let first = NotificationStore(file: file)
        first.ingest([note("kept", date: Date())], announce: false)

        let second = NotificationStore(file: file)
        XCTAssertEqual(second.items.map(\.title), ["kept"])
        // Read back, not new: nothing to announce.
        XCTAssertEqual(second.unread, 0)
    }

    func testDismissedCardsDoNotComeBackAfterARestart() {
        let first = NotificationStore(file: file)
        first.ingest([
            note("keep", date: Date()),
            note("gone", date: Date().addingTimeInterval(1)),
        ], announce: false)
        first.dismiss("kshell:gone")

        let second = NotificationStore(file: file)
        XCTAssertEqual(second.items.map(\.title), ["keep"])
        // The watcher re-reads the system database on every launch; a dismissed
        // id must not be ingested again.
        second.ingest([note("gone", date: Date().addingTimeInterval(1))], announce: false)
        XCTAssertEqual(second.items.map(\.title), ["keep"])
    }

    func testClearKeepsClearedCardsCleared() {
        let first = NotificationStore(file: file)
        first.ingest([note("a", date: Date()), note("b", date: Date().addingTimeInterval(1))], announce: false)
        first.clear()
        XCTAssertTrue(first.items.isEmpty)

        let second = NotificationStore(file: file)
        second.ingest([note("a", date: Date()), note("b", date: Date().addingTimeInterval(1))], announce: false)
        XCTAssertTrue(second.items.isEmpty)
    }

    func testLegacyHistoryFileStillLoads() throws {
        let legacy = [note("old", date: Date())]
        let data = try JSONEncoder().encode(legacy)
        try data.write(to: file)
        let store = NotificationStore(file: file)
        XCTAssertEqual(store.items.map(\.title), ["old"])
    }

    // MARK: - the system database

    /// Reads the real database, so it is only meaningful on a Mac with
    /// notifications in it — skipped anywhere else rather than made flaky.
    func testSystemDatabaseIsReadableAndParses() throws {
        guard MacNotificationDatabase.isAvailable else {
            throw XCTSkip("no Notification Center database on this machine")
        }
        let rows = MacNotificationDatabase.rows(limit: 20)
        XCTAssertLessThanOrEqual(rows.count, 20)
        for row in rows {
            XCTAssertFalse(row.uuid.isEmpty)
        }
        guard !rows.isEmpty else { return }

        // At least one recent row should yield a usable title or body; macOS
        // records a few internal notifications that carry neither.
        let parsed = rows.compactMap { MacNotificationReader.notification(for: $0) }
        if let first = parsed.first {
            XCTAssertEqual(first.source, .macos)
            XCTAssertTrue(first.id.hasPrefix("mac:"))
            XCTAssertFalse(first.headline.isEmpty)
        }
    }
}
