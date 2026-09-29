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

/// The app launcher overlay (search + grid of applications).
public final class AppLauncher {
    private let overlay: OverlayLauncher

    public init(viewModel: BarViewModel) {
        let items = AppCatalog.load().map {
            LauncherItem(id: $0.url.path, name: $0.name, appIcon: $0.icon)
        }
        overlay = OverlayLauncher(
            viewModel: viewModel,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.7, 900),
                    height: min(screen.frame.height * 0.55, 460)
                )
            },
            content: { onClose in
                AnyView(LauncherView(
                    viewModel: viewModel,
                    placeholder: "Search apps…",
                    minItemWidth: 104,
                    items: items,
                    onSelect: { item in
                        NSWorkspace.shared.open(URL(fileURLWithPath: item.id))
                        onClose()
                    },
                    onClose: onClose
                ))
            }
        )
    }

    public func toggle() { overlay.toggle() }
    public func show() { overlay.show() }
    public func hide() { overlay.hide() }
}
