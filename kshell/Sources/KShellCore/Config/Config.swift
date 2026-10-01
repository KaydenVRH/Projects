import Foundation
import TOMLKit

public enum BarEdge: String {
    case top, bottom
}

/// Resolved bar appearance.
public struct BarConfig {
    public var height: Double = 34
    public var edge: BarEdge = .top
    public var marginX: Double = 0
    public var marginY: Double = 0
    public var cornerRadius: Double = 0
    public var paddingX: Double = 10
    public var background: RGBA = RGBA(hex: "#00000066")!
    public var blur: Bool = false
    public var fontFamily: String = "Hack Nerd Font"
    public var fontSize: Double = 13
    public var iconFontFamily: String? = nil
    public var iconFontSize: Double? = nil
    /// Width of the display notch to keep clear of widgets. `nil` = auto-detect
    /// from the screen; 0 disables the reserved gap.
    public var notchWidth: Double? = nil
    /// `all` shows a bar on every display; `main` only the main display.
    public var display: String = "all"
    /// Hide the bar off the top edge until the pointer nears it.
    public var autohide: Bool = false
    /// Pixels left visible at the top edge while hidden.
    public var autohidePeek: Double = 2
    /// Distance from the top edge (points) that reveals the bar.
    public var autohideZone: Double = 6
    /// Seconds to wait after the pointer leaves before hiding.
    public var autohideDelay: Double = 0.4

    public init() {}
}

/// Resolved theme colors. Widgets may override per-instance.
public struct Theme {
    /// Name of the active theme (written by the theme switcher).
    public var name: String? = nil
    public var accent: RGBA? = RGBA(hex: "#d0d0d0")
    public var highlight: RGBA? = RGBA(hex: "#a0a0a0")
    public var foreground: RGBA? = RGBA(hex: "#e0e0e0")
    /// Muted color, for de-emphasised text.
    public var dim: RGBA? = nil
    /// Secondary accent (statusline insert mode, etc.).
    public var mid: RGBA? = nil
    public var background: RGBA? = nil

    public init() {}
}

/// One switchable theme: its palette plus the kitty/nvim/wallpaper to apply.
public struct ThemeDefinition {
    public var name: String
    /// Kitty theme file to `include` (relative to the kitty config directory).
    public var kitty: String?
    /// Neovim colorscheme name.
    public var nvim: String?
    /// Wallpaper path (image, or a video for the live-wallpaper engine).
    public var wallpaper: String?
    public var accent: String?
    public var highlight: String?
    public var foreground: String?
    public var dim: String?
    public var mid: String?
    public var background: String?
    /// Kitty `background_opacity` for this theme (falls back to the launcher).
    public var opacity: Double?
    /// Kitty `background_blur` for this theme (falls back to the launcher).
    public var blur: Double?

    public init(
        name: String,
        kitty: String? = nil,
        nvim: String? = nil,
        wallpaper: String? = nil,
        accent: String? = nil,
        highlight: String? = nil,
        foreground: String? = nil,
        dim: String? = nil,
        mid: String? = nil,
        background: String? = nil,
        opacity: Double? = nil,
        blur: Double? = nil
    ) {
        self.name = name
        self.kitty = kitty
        self.nvim = nvim
        self.wallpaper = wallpaper
        self.accent = accent
        self.highlight = highlight
        self.foreground = foreground
        self.dim = dim
        self.mid = mid
        self.background = background
        self.opacity = opacity
        self.blur = blur
    }

    /// `[theme]` entries for this theme, in config order.
    public var palette: [(key: String, value: String)] {
        [
            ("accent", accent),
            ("highlight", highlight),
            ("foreground", foreground),
            ("dim", dim),
            ("mid", mid),
            ("background", background),
        ].compactMap { key, value in value.map { (key, $0) } }
    }
}

/// The theme-switcher launcher: which files it rewrites and what it can pick.
public struct ThemeLauncherConfig {    public var kittyConfig: String = "~/dotfiles/kitty/.config/kitty/kitty.conf"
    public var nvimInit: String = "~/dotfiles/nvim/.config/nvim/init.lua"
    public var shellConfig: String = "~/.config/kshell/config.toml"
    /// Kitty `background_opacity` used by themes that don't set their own.
    public var opacity: Double?
    /// Kitty `background_blur` used by themes that don't set their own.
    public var blur: Double?
    public var themes: [ThemeDefinition] = []

    public init() {}
}

