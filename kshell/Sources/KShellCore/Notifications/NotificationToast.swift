import AppKit
import SwiftUI

/// The card that drops out from under the bar when something arrives.
///
/// It borrows the launchers' outline and glass — the same seed-and-grow motion,
/// stepped rather than animated, so the material keeps up — but it never takes
/// the keyboard: a notification that stole focus mid-sentence would be worse than
/// the system's own banner. Hovering pauses the countdown, clicking opens the
/// centre, and cards queue rather than pile up, so a burst arrives one at a time.
final class NotificationToast {
    private let viewModel: BarViewModel
    private var margin: CGFloat
    private var seconds: TimeInterval
    private let onOpen: () -> Void

    private var panel: OverlayPanel?
    private var queue: [KShellNotification] = []
    private var current: KShellNotification?
    private var hovering = false
    /// Bumped on every show so a timer from a previous card can't dismiss this one.
    private var generation = 0

    init(
        viewModel: BarViewModel,
        margin: CGFloat,
        seconds: TimeInterval = 6,
        onOpen: @escaping () -> Void
    ) {
        self.viewModel = viewModel
        self.margin = margin
        self.seconds = max(1, seconds)
        self.onOpen = onOpen
    }

    func update(margin: CGFloat, seconds: TimeInterval) {
        self.margin = margin
        self.seconds = max(1, seconds)
    }

    func present(_ item: KShellNotification) {
        guard current != nil else {
            show(item)
            return
        }
        queue.append(item)
        if queue.count > 3 { queue.removeFirst(queue.count - 3) }
    }

    /// Hide the current card and drop anything waiting — used when toasts are
    /// switched off, or the shell is stopping.
    func dismissNow() {
        queue.removeAll()
        generation += 1
        guard let panel else { return }
        self.panel = nil
        current = nil
        panel.hide(animated: false)
    }

    func hover(_ inside: Bool) {
        hovering = inside
        // Leaving restarts the countdown, so a card you read for a moment doesn't
        // vanish the instant the pointer moves away.
        if !inside, current != nil { scheduleDismiss(generation) }
    }

    // MARK: - one card at a time

    private func show(_ item: KShellNotification) {
        guard let screen = NSScreen.main ?? NSScreen.screens.first else { return }
        current = item
        hovering = false
        generation += 1
        let generation = self.generation

        let bar = viewModel.appearance
        let edge: OverlayEdge = bar.edge == .top ? .top : .bottom
        // Under a top bar, above a bottom one: either way, clear of it.
        let distance = margin + CGFloat(bar.height) + 8
        let height: CGFloat = item.hasBody ? 92 : 72

        let panel = OverlayPanel(
            screen: screen,
            size: NSSize(width: 384, height: height),
            edge: edge,
            margin: distance,
            align: .trailing,
            appearance: bar,
            style: viewModel.panelStyle(for: edge),
            glass: viewModel.makeGlass,
            takesKeyboard: false,
            content: AnyView(NotificationToastView(
                viewModel: viewModel,
                item: item,
                onHover: { [weak self] inside in self?.hover(inside) },
                onOpen: { [weak self] in
                    guard let self else { return }
                    self.hideCurrent()
                    self.onOpen()
                },
                onDismiss: { [weak self] in self?.hideCurrent() }
            ))
        )
        self.panel = panel
        panel.show(animated: true)
        scheduleDismiss(generation)
    }

    private func scheduleDismiss(_ generation: Int) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in
            guard let self, self.generation == generation, !self.hovering else { return }
            self.hideCurrent()
        }
    }

    private func hideCurrent() {
        guard let panel else { return }
        self.panel = nil
        current = nil
        generation += 1
        panel.hide(animated: true)

        // The next card comes in once this one has folded away.
        DispatchQueue.main.asyncAfter(deadline: .now() + OverlayPanel.hideDuration + 0.06) { [weak self] in
            guard let self, self.current == nil, !self.queue.isEmpty else { return }
            self.show(self.queue.removeFirst())
        }
    }
}

struct NotificationToastView: View {
    @ObservedObject var viewModel: BarViewModel
    let item: KShellNotification
    let onHover: (Bool) -> Void
    let onOpen: () -> Void
    let onDismiss: () -> Void

    @State private var hovering = false

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }

    var body: some View {
        HStack(alignment: .top, spacing: 11) {
            icon
                .frame(width: 28, height: 28)

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Circle()
                        .fill(tint)
                        .frame(width: 6, height: 6)
                    Text(item.app)
                        .font(.custom(bar.fontFamily, size: 10.5))
                        .foregroundColor(dim)
                    Text(NotificationTime.compact(item.date))
                        .font(.custom(bar.fontFamily, size: 10.5))
                        .foregroundColor(dim.opacity(0.75))
                    Spacer(minLength: 4)
                }

                Text(item.headline)
                    .font(.custom(bar.fontFamily, size: 13.5))
                    .foregroundColor(foreground)
                    .lineLimit(2)

                if item.hasBody {
                    Text(item.body)
                        .font(.custom(bar.fontFamily, size: 11.5))
                        .foregroundColor(dim)
                        .lineLimit(2)
                }
            }

            if hovering {
                Button(action: onDismiss) {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundColor(dim)
                        .padding(5)
                        .background(Circle().fill(Color.white.opacity(0.10)))
                }
                .buttonStyle(.plain)
                .transition(.opacity)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .contentShape(Rectangle())
        .onTapGesture(perform: onOpen)
        .onHover { inside in
            withAnimation(.easeOut(duration: 0.15)) { hovering = inside }
            onHover(inside)
        }
    }

    @ViewBuilder
    private var icon: some View {
        if let image = AppIdentity.icon(for: item.bundleID) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fit)
        } else {
            Image(systemName: "bell.fill")
                .font(.system(size: 15, weight: .medium))
                .foregroundColor(tint)
        }
    }

    private var tint: Color {
        switch item.tone {
        case .info: return accent
        case .success: return .green
        case .warning: return .orange
        case .error: return .red
        }
    }
}
