import XCTest
@testable import KShellCore

final class PlaylistTests: XCTestCase {
    private func track(_ name: String) -> MusicTrack {
        MusicTrack(path: "/tmp/\(name).mp3", title: name, artist: "Test")
    }

    private func tempFile() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("kshell-playlists-\(UUID().uuidString).json")
    }

    func testSaveAndReload() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }

        let store = MusicPlaylists(file: file)
        XCTAssertTrue(store.isEmpty)
        store.save(name: "Late night", tracks: [track("a"), track("b")])
        XCTAssertEqual(store.count, 1)

        let reloaded = MusicPlaylists(file: file)
        XCTAssertEqual(reloaded.count, 1)
        XCTAssertEqual(reloaded.playlists.first?.name, "Late night")
        XCTAssertEqual(reloaded.playlists.first?.tracks.count, 2)
    }

    func testSavingTheSameNameReplaces() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }

        let store = MusicPlaylists(file: file)
        store.save(name: "Focus", tracks: [track("a")])
        store.save(name: "Focus", tracks: [track("b"), track("c")])
        XCTAssertEqual(store.count, 1, "saving under an existing name updates it")
        XCTAssertEqual(store.playlist(named: "Focus")?.tracks.count, 2)
    }

    func testBlankNameIsRefused() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }

        let store = MusicPlaylists(file: file)
        XCTAssertNil(store.save(name: "   ", tracks: [track("a")]))
        XCTAssertTrue(store.isEmpty)
    }

    func testUniqueNameCountsUp() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }

        let store = MusicPlaylists(file: file)
        XCTAssertEqual(store.uniqueName(), "Playlist")
        store.save(name: "Playlist", tracks: [])
        XCTAssertEqual(store.uniqueName(), "Playlist 2")
        store.save(name: "Playlist 2", tracks: [])
        XCTAssertEqual(store.uniqueName(), "Playlist 3")
    }

    func testAddTrackIsIdempotentAndRemovable() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }

        let store = MusicPlaylists(file: file)
        store.save(name: "Gym", tracks: [track("a")])
        let added = store.add(track("b"), to: "Gym")
        XCTAssertTrue(added)
        XCTAssertFalse(store.add(track("b"), to: "Gym"), "same path is not added twice")
        XCTAssertEqual(store.playlist(named: "Gym")?.tracks.count, 2)

        XCTAssertTrue(store.remove(track("a"), from: "Gym"))
        XCTAssertEqual(store.playlist(named: "Gym")?.tracks.count, 1)
        XCTAssertFalse(store.remove(track("a"), from: "Gym"), "already gone")
    }

    func testAddToMissingPlaylistDoesNothing() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }

        let store = MusicPlaylists(file: file)
        XCTAssertFalse(store.add(track("a"), to: "nope"))
        XCTAssertTrue(store.isEmpty)
    }

    func testRemoveDeletesThePlaylist() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }

        let store = MusicPlaylists(file: file)
        store.save(name: "One", tracks: [track("a")])
        store.save(name: "Two", tracks: [track("b")])
        store.remove(named: "One")
        XCTAssertEqual(store.playlists.map(\.name), ["Two"])
    }

    func testPlaylistsSurviveACorruptFile() throws {
        let file = tempFile()
        defer { try? FileManager.default.removeItem(at: file) }
        try Data("not json".utf8).write(to: file)

        let store = MusicPlaylists(file: file)
        XCTAssertTrue(store.isEmpty, "a bad file is ignored, not fatal")
    }

    // MARK: - repeat mode

    func testRepeatModeCyclesOffAllOne() {
        let player = MusicPlayer.shared
        let before = player.repeatMode
        defer { player.setRepeat(before) }

        player.setRepeat(.off)
        player.cycleRepeat()
        XCTAssertEqual(player.repeatMode, .all)
        player.cycleRepeat()
        XCTAssertEqual(player.repeatMode, .one, "one song loops")
        player.cycleRepeat()
        XCTAssertEqual(player.repeatMode, .off)
    }

    func testRepeatModeGlyphs() {
        XCTAssertEqual(MusicPlayer.RepeatMode.off.symbol, "repeat")
        XCTAssertEqual(MusicPlayer.RepeatMode.all.symbol, "repeat")
        XCTAssertEqual(MusicPlayer.RepeatMode.one.symbol, "repeat.1")
        XCTAssertEqual(MusicPlayer.RepeatMode.one.label, "repeat one")
    }
}
