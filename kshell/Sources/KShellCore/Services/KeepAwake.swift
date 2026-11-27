import Foundation

/// Keeps the Mac and its display awake on request, through `caffeinate`.
///
/// `caffeinate -w` watches the shell's own pid, so if the shell goes away the
/// assertion goes with it — nothing is left keeping the Mac up behind it.
public final class KeepAwake: ObservableObject {
    public static let shared = KeepAwake()

    @Published public private(set) var isOn = false
    private var process: Process?

    public func toggle() { isOn ? stop() : start() }

    public func start() {
        guard process == nil else { return }
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/usr/bin/caffeinate")
        task.arguments = ["-di", "-w", String(ProcessInfo.processInfo.processIdentifier)]
        task.terminationHandler = { [weak self] _ in
            DispatchQueue.main.async {
                self?.process = nil
                self?.isOn = false
            }
        }
        do {
            try task.run()
            process = task
            isOn = true
        } catch {
            isOn = false
        }
    }

    public func stop() {
        process?.terminate()
        process = nil
        isOn = false
    }
}
