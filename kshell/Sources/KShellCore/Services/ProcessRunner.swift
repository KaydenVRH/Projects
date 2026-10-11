import Foundation

/// Runs a shell command immediately and then on an interval, delivering the
/// trimmed stdout on the main queue.
public final class ProcessRunner {
    private var timer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "kshell.process", qos: .utility)

    public init() {}

    public func run(
        command: String,
        interval: TimeInterval,
        onOutput: @escaping (String) -> Void
    ) {
        stop()
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: max(interval, 0.2))
        timer.setEventHandler { [weak self] in
            self?.execute(command: command, onOutput: onOutput)
        }
        timer.resume()
        self.timer = timer
    }

    /// Run a command and return its stdout. Blocking — call it off the main
    /// queue.
    /// Run a command and return its stdout.
    ///
    /// Bounded: a probe that hangs — a malformed file, a codec that never
    /// finishes — is killed rather than wedging its caller for ever. The pipe is
    /// drained off this thread so a child that fills it cannot deadlock before
    /// the timeout can fire.
    public static func capture(_ command: String, timeout: TimeInterval = 20) -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = FileHandle.nullDevice

        guard (try? process.run()) != nil else { return "" }

        let lock = NSLock()
        var output = Data()
        let drained = DispatchSemaphore(value: 0)
        DispatchQueue.global(qos: .utility).async {
            let data = stdout.fileHandleForReading.readDataToEndOfFile()
            lock.lock()
            output = data
            lock.unlock()
            drained.signal()
        }

        if drained.wait(timeout: .now() + timeout) == .timedOut {
            process.terminate()
            _ = drained.wait(timeout: .now() + 2)
        }
        process.waitUntilExit()

        lock.lock()
        defer { lock.unlock() }
        return String(data: output, encoding: .utf8) ?? ""
    }

    public func runOnce(command: String, onOutput: @escaping (String) -> Void) {
        queue.async { [weak self] in
            self?.execute(command: command, onOutput: onOutput)
        }
    }

    public func stop() {
        timer?.cancel()
        timer = nil
    }

    deinit { stop() }

    private func execute(command: String, onOutput: @escaping (String) -> Void) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]

        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = Pipe()

        do {
            try process.run()
        } catch {
            DispatchQueue.main.async { onOutput("") }
            return
        }

        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        let output = String(data: data, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        DispatchQueue.main.async { onOutput(output) }
    }
}
