import AppKit
import SwiftUI

/// One entry in a launcher overlay: an app (with its icon) or a script (with an
/// SF Symbol).
struct LauncherItem: Identifiable {
    let id: String
    let name: String
    let detail: String?
    let appIcon: NSImage?
    let symbol: String?

    init(id: String, name: String, detail: String? = nil, appIcon: NSImage? = nil, symbol: String? = nil) {
        self.id = id
        self.name = name
        self.detail = detail
        self.appIcon = appIcon
        self.symbol = symbol
    }
}

/// A themed search + grid overlay, shared by the app and script launchers.
struct LauncherView: View {
    @ObservedObject var viewModel: BarViewModel
    let placeholder: String
    let minItemWidth: CGFloat
    let items: [LauncherItem]
    let onSelect: (LauncherItem) -> Void
    let onClose: () -> Void

    @State private var query = ""
    @FocusState private var focused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }

    private var results: [LauncherItem] {
        query.isEmpty ? items : items.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass")
                    .foregroundColor(accent.opacity(0.8))
                TextField(placeholder, text: $query)
                    .textFieldStyle(.plain)
                    .font(.custom(bar.fontFamily, size: 18))
                    .foregroundColor(foreground)
                    .focused($focused)
                    .onSubmit { if let first = results.first { onSelect(first) } }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 14)

            Divider().overlay(accent.opacity(0.25))

            ScrollView {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: minItemWidth), spacing: 14)],
                    spacing: 18
                ) {
                    ForEach(results) { item in
                        LauncherItemView(item: item, theme: theme, font: bar.fontFamily) {
                            onSelect(item)
                        }
                    }
                }
                .padding(18)
            }
        }
        .background {
            ZStack {
                if bar.blur { VisualEffectBackground() }
                bar.background.color
            }
        }
        .overlay { SheetShape(radius: 22).stroke(accent.opacity(0.35), lineWidth: 1) }
        .clipShape(SheetShape(radius: 22))
        .onExitCommand { onClose() }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
    }

    /// Claim keyboard focus once the panel is actually key (activation is async,
    /// so setting this only on `onAppear` is unreliable).
    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }

    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
}

private struct LauncherItemView: View {
    let item: LauncherItem
    let theme: Theme
    let font: String
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
                    .fill(hovered ? (theme.accent?.color ?? .white).opacity(0.18) : .clear)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
    }

    @ViewBuilder
    private var icon: some View {
        if let appIcon = item.appIcon {
            Image(nsImage: appIcon)
                .resizable()
                .interpolation(.high)
                .frame(width: 54, height: 54)
        } else if let symbol = item.symbol {
            Image(systemName: symbol)
                .font(.system(size: 30, weight: .regular))
                .foregroundColor(theme.accent?.color ?? .white)
                .frame(width: 54, height: 54)
        } else {
            Image(systemName: "doc")
                .font(.system(size: 30))
                .foregroundColor(theme.accent?.color ?? .white)
                .frame(width: 54, height: 54)
        }
    }
}
