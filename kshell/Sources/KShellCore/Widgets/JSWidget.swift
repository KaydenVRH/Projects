import Foundation

/// A widget whose content is produced by a user JavaScript file (or inline
/// `source`). The script defines `render()` (and optionally `onEvent`), and can
/// drive itself with `exec`. The file is watched and hot-reloaded.
final class JSWidgetRuntime: WidgetRuntime, JSRunner {
    let model: WidgetModel

    private var source: String
    private let fileURL: URL?
    private var engine: JSEngine?
    private var renderTimer: DispatchSourceTimer?
    private var execRunners: [ProcessRunner] = []
    private var watcher: FileWatcher?
    private var isRendering = false

    init?(spec: WidgetSpec, theme: Theme) {
        if let inline = spec.string("source") {
            source = inline
            fileURL = nil
        } else if let path = spec.string("file") {
            let url = KShellPaths.resolve(path)
            guard let text = try? String(contentsOf: url, encoding: .utf8) else {
                FileHandle.standardError.write(
                    Data("kshell: js widget cannot read \(url.path)\n".utf8)
                )
                return nil
            }
            source = text
            fileURL = url
        } else {
            FileHandle.standardError.write(
                Data("kshell: js widget needs 'file' or 'source'\n".utf8)
            )
            return nil
        }
        model = WidgetFactory.baseModel(spec, theme: theme)
    }

    func start() {
        buildEngine()
        scheduleRender()
        subscribeToConfiguredEvents()
        if let fileURL {
            watcher = FileWatcher(path: fileURL) { [weak self] in self?.reload() }
        }
    }

    func stop() {
        renderTimer?.cancel()
        renderTimer = nil
        stopExec()
        for token in eventTokens { EventBus.unobserve(token) }
        eventTokens.removeAll()
        watcher?.stop()
        watcher = nil
        engine = nil
    }

    private var eventTokens: [NSObjectProtocol] = []

    /// If the widget declares `events = "a,b"`, its `onEvent(name, payload)` is
    /// called whenever those events fire.
    private func subscribeToConfiguredEvents() {
        guard let list = model.spec.string("events") else { return }
        for name in list.split(separator: ",") {
            let event = name.trimmingCharacters(in: .whitespaces)
            guard !event.isEmpty else { continue }
            eventTokens.append(EventBus.observe(event) { [weak self] payload in
                self?.engine?.callEvent(event, payload: payload)
                self?.render()
            })
        }
    }

    // MARK: - Engine lifecycle

    private func buildEngine() {
        stopExec()
        engine = JSEngine(source: source, runner: self)
        render()
    }

    private func reload() {
        if let fileURL, let text = try? String(contentsOf: fileURL, encoding: .utf8) {
            source = text
        }
        buildEngine()
    }

    private func stopExec() {
        for runner in execRunners { runner.stop() }
        execRunners.removeAll()
    }

    private func scheduleRender() {
        let interval = model.spec.number("interval") ?? 1
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: max(interval, 0.25))
        timer.setEventHandler { [weak self] in self?.render() }
        timer.resume()
        renderTimer = timer
    }

    private func render() {
        guard !isRendering, let render = engine?.render() else { return }
        isRendering = true
        defer { isRendering = false }

        if let icon = render.icon { model.icon = icon }
        if let label = render.label { model.label = label }
        if let color = render.iconColor { model.iconColor = color }
        if let color = render.labelColor { model.labelColor = color }
        if let padding = render.padding { model.paddingX = padding }
        if let flexible = render.flexible { model.flexible = flexible }

        if let action = render.action {
            model.action = { Shell.run(action) }
        } else if let event = render.actionEvent {
            model.action = { EventBus.post(event) }
        }
    }

    // MARK: - JSRunner

    func run(command: String, interval: TimeInterval, onOutput: @escaping (String) -> Void) {
        let runner = ProcessRunner()
        execRunners.append(runner)
        runner.run(command: command, interval: interval) { [weak self] output in
            onOutput(output)
            self?.render()
        }
    }

    func runOnce(command: String, onOutput: @escaping (String) -> Void) {
        let runner = ProcessRunner()
        execRunners.append(runner)
        runner.runOnce(command: command) { [weak self] output in
            onOutput(output)
            self?.render()
        }
    }

    func fire(command: String) {
        Shell.run(command)
    }

    func postEvent(_ name: String, payload: String) {
        EventBus.post(name, payload: payload)
    }
}
