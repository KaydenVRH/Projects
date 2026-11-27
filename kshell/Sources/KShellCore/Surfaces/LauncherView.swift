import AppKit
import SwiftUI

/// One entry in a launcher overlay: an app (with its icon, resolved lazily) or a
/// script (with an SF Symbol).
struct LauncherItem: Identifiable {
    let id: String
    let name: String
    let detail: String?
    /// Resolved on demand — see `AppIconCache`.
    let iconPath: String?
    let symbol: String?
    /// What the search matches, when it's more than the name (a clipboard
    /// entry is found by its whole text).
    let search: String?

    init(id: String, name: String, detail: String? = nil, iconPath: String? = nil, symbol: String? = nil, search: String? = nil) {
        self.id = id
        self.name = name
        self.detail = detail
        self.iconPath = iconPath
        self.symbol = symbol
        self.search = search
    }
}

/// App icons, fetched once each.
///
/// `NSWorkspace.icon(forFile:)` reads and decodes from disk, so doing it for a
/// whole catalogue up front is what made the launcher feel slow to open. Only
/// the rows on screen ask for one, and they are kept.
final class AppIconCache {
    static let shared = AppIconCache()

    private var icons: [String: NSImage] = [:]
    private let lock = NSLock()

    func icon(for path: String) -> NSImage? {
        lock.lock()
        defer { lock.unlock() }
        if let cached = icons[path] { return cached }
        let image = NSWorkspace.shared.icon(forFile: path)
        icons[path] = image
        return image
    }
}

/// How a launcher lays its items out.
enum LauncherStyle {
    /// A vertical list of rows — icons beside names, walked with ↑/↓.
    case list
    /// A wrapping grid of icons above names.
    case grid
}

/// A themed search + item overlay, shared by the app and script launchers.
struct LauncherView: View {
    @ObservedObject var viewModel: BarViewModel
    let placeholder: String
    let style: LauncherStyle
    let items: [LauncherItem]
    let onSelect: (LauncherItem) -> Void
    let onClose: () -> Void
    /// Rows made from the query, above the matches (a calculation's answer).
    var leading: ((String) -> [LauncherItem])? = nil
    /// Rows offered when nothing matches (search the web for it).
    var fallback: ((String) -> [LauncherItem])? = nil

    @State private var query = ""
    /// The armed row, moved with the arrow keys like the other pickers.
    @State private var selected = 0
    @FocusState private var focused: Bool
    /// The armed highlight is one shape that glides from row to row.
    @Namespace private var highlight

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }

    private var results: [LauncherItem] {
        let found = query.isEmpty ? items : items.filter { ($0.search ?? $0.name).localizedCaseInsensitiveContains(query) }
        guard !query.isEmpty else { return found }
        let extra = leading?(query) ?? []
        return extra + (found.isEmpty ? (fallback?(query) ?? []) : found)
    }

    private var palette: KitPalette { KitPalette(theme: theme, bar: bar) }

    var body: some View {
        VStack(spacing: 0) {
            field
            switch style {
            case .list: list
            case .grid: grid
            }
            KitFooter(hints: [("↑↓", "move"), ("⏎", "open"), ("esc", "close")], palette: palette) {
                Text("\(results.count) of \(items.count)")
                    .font(.custom(bar.fontFamily, size: 10.5))
                    .foregroundColor(foreground.opacity(0.5))
                    .contentTransition(.numericText())
                    .animation(.kLayout, value: results.count)
            }
        }
        .modifier(PanelSurface(viewModel: viewModel, edge: .bottom, fallback: SheetShape(radius: 22)))
        .onExitCommand { onClose() }
        .onKeyPress(.downArrow) { step(1) }
        .onKeyPress(.upArrow) { step(-1) }
        .modifier(VimListKeys(
            move: step,
            jump: jump,
            search: { focused = true; return .handled },
            fieldFocused: focused
        ))
        .onChange(of: query) { _, _ in
            // A new filter is a new list: let its rows cascade in.
            Motion.reveal()
            withAnimation(.kSelect) { selected = 0 }
        }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
    }

    // MARK: - pieces

    /// The search box's outline: a capsule, or cut corners under the HUD.
    private var fieldShape: AnyShape {
        palette.hud ? AnyShape(ChamferShape(cut: 8)) : AnyShape(Capsule(style: .continuous))
    }

    /// The search box: a capsule that lights up while it has the keyboard, with
    /// a magnifier that wiggles on every keystroke.
    private var field: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 15, weight: .semibold))
                .foregroundColor(accent)
                .symbolEffect(.bounce, value: query.count)
            TextField(placeholder, text: $query)
                .textFieldStyle(.plain)
                .font(.custom(bar.fontFamily, size: 17))
                .foregroundColor(foreground)
                .focused($focused)
                .onSubmit { if let item = results[safe: selected] ?? results.first { onSelect(item) } }
            if !query.isEmpty {
                Button { query = "" } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 14))
                        .foregroundColor(foreground.opacity(0.45))
                }
                .pressBounce()
                .transition(.scale.combined(with: .opacity))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .background(
            fieldShape
                .fill(Color.white.opacity(focused ? 0.09 : 0.05))
                .overlay(
                    fieldShape
                        .stroke(accent.opacity(focused ? 0.6 : 0.15), lineWidth: 1.2)
                )
                .shadow(color: accent.opacity(focused ? 0.3 : 0), radius: 12)
        )
        .padding(.horizontal, 14)
        .padding(.top, 14)
        .padding(.bottom, 8)
        .animation(.kHover, value: focused)
        .animation(.kBouncy, value: query.isEmpty)
    }

    @ViewBuilder
    private var list: some View {
        if results.isEmpty {
            empty
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(Array(results.enumerated()), id: \.element.id) { index, item in
                            LauncherRow(
                                item: item,
                                theme: theme,
                                font: bar.fontFamily,
                                selected: index == selected,
                                highlight: highlight,
                                action: { onSelect(item) }
                            )
                            .id(item.id)
                            .cascade(index)
                        }
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                }
                // Keep the armed row in view as the arrows walk past the fold.
                .onChange(of: selected) { _, new in
                    guard results.indices.contains(new) else { return }
                    withAnimation(.kLayout) {
                        proxy.scrollTo(results[new].id, anchor: .center)
                    }
                }
            }
        }
    }

    private var grid: some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 104), spacing: 14)], spacing: 18) {
                ForEach(Array(results.enumerated()), id: \.element.id) { index, item in
                    LauncherItemView(
                        item: item,
                        theme: theme,
                        font: bar.fontFamily,
                        selected: index == selected,
                        highlight: highlight
                    ) {
                        onSelect(item)
                    }
                    .cascade(index, distance: 24)
                }
            }
            .padding(18)
        }
    }

    private var empty: some View {
        VStack(spacing: 6) {
            Image(systemName: "questionmark.circle")
                .font(.system(size: 26))
                .foregroundColor(foreground.opacity(0.4))
            Text("Nothing matches “\(query)”")
                .font(.custom(bar.fontFamily, size: 13))
                .foregroundColor(foreground.opacity(0.6))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// `g`/`G` — the ends of the list.
    private func jump(_ edge: VimEdge) -> KeyPress.Result {
        guard !results.isEmpty else { return .ignored }
        withAnimation(.kSelect) { selected = edge == .first ? 0 : results.count - 1 }
        return .handled
    }

    /// Move the armed row, clamped to the results.
    private func step(_ delta: Int) -> KeyPress.Result {
        guard !results.isEmpty else { return .ignored }
        let next = min(max(selected + delta, 0), results.count - 1)
        guard next != selected else { return .handled }
        withAnimation(.kSelect) { selected = next }
        return .handled
    }

    /// Claim keyboard focus once the panel is actually key (activation is async,
    /// so setting this only on `onAppear` is unreliable).
    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }
}