/// The thin rounded line drawn just inside the screen edges — the bar's glass
/// carried around the screen, in the same style as the media centre's surface.
/// It only spans the sides the bar doesn't cover, so it never crosses the bar.
public struct BorderConfig {
    public var enabled: Bool = true
    /// Distance from the screen's edge to the line's outer edge. Set this to the
    /// window manager's outer gap so the line sits in the padding.
    public var inset: Double = 5
    /// How thick the line is.
    public var thickness: Double = 3
    /// Corner radius the line turns with.
    public var radius: Double = 20
    /// The line's surface tint, like the bar's and media centre's background.
    /// Tokens such as `$background` work here.
    public var color: RGBA = RGBA(hex: "#1a1b26aa") ?? RGBA(hex: "#000000aa")!
    /// Frost the line with whatever is behind it, exactly like the bar.
    public var blur: Bool = true

    public init() {}
}

/// The music panel: where downloads land and how they are fetched.
public struct MusicConfig {
    /// Where downloaded tracks are kept.
    public var directory: String = "~/Music/kshell"
    /// How many search results to ask YouTube for.
    public var searchCount: Int = 15
    /// Audio container `yt-dlp` extracts to. m4a keeps AVFoundation happy.
    public var format: String = "m4a"
    public var ytdlp: String = "yt-dlp"

    public init() {}
}

/// The media centre: where termusic's helper script lives and where the panel
/// hangs from the bar.
public struct MediaConfig {
    public var termusic: String = "~/programs/projects/kshell/scripts/termusic/termusic.sh"
    /// Where the panel lines up along the bar: `leading`, `center` or `trailing`.
    public var align: OverlayAlign = .leading

    public init() {}
}

/// A single widget declaration. Kept as a raw TOML table plus a `type` so the
/// registry can interpret it and new widget types stay cheap to add.
public struct WidgetSpec {
    public let type: String
    public let raw: TOMLTable

    public init(type: String, raw: TOMLTable) {
        self.type = type
        self.raw = raw
    }

    public func string(_ key: String) -> String? { raw.string(key) }
    public func double(_ key: String) -> Double? { raw.double(key) }
    public func number(_ key: String) -> Double? { raw.number(key) }
    public func bool(_ key: String) -> Bool? { raw.bool(key) }
    public func int(_ key: String) -> Int? { raw.int(key) }
    public func color(_ key: String) -> RGBA? { RGBA.parse(raw.string(key)) }
}

/// The whole shell configuration.
public struct ShellConfig {
    public var bar: BarConfig
    public var theme: Theme
    public var left: [WidgetSpec]
    public var center: [WidgetSpec]
    public var right: [WidgetSpec]
    /// Directory the script launcher lists (`.sh` files).
    public var scriptDirectory: String
    /// Directories the wallpaper launcher scans.
    public var wallpaperDirectories: [String]
    /// The theme-switcher launcher.
    public var themeLauncher: ThemeLauncherConfig
    /// The media centre.
    public var media: MediaConfig
    /// The line drawn inside the screen edges.
    public var border: BorderConfig
    /// The music panel.
    public var music: MusicConfig

    public init(
        bar: BarConfig = BarConfig(),
        theme: Theme = Theme(),
        left: [WidgetSpec] = [],
        center: [WidgetSpec] = [],
        right: [WidgetSpec] = [],
        scriptDirectory: String = "~/dotfiles/scripts",
        wallpaperDirectories: [String] = [
            "~/dotfiles/wallpapers",
            "~/wallpapers",
            "~/dotfiles/live-wallpapers",
        ],
        themeLauncher: ThemeLauncherConfig = ThemeLauncherConfig(),
        media: MediaConfig = MediaConfig(),
        border: BorderConfig = BorderConfig(),
        music: MusicConfig = MusicConfig()
    ) {
        self.bar = bar
        self.theme = theme
        self.left = left
        self.center = center
        self.right = right
        self.scriptDirectory = scriptDirectory
        self.wallpaperDirectories = wallpaperDirectories
        self.themeLauncher = themeLauncher
        self.media = media
        self.border = border
        self.music = music
    }

