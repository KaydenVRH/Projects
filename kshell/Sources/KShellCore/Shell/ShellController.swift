import AppKit

/// Builds and owns one bar surface per display, rebuilding when the config or
/// the display arrangement changes.
public final class ShellController {
    private let loader: ConfigLoader
    private let viewModel: BarViewModel
    private let ipc = IPCServer()
    private var appLauncher: AppLauncher?
    private var scriptLauncher: ScriptLauncher?
    private var wallpaperLauncher: WallpaperLauncher?
    private var themeLauncher: ThemeLauncher?
    private var mediaCenter: MediaCenter?
    private var musicLauncher: MusicPanel?
    private var musicLibrary: MusicLibrary?
    private var mediaCenterAlign: OverlayAlign?
    private lazy var autoHide = BarAutoHide(
        bars: { [weak self] in self?.bars ?? [] },
        suspended: { [weak self] in self?.manualHidden ?? false }
    )
    private var eventTokens: [NSObjectProtocol] = []
    private var bars: [BarPanel] = []
    private var borders: [ScreenBorderPanel] = []
    private var manualHidden = false
    /// Runtime override for `[border] enabled`; `nil` follows the config.
    private var borderOverride: Bool?
    /// Displays whose frontmost window fills them.
    private var fullscreenDisplays: Set<CGDirectDisplayID> = []
    private lazy var fullscreen = FullscreenWatch { [weak self] covered in
        self?.fullscreenDisplays = covered
        self?.applyFullscreenState()
    }

    public init(loader: ConfigLoader = ConfigLoader()) {
        self.loader = loader
        self.viewModel = BarViewModel(config: loader.config)
    }

    public func start() {
        ipc.start()
        makeAppLauncher()
        makeScriptLauncher()
        makeWallpaperLauncher()
        makeThemeLauncher()
        makeMediaCenter()
        makeMusicLauncher()
        subscribeToEvents()
        rebuildBars()
        fullscreen.start()

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(screenParametersChanged),
            name: NSApplication.didChangeScreenParametersNotification,
            object: nil
        )

