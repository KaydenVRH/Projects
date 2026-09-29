import Foundation

/// Maps a config `type` to runtime instance(s). Add new widget types here.
enum WidgetFactory {
    static func make(spec: WidgetSpec, theme: Theme, bar: BarConfig) -> [WidgetRuntime] {
        switch spec.type {
        case "spacer":     return [SpacerRuntime(spec: spec, theme: theme)]
        case "text":       return [TextRuntime(spec: spec, theme: theme)]
        case "chevron":    return [ChevronRuntime(spec: spec, theme: theme)]
        case "apple":      return [AppleRuntime(spec: spec, theme: theme)]
        case "clock":      return [ClockRuntime(spec: spec, theme: theme)]
        case "script":     return [ScriptRuntime(spec: spec, theme: theme)]
        case "front_app":  return [FrontAppRuntime(spec: spec, theme: theme)]
        case "cpu":        return [CPURuntime(spec: spec, theme: theme)]
        case "ram":        return [RAMRuntime(spec: spec, theme: theme)]
        case "battery":    return [BatteryRuntime(spec: spec, theme: theme)]
        case "wifi":       return [WifiRuntime(spec: spec, theme: theme)]
        case "volume":     return [VolumeRuntime(spec: spec, theme: theme)]
        case "bluetooth":  return [BluetoothRuntime(spec: spec, theme: theme)]
        case "workspaces": return WorkspacesWidget.make(spec: spec, theme: theme)
        case "js":
            if let runtime = JSWidgetRuntime(spec: spec, theme: theme) { return [runtime] }
            return []
        default:
            FileHandle.standardError.write(Data("kshell: unknown widget type '\(spec.type)'\n".utf8))
            return []
        }
    }

    /// Resolve a widget's icon from either a literal `icon` string or an
    /// ASCII-safe `icon_hex` codepoint (e.g. "f8ff" for the Apple logo).
    static func resolvedIcon(_ spec: WidgetSpec) -> String? {
        if let literal = spec.string("icon"), !literal.isEmpty { return literal }
        if let hex = spec.string("icon_hex"), let value = UInt32(hex, radix: 16),
           let scalar = UnicodeScalar(value) {
            return String(Character(scalar))
        }
        return nil
    }

    static func baseModel(
        _ spec: WidgetSpec,
        theme: Theme,
        icon: String? = nil,
        label: String = "",
        flexible: Bool = false,
        paddingX: Double? = nil,
        spacing: Double? = nil
    ) -> WidgetModel {
        let model = WidgetModel(
            spec: spec,
            icon: icon ?? resolvedIcon(spec),
            label: label,
            flexible: flexible,
            paddingX: paddingX ?? spec.number("padding") ?? 6,
            spacing: spacing ?? spec.number("spacing") ?? 10
        )
        model.iconColor = spec.color("icon_color") ?? spec.color("color") ?? theme.accent
        model.labelColor = spec.color("label_color") ?? spec.color("color") ?? theme.highlight
        model.iconFont = spec.string("icon_font")
        return model
    }
}
