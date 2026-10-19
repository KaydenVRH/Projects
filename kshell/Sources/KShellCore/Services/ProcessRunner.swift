import Foundation

/// Runs a shell command immediately and then on an interval, delivering the
/// trimmed stdout on the main queue.
public final class ProcessRunner {
    private var timer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "kshell.process", qos: .utility)
    private var inFlight = false
    private let lock = NSLock()

    public init() {}

    /// How long a command may run before it is killed. A polled command that
    /// never returns would otherwise wedge this runner for the life of the
    /// shell — and the fullscreen watch, which polls every half second, wedging
    /// leaves the shell hidden and deaf to input for ever.
    static let timeout: TimeInterval = 10

    public func run(
        command: String,
        interval: TimeInterval,
        onOutput: @escaping (String) -> Void
    ) {
        stop()
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: max(interval, 0.2))
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            // One at a time: a command slower than its own interval should skip
            // ticks rather than queue up behind itself.
            self.lock.lock()
            if self.inFlight {
                self.lock.unlock()
                return
            }
            self.inFlight = true
            self.lock.unlock()

            let output = ProcessRunner.runBounded(command: command, timeout: ProcessRunner.timeout)

            self.lock.lock()
            self.inFlight = false
            self.lock.unlock()
            DispatchQueue.main.async { onOutput(output) }
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
        runBounded(command: command, timeout: timeout)
    }

    /// The one place a child process is run.
    ///
    /// Bounded — a command that hangs is killed rather than wedging its caller
    /// for ever — and the pipe is drained off the calling thread, so a child that
    /// fills it cannot deadlock the caller before the timeout can fire.
    static func runBounded(command: String, timeout: TimeInterval) -> String {
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
        queue.async {
            let output = ProcessRunner.runBounded(command: command, timeout: ProcessRunner.timeout)
            DispatchQueue.main.async { onOutput(output) }
        }
    }

    public func stop() {
        timer?.cancel()
        timer = nil
    }

    deinit { stop() }
}
