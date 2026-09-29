import AppKit
import SwiftUI

/// One launchable application.
public struct AppItem: Identifiable, Hashable {
    public let id: String
    public let name: String
    public let url: URL
    public let icon: NSImage

    public static func == (lhs: AppItem, rhs: AppItem) -> Bool { lhs.id == rhs.id }
    public func hash(into hasher: inout Hasher) { hasher.combine(id) }
}

/// Scans the usual application directories.
public enum AppCatalog {
    static var searchPaths: [String] {
        [
            "/Applications",
            "/System/Applications",
            "/System/Applications/Utilities",
            NSHomeDirectory() + "/Applications",
        ]
    }

    public static func load() -> [AppItem] {
        var seenNames = Set<String>()
        var items: [AppItem] = []

        for directory in searchPaths {
            guard let entries = try? FileManager.default.contentsOfDirectory(atPath: directory) else { continue }
            for entry in entries where entry.hasSuffix(".app") {
                let path = directory + "/" + entry
                let name = FileManager.default.displayName(atPath: path)
                    .replacingOccurrences(of: ".app", with: "")
                guard seenNames.insert(name).inserted else { continue }
                items.append(AppItem(
                    id: path,
                    name: name,
                    url: URL(fileURLWithPath: path),
                    icon: NSWorkspace.shared.icon(forFile: path)
                ))
            }
        }
        return items.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    /// Pure filter helper (also used by tests).
    public static func filter(_ apps: [AppItem], query: String) -> [AppItem] {
        guard !query.isEmpty else { return apps }
        return apps.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }
}

/// Manages the launcher overlay for the main screen.
public final class AppLauncher {
    private let viewModel: BarViewModel
    private var panel: OverlayPanel?
    private var isVisible = false

    public init(viewModel: BarViewModel) {
        self.viewModel = viewModel
    }

    public func toggle() {
        isVisible ? hide() : show()
    }

    public func show() {
        guard !isVisible else { return }
        let mouse = NSEvent.mouseLocation
        guard let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) })
            ?? NSScreen.main
            ?? NSScreen.screens.first else { return }
        isVisible = true

        let width = min(screen.frame.width * 0.7, 900)
        let height = min(screen.frame.height * 0.55, 460)
        let bar = viewModel.appearance

        let content = AnyView(
            AppLauncherView(viewModel: viewModel, onClose: { [weak self] in self?.hide() })
        )
        let overlay = OverlayPanel(
            screen: screen,
            size: NSSize(width: width, height: height),
            bottomMargin: 44,
            blur: bar.blur,
            cornerRadius: 18,
            content: content
        )
        panel = overlay
        overlay.show(animated: true)
    }

    public func hide() {
        guard isVisible else { return }
        isVisible = false
        let current = panel
        panel = nil
        current?.hide(animated: true)
    }
}

/// The launcher UI: search field + application grid, themed like the bar.
struct AppLauncherView: View {
    @ObservedObject var viewModel: BarViewModel
    let onClose: () -> Void

    @State private var query = ""
    @State private var apps: [AppItem] = []
    @FocusState private var searchFocused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }

    private var results: [AppItem] {
        AppCatalog.filter(apps, query: query)
    }

    var body: some View {
        VStack(spacing: 0) {
            searchField
            Divider().overlay(accent.opacity(0.25))
            grid
        }
        .background(bar.background.color)
        .overlay(
            RoundedRectangle(cornerRadius: 18)
                .stroke(accent.opacity(0.35), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 18))
        .onExitCommand { onClose() }
        .onAppear {
            if apps.isEmpty { apps = AppCatalog.load() }
            searchFocused = true
        }
    }

    private var searchField: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .foregroundColor(accent.opacity(0.8))
            TextField("Search apps…", text: $query)
                .textFieldStyle(.plain)
                .font(.custom(bar.fontFamily, size: 18))
                .foregroundColor(foreground)
                .focused($searchFocused)
                .onSubmit { if let first = results.first { launch(first) } }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
    }

    private var grid: some View {
        ScrollView {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 104), spacing: 14)],
                spacing: 18
            ) {
                ForEach(results) { app in
                    AppIconView(app: app, theme: theme, font: bar.fontFamily) {
                        launch(app)
                    }
                }
            }
            .padding(18)
        }
    }

    private func launch(_ app: AppItem) {
        NSWorkspace.shared.open(app.url)
        onClose()
    }

    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
}

private struct AppIconView: View {
    let app: AppItem
    let theme: Theme
    let font: String
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(nsImage: app.icon)
                    .resizable()
                    .interpolation(.high)
                    .frame(width: 54, height: 54)
                Text(app.name)
                    .font(.custom(font, size: 12))
                    .foregroundColor(theme.foreground?.color ?? .white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 96)
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
}
