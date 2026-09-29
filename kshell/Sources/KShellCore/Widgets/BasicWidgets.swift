import Foundation

/// A flexible gap that pushes its siblings apart.
final class SpacerRuntime: StaticWidgetRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        super.init(model: WidgetModel(spec: spec, flexible: true, paddingX: 0))
    }
}

/// Static icon + text.
final class TextRuntime: StaticWidgetRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        let model = WidgetFactory.baseModel(
            spec,
            theme: theme,
            icon: spec.string("icon"),
            label: spec.string("text") ?? spec.string("label") ?? ""
        )
        super.init(model: model)
    }
}

/// A small separator glyph (defaults to a Nerd Font chevron).
final class ChevronRuntime: StaticWidgetRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        let model = WidgetFactory.baseModel(
            spec,
            theme: theme,
            icon: WidgetFactory.resolvedIcon(spec) ?? "\u{ea9c}",
            label: ""
        )
        super.init(model: model)
    }
}

/// The Apple logo. Clicking it toggles the app launcher (unless an `action`
/// is configured).
final class AppleRuntime: StaticWidgetRuntime {
    init(spec: WidgetSpec, theme: Theme) {
        let model = WidgetFactory.baseModel(
            spec,
            theme: theme,
            icon: WidgetFactory.resolvedIcon(spec) ?? "\u{f8ff}",
            label: ""
        )
        model.iconFont = spec.string("icon_font") ?? "SF Pro Display"
        if let action = spec.string("action"), !action.isEmpty {
            model.action = { Shell.run(action) }
        } else {
            model.action = { EventBus.post("app_launcher_toggle") }
        }
        super.init(model: model)
    }
}
