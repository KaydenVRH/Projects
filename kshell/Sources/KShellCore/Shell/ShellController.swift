import AppKit
import Combine

/// Builds and owns one bar surface per display, rebuilding when the config or
/// the display arrangement changes.
public final class ShellController {
    private let loader: ConfigLoader
    private let viewModel: BarViewModel
    private let ipc = IPCServer()
    private var appLauncher: AppLauncher?
    private var scriptLauncher: ScriptLauncher?
    private var clipboardLauncher: ClipboardLauncher?
    private var powerMenu: PowerMenu?
    private var calendarPanel: CalendarPanel?
    private var wallpaperLauncher: WallpaperLauncher?
    private var themeLauncher: ThemeLauncher?
    private var mediaCenter: MediaIsland?
    private var musicLauncher: MusicPanel?
    private var musicLibrary: MusicLibrary?
    private var musicQueue: MusicQueue?
    private var musicPlaylists: MusicPlaylists?
    private var mediaCenterAlign: OverlayAlign?
    private lazy var autoHide = BarAutoHide(
        bars: { [weak self] in self?.bars ?? [] },
        suspended: { [weak self] in self?.manualHidden ?? false }
    )
    private var eventTokens: [NSObjectProtocol] = []
    private var bars: [BarPanel] = []
    private var borders: [ScreenBorderPanel] = []
    /// The Stark HUD's brackets and running light over each frame.
    private var hudFrames: [HudFramePanel] = []
    /// The Arcane theme's glowing frame and the sparks trailing the pointer.
    private var arcaneFrames: [ArcaneFramePanel] = []
    private var cursorTrail: CursorTrail?
    /// The Neon theme's tubes over the frame.
    private var neonFrames: [NeonFramePanel] = []
    private var visualisers: [VisualiserPanel] = []
    private var playingObserver: AnyCancellable?
    private var manualHidden = false
    /// Runtime override for `[border] enabled`; `nil` follows the config.
    private var borderOverride: Bool?
    /// Runtime override for `[visualiser] enabled`; `nil` follows the config.
    private var visualiserOverride: Bool?
    private var bluetoothMenu: BluetoothMenu?
    private var notificationCentre: NotificationCentre?
    private var notificationToast: NotificationToast?
    private var macNotifications: MacNotificationWatcher?
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
        makeNotifications()
        makeBluetoothMenu()
        ClipboardHistory.shared.start()
        subscribeToEvents()
        rebuildBars()
        fullscreen.start()
        // Music that was playing when the shell last went down carries on.
        MusicPlayer.shared.restoreSession()

