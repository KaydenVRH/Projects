import AppKit
import SwiftUI

/// One launchable application.
///
/// Deliberately carries no icon. Extracting and decoding an icon per app is the
/// expensive half of a scan and it is only wanted for the dozen rows actually on
/// screen, so `AppIconCache` resolves those on demand instead.
public struct AppItem: Identifiable, Hashable {
    public let id: String
    public let name: String
    public let url: URL

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

    /// The scan, kept once. The shell rebuilds the launcher on every config
    /// reload and border toggle, and re-reading four directories each time is
    /// needless — the applications on disk do not change while the shell runs.
    private static var cached: [AppItem]?
    private static let lock = NSLock()

    public static func items() -> [AppItem] {
        lock.lock()
        defer { lock.unlock() }
        if let cached { return cached }
        let loaded = load()
        cached = loaded
        return loaded
    }

    /// Drop the cache, so the next `items()` sees a freshly installed app.
    public static func invalidate() {
        lock.lock()
        cached = nil
        lock.unlock()
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
                items.append(AppItem(id: path, name: name, url: URL(fileURLWithPath: path)))
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

/// The app launcher overlay: a search field over a list of applications.
public final class AppLauncher {
    private let overlay: OverlayLauncher

    public init(viewModel: BarViewModel, margin: CGFloat = 0) {
        // Built once here rather than scanned per open: names only, no icons.
        let items = AppCatalog.items().map {
            LauncherItem(id: $0.url.path, name: $0.name, iconPath: $0.url.path)
        }
        overlay = OverlayLauncher(
            viewModel: viewModel,
            margin: margin,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.7, 900),
                    height: min(screen.frame.height * 0.72, 620)
                )
            },
            content: { onClose in
                AnyView(LauncherView(
                    viewModel: viewModel,
                    placeholder: "Search apps…",
                    style: .list,
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
