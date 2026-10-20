import Foundation

/// Fire-and-forget command execution (used for click actions) and synchronous capture.
public enum Shell {
    /// Where a detached command's output goes.
    public static let logPath = "~/Library/Logs/kshell/scripts.log"

    /// Run a command detached from the shell.
    ///
    /// The command outlives the launcher that started it — and kshell itself —
    /// and its output goes to a log. Running it as a plain child with pipes that
    /// nobody read is what used to kill scripts the moment the overlay closed:
    /// the pipes went away with the `Process`, and the script's next write took
    /// it down.
    public static func runDetached(_ command: String, log: String = logPath) {
        let url = KShellPaths.resolve(log)
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        rotateLogIfNeeded(url)

        // A short-lived sh that nohups the real command and returns: `nohup`
        // redirects stdin from nowhere and ignores the hangup, so the command
        // survives the shell and never holds a pipe kshell owns.
        let shell = Process()
        shell.executableURL = URL(fileURLWithPath: "/bin/sh")
        shell.arguments = ["-c", "nohup \(command) >> \(quote(url.path)) 2>&1 &"]
        shell.standardOutput = FileHandle.nullDevice
        shell.standardError = FileHandle.nullDevice
        try? shell.run()
        shell.waitUntilExit()
    }

    /// Keep the log from growing for ever.
    private static func rotateLogIfNeeded(_ url: URL) {
        let attributes = try? FileManager.default.attributesOfItem(atPath: url.path)
        let size = (attributes?[.size] as? NSNumber)?.intValue ?? 0
        guard size > 2 * 1024 * 1024 else { return }
        let old = url.appendingPathExtension("1")
        try? FileManager.default.removeItem(at: old)
        try? FileManager.default.moveItem(at: url, to: old)
    }

    /// Fire-and-forget, detached (used for click actions).
    public static func run(_ command: String) {
        runDetached("/bin/zsh -lc \(quote(command))")
    }

    /// Single-quote a string for safe interpolation into a shell command.
    public static func quote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// Run a shell script, detached, avoiding any quoting of the path.
    public static func runScript(_ path: String) {
        runDetached("/bin/bash \(quote(path))")
    }

    /// Runs a command synchronously and returns trimmed stdout ("" on failure).
    /// Call off the main thread for anything slow.
    public static func capture(_ command: String) -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = Pipe()
        do {
            try process.run()
        } catch {
            return ""
        }
        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return String(data: data, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }
}