private extension Array {
    /// Nil instead of trapping when a list shrinks under an armed index.
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

// MARK: - rows

/// A list row: icon, name, and the armed highlight.
private struct LauncherRow: View {
    let item: LauncherItem
    let theme: Theme
    let font: String
    let selected: Bool
    let highlight: Namespace.ID
    let action: () -> Void

    /// Only a tint — the armed row belongs to the arrow keys. Letting hover arm
    /// it meant the rows sliding under a stationary pointer during the opening
    /// morph armed one after another, and the list ran away with them.
    @State private var hovered = false

    private var foreground: Color { theme.foreground?.color ?? .white }
    private var accent: Color { theme.accent?.color ?? .accentColor }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                icon
                VStack(alignment: .leading, spacing: 1) {
                    Text(item.name)
                        .font(.custom(font, size: 14))
                        .foregroundColor(foreground)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    if let detail = item.detail {
                        Text(detail)
                            .font(.custom(font, size: 10))
                            .foregroundColor(foreground.opacity(0.45))
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
            // The armed row nudges right, like it was pushed by the highlight.
            .offset(x: selected ? 4 : 0)
            .kitCard(
                selected: selected,
                hovered: hovered,
                palette: KitPalette(theme: theme, font: font),
                highlight: highlight,
                radius: 11
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .animation(.kSelect, value: selected)
    }

    @ViewBuilder
    private var icon: some View {
        if let path = item.iconPath, let image = AppIconCache.shared.icon(for: path) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .frame(width: 26, height: 26)
        } else {
            Image(systemName: item.symbol ?? "doc")
                .font(.system(size: 15, weight: .regular))
                .foregroundColor(accent)
                .frame(width: 26, height: 26)
        }
    }
}

/// The grid cell, still used by the script launcher.
private struct LauncherItemView: View {
    let item: LauncherItem
    let theme: Theme
    let font: String
    let selected: Bool
    let highlight: Namespace.ID
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                icon
                Text(item.name)
                    .font(.custom(font, size: 12))
                    .foregroundColor(theme.foreground?.color ?? .white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 120)
            }
            .padding(8)
            .background {
                ZStack {
                    RoundedRectangle(cornerRadius: 12)
                        .fill((theme.accent?.color ?? .white).opacity(hovered && !selected ? 0.12 : 0))
                    if selected {
                        RoundedRectangle(cornerRadius: 12)
                            .fill((theme.accent?.color ?? .white).opacity(0.20))
                            .overlay(
                                RoundedRectangle(cornerRadius: 12)
                                    .stroke((theme.accent?.color ?? .white).opacity(0.55), lineWidth: 1)
                            )
                            .matchedGeometryEffect(id: "armed", in: highlight)
                    }
                }
            }
        }
        .pressBounce()
        // Hovered icons hop up a little.
        .scaleEffect(hovered ? 1.06 : 1)
        .offset(y: hovered ? -3 : 0)
        .onHover { hovered = $0 }
        .animation(.kBouncy, value: hovered)
        .animation(.kSelect, value: selected)
    }

    @ViewBuilder
    private var icon: some View {
        if let path = item.iconPath, let image = AppIconCache.shared.icon(for: path) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .frame(width: 54, height: 54)
        } else {
            Image(systemName: item.symbol ?? "doc")
                .font(.system(size: 30, weight: .regular))
                .foregroundColor(theme.accent?.color ?? .white)
                .frame(width: 54, height: 54)
        }
    }
}