    public static func parse(toml text: String) throws -> ShellConfig {
        let root = try TOMLTable(string: text)

        // Colors anywhere in the config may be written as `$name` referring to
        // the `[theme]` section, so install the tokens before parsing anything.
        ThemeTokens.install(from: root.table("theme"))

        var bar = BarConfig()
        if let t = root.table("bar") {
            if let v = t.number("height") { bar.height = v }
            if let v = t.string("edge"), let e = BarEdge(rawValue: v) { bar.edge = e }
            if let v = t.number("margin_x") { bar.marginX = v }
            if let v = t.number("margin_y") { bar.marginY = v }
            if let v = t.number("corner_radius") { bar.cornerRadius = v }
            if let v = t.number("padding_x") { bar.paddingX = v }
            if let v = RGBA.parse(t.string("background")) { bar.background = v }
            if let v = t.bool("blur") { bar.blur = v }
            if let v = t.string("font") { bar.fontFamily = v }
            if let v = t.number("font_size") { bar.fontSize = v }
            if let v = t.string("icon_font") { bar.iconFontFamily = v }
            if let v = t.number("icon_font_size") { bar.iconFontSize = v }
            if let v = t.number("notch_width") { bar.notchWidth = v }
            if let v = t.string("display") { bar.display = v.lowercased() }
            if let v = t.bool("autohide") { bar.autohide = v }
            if let v = t.number("autohide_peek") { bar.autohidePeek = v }
            if let v = t.number("autohide_zone") { bar.autohideZone = v }
            if let v = t.number("autohide_delay") { bar.autohideDelay = v }
        }

        var theme = Theme()
        let themeTable = root.table("theme")
        if let t = themeTable {
            if let v = t.string("name") { theme.name = v }
            if let v = RGBA.parse(t.string("accent")) { theme.accent = v }
            if let v = RGBA.parse(t.string("highlight")) { theme.highlight = v }
            if let v = RGBA.parse(t.string("foreground")) { theme.foreground = v }
            if let v = RGBA.parse(t.string("dim")) { theme.dim = v }
            if let v = RGBA.parse(t.string("mid")) { theme.mid = v }
            if let v = RGBA.parse(t.string("background")) { theme.background = v }
        }

        let barTable = root.table("bar")
        var scriptDirectory = "~/dotfiles/scripts"
        if let launcher = root.table("script_launcher"), let directory = launcher.string("directory") {
            scriptDirectory = directory
        }
        var wallpaperDirectories = [
            "~/dotfiles/wallpapers",
            "~/wallpapers",
            "~/dotfiles/live-wallpapers",
        ]
        if let launcher = root.table("wallpaper_launcher"), let array = launcher.array("directories") {
            let directories = array.compactMap { $0.string }.filter { !$0.isEmpty }
            if !directories.isEmpty { wallpaperDirectories = directories }
        }
        var themeLauncher = ThemeLauncherConfig()
        if let launcher = root.table("theme_launcher") {
            if let v = launcher.string("kitty_config") { themeLauncher.kittyConfig = v }
            if let v = launcher.string("nvim_init") { themeLauncher.nvimInit = v }
            if let v = launcher.string("shell_config") { themeLauncher.shellConfig = v }
            if let v = launcher.number("opacity") { themeLauncher.opacity = v }
            if let v = launcher.number("blur") { themeLauncher.blur = v }
            if let array = launcher.array("themes") {
                themeLauncher.themes = array.compactMap { element in
                    guard let table = element.table, let name = table.string("name") else { return nil }
                    return ThemeDefinition(
                        name: name,
                        kitty: table.string("kitty"),
                        nvim: table.string("nvim"),
                        wallpaper: table.string("wallpaper"),
                        accent: table.string("accent"),
                        highlight: table.string("highlight"),
                        foreground: table.string("foreground"),
                        dim: table.string("dim"),
                        mid: table.string("mid"),
                        background: table.string("background"),
                        opacity: table.number("opacity"),
                        blur: table.number("blur")
                    )
                }
            }
        }
        var media = MediaConfig()
        if let table = root.table("media") {
            if let v = table.string("termusic") { media.termusic = v }
            if let v = table.string("align") {
                switch v.lowercased() {
                case "leading", "left": media.align = .leading
                case "trailing", "right": media.align = .trailing
                default: media.align = .center
                }
            }
        }
        var music = MusicConfig()
        if let table = root.table("music") {
            if let v = table.string("directory") { music.directory = v }
            if let v = table.number("search_results") { music.searchCount = Int(v) }
            if let v = table.string("format") { music.format = v }
            if let v = table.string("ytdlp") { music.ytdlp = v }
        }
        var border = BorderConfig()
        if let table = root.table("border") {
            if let v = table.bool("enabled") { border.enabled = v }
            if let v = table.number("inset") { border.inset = v }
            if let v = table.number("thickness") { border.thickness = v }
            if let v = table.number("radius") { border.radius = v }
            if let v = table.string("color"), let parsed = RGBA.parse(v) { border.color = parsed }
            if let v = table.bool("blur") { border.blur = v }
        }
        return ShellConfig(
            bar: bar,
            theme: theme,
            left: parseWidgets(barTable?.array("left")),
            center: parseWidgets(barTable?.array("center")),
            right: parseWidgets(barTable?.array("right")),
            scriptDirectory: scriptDirectory,
            wallpaperDirectories: wallpaperDirectories,
            themeLauncher: themeLauncher,
            media: media,
            border: border,
            music: music
        )
    }

    private static func parseWidgets(_ array: TOMLArray?) -> [WidgetSpec] {
        guard let array else { return [] }
        var specs: [WidgetSpec] = []
        for element in array {
            guard let table = element.table, let type = table.string("type") else { continue }
            specs.append(WidgetSpec(type: type, raw: table))
        }
        return specs
    }
}
