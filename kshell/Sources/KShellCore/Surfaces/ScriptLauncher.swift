import AppKit
import SwiftUI

/// A launchable shell script.
public struct ScriptItem: Identifiable, Hashable {
    public let id: String
    public let name: String
    public let path: String
}

/// Lists executable-ish scripts in a directory (one level deep).
public enum ScriptCatalog {
    public static func load(directory: URL) -> [ScriptItem] {
        guard let entries = try? FileManager.default.contentsOfDirectory(atPath: directory.path) else {
            return []
        }
        return entries
            .filter { $0.hasSuffix(".sh") }
            .sorted()
            .map { filename in
                let path = directory.appendingPathComponent(filename).path
                return ScriptItem(
                    id: path,
                    name: String(filename.dropLast(3)),
                    path: path
                )
            }
    }

    public static func filter(_ scripts: [ScriptItem], query: String) -> [ScriptItem] {
        guard !query.isEmpty else { return scripts }
        return scripts.filter { $0.name.localizedCaseInsensitiveContains(query) }
    }
}

/// The script launcher overlay: search + grid of `~/dotfiles/scripts/*.sh`.
/// The list is re-scanned each time it opens.
public final class ScriptLauncher {
    private let overlay: OverlayLauncher
    private let directory: URL

    public init(viewModel: BarViewModel, directory: URL) {
        self.directory = directory
        overlay = OverlayLauncher(
            viewModel: viewModel,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.5, 640),
                    height: min(screen.frame.height * 0.5, 420)
                )
            },
            content: { [directory] onClose in
                let scripts = ScriptCatalog.load(directory: directory)
                let items = scripts.map {
                    LauncherItem(id: $0.path, name: $0.name, symbol: "chevron.left.forwardslash.chevron.right")
                }
                return AnyView(LauncherView(
                    viewModel: viewModel,
                    placeholder: "Search scripts…",
                    minItemWidth: 150,
                    items: items,
                    onSelect: { item in
                        Shell.runScript(item.id)
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
