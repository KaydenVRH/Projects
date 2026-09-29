import AppKit

/// Owns the lifetime of the shell. One instance per running `kshell` process.
public final class AppController {
    private var shell: ShellController?

    public init() {}

    public func start() {
        let shell = ShellController()
        shell.start()
        self.shell = shell
    }

    public func stop() {
        shell?.stop()
        shell = nil
    }
}
