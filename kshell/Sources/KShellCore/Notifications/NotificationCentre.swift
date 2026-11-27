import AppKit
import SwiftUI

/// How notification dates read in a list.
enum NotificationTime {
    private static let clock: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter
    }()

    private static let day: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "d MMM"
        return formatter
    }()

    /// "now", "4m", "3h", "2d", then the date.
    static func compact(_ date: Date, now: Date = Date()) -> String {
        let seconds = now.timeIntervalSince(date)
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m" }
        if seconds < 86_400 { return "\(Int(seconds / 3600))h" }
        let days = Int(seconds / 86_400)
        if days < 7 { return "\(days)d" }
        return day.string(from: date)
    }

    static func clock(_ date: Date) -> String { clock.string(from: date) }

    /// "Today", "Yesterday", or the date.
    static func section(_ date: Date, now: Date = Date()) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInYesterday(date) { return "Yesterday" }
        return day.string(from: date)
    }
}

/// The notification centre: a right-edge sidebar listing what has arrived.
///
/// It is the same kind of panel as the theme launcher — it grows out of the
/// screen edge, borrows the frame's glass and takes the keyboard — because a
/// centre you walk with `j`/`k` is a launcher with different rows.
final class NotificationCentre {
    /// The config is read when the panel is built, and a reload can change it
    /// while the panel is closed — keep the live copy in a box the view can see.
    private final class ConfigBox {
        var config: NotificationsConfig
        init(_ config: NotificationsConfig) { self.config = config }
    }

    private let viewModel: BarViewModel
    private let store: NotificationStore
    private let box: ConfigBox
    private let overlay: OverlayLauncher

    init(viewModel: BarViewModel, config: NotificationsConfig, margin: CGFloat) {
        self.viewModel = viewModel
        self.store = .shared
        let store = self.store
        let box = ConfigBox(config)
        self.box = box
        overlay = OverlayLauncher(
            viewModel: viewModel,
            edge: .trailing,
            margin: margin,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.33, 440),
                    height: min(screen.frame.height * 0.80, 760)
                )
            },
            content: { onClose in
                AnyView(NotificationCentreView(
                    viewModel: viewModel,
                    store: store,
                    isMirroring: { box.config.mirrorMacOS },
                    onClose: {
                        store.markAllRead()
                        onClose()
                    }
                ))
            }
        )
    }

    func update(config: NotificationsConfig, margin: CGFloat) {
        box.config = config
        overlay.setMargin(margin)
    }

    func toggle() { overlay.toggle() }
    func show(animated: Bool = true) { overlay.show(animated: animated) }
    func hide(animated: Bool = true) { overlay.hide(animated: animated) }
    var visible: Bool { overlay.visible }
}

struct NotificationCentreView: View {
    @ObservedObject var viewModel: BarViewModel
    @ObservedObject var store: NotificationStore
    let isMirroring: () -> Bool
    let onClose: () -> Void

