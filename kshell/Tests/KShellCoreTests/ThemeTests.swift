import XCTest
@testable import KShellCore

final class ThemeTests: XCTestCase {
    /// The applier runs the dotfiles' tmux writer, which rewrites the real
    /// theme and reloads the running server. Point it at a scratch file and an
    /// empty socket directory instead, so a test run never recolours tmux.
    private var savedEnvironment: [String: String?] = [:]

    override func setUp() {
        super.setUp()
        let scratch = FileManager.default.temporaryDirectory.appendingPathComponent("kshell-tmux-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
        for key in ["TMUX_THEME_CONF", "TMUX_TMPDIR", "TMUX"] {
            savedEnvironment[key] = ProcessInfo.processInfo.environment[key]
        }
        setenv("TMUX_THEME_CONF", scratch.appendingPathComponent("theme.conf").path, 1)
        setenv("TMUX_TMPDIR", scratch.path, 1)
        unsetenv("TMUX")
    }

    override func tearDown() {
        for (key, value) in savedEnvironment {
            if let value { setenv(key, value, 1) } else { unsetenv(key) }
        }
        savedEnvironment.removeAll()
        super.tearDown()
    }

    private let toml = """
    [theme]
    name       = "Test"
    accent     = "#ff2a85"
    highlight  = "#00d4ff"
    foreground = "#e0d0f0"
    dim        = "#9a80aa"
    mid        = "#00d4ff"
    background = "#100018AA"

    [bar]
    background = "$background"

    [[bar.left]]
    type       = "apple"
    icon_color = "$accent"

    [theme_launcher]
    kitty_config = "/tmp/kitty.conf"

    [[theme_launcher.themes]]
    name  = "Synthwave"
    kitty = "synthwave.conf"
    nvim  = "synthwave"
    """

    func testTokensResolveEverywhere() throws {
        let config = try ShellConfig.parse(toml: toml)
        XCTAssertEqual(config.bar.background, RGBA(hex: "#100018AA"))
        XCTAssertEqual(config.left.first?.color("icon_color"), RGBA(hex: "#ff2a85"))
    }

    func testThemeMetadataParses() throws {
        let config = try ShellConfig.parse(toml: toml)
        XCTAssertEqual(config.theme.name, "Test")
        XCTAssertEqual(config.theme.dim, RGBA(hex: "#9a80aa"))
        XCTAssertEqual(config.theme.mid, RGBA(hex: "#00d4ff"))
    }

    func testThemeLauncherParses() throws {
        let config = try ShellConfig.parse(toml: toml)
        XCTAssertEqual(config.themeLauncher.kittyConfig, "/tmp/kitty.conf")
        XCTAssertEqual(config.themeLauncher.themes.count, 1)
        XCTAssertEqual(config.themeLauncher.themes.first?.name, "Synthwave")
        XCTAssertEqual(config.themeLauncher.themes.first?.nvim, "synthwave")
    }

    func testLiveWallpaperParses() throws {
        let config = try ShellConfig.parse(toml: """
        [[theme_launcher.themes]]
        name           = "Glow"
        kitty          = "glow.conf"
        nvim           = "glow"
        wallpaper      = "~/wall.jpg"
        live_wallpaper = "~/glow.mp4"
        """)
        XCTAssertEqual(config.themeLauncher.themes.first?.wallpaper, "~/wall.jpg")
        XCTAssertEqual(config.themeLauncher.themes.first?.liveWallpaper, "~/glow.mp4")
    }

    func testUnknownTokenBecomesNil() throws {
        let config = try ShellConfig.parse(toml: """
        [theme]
        accent = "#ff2a85"

        [[bar.left]]
        type       = "apple"
        icon_color = "$nope"
        """)
        XCTAssertNil(config.left.first?.color("icon_color"))
    }

    // MARK: - applying

    private func makeFixture() throws -> (URL, ThemeLauncherConfig) {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("kshell-theme-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

        try """
        font_size 12
        # BEGIN_KITTY_THEME
        include synthwave.conf
        # END_KITTY_THEME
        tab_bar_style powerline
        """.write(to: dir.appendingPathComponent("kitty.conf"), atomically: true, encoding: .utf8)

        try """
        vim.cmd.colorscheme("mono")
        local c = {
          bg     = "none",
          fg     = "#e0d0f0",
          dim    = "#9a80aa",
          accent = "#ff2a85",
          mid    = "#00d4ff",
        }
        """.write(to: dir.appendingPathComponent("init.lua"), atomically: true, encoding: .utf8)

        try """
        [bar]
        height = 30

        [theme]
        accent = "#ff2a85"

        [script_launcher]
        directory = "~/x"
        """.write(to: dir.appendingPathComponent("config.toml"), atomically: true, encoding: .utf8)

        var config = ThemeLauncherConfig()
        config.kittyConfig = dir.appendingPathComponent("kitty.conf").path
        config.nvimInit = dir.appendingPathComponent("init.lua").path
        config.shellConfig = dir.appendingPathComponent("config.toml").path
        // Never the real one: the applier rewrites it when it exists.
        config.bordersConfig = dir.appendingPathComponent("bordersrc").path
        return (dir, config)
    }

    func testApplierRewritesEachTool() throws {
        let (dir, config) = try makeFixture()
        defer { try? FileManager.default.removeItem(at: dir) }

        let theme = ThemeDefinition(
            name: "Everforest", kitty: "everforest.conf", nvim: "everforest",
            accent: "#83c092", highlight: "#7fbbb3", foreground: "#d3c6aa",
            dim: "#859289", mid: "#7fbbb3", background: "#2d353bAA"
        )
        let result = ThemeApplier.apply(theme, config: config)
        XCTAssertEqual(result.failed, [])
        XCTAssertEqual(result.changed, ["kitty", "tmux", "nvim", "kshell"])

        let kitty = try String(contentsOf: config.kittyConfig.url, encoding: .utf8)
        XCTAssertTrue(kitty.contains("include everforest.conf"))
        XCTAssertFalse(kitty.contains("synthwave.conf"))
        XCTAssertTrue(kitty.contains("tab_bar_style powerline"), "other lines untouched")

        let nvim = try String(contentsOf: config.nvimInit.url, encoding: .utf8)
        XCTAssertTrue(nvim.contains("vim.cmd.colorscheme(\"everforest\")"))
        XCTAssertTrue(nvim.contains("fg     = \"#d3c6aa\""), "alignment preserved")
        XCTAssertTrue(nvim.contains("accent = \"#83c092\""))
        XCTAssertTrue(nvim.contains("mid    = \"#7fbbb3\""))
        XCTAssertTrue(nvim.contains("bg     = \"none\""), "bg is not part of the palette")

        let shell = try String(contentsOf: config.shellConfig.url, encoding: .utf8)
        XCTAssertTrue(shell.contains("name = \"Everforest\""))
        XCTAssertTrue(shell.contains("background = \"#2d353bAA\""))
        XCTAssertTrue(shell.contains("[script_launcher]"), "later sections kept")

        // The rewritten config must still parse, with the theme applied.
        let parsed = try ShellConfig.parse(toml: shell)
        XCTAssertEqual(parsed.bar.height, 30)
        XCTAssertEqual(parsed.theme.name, "Everforest")
        XCTAssertEqual(parsed.theme.accent, RGBA(hex: "#83c092"))
        XCTAssertEqual(parsed.scriptDirectory, "~/x")
    }

    func testApplierWritesKittyGlass() throws {
        let (dir, config) = try makeFixture()
        defer { try? FileManager.default.removeItem(at: dir) }

        var glassy = config
        glassy.opacity = 0.7
        glassy.blur = 40

        let mono = ThemeDefinition(
            name: "Monochrome", kitty: "bi.conf", nvim: "mono",
            accent: "#d0d0d0", highlight: "#808080", foreground: "#e0e0e0",
            dim: "#606060", mid: "#a0a0a0", background: "#1a1a1a55",
            opacity: 0.45, blur: 64
        )
        XCTAssertEqual(ThemeApplier.apply(mono, config: glassy).failed, [])

        let kitty = try String(contentsOf: config.kittyConfig.url, encoding: .utf8)
        XCTAssertTrue(kitty.contains("include bi.conf"))
        XCTAssertTrue(kitty.contains("background_opacity 0.45"))
        XCTAssertTrue(kitty.contains("background_blur 64"))

        // A theme without its own glass falls back to the launcher defaults.
        let plain = ThemeDefinition(name: "Plain", kitty: "everforest.conf", nvim: "everforest")
        _ = ThemeApplier.apply(plain, config: glassy)
        let again = try String(contentsOf: config.kittyConfig.url, encoding: .utf8)
        XCTAssertTrue(again.contains("background_opacity 0.7"))
        XCTAssertTrue(again.contains("background_blur 40"))
        XCTAssertFalse(again.contains("0.45"), "glass is replaced, not duplicated")
    }

    func testBordersFollowTheTheme() throws {
        let (dir, config) = try makeFixture()
        defer { try? FileManager.default.removeItem(at: dir) }
        try """
        #!/bin/bash
        options=(
        \tstyle=round
        \tactive_color=0xffebbcba
        \tinactive_color=0x4426233a
        )
        borders "${options[@]}"
        """.write(to: config.bordersConfig.url, atomically: true, encoding: .utf8)

        let theme = ThemeDefinition(name: "Neon", accent: "#FF2E97", highlight: "#00F0FF", dim: "#7A5C99")
        let result = ThemeApplier.apply(theme, config: config)
        XCTAssertTrue(result.changed.contains("borders"))
        let rc = try String(contentsOf: config.bordersConfig.url, encoding: .utf8)
        XCTAssertTrue(rc.contains("\tactive_color=\"gradient(top_left=0xffff2e97,bottom_right=0xff00f0ff)\""))
        XCTAssertTrue(rc.contains("\tinactive_color=0x447a5c99"))
        XCTAssertTrue(rc.contains("\tstyle=round"), "other lines untouched")
    }
}

private extension String {
    var url: URL { URL(fileURLWithPath: self) }
}
