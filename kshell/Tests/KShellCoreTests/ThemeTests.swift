import XCTest
@testable import KShellCore

final class ThemeTests: XCTestCase {
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
        XCTAssertEqual(result.changed, ["kitty", "nvim", "kshell"])

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
}

private extension String {
    var url: URL { URL(fileURLWithPath: self) }
}
