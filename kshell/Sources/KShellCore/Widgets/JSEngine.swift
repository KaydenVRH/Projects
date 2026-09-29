import Foundation
import JavaScriptCore

/// Host callbacks the JS side can use. Implemented by the widget runtime; a
/// stub is used in tests.
public protocol JSRunner: AnyObject {
    func run(command: String, interval: TimeInterval, onOutput: @escaping (String) -> Void)
    func runOnce(command: String, onOutput: @escaping (String) -> Void)
    func fire(command: String)
    func postEvent(_ name: String, payload: String)
}

/// What a JS `render()` call can produce.
struct JSRender {
    var icon: String?
    var label: String?
    var iconColor: RGBA?
    var labelColor: RGBA?
    var action: String?
    var actionEvent: String?
    var padding: Double?
    var flexible: Bool?
}

/// Evaluates a user JS widget. The script may define:
///   render()             -> { icon, label, iconColor, labelColor, action, actionEvent, padding, flexible }
///   onEvent(name, payload)
/// and use the host functions `exec`, `execOnce`, `shell`, `post`, `battery`,
/// `ram`, `volume`, `date`, `log`.
final class JSEngine {
    private let context: JSContext
    private weak var runner: JSRunner?

    init?(source: String, runner: JSRunner) {
        guard let context = JSContext() else { return nil }
        self.context = context
        self.runner = runner
        context.exceptionHandler = { _, exception in
            JSEngine.logError(exception?.toString() ?? "unknown")
        }
        installHost()
        context.evaluateScript(source)
        if let exception = context.exception {
            JSEngine.logError(exception.toString() ?? "unknown")
        }
    }

    func render() -> JSRender? {
        guard let function = context.objectForKeyedSubscript("render"), !function.isUndefined else {
            return nil
        }
        guard let value = function.call(withArguments: []), !value.isUndefined, !value.isNull else {
            return nil
        }
        guard let dictionary = value.toObject() as? [String: Any] else { return nil }

        var result = JSRender()
        result.icon = dictionary["icon"] as? String
        result.label = dictionary["label"] as? String
        if let hex = dictionary["iconColor"] as? String { result.iconColor = RGBA(hex: hex) }
        if let hex = dictionary["labelColor"] as? String { result.labelColor = RGBA(hex: hex) }
        result.action = dictionary["action"] as? String
        result.actionEvent = dictionary["actionEvent"] as? String
        result.padding = (dictionary["padding"] as? NSNumber)?.doubleValue
        result.flexible = dictionary["flexible"] as? Bool
        return result
    }

    func callEvent(_ name: String, payload: String) {
        guard let function = context.objectForKeyedSubscript("onEvent"), !function.isUndefined else { return }
        function.call(withArguments: [name, payload])
    }

    private func installHost() {
        let exec: @convention(block) (String, Double, JSValue) -> Void = { [weak self] command, interval, callback in
            self?.runner?.run(command: command, interval: max(interval, 0.25)) { output in
                callback.call(withArguments: [output])
            }
        }
        let execOnce: @convention(block) (String, JSValue) -> Void = { [weak self] command, callback in
            self?.runner?.runOnce(command: command) { output in
                callback.call(withArguments: [output])
            }
        }
        let shell: @convention(block) (String) -> Void = { [weak self] command in
            self?.runner?.fire(command: command)
        }
        let post: @convention(block) (String, String) -> Void = { [weak self] name, payload in
            self?.runner?.postEvent(name, payload: payload)
        }
        let battery: @convention(block) () -> [String: Any] = {
            let b = SystemMetrics.battery()
            return ["percent": b.percent, "charging": b.charging, "present": b.present]
        }
        let ram: @convention(block) () -> Double = {
            (SystemMetrics.memoryUsage() * 100).rounded()
        }
        let volume: @convention(block) () -> [String: Any] = {
            let state = SystemAudio.defaultOutput()
            return ["percent": Int((state.volume * 100).rounded()), "muted": state.muted]
        }
        let date: @convention(block) (String) -> String = { pattern in
            let formatter = DateFormatter()
            formatter.dateFormat = pattern
            return formatter.string(from: Date())
        }
        let log: @convention(block) (String) -> Void = { message in
            FileHandle.standardError.write(Data("kshell: js \(message)\n".utf8))
        }

        context.setObject(exec, forKeyedSubscript: "exec" as NSString)
        context.setObject(execOnce, forKeyedSubscript: "execOnce" as NSString)
        context.setObject(shell, forKeyedSubscript: "shell" as NSString)
        context.setObject(post, forKeyedSubscript: "post" as NSString)
        context.setObject(battery, forKeyedSubscript: "battery" as NSString)
        context.setObject(ram, forKeyedSubscript: "ram" as NSString)
        context.setObject(volume, forKeyedSubscript: "volume" as NSString)
        context.setObject(date, forKeyedSubscript: "date" as NSString)
        context.setObject(log, forKeyedSubscript: "log" as NSString)
    }

    private static func logError(_ message: String) {
        FileHandle.standardError.write(Data("kshell: js error: \(message)\n".utf8))
    }
}
