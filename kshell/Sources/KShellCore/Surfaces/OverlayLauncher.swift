import AppKit
import SwiftUI

/// Manages a themed overlay panel that slides in from a screen edge. Used by
/// the app launcher, the script launcher, and future popups.
final class OverlayLauncher {
    private let viewModel: BarViewModel
    private let edge: OverlayEdge
    private let align: OverlayAlign
    private let margin: CGFloat
    private let preferredSize: (NSScreen) -> NSSize
    private let makeContent: (@escaping () -> Void) -> AnyView

    private var panel: OverlayPanel?
    private var isVisible = false

    init(
        viewModel: BarViewModel,
        edge: OverlayEdge = .bottom,
        align: OverlayAlign = .center,
        margin: CGFloat = 0,
        size: @escaping (NSScreen) -> NSSize,
        content: @escaping (@escaping () -> Void) -> AnyView
    ) {
        self.viewModel = viewModel
        self.edge = edge
        self.align = align
        self.margin = margin
        self.preferredSize = size
        self.makeContent = content
    }

    func toggle() { visible ? hide() : show() }

    var visible: Bool { isVisible }

    func show(animated: Bool = true) {
        guard !isVisible else { return }
        let mouse = NSEvent.mouseLocation
        guard let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) })
            ?? NSScreen.main
            ?? NSScreen.screens.first else { return }

        isVisible = true
        let overlay = OverlayPanel(
            screen: screen,
            size: preferredSize(screen),
            edge: edge,
            margin: margin,
            align: align,
            appearance: viewModel.appearance,
            style: viewModel.panelStyle(for: edge),
            glass: viewModel.makeGlass,
            content: makeContent { [weak self] in self?.hide() }
        )
        panel = overlay
        overlay.show(animated: animated)
    }

    func hide(animated: Bool = true) {
        guard isVisible else { return }
        isVisible = false
        let current = panel
        panel = nil
        current?.hide(animated: animated)
    }
}
