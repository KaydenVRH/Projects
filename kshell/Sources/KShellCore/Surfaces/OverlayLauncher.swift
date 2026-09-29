import AppKit
import SwiftUI

/// Manages a themed overlay panel that slides in from a screen edge. Used by
/// the app launcher, the script launcher, and future popups.
final class OverlayLauncher {
    private let viewModel: BarViewModel
    private let edge: OverlayEdge
    private let preferredSize: (NSScreen) -> NSSize
    private let makeContent: (@escaping () -> Void) -> AnyView

    private var panel: OverlayPanel?
    private var isVisible = false

    init(
        viewModel: BarViewModel,
        edge: OverlayEdge = .bottom,
        size: @escaping (NSScreen) -> NSSize,
        content: @escaping (@escaping () -> Void) -> AnyView
    ) {
        self.viewModel = viewModel
        self.edge = edge
        self.preferredSize = size
        self.makeContent = content
    }

    func toggle() { isVisible ? hide() : show() }

    func show() {
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
            content: makeContent { [weak self] in self?.hide() }
        )
        panel = overlay
        overlay.show(animated: true)
    }

    func hide() {
        guard isVisible else { return }
        isVisible = false
        let current = panel
        panel = nil
        current?.hide(animated: true)
    }
}
