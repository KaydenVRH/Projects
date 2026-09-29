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
    public var accent: RGBA? = RGBA(hex: "#d0d0d0")
    public var highlight: RGBA? = RGBA(hex: "#a0a0a0")
    public var foreground: RGBA? = RGBA(hex: "#e0e0e0")
    public var background: RGBA? = nil

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

    public init(
        bar: BarConfig = BarConfig(),
        theme: Theme = Theme(),
        left: [WidgetSpec] = [],
        center: [WidgetSpec] = [],
        right: [WidgetSpec] = []
    ) {
        self.bar = bar
        self.theme = theme
        self.left = left
        self.center = center
        self.right = right
    }

    public static func parse(toml text: String) throws -> ShellConfig {
        let root = try TOMLTable(string: text)

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
        if let t = root.table("theme") {
            if let v = RGBA.parse(t.string("accent")) { theme.accent = v }
            if let v = RGBA.parse(t.string("highlight")) { theme.highlight = v }
            if let v = RGBA.parse(t.string("foreground")) { theme.foreground = v }
            if let v = RGBA.parse(t.string("background")) { theme.background = v }
        }

        let barTable = root.table("bar")
        return ShellConfig(
            bar: bar,
            theme: theme,
            left: parseWidgets(barTable?.array("left")),
            center: parseWidgets(barTable?.array("center")),
            right: parseWidgets(barTable?.array("right"))
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
