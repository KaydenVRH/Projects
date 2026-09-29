import Foundation

/// Runs a shell command on an interval and shows its output.
///
/// Config:
///   command   = "~/path/to/script.sh"   (required)
///   interval  = 5                        (seconds, default 5)
///   icon      = ""                    (optional static icon)
///   separator = "|"                      (split stdout into icon|label)
///   action    = "..."                    (run on click)
final class ScriptRuntime: WidgetRuntime {
    let model: WidgetModel
    private let runner = ProcessRunner()

    init(spec: WidgetSpec, theme: Theme) {
        model = WidgetFactory.baseModel(spec, theme: theme, icon: spec.string("icon"))
    }

    func start() {
        guard let command = model.spec.string("command"), !command.isEmpty else {
            FileHandle.standardError.write(Data("kshell: script widget missing 'command'\n".utf8))
            return
        }
        let interval = model.spec.number("interval") ?? 5
        let separator = model.spec.string("separator")

        if let action = model.spec.string("action"), !action.isEmpty {
            model.action = { Shell.run(action) }
        }

        runner.run(command: command, interval: interval) { [weak self] output in
            guard let self else { return }
            if let separator, output.contains(separator) {
                let parts = output.components(separatedBy: separator)
                let icon = parts[0].trimmingCharacters(in: .whitespaces)
                let label = parts.dropFirst().joined(separator: separator)
                    .trimmingCharacters(in: .whitespaces)
                if self.model.spec.string("icon") == nil { self.model.icon = icon }
                self.model.label = label
            } else {
                self.model.label = output
            }
        }
    }

    func stop() {
        runner.stop()
    }
}
