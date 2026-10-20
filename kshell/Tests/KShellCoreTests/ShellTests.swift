import XCTest
@testable import KShellCore

final class ShellTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("kshell-shell-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func script(_ body: String, name: String = "job.sh") throws -> URL {
        let url = directory.appendingPathComponent(name)
        try "#!/bin/bash\n\(body)\n".write(to: url, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
        return url
    }

    /// The bug this guards: a script run as a plain child of the shell died the
    /// moment the launcher closed, because the `Process` — and the pipes nobody
    /// read — went away with it.
    func testDetachedScriptReturnsImmediatelyAndKeepsRunning() throws {
        let marker = directory.appendingPathComponent("done")
        let job = try script("""
        sleep 1
        touch \(Shell.quote(marker.path))
        """)

        let start = Date()
        Shell.runScript(job.path)
        let elapsed = Date().timeIntervalSince(start)

        // Detached: the call does not wait for the second of sleeping.
        XCTAssertLessThan(elapsed, 0.8, "runScript should not wait for the job")
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path),
                       "the job should still be running")

        // …but it does run to completion on its own.
        let deadline = Date().addingTimeInterval(6)
        while Date() < deadline, !FileManager.default.fileExists(atPath: marker.path) {
            usleep(100_000)
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: marker.path),
                      "the detached job should have finished on its own")
    }

    func testDetachedOutputGoesToTheLog() throws {
        let log = directory.appendingPathComponent("run.log").path
        let token = "kshell-test-\(UUID().uuidString)"
        let job = try script("echo \(Shell.quote(token))")

        Shell.runDetached("/bin/bash \(Shell.quote(job.path))", log: log)

        // Output is written by the job after the call returns; poll briefly.
        var text = ""
        let deadline = Date().addingTimeInterval(6)
        while Date() < deadline {
            text = (try? String(contentsOfFile: log, encoding: .utf8)) ?? ""
            if text.contains(token) { break }
            usleep(100_000)
        }
        XCTAssertTrue(text.contains(token), "the job's output should land in the log, got: \(text)")
    }

    /// A detached job must not hold the shell open or inherit its pipes — it gets
    /// nowhere to read from, so it cannot sit waiting on stdin either.
    func testDetachedJobDoesNotBlockOnStdin() throws {
        let log = directory.appendingPathComponent("stdin.log").path
        // `cat` with no input would block for ever if it inherited a terminal.
        let job = try script("""
        cat >/dev/null
        echo "cat returned"
        """)

        Shell.runDetached("/bin/bash \(Shell.quote(job.path))", log: log)

        var text = ""
        let deadline = Date().addingTimeInterval(6)
        while Date() < deadline {
            text = (try? String(contentsOfFile: log, encoding: .utf8)) ?? ""
            if text.contains("cat returned") { break }
            usleep(100_000)
        }
        XCTAssertTrue(text.contains("cat returned"), "detached job should not wait on stdin")
    }
}