    @State private var selected = 0
    @State private var hovering: String?
    @FocusState private var focused: Bool
    @Namespace private var highlight

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }
    private var palette: KitPalette { KitPalette(theme: theme, bar: bar) }
    private var sheet: SheetShape { SheetShape(radius: 22, corners: .leading) }
    private var items: [KShellNotification] { store.items }

    var body: some View {
        VStack(spacing: 0) {
            header
            if items.isEmpty {
                empty
            } else {
                list
            }
            footer
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .modifier(PanelSurface(viewModel: viewModel, edge: .trailing, fallback: sheet))
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .modifier(VimListKeys(move: vimMove, jump: jump))
        .onKeyPress(.upArrow) { vimMove(-1) }
        .onKeyPress(.downArrow) { vimMove(1) }
        .onKeyPress(.return) { openSelected(); return .handled }
        .onKeyPress("x", phases: .down) { _ in dismissSelected(); return .handled }
        .onKeyPress("d", phases: .down) { _ in dismissSelected(); return .handled }
        .onKeyPress("C", phases: .down) { _ in
            store.clear()
            selected = 0
            return .handled
        }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
        .onChange(of: items.count) { _, _ in
            selected = min(selected, max(0, items.count - 1))
        }
    }

    // MARK: - chrome

    private var header: some View {
        KitHeader(
            icon: "bell.fill",
            title: "Notifications",
            subtitle: items.isEmpty ? "All caught up" : "\(items.count) \(items.count == 1 ? "notification" : "notifications")",
            palette: palette
        ) {
            HStack(spacing: 8) {
                if store.unread > 0 {
                    Text("\(store.unread) new")
                        .font(.custom(bar.fontFamily, size: 10.5))
                        .foregroundColor(.black.opacity(0.78))
                        .contentTransition(.numericText())
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .background(Capsule().fill(accent))
                        .transition(.scale.combined(with: .opacity))
                }
                Button {
                    withAnimation(.kLayout) {
                        store.clear()
                        selected = 0
                    }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "trash")
                        Text("Clear")
                    }
                    .font(.custom(bar.fontFamily, size: 11))
                    .foregroundColor(foreground.opacity(0.8))
                    .padding(.horizontal, 9)
                    .padding(.vertical, 4)
                    .background(Capsule().fill(Color.white.opacity(0.08)))
                }
                .pressBounce()
                .opacity(items.isEmpty ? 0.35 : 1)
                .disabled(items.isEmpty)
            }
            .animation(.kBouncy, value: store.unread)
        }
    }

    private var footer: some View {
        KitFooter(
            hints: [("⏎", "open"), ("x", "dismiss"), ("C", "clear")],
            palette: palette
        ) { EmptyView() }
    }

    private var empty: some View {
        KitEmpty(
            icon: "bell.slash",
            title: "Nothing here yet",
            detail: isMirroring()
                ? "Notifications from macOS appear here as they arrive."
                : "Post one with  kshell notify \"Title\" \"Body\"",
            palette: palette
        )
    }

    // MARK: - the list

    private var list: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(alignment: .leading, spacing: 8, pinnedViews: [.sectionHeaders]) {
                    ForEach(sections, id: \.title) { section in
                        Section {
                            ForEach(section.items) { item in
                                row(item, index: index(of: item), selected: index(of: item) == selected)
                                    .cascade(index(of: item), from: .trailing)
                                    .transition(.asymmetric(
                                        insertion: .move(edge: .top).combined(with: .opacity),
                                        removal: .move(edge: .trailing).combined(with: .opacity).combined(with: .scale(scale: 0.8))
                                    ))
                            }
                        } header: {
                            Text(section.title.uppercased())
                                .font(.system(size: 9.5, weight: .bold))
                                .tracking(1.2)
                                .foregroundColor(accent.opacity(0.85))
                                .textCase(nil)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 5)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(bar.background.color.opacity(0.92))
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .scrollTargetLayout()
            }
            .scrollTargetBehavior(.viewAligned)
            .frame(maxHeight: .infinity)
            .onChange(of: selected) { _, newValue in
                guard items.indices.contains(newValue) else { return }
                withAnimation(.kLayout) {
                    proxy.scrollTo(items[newValue].id, anchor: .center)
                }
            }
        }
    }

    private func row(_ item: KShellNotification, index: Int, selected: Bool) -> some View {
        HStack(alignment: .top, spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(tint(item).opacity(0.16))
                icon(for: item)
                    .frame(width: 24, height: 24)
            }
            .frame(width: 36, height: 36)

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 7) {
                    Text(item.app)
                        .font(.custom(bar.fontFamily, size: 10.5))
                        .foregroundColor(dim)
                    Spacer(minLength: 6)
                    Text(NotificationTime.clock(item.date))
                        .font(.custom(bar.fontFamily, size: 10.5))
                        .foregroundColor(dim.opacity(0.75))
                }

                Text(item.headline)
                    .font(.custom(bar.fontFamily, size: 13))
                    .foregroundColor(foreground)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)

                if item.hasBody {
                    Text(item.body)
                        .font(.custom(bar.fontFamily, size: 11.5))
                        .foregroundColor(dim)
                        .lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            if selected || hovering == item.id {
                Button(action: { withAnimation(.kLayout) { store.dismiss(item.id) } }) {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundColor(dim)
                        .padding(5)
                        .background(Circle().fill(Color.white.opacity(0.10)))
                }
                .pressBounce()
                .help("Dismiss")
                .transition(.scale.combined(with: .opacity))
            }
        }
        .padding(.leading, 14)
        .padding(.trailing, 10)
        .padding(.vertical, 10)
        .overlay(alignment: .leading) {
            // The card's tone, as a lit edge.
            Capsule()
                .fill(tint(item))
                .frame(width: 3)
                .padding(.vertical, 10)
                .padding(.leading, 5)
                .shadow(color: tint(item).opacity(0.6), radius: 4)
                .opacity(item.source == .kshell ? 1 : 0.35)
        }
        .kitCard(selected: selected, hovered: hovering == item.id, palette: palette, highlight: highlight)
        .animation(.kHover, value: hovering)
        .contentShape(Rectangle())
        .onTapGesture {
            self.selected = index
            open(item)
        }
        .onHover { inside in
            hovering = inside ? item.id : (hovering == item.id ? nil : hovering)
        }
        .id(item.id)
    }

    @ViewBuilder
    private func icon(for item: KShellNotification) -> some View {
        if let image = AppIdentity.icon(for: item.bundleID) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fit)
        } else {
            Image(systemName: "bell.fill")
                .font(.system(size: 13, weight: .medium))
                .foregroundColor(tint(item))
        }
    }

    private func tint(_ item: KShellNotification) -> Color {
        switch item.tone {
        case .info: return accent
        case .success: return .green
        case .warning: return .orange
        case .error: return .red
        }
    }

    // MARK: - grouping

    private var sections: [(title: String, items: [KShellNotification])] {
        var order: [String] = []
        var buckets: [String: [KShellNotification]] = [:]
        for item in items {
            let key = NotificationTime.section(item.date)
            if buckets[key] == nil { order.append(key) }
            buckets[key, default: []].append(item)
        }
        return order.map { ($0, buckets[$0] ?? []) }
    }

    private func index(of item: KShellNotification) -> Int {
        items.firstIndex { $0.id == item.id } ?? 0
    }

    // MARK: - keys

    private func vimMove(_ delta: Int) -> KeyPress.Result {
        move(delta)
        return .handled
    }

    private func jump(_ edge: VimEdge) -> KeyPress.Result {
        guard !items.isEmpty else { return .ignored }
        selected = edge == .first ? 0 : items.count - 1
        return .handled
    }

    private func move(_ delta: Int) {
        guard !items.isEmpty else { return }
        withAnimation(.kSelect) { selected = max(0, min(items.count - 1, selected + delta)) }
    }

    private func openSelected() {
        guard items.indices.contains(selected) else { return }
        open(items[selected])
    }

    private func open(_ item: KShellNotification) {
        NotificationAction.open(item)
        store.dismiss(item.id)
        onClose()
    }

    private func dismissSelected() {
        guard items.indices.contains(selected) else { return }
        let item = items[selected]
        withAnimation(.kLayout) { store.dismiss(item.id) }
        selected = min(selected, max(0, items.count - 1))
    }

    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }
}
