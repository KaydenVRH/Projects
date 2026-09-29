import Foundation

/// Observable state for one rendered widget. Runtimes mutate this; SwiftUI
/// views observe it and re-render.
public final class WidgetModel: ObservableObject {
    @Published public var icon: String?
    @Published public var label: String
    @Published public var iconColor: RGBA?
    @Published public var labelColor: RGBA?
    @Published public var iconFont: String?
    @Published public var action: (() -> Void)?

    /// Horizontal breathing room contributed by this widget.
    public var paddingX: Double
    /// Icons and labels are separated by this gap.
    public var spacing: Double
    /// Spacers stretch to push siblings apart.
    public var flexible: Bool

    public let spec: WidgetSpec

    public init(
        spec: WidgetSpec,
        icon: String? = nil,
        label: String = "",
        flexible: Bool = false,
        paddingX: Double = 4,
        spacing: Double = 4
    ) {
        self.spec = spec
        self.icon = icon
        self.label = label
        self.flexible = flexible
        self.paddingX = paddingX
        self.spacing = spacing
    }

    public var isEmpty: Bool {
        (icon?.isEmpty ?? true) && label.isEmpty && !flexible
    }
}

/// A running widget instance: owns a model and drives updates.
public protocol WidgetRuntime: AnyObject {
    var model: WidgetModel { get }
    func start()
    func stop()
}

public extension WidgetRuntime {
    func stop() {}
}

/// Convenience base for widgets that need nothing on start/stop.
open class StaticWidgetRuntime: WidgetRuntime {
    public let model: WidgetModel

    public init(model: WidgetModel) {
        self.model = model
    }

    open func start() {}
    open func stop() {}
}