        loader.startWatching { [weak self] config in
            guard let self else { return }
            self.viewModel.apply(config)
            self.makeAppLauncher()
            self.makeScriptLauncher()
            self.makeWallpaperLauncher()
            self.makeThemeLauncher()
            self.makeMediaCenter()
            self.makeMusicLauncher()
            self.rebuildBars()
        }
    }

    public func stop() {
        ipc.stop()
        fullscreen.stop()
        appLauncher?.hide()
        scriptLauncher?.hide()
        wallpaperLauncher?.hide()
        themeLauncher?.hide(animated: false)
        mediaCenter?.hide(animated: false)
        musicLauncher?.hide(animated: false)
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
        eventTokens.append(bus.observe("app_launcher_toggle") { [weak self] _ in self?.appLauncher?.toggle() })
        eventTokens.append(bus.observe("app_launcher_open") { [weak self] _ in self?.appLauncher?.show() })
        eventTokens.append(bus.observe("app_launcher_close") { [weak self] _ in self?.appLauncher?.hide() })
        eventTokens.append(bus.observe("script_launcher_toggle") { [weak self] _ in self?.scriptLauncher?.toggle() })
        eventTokens.append(bus.observe("script_launcher_open") { [weak self] _ in self?.scriptLauncher?.show() })
        eventTokens.append(bus.observe("script_launcher_close") { [weak self] _ in self?.scriptLauncher?.hide() })
        eventTokens.append(bus.observe("wallpaper_launcher_toggle") { [weak self] _ in self?.wallpaperLauncher?.toggle() })
        eventTokens.append(bus.observe("wallpaper_launcher_open") { [weak self] _ in self?.wallpaperLauncher?.show() })
        eventTokens.append(bus.observe("wallpaper_launcher_close") { [weak self] _ in self?.wallpaperLauncher?.hide() })
        eventTokens.append(bus.observe("theme_launcher_toggle") { [weak self] _ in self?.themeLauncher?.toggle() })
        eventTokens.append(bus.observe("theme_launcher_open") { [weak self] _ in self?.themeLauncher?.show() })
        eventTokens.append(bus.observe("theme_launcher_close") { [weak self] _ in self?.themeLauncher?.hide() })
        eventTokens.append(bus.observe("media_play_pause") { _ in MediaMonitor.shared.control(.playPause) })
        eventTokens.append(bus.observe("media_next") { _ in MediaMonitor.shared.control(.next) })
        eventTokens.append(bus.observe("media_previous") { _ in MediaMonitor.shared.control(.previous) })
        eventTokens.append(bus.observe("music_launcher_toggle") { [weak self] _ in self?.musicLauncher?.toggle() })
        eventTokens.append(bus.observe("music_launcher_open") { [weak self] _ in self?.musicLauncher?.show() })
        eventTokens.append(bus.observe("music_launcher_close") { [weak self] _ in self?.musicLauncher?.hide() })
        eventTokens.append(bus.observe("music_play_pause") { _ in MusicPlayer.shared.toggle() })
        eventTokens.append(bus.observe("music_play_library") { [weak self] _ in
            guard let tracks = self?.musicLibrary?.tracks, !tracks.isEmpty else { return }
            MusicPlayer.shared.play(tracks)
        })
        eventTokens.append(bus.observe("music_next") { _ in MusicPlayer.shared.next() })
        eventTokens.append(bus.observe("music_previous") { _ in MusicPlayer.shared.previous() })
        eventTokens.append(bus.observe("media_center_toggle") { [weak self] _ in self?.mediaCenter?.toggle() })
        eventTokens.append(bus.observe("media_center_open") { [weak self] _ in self?.mediaCenter?.show() })
        eventTokens.append(bus.observe("media_center_close") { [weak self] _ in self?.mediaCenter?.hide() })
        eventTokens.append(bus.observe("bar_hide") { [weak self] _ in self?.setBarsHidden(true) })
        eventTokens.append(bus.observe("bar_show") { [weak self] _ in self?.setBarsHidden(false) })
        eventTokens.append(bus.observe("bar_toggle") { [weak self] _ in
            guard let self else { return }
            self.setBarsHidden(!self.manualHidden)
        })
        eventTokens.append(bus.observe("border_toggle") { [weak self] _ in
            guard let self else { return }
            self.setBorderEnabled(!self.borderEnabled)
        })
        eventTokens.append(bus.observe("border_show") { [weak self] _ in self?.setBorderEnabled(true) })
        eventTokens.append(bus.observe("border_hide") { [weak self] _ in self?.setBorderEnabled(false) })
        eventTokens.append(bus.observe("config_reload") { [weak self] _ in
            self?.loader.reloadNow()
        })
    }

    /// Whether the border frame is on: a runtime override, or the config.
    private var borderEnabled: Bool {
        borderOverride ?? loader.config.border.enabled
    }

    /// Where the launcher panels start: flush against the frame's inner opening,
    /// so there is no sliver of wallpaper between a panel and the border. The bar
    /// uses the same inset, and the media centre hangs off the bar.
    private var panelMargin: CGFloat {
        let border = loader.config.border
        guard borderEnabled else { return 0 }
        return CGFloat(border.inset + border.thickness)
    }

    private func makeAppLauncher() {
        appLauncher = AppLauncher(viewModel: viewModel, margin: panelMargin)
    }

    /// Turn the frame on or off at runtime. The bar's glass comes from the frame
    /// when it is on and from its own material view when it is off, so the bars
    /// are rebuilt either way.
    public func setBorderEnabled(_ enabled: Bool) {
        guard enabled != borderEnabled else { return }
        borderOverride = enabled
        appLauncher?.hide()
        scriptLauncher?.hide()
        wallpaperLauncher?.hide()
        themeLauncher?.hide(animated: false)
        mediaCenter?.hide(animated: false)
        makeAppLauncher()
        makeScriptLauncher()
        makeWallpaperLauncher()
        // The theme picker is normally updated in place so it never blinks, but
        // its margin is fixed when it is built — so it is rebuilt here.
        themeLauncher = nil
        makeThemeLauncher()
        rebuildBars()
    }

    /// Get out of the way of full-screen windows, and come back afterwards.
    private func applyFullscreenState() {
        for bar in bars {
            guard let id = FullscreenWatch.displayID(bar.targetScreen) else { continue }
            if fullscreenDisplays.contains(id) {
                bar.dismiss()
            } else {
                bar.present()
                bar.setRevealed(!manualHidden, animated: false)
            }
        }
        for border in borders {
            if fullscreenDisplays.contains(FullscreenWatch.displayID(border.borderScreen) ?? 0) {
                border.orderOut(nil)
            } else {
                border.orderFrontRegardless()
            }
        }
        if !fullscreenDisplays.isEmpty {
            appLauncher?.hide()
            scriptLauncher?.hide()
            wallpaperLauncher?.hide()
            themeLauncher?.hide(animated: false)
            mediaCenter?.hide(animated: false)
        }
        updateAutoHide()
    }

    private func makeScriptLauncher() {
        let directory = KShellPaths.resolve(loader.config.scriptDirectory)
        scriptLauncher = ScriptLauncher(viewModel: viewModel, directory: directory, margin: panelMargin)
    }

    private func makeWallpaperLauncher() {
        let directories = loader.config.wallpaperDirectories.map { KShellPaths.resolve($0) }
        wallpaperLauncher = WallpaperLauncher(
            viewModel: viewModel,
            directories: directories,
            margin: panelMargin
        )
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

        let launcher = ThemeLauncher(viewModel: viewModel, margin: panelMargin) { [weak self] in
            self?.loader.reloadNow()
        }
        launcher.update(config: config, activeName: viewModel.theme.name)
        themeLauncher = launcher
    }

    /// The music panel: a bottom sheet holding the library, the downloader and
    /// the player. The library outlives the panel, since music keeps playing
    /// with the panel closed.
    private func makeMusicLauncher() {
        let config = loader.config.music
        let directory = KShellPaths.resolve(config.directory)

        let library: MusicLibrary
        if let existing = musicLibrary {
            existing.setDirectory(directory)
            library = existing
        } else {
            library = MusicLibrary(directory: directory)
            musicLibrary = library
        }

        MusicDownloads.shared.directory = directory
        MusicDownloads.shared.searchCount = config.searchCount
        MusicDownloads.shared.audioFormat = config.format
        MusicDownloads.shared.ytdlp = config.ytdlp

        if musicLauncher == nil {
            musicLauncher = MusicPanel(viewModel: viewModel, library: library, margin: panelMargin)
        }
        library.refresh()
    }

    /// The media centre hangs off the bar's inner edge. Rebuilt only when what
    /// it needs changes, so an open panel survives a config reload.
    private func makeMediaCenter() {
        let media = loader.config.media
        MediaMonitor.shared.termusicScript = media.termusic
        guard mediaCenter == nil || mediaCenterAlign != media.align else { return }

        mediaCenterAlign = media.align
        let wasVisible = mediaCenter?.visible ?? false
        mediaCenter?.hide(animated: false)
        mediaCenter = MediaCenter(
            viewModel: viewModel,
            monitor: .shared,
            align: media.align,
            bar: { [weak self] in self?.barUnderMouse() }
        )
        if wasVisible { mediaCenter?.show(animated: false) }
    }

    /// The bar on the display the pointer is on — the one a sheet unfurls from.
    private func barUnderMouse() -> BarPanel? {
        let mouse = NSEvent.mouseLocation
        return bars.first { NSMouseInRect(mouse, $0.targetScreen.frame, false) }
            ?? bars.first { $0.targetScreen == NSScreen.main }
            ?? bars.first
    }

    /// Manually show/hide every bar, overriding auto-hide while hidden.
    public func setBarsHidden(_ hidden: Bool) {
        manualHidden = hidden
        for bar in bars { bar.setRevealed(!hidden, animated: true) }
    }

    private func rebuildBars() {
        for bar in bars { bar.dismiss() }
        for border in borders { border.orderOut(nil) }

        let screens: [NSScreen]
        if viewModel.appearance.display == "main" {
            screens = [NSScreen.main ?? NSScreen.screens.first].compactMap { $0 }
        } else {
            screens = NSScreen.screens
        }

        // The frame is a bezel around the screen: the bar nests inside it and the
        // launchers sit just inside it, so everything lines up with the padding.
        // One material view (in the border's window) is shared by the frame, the
        // bar and the sheet — that is what keeps their junctions seamless.
        let borderConfig = loader.config.border
        var appearance = viewModel.appearance
        let usesSurface = borderEnabled
        if usesSurface {
            let thickness = CGFloat(borderConfig.inset + borderConfig.thickness)
            appearance.marginX = max(appearance.marginX, thickness)
            appearance.marginY = max(appearance.marginY, thickness)
            appearance.cornerRadius = max(appearance.cornerRadius, borderConfig.radius)
        }

        viewModel.panelSurface = usesSurface
            ? PanelSurfaceSettings(radius: CGFloat(borderConfig.radius))
            : nil
        viewModel.makeGlass = { [weak self] screen in
            self?.borders.first { $0.borderScreen == screen }?.makeGlass()
        }

        bars = screens.map { screen in
            let bar = BarPanel(screen: screen, appearance: appearance, viewModel: viewModel)
            bar.usesSharedSurface = usesSurface
            bar.present()
            return bar
        }

        borders = usesSurface ? screens.enumerated().map { index, screen in
            let shown = BarPanel.frame(for: screen, appearance: appearance)
            let border = ScreenBorderPanel(
                screen: screen,
                config: borderConfig,
                barFrame: shown,
                sheetResting: NSRect(x: shown.minX, y: shown.minY - 1, width: 1, height: 1)
            )
            bars[index].surface = border
            // Now that the bar has its surface, let it publish the real geometry.
            bars[index].publishSurface()
            border.orderFrontRegardless()
            return border
        } : []

        if manualHidden {
            for bar in bars { bar.setRevealed(false, animated: false) }
        }
        updateAutoHide()
    }

    private func updateAutoHide() {
        if !fullscreenDisplays.isEmpty {
            autoHide.stop()
        } else if bars.contains(where: { $0.autohideEnabled }) {
            autoHide.start()
        } else {
            autoHide.stop()
        }
    }
}
