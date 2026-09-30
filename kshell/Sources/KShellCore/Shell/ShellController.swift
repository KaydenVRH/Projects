import AppKit

/// Builds and owns one bar surface per display, rebuilding when the config or
/// the display arrangement changes.
public final class ShellController {
    private let loader: ConfigLoader
    private let viewModel: BarViewModel
    private let ipc = IPCServer()
    private lazy var appLauncher = AppLauncher(viewModel: viewModel)
    private var scriptLauncher: ScriptLauncher?
    private var wallpaperLauncher: WallpaperLauncher?
    private var themeLauncher: ThemeLauncher?
    private lazy var autoHide = BarAutoHide(
        bars: { [weak self] in self?.bars ?? [] },
        suspended: { [weak self] in self?.manualHidden ?? false }
    )
    private var eventTokens: [NSObjectProtocol] = []
    private var bars: [BarPanel] = []
    private var manualHidden = false

    public init(loader: ConfigLoader = ConfigLoader()) {
        self.loader = loader
        self.viewModel = BarViewModel(config: loader.config)
    }

    public func start() {
        ipc.start()
        makeScriptLauncher()
        makeWallpaperLauncher()
        makeThemeLauncher()
        subscribeToEvents()
        rebuildBars()

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(screenParametersChanged),
            name: NSApplication.didChangeScreenParametersNotification,
            object: nil
        )

        loader.startWatching { [weak self] config in
            guard let self else { return }
            self.viewModel.apply(config)
            self.makeScriptLauncher()
            self.makeWallpaperLauncher()
            self.makeThemeLauncher()
            self.rebuildBars()
        }
    }

    public func stop() {
        ipc.stop()
        appLauncher.hide()
        scriptLauncher?.hide()
        wallpaperLauncher?.hide()
        themeLauncher?.hide(animated: false)
        autoHide.stop()
        for token in eventTokens { EventBus.unobserve(token) }
        eventTokens.removeAll()
        loader.stopWatching()
        viewModel.stop()
        for bar in bars { bar.dismiss() }
        bars.removeAll()
        NotificationCenter.default.removeObserver(self)
    }

    @objc private func screenParametersChanged() {
        rebuildBars()
    }

    private func subscribeToEvents() {
        let bus = EventBus.self
        eventTokens.append(bus.observe("app_launcher_toggle") { [weak self] _ in self?.appLauncher.toggle() })
        eventTokens.append(bus.observe("app_launcher_open") { [weak self] _ in self?.appLauncher.show() })
        eventTokens.append(bus.observe("app_launcher_close") { [weak self] _ in self?.appLauncher.hide() })
        eventTokens.append(bus.observe("script_launcher_toggle") { [weak self] _ in self?.scriptLauncher?.toggle() })
        eventTokens.append(bus.observe("script_launcher_open") { [weak self] _ in self?.scriptLauncher?.show() })
        eventTokens.append(bus.observe("script_launcher_close") { [weak self] _ in self?.scriptLauncher?.hide() })
        eventTokens.append(bus.observe("wallpaper_launcher_toggle") { [weak self] _ in self?.wallpaperLauncher?.toggle() })
        eventTokens.append(bus.observe("wallpaper_launcher_open") { [weak self] _ in self?.wallpaperLauncher?.show() })
        eventTokens.append(bus.observe("wallpaper_launcher_close") { [weak self] _ in self?.wallpaperLauncher?.hide() })
        eventTokens.append(bus.observe("theme_launcher_toggle") { [weak self] _ in self?.themeLauncher?.toggle() })
        eventTokens.append(bus.observe("theme_launcher_open") { [weak self] _ in self?.themeLauncher?.show() })
        eventTokens.append(bus.observe("theme_launcher_close") { [weak self] _ in self?.themeLauncher?.hide() })
        eventTokens.append(bus.observe("bar_hide") { [weak self] _ in self?.setBarsHidden(true) })
        eventTokens.append(bus.observe("bar_show") { [weak self] _ in self?.setBarsHidden(false) })
        eventTokens.append(bus.observe("bar_toggle") { [weak self] _ in
            guard let self else { return }
            self.setBarsHidden(!self.manualHidden)
        })
        eventTokens.append(bus.observe("config_reload") { [weak self] _ in
            self?.loader.reloadNow()
        })
    }

    private func makeScriptLauncher() {
        let directory = KShellPaths.resolve(loader.config.scriptDirectory)
        scriptLauncher = ScriptLauncher(viewModel: viewModel, directory: directory)
    }

    private func makeWallpaperLauncher() {
        let directories = loader.config.wallpaperDirectories.map { KShellPaths.resolve($0) }
        wallpaperLauncher = WallpaperLauncher(viewModel: viewModel, directories: directories)
    }

    /// Applying a theme rewrites `config.toml`, which reloads straight back into
    /// here. The picker is updated in place rather than re-created, so it never
    /// blinks or re-animates while it is open.
    private func makeThemeLauncher() {
        let config = loader.config.themeLauncher
        if let existing = themeLauncher {
            existing.update(config: config, activeName: viewModel.theme.name)
            return
        }
        guard !config.themes.isEmpty else { return }

        let launcher = ThemeLauncher(viewModel: viewModel) { [weak self] in
            self?.loader.reloadNow()
        }
        launcher.update(config: config, activeName: viewModel.theme.name)
        themeLauncher = launcher
    }

    /// Manually show/hide every bar, overriding auto-hide while hidden.
    public func setBarsHidden(_ hidden: Bool) {
        manualHidden = hidden
        for bar in bars { bar.setRevealed(!hidden, animated: true) }
    }

    private func rebuildBars() {
        for bar in bars { bar.dismiss() }

        let screens: [NSScreen]
        if viewModel.appearance.display == "main" {
            screens = [NSScreen.main ?? NSScreen.screens.first].compactMap { $0 }
        } else {
            screens = NSScreen.screens
        }

        bars = screens.map { screen in
            let bar = BarPanel(screen: screen, appearance: viewModel.appearance, viewModel: viewModel)
            bar.present()
            return bar
        }

        if manualHidden {
            for bar in bars { bar.setRevealed(false, animated: false) }
        }
        updateAutoHide()
    }

    private func updateAutoHide() {
        if bars.contains(where: { $0.autohideEnabled }) {
            autoHide.start()
        } else {
            autoHide.stop()
        }
    }
}