        // The bars follow the player: up while something plays, away when it
        // stops. The levels themselves arrive from the engine's tap.
        playingObserver = MusicPlayer.shared.$isPlaying
            .receive(on: RunLoop.main)
            .sink { [weak self] playing in self?.setVisualiserPlaying(playing) }

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
            self.makeNotifications()
            self.makeBluetoothMenu()
            self.rebuildBars()
        }
    }

    public func stop() {
        ipc.stop()
        fullscreen.stop()
        appLauncher?.hide()
        scriptLauncher?.hide()
        clipboardLauncher?.hide()
        ClipboardHistory.shared.stop()
        wallpaperLauncher?.hide()
        themeLauncher?.hide(animated: false)
        mediaCenter?.hide(animated: false)
        musicLauncher?.hide(animated: false)
        bluetoothMenu?.hide(animated: false)
        bluetoothMenu = nil
        notificationCentre?.hide(animated: false)
        notificationCentre = nil
        notificationToast?.dismissNow()
        notificationToast = nil
        macNotifications?.stop()
        macNotifications = nil
        NotificationStore.shared.onArrive = nil
        autoHide.stop()
        playingObserver = nil
        for panel in visualisers { panel.dismiss() }
        visualisers.removeAll()
        for token in eventTokens { EventBus.unobserve(token) }
        eventTokens.removeAll()
        loader.stopWatching()
        viewModel.stop()
        for bar in bars { bar.dismiss() }
        bars.removeAll()
        for hud in hudFrames { hud.orderOut(nil) }
        hudFrames.removeAll()
        for frame in arcaneFrames { frame.orderOut(nil) }
        arcaneFrames.removeAll()
        cursorTrail?.stop()
        cursorTrail = nil
        for frame in neonFrames { frame.orderOut(nil) }
        neonFrames.removeAll()
        NotificationCenter.default.removeObserver(self)
    }

    @objc private func screenParametersChanged() {
        rebuildBars()
    }

    private func subscribeToEvents() {
        let bus = EventBus.self
        // `kshell trigger diagnose` — dump the state that decides whether the
        // shell is on screen, into kshell.log. The bar can look dead while the
        // process is perfectly healthy, and this is what tells the two apart.
        eventTokens.append(bus.observe("diagnose") { [weak self] _ in self?.dumpState() })
        eventTokens.append(bus.observe("app_launcher_toggle") { [weak self] _ in self?.appLauncher?.toggle() })
        eventTokens.append(bus.observe("app_launcher_open") { [weak self] _ in self?.appLauncher?.show() })
        eventTokens.append(bus.observe("app_launcher_close") { [weak self] _ in self?.appLauncher?.hide() })
        eventTokens.append(bus.observe("script_launcher_toggle") { [weak self] _ in self?.scriptLauncher?.toggle() })
        eventTokens.append(bus.observe("script_launcher_open") { [weak self] _ in self?.scriptLauncher?.show() })
        eventTokens.append(bus.observe("script_launcher_close") { [weak self] _ in self?.scriptLauncher?.hide() })
        eventTokens.append(bus.observe("clipboard_toggle") { [weak self] _ in self?.clipboardLauncher?.toggle() })
        eventTokens.append(bus.observe("clipboard_open") { [weak self] _ in self?.clipboardLauncher?.show() })
        eventTokens.append(bus.observe("clipboard_close") { [weak self] _ in self?.clipboardLauncher?.hide() })
        eventTokens.append(bus.observe("clipboard_clear") { _ in ClipboardHistory.shared.clear() })
        eventTokens.append(bus.observe("keep_awake_toggle") { _ in KeepAwake.shared.toggle() })
        eventTokens.append(bus.observe("power_menu_toggle") { [weak self] _ in self?.powerMenu?.toggle() })
        eventTokens.append(bus.observe("power_menu_open") { [weak self] _ in self?.powerMenu?.show() })
        eventTokens.append(bus.observe("power_menu_close") { [weak self] _ in self?.powerMenu?.hide() })
        eventTokens.append(bus.observe("calendar_toggle") { [weak self] _ in self?.calendarPanel?.toggle() })
        eventTokens.append(bus.observe("calendar_open") { [weak self] _ in self?.calendarPanel?.show() })
        eventTokens.append(bus.observe("calendar_close") { [weak self] _ in self?.calendarPanel?.hide() })
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
        // Apply a theme by name, as the picker would: `kshell trigger theme_apply Stark`.
        eventTokens.append(bus.observe("theme_apply") { [weak self] payload in
            guard let self else { return }
            let name = payload.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            let config = self.loader.config.themeLauncher
            guard let theme = config.themes.first(where: { $0.name.lowercased() == name }) else { return }
            _ = ThemeApplier.apply(theme, config: config)
            self.loader.reloadNow()
        })
        // Replay the Stark HUD's boot sequence.
        eventTokens.append(bus.observe("hud_boot") { _ in HudBoot.play() })
        // Replay the Arcane theme's casting.
        eventTokens.append(bus.observe("arcane_cast") { _ in ArcaneCast.play() })
        // Replay the Neon theme's glitch-in.
        eventTokens.append(bus.observe("neon_boot") { _ in NeonBoot.play() })
        eventTokens.append(bus.observe("visualiser_toggle") { [weak self] _ in
            guard let self else { return }
            self.setVisualiserEnabled(!self.visualiserEnabled)
        })
        eventTokens.append(bus.observe("visualiser_show") { [weak self] _ in self?.setVisualiserEnabled(true) })
        eventTokens.append(bus.observe("visualiser_hide") { [weak self] _ in self?.setVisualiserEnabled(false) })
        eventTokens.append(bus.observe("notification_center_toggle") { [weak self] _ in self?.notificationCentre?.toggle() })
        eventTokens.append(bus.observe("notification_center_open") { [weak self] _ in self?.notificationCentre?.show() })
        eventTokens.append(bus.observe("notification_center_close") { [weak self] _ in self?.notificationCentre?.hide() })
        eventTokens.append(bus.observe("notify") { payload in
            guard let item = KShellNotification.parse(payload: payload) else { return }
            NotificationStore.shared.ingest([item])
        })
        eventTokens.append(bus.observe("bluetooth_menu_toggle") { [weak self] _ in self?.bluetoothMenu?.toggle() })
        eventTokens.append(bus.observe("bluetooth_menu_open") { [weak self] _ in self?.bluetoothMenu?.show() })
        eventTokens.append(bus.observe("bluetooth_menu_close") { [weak self] _ in self?.bluetoothMenu?.hide() })
        eventTokens.append(bus.observe("bluetooth_toggle") { [weak self] payload in
            self?.bluetoothMenu?.toggleDevice(matching: payload)
        })
        eventTokens.append(bus.observe("bluetooth_power") { [weak self] payload in
            self?.bluetoothMenu?.setPower(payload)
        })
        eventTokens.append(bus.observe("notification_clear") { _ in NotificationStore.shared.clear() })
        eventTokens.append(bus.observe("config_reload") { [weak self] _ in
            self?.loader.reloadNow()
        })
    }

    /// Whether the border frame is on: a runtime override, or the config.
    private var borderEnabled: Bool {
        borderOverride ?? loader.config.border.enabled
    }

    /// Whether the visualiser is on: a runtime override, or the config.
    private var visualiserEnabled: Bool {
        visualiserOverride ?? loader.config.visualiser.enabled
    }

    public func setVisualiserEnabled(_ enabled: Bool) {
        guard enabled != visualiserEnabled else { return }
        visualiserOverride = enabled
        rebuildVisualisers()
    }

    private func setVisualiserPlaying(_ playing: Bool, animated: Bool = true) {
        for panel in visualisers { panel.setPlaying(playing, animated: animated) }
    }

    /// Build the Bluetooth menu, or tear it down when it is switched off.
    private func makeBluetoothMenu() {
        guard loader.config.bluetooth.enabled else {
            bluetoothMenu?.hide(animated: false)
            bluetoothMenu = nil
            return
        }
        if bluetoothMenu == nil {
            bluetoothMenu = BluetoothMenu(viewModel: viewModel, margin: panelMargin)
        }
    }

    /// Build the notification centre, its popups and the system database watcher.
    ///
    /// Rebuilt on every config reload so `[notifications]` takes effect without a
    /// restart; the store itself outlives the panels, so the history is not lost.
    private func makeNotifications() {
        let config = loader.config.notifications
        let store = NotificationStore.shared

        guard config.enabled else {
            notificationCentre?.hide(animated: false)
            notificationCentre = nil
            notificationToast?.dismissNow()
            notificationToast = nil
            macNotifications?.stop()
            macNotifications = nil
            store.onArrive = nil
            return
        }

        store.limit = config.history

        if let centre = notificationCentre {
            centre.update(config: config, margin: panelMargin)
        } else {
            notificationCentre = NotificationCentre(viewModel: viewModel, config: config, margin: panelMargin)
        }

        if config.toasts {
            if let toast = notificationToast {
                toast.update(margin: panelMargin, seconds: config.toastSeconds)
            } else {
                notificationToast = NotificationToast(
                    viewModel: viewModel,
                    margin: panelMargin,
                    seconds: config.toastSeconds,
                    onOpen: { [weak self] in self?.notificationCentre?.show() }
                )
            }
            store.onArrive = { [weak self] item in self?.notificationToast?.present(item) }
        } else {
            notificationToast?.dismissNow()
            notificationToast = nil
            store.onArrive = nil
        }

        if config.mirrorMacOS {
            if macNotifications == nil {
                let watcher = MacNotificationWatcher(store: store, history: min(config.history, 60))
                watcher.start()
                macNotifications = watcher
            }
        } else {
            macNotifications?.stop()
            macNotifications = nil
        }
    }

    /// Build the visualiser strips for whichever displays the config asks for.
    /// Panels are updated in place when the screen set is unchanged, so a theme
    /// change re-colours the bars without the window blinking.
    private func rebuildVisualisers() {
        let config = loader.config.visualiser
        let player = MusicPlayer.shared
        player.spectrum.bandCount = config.bars
        player.spectrumWanted = visualiserEnabled
        if !visualiserEnabled { player.spectrum.isEnabled = false }

        let screens: [NSScreen]
        if config.display == "main" {
            screens = [NSScreen.main ?? NSScreen.screens.first].compactMap { $0 }
        } else {
            screens = NSScreen.screens
        }

        // Inside the bar: the bar draws it, so there are no strips to build.
        viewModel.barVisualiser = visualiserEnabled && config.inBar ? config : nil

        guard visualiserEnabled, !config.inBar, !screens.isEmpty else {
            for panel in visualisers { panel.dismiss() }
            visualisers.removeAll()
            return
        }

        if visualisers.count == screens.count,
           zip(visualisers, screens).allSatisfy({ $0.visualiserScreen == $1 }) {
            for (panel, screen) in zip(visualisers, screens) {
                panel.apply(config: config, theme: viewModel.theme, margin: panelMargin, screen: screen)
            }
        } else {
            for panel in visualisers { panel.dismiss() }
            visualisers = screens.map { screen in
                VisualiserPanel(screen: screen, config: config, theme: viewModel.theme, margin: panelMargin)
            }
        }
        setVisualiserPlaying(MusicPlayer.shared.isPlaying, animated: false)
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
        rebuildVisualisers()
    }

    /// Print what the shell thinks its own windows are doing.
    private func dumpState() {
        let front = NSWorkspace.shared.frontmostApplication?.localizedName ?? "?"
        let me = NSRunningApplication.current
        var lines = [
            "kshell: diagnose — front=\(front) shellFront=\(me.isActive) keyWindow=\(NSApp.keyWindow.map { "\(type(of: $0))" } ?? "none") covered=\(fullscreenDisplays.sorted()) manualHidden=\(manualHidden) autoHide=\(autoHide.isRunning)"
        ]
        for (i, bar) in bars.enumerated() {
            let screen = FullscreenWatch.displayID(bar.targetScreen).map(String.init) ?? "?"
            lines.append(
                "  bar[\(i)] screen=\(screen) onscreen=\(bar.isVisible) revealed=\(bar.isRevealed) "
                + "frame=\(bar.frame) sheet=\(bar.sheetHeight) hitRects=\(bar.hitRegionCount)"
            )
        }
        // Every window the shell owns, whatever kind it is: a window that is
        // still ordered in at a high level is what swallows clicks.
        for window in NSApp.windows {
            lines.append(
                "  window \(type(of: window)) visible=\(window.isVisible) "
                + "level=\(window.level.rawValue) frame=\(window.frame)"
            )
        }
        FileHandle.standardError.write(Data((lines.joined(separator: "\n") + "\n").utf8))
        // stderr is a launchd redirect, and it goes stale when the log file is
        // rotated or recreated — a diagnose that writes only there can vanish
        // entirely. Keep the last dump on disk too, where it can always be read.
        let text = lines.joined(separator: "\n") + "\n"
        let path = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Caches/kshell/diagnose.log")
        try? FileManager.default.createDirectory(
            at: path.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try? text.write(to: path, atomically: true, encoding: .utf8)
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
        for hud in hudFrames {
            if fullscreenDisplays.contains(FullscreenWatch.displayID(hud.frameScreen) ?? 0) {
                hud.orderOut(nil)
            } else {
                hud.orderFrontRegardless()
            }
        }
        for frame in arcaneFrames {
            if fullscreenDisplays.contains(FullscreenWatch.displayID(frame.frameScreen) ?? 0) {
                frame.orderOut(nil)
            } else {
                frame.orderFrontRegardless()
            }
        }
        for frame in neonFrames {
            if fullscreenDisplays.contains(FullscreenWatch.displayID(frame.frameScreen) ?? 0) {
                frame.orderOut(nil)
            } else {
                frame.orderFrontRegardless()
            }
        }
        for screen in NSScreen.screens {
            cursorTrail?.setHidden(fullscreenDisplays.contains(FullscreenWatch.displayID(screen) ?? 0), on: screen)
        }
        for panel in visualisers {
            if fullscreenDisplays.contains(FullscreenWatch.displayID(panel.visualiserScreen) ?? 0) {
                panel.dismiss()
            } else {
                panel.present()
            }
        }
        if !fullscreenDisplays.isEmpty {
            appLauncher?.hide()
            scriptLauncher?.hide()
            clipboardLauncher?.hide()
            powerMenu?.hide()
            calendarPanel?.hide()
            wallpaperLauncher?.hide()
            themeLauncher?.hide(animated: false)
            mediaCenter?.hide(animated: false)
        }
        updateAutoHide()
    }

    private func makeScriptLauncher() {
        let directory = KShellPaths.resolve(loader.config.scriptDirectory)
        scriptLauncher = ScriptLauncher(viewModel: viewModel, directory: directory, margin: panelMargin)
        // Built alongside: it follows the same frame margin.
        clipboardLauncher?.hide()
        clipboardLauncher = ClipboardLauncher(viewModel: viewModel, margin: panelMargin)
        powerMenu?.hide()
        powerMenu = PowerMenu(viewModel: viewModel, margin: panelMargin)
        calendarPanel?.hide()
        calendarPanel = CalendarPanel(viewModel: viewModel, margin: panelMargin)
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

        // The queue outlives the panel and is kept between runs, like the library.
        let queue: MusicQueue
        if let existing = musicQueue {
            queue = existing
        } else {
            queue = MusicQueue()
            musicQueue = queue
        }

        // Playlists outlive the panel too — they are the user's, not the view's.
        let playlists: MusicPlaylists
        if let existing = musicPlaylists {
            playlists = existing
        } else {
            playlists = MusicPlaylists()
            musicPlaylists = playlists
        }

        MusicPlayer.shared.libraryProvider = { [weak library] in library?.tracks ?? [] }
        if musicLauncher == nil {
            musicLauncher = MusicPanel(
                viewModel: viewModel,
                library: library,
                queue: queue,
                playlists: playlists,
                margin: panelMargin
            )
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
        mediaCenter = MediaIsland(
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
        for hud in hudFrames { hud.orderOut(nil) }
        hudFrames.removeAll()
        for frame in arcaneFrames { frame.orderOut(nil) }
        arcaneFrames.removeAll()
        cursorTrail?.stop()
        cursorTrail = nil
        for frame in neonFrames { frame.orderOut(nil) }
        neonFrames.removeAll()

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

        // Stark: brackets and a running light over the frame, above its glass.
        if usesSurface, Hud.active {
            let inset = CGFloat(borderConfig.inset + borderConfig.thickness)
            hudFrames = screens.map { screen in
                let hud = HudFramePanel(screen: screen, inset: inset, radius: CGFloat(borderConfig.radius))
                hud.orderFrontRegardless()
                return hud
            }
        }

        // Arcane: a breathing violet edge with motes rising off it, and sparks
        // trailing the pointer (the trail needs no frame).
        if Arcane.active {
            if usesSurface {
                let inset = CGFloat(borderConfig.inset + borderConfig.thickness)
                arcaneFrames = screens.map { screen in
                    let frame = ArcaneFramePanel(screen: screen, inset: inset, radius: CGFloat(borderConfig.radius))
                    frame.orderFrontRegardless()
                    return frame
                }
            }
            cursorTrail = ArcaneCursorTrail()
        }
        // Neon: double tubes round the frame, flickering corner signs, and a
        // light streak after a quick pointer.
        if Neon.active {
            if usesSurface {
                let inset = CGFloat(borderConfig.inset + borderConfig.thickness)
                neonFrames = screens.map { screen in
                    let frame = NeonFramePanel(screen: screen, inset: inset, radius: CGFloat(borderConfig.radius))
                    frame.orderFrontRegardless()
                    return frame
                }
            }
            cursorTrail = NeonCursorTrail()
        }

        if manualHidden {
            for bar in bars { bar.setRevealed(false, animated: false) }
        }
        rebuildVisualisers()
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
