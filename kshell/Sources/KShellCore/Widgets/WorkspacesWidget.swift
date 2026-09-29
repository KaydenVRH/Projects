import AppKit
import Foundation

/// Tracks the focused aerospace workspace. Prefers pushed events
/// (`kshell trigger aerospace_workspace_change FOCUSED_WORKSPACE=2`) and falls
/// back to a light poll so it works before aerospace is wired up.
final class WorkspacesMonitor {
    static let shared = WorkspacesMonitor()

    private var observers: [ObjectIdentifier: (String) -> Void] = [:]
    private var timer: DispatchSourceTimer?
    private var eventToken: NSObjectProtocol?
    private let queue = DispatchQueue(label: "kshell.aerospace", qos: .utility)

    func subscribe(_ token: ObjectIdentifier, _ callback: @escaping (String) -> Void) {
        observers[token] = callback

        if eventToken == nil {
            eventToken = EventBus.observe("aerospace_workspace_change") { [weak self] payload in
                self?.handle(event: payload)
            }
        }
        startPollingIfNeeded()
        poll()
    }

    func unsubscribe(_ token: ObjectIdentifier) {
        observers[token] = nil
        guard observers.isEmpty else { return }
        timer?.cancel()
        timer = nil
        if let eventToken { EventBus.unobserve(eventToken) }
        eventToken = nil
    }

    private func startPollingIfNeeded() {
        guard timer == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 1, repeating: 1)
        timer.setEventHandler { [weak self] in self?.poll() }
        timer.resume()
        self.timer = timer
    }

    private func handle(event payload: String) {
        if let focused = WorkspacesMonitor.focused(from: payload) {
            notify(focused)
        } else {
            poll()
        }
    }

    private func poll() {
        let focused = Shell.capture("aerospace list-workspaces --focused --format '%{workspace}'")
            .components(separatedBy: .newlines)
            .first ?? ""
        notify(focused)
    }

    private func notify(_ focused: String) {
        DispatchQueue.main.async {
            for callback in self.observers.values { callback(focused) }
        }
    }

    /// Parse a bare workspace id or a `FOCUSED_WORKSPACE=<id>` token.
    static func focused(from payload: String) -> String? {
        for token in payload.split(separator: " ") where token.hasPrefix("FOCUSED_WORKSPACE=") {
            return String(token.dropFirst("FOCUSED_WORKSPACE=".count))
        }
        let trimmed = payload.trimmingCharacters(in: .whitespaces)
        if !trimmed.isEmpty, trimmed.allSatisfy({ $0.isNumber }) { return trimmed }
        return nil
    }
}

/// One workspace item (e.g. `1`..`9`). Highlights when it is the focused workspace.
final class WorkspaceItemRuntime: WidgetRuntime {
    let model: WidgetModel
    private let id: String
    private let focusedColor: RGBA?
    private let unfocusedColor: RGBA?
    private var token: ObjectIdentifier { ObjectIdentifier(self) }

    init(id: String, spec: WidgetSpec, theme: Theme) {
        self.id = id
        self.focusedColor = spec.color("focused_color") ?? theme.highlight
        self.unfocusedColor = spec.color("unfocused_color") ?? theme.accent
        model = WidgetFactory.baseModel(spec, theme: theme, label: id, paddingX: spec.number("padding") ?? 3)
        model.labelColor = unfocusedColor
        model.action = { Shell.run("aerospace workspace \(id)") }
    }

    func start() {
        WorkspacesMonitor.shared.subscribe(token) { [weak self] focused in
            guard let self else { return }
            self.model.labelColor = (focused == self.id) ? self.focusedColor : self.unfocusedColor
        }
    }

    func stop() {
        WorkspacesMonitor.shared.unsubscribe(token)
    }
}

/// Convenience factory: expands a `workspaces` spec into one runtime per id.
enum WorkspacesWidget {
    static func make(spec: WidgetSpec, theme: Theme) -> [WidgetRuntime] {
        let ids: [String]
        if let listed = spec.string("workspaces") {
            ids = listed.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
        } else if let count = spec.int("count") {
            ids = (1...max(count, 1)).map(String.init)
        } else if let live = aerospaceWorkspaceIds(), !live.isEmpty {
            ids = live
        } else {
            ids = (1...9).map(String.init)
        }
        return ids.map { WorkspaceItemRuntime(id: $0, spec: spec, theme: theme) }
    }

    /// Ask aerospace for the full set of workspaces so the bar tracks whatever
    /// aerospace is actually configured with.
    static func aerospaceWorkspaceIds() -> [String]? {
        let output = Shell.capture("aerospace list-workspaces --all --format '%{workspace}'")
        let ids = output
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
        return ids.isEmpty ? nil : ids
    }
}
