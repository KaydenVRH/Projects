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
        // Out of the right edge, like the other menus: flush against the frame,
        // with the same concave fillets where it meets it. It starts just under
        // the bar so it does not cover it.
        let height: CGFloat = item.hasBody ? 92 : 72
        let top = margin + CGFloat(bar.height) + 8

        let panel = OverlayPanel(
            screen: screen,
            size: NSSize(width: 384, height: height),
            edge: .trailing,
            margin: margin,
            vertical: .leading,
            verticalMargin: top,
            appearance: bar,
            style: viewModel.panelStyle(for: .trailing),
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
    /// Drives the icon's pop as the card lands.
    @State private var arrived = false

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            NotificationIconTile(tint: tint, size: 38, neon: neon) { icon }
            .scaleEffect(arrived ? 1 : 0.4)
            .rotationEffect(.degrees(arrived ? 0 : -25))

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    if !neon {
                        Circle()
                            .fill(tint)
                            .frame(width: 6, height: 6)
                    }
                    NotificationAppLabel(app: item.app, tint: tint, dim: dim, font: bar.fontFamily, neon: neon)
                    Text(NotificationTime.compact(item.date))
                        .font(.custom(bar.fontFamily, size: 10.5))
                        .foregroundColor(dim.opacity(0.75))
                    Spacer(minLength: 4)
                }

                NotificationHeadline(text: item.headline, size: 13.5, foreground: foreground,
                                     font: bar.fontFamily, neon: neon)
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
                .pressBounce()
                .transition(.scale.combined(with: .opacity))
            }
        }
        .padding(.leading, 18)
        .padding(.trailing, 14)
        .padding(.vertical, 12)
        .overlay(alignment: .leading) {
            // The tone as a lit edge, the same as in the centre.
            NotificationToneEdge(tint: tint, neon: neon)
                .padding(.vertical, 14)
                .padding(.leading, 7)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .modifier(PanelSurface(
            viewModel: viewModel,
            edge: .trailing,
            fallback: SheetShape(radius: 22, corners: .leading)
        ))
        .contentShape(Rectangle())
        .onTapGesture(perform: onOpen)
        .onHover { inside in
            withAnimation(.kHover) { hovering = inside }
            onHover(inside)
        }
        .onAppear {
            withAnimation(.kBouncy.delay(0.12)) { arrived = true }
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

    private var neon: Bool { Neon.isNeon(theme) }

    private var tint: Color { NotificationLook.tint(item.tone, accent: accent, neon: neon) }
}
