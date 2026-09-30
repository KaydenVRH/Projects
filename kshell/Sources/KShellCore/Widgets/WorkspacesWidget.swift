import AppKit
import Foundation

/// Which workspaces the bar should currently show: the focused one, plus every
/// workspace holding at least one window — the same rule as the sketchybar
/// `aerospace.sh` plugin.
struct WorkspaceSnapshot: Equatable {
    var focused: String = ""
    var occupied: Set<String> = []

    /// True when aerospace could not be queried at all. Show everything rather
    /// than blanking the bar.
    var isUnknown: Bool { focused.isEmpty && occupied.isEmpty }

    func shows(_ id: String) -> Bool {
        isUnknown || focused == id || occupied.contains(id)
    }
}

/// Tracks the focused aerospace workspace and which workspaces have windows.
/// Prefers pushed events (`kshell trigger aerospace_workspace_change
/// FOCUSED_WORKSPACE=2`) and falls back to a light poll so it works before
/// aerospace is wired up.
final class WorkspacesMonitor {
    static let shared = WorkspacesMonitor()

    private var observers: [ObjectIdentifier: (WorkspaceSnapshot) -> Void] = [:]
    private var timer: DispatchSourceTimer?
    private var eventToken: NSObjectProtocol?
    private let queue = DispatchQueue(label: "kshell.aerospace", qos: .utility)
    private var last: WorkspaceSnapshot?

    func subscribe(_ token: ObjectIdentifier, _ callback: @escaping (WorkspaceSnapshot) -> Void) {
        observers[token] = callback

        if eventToken == nil {
            eventToken = EventBus.observe("aerospace_workspace_change") { [weak self] payload in
                self?.handle(event: payload)
            }
        }
        startPollingIfNeeded()
        queue.async { [weak self] in self?.poll() }
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

    /// A pushed event carries the new focused workspace, so move the highlight
    /// right away; the poll that follows refreshes which workspaces hold windows.
    private func handle(event payload: String) {
        let focused = WorkspacesMonitor.focused(from: payload)
        queue.async { [weak self] in
            guard let self else { return }
            if let focused {
                var snapshot = self.last ?? WorkspaceSnapshot()
                snapshot.focused = focused
                self.publish(snapshot)
            }
            self.poll()
        }
    }

    private func poll() {
        publish(WorkspacesMonitor.parse(Shell.capture(WorkspacesMonitor.query)))
    }

    /// Only notify on real changes — this poll runs every second, and every
    /// notification repaints the bar.
    private func publish(_ snapshot: WorkspaceSnapshot) {
        guard snapshot != last else { return }
        last = snapshot
        DispatchQueue.main.async {
            for callback in self.observers.values { callback(snapshot) }
        }
    }

    /// Emits `F:<focused workspace>` followed by one line per open window's
    /// workspace. One shell spawn, two fast aerospace queries.
    static let query = """
        focused=$(aerospace list-workspaces --focused --format '%{workspace}' 2>/dev/null); \
        echo "F:$focused"; \
        aerospace list-windows --all --format '%{workspace}' 2>/dev/null
        """

    static func parse(_ output: String) -> WorkspaceSnapshot {
        var snapshot = WorkspaceSnapshot()
        var occupied = Set<String>()
        for line in output.components(separatedBy: .newlines) {
            let token = line.trimmingCharacters(in: .whitespaces)
            guard !token.isEmpty else { continue }
            if token.hasPrefix("F:") {
                snapshot.focused = String(token.dropFirst(2))
            } else {
                occupied.insert(token)
            }
        }
        snapshot.occupied = occupied
        return snapshot
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

/// One workspace item (e.g. `1`..`9`). Highlights when it is the focused
/// workspace, and — like sketchybar — is hidden unless it is focused or holds a
/// window (disable with `hide_empty = false`).
final class WorkspaceItemRuntime: WidgetRuntime {
    let model: WidgetModel
    private let id: String
    private let focusedColor: RGBA?
    private let unfocusedColor: RGBA?
    private let hideEmpty: Bool
    private var token: ObjectIdentifier { ObjectIdentifier(self) }

    init(id: String, spec: WidgetSpec, theme: Theme) {
        self.id = id
        self.focusedColor = spec.color("focused_color") ?? theme.highlight
        self.unfocusedColor = spec.color("unfocused_color") ?? theme.accent
        self.hideEmpty = spec.bool("hide_empty") ?? true
        model = WidgetFactory.baseModel(spec, theme: theme, label: id, paddingX: spec.number("padding") ?? 3)
        model.labelColor = unfocusedColor
        model.action = { Shell.run("aerospace workspace \(id)") }
    }

    func start() {
        WorkspacesMonitor.shared.subscribe(token) { [weak self] snapshot in
            guard let self else { return }
            self.model.labelColor = (snapshot.focused == self.id) ? self.focusedColor : self.unfocusedColor
            self.model.hidden = self.hideEmpty && !snapshot.shows(self.id)
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
