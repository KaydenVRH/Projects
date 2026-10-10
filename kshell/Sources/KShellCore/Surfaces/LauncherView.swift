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

    init(id: String, name: String, detail: String? = nil, iconPath: String? = nil, symbol: String? = nil) {
        self.id = id
        self.name = name
        self.detail = detail
        self.iconPath = iconPath
        self.symbol = symbol
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

    @State private var query = ""
    /// The armed row, moved with the arrow keys like the other pickers.
    @State private var selected = 0
    @FocusState private var focused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }

    private var results: [LauncherItem] {
        query.isEmpty ? items : items.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        VStack(spacing: 0) {
            field
            Divider().overlay(accent.opacity(0.25))
            switch style {
            case .list: list
            case .grid: grid
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
        .onChange(of: query) { _, _ in selected = 0 }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
    }

    // MARK: - pieces

    private var field: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .foregroundColor(accent.opacity(0.8))
            TextField(placeholder, text: $query)
                .textFieldStyle(.plain)
                .font(.custom(bar.fontFamily, size: 18))
                .foregroundColor(foreground)
                .focused($focused)
                .onSubmit { if let item = results[safe: selected] ?? results.first { onSelect(item) } }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
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
                                action: { onSelect(item) }
                            )
                            .id(item.id)
                        }
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                }
                // Keep the armed row in view as the arrows walk past the fold.
                .onChange(of: selected) { _, new in
                    guard results.indices.contains(new) else { return }
                    withAnimation(.snappy(duration: 0.22, extraBounce: 0.1)) {
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
                        selected: index == selected
                    ) {
                        onSelect(item)
                    }
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
        selected = edge == .first ? 0 : results.count - 1
        return .handled
    }

    /// Move the armed row, clamped to the results.
    private func step(_ delta: Int) -> KeyPress.Result {
        guard !results.isEmpty else { return .ignored }
        let next = min(max(selected + delta, 0), results.count - 1)
        guard next != selected else { return .handled }
        selected = next
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
            .background(
                RoundedRectangle(cornerRadius: 9)
                    .fill(selected ? accent.opacity(0.18)
                                   : (hovered ? accent.opacity(0.09) : .clear))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 9)
                    .stroke(accent.opacity(selected ? 0.55 : 0), lineWidth: 1)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
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
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(selected ? (theme.accent?.color ?? .white).opacity(0.20)
                                   : (hovered ? (theme.accent?.color ?? .white).opacity(0.12) : .clear))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .stroke((theme.accent?.color ?? .white).opacity(selected ? 0.55 : 0), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
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
