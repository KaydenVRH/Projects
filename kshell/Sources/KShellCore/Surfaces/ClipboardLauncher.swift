import AppKit
import SwiftUI

/// The clipboard launcher: what's been copied lately, newest first, searched
/// by its whole text. Choosing one copies it again and — when the shell has
/// Accessibility access — pastes it into the app you were in.
public final class ClipboardLauncher {
    private let overlay: OverlayLauncher

    public init(viewModel: BarViewModel, margin: CGFloat = 0) {
        overlay = OverlayLauncher(
            viewModel: viewModel,
            margin: margin,
            size: { screen in
                NSSize(width: min(screen.frame.width * 0.5, 600), height: min(screen.frame.height * 0.6, 480))
            },
            content: { onClose in
                let entries = ClipboardHistory.shared.entries
                let items = entries.enumerated().map { index, entry in
                    LauncherItem(
                        id: String(index),
                        name: Self.headline(entry.text),
                        detail: Self.detail(entry),
                        symbol: Self.symbol(entry.text),
                        search: entry.text
                    )
                }
                return AnyView(LauncherView(
                    viewModel: viewModel,
                    placeholder: entries.isEmpty ? "Nothing copied yet…" : "Search what you copied…",
                    style: .list,
                    items: items,
                    onSelect: { item in
                        guard let index = Int(item.id), entries.indices.contains(index) else { return }
                        ClipboardHistory.shared.copy(entries[index])
                        onClose()
                        Self.pasteWhenBack()
                    },
                    onClose: onClose
                ))
            }
        )
    }

    public func toggle() { overlay.toggle() }
    public func show() { overlay.show() }
    public func hide() { overlay.hide() }

    /// The first line with something on it, trimmed to a row.
    static func headline(_ text: String) -> String {
        let line = text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? ""
        return line.count > 90 ? String(line.prefix(89)) + "…" : line
    }

    static func detail(_ entry: ClipboardHistory.Entry) -> String {
        let lines = entry.text.split(whereSeparator: \.isNewline).count
        let size = lines > 1 ? "\(lines) lines" : "\(entry.text.count) chars"
        let age = Date().timeIntervalSince(entry.copied)
        let when = age < 10 ? "just now" : RelativeDateTimeFormatter().localizedString(for: entry.copied, relativeTo: Date())
        return [entry.app, size, when].compactMap { $0 }.joined(separator: " · ")
    }

    static func symbol(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasPrefix("http://") || trimmed.hasPrefix("https://") { return "link" }
        if trimmed.hasPrefix("/") || trimmed.hasPrefix("~/") { return "folder" }
        if Double(trimmed) != nil { return "number" }
        if trimmed.contains("\n") { return "text.alignleft" }
        return "doc.on.clipboard"
    }

    /// Once the app you were in has the front again, press ⌘V in it. Without
    /// Accessibility access the text is only copied.
    private static func pasteWhenBack(attempt: Int = 0) {
        guard AXIsProcessTrusted() else { return }
        let me = ProcessInfo.processInfo.processIdentifier
        guard NSWorkspace.shared.frontmostApplication?.processIdentifier == me else {
            // A beat for the app to take its key window back.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) { pressPaste() }
            return
        }
        guard attempt < 30 else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { pasteWhenBack(attempt: attempt + 1) }
    }

    private static func pressPaste() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let v: CGKeyCode = 9
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: v, keyDown: down)
            event?.flags = .maskCommand
            event?.post(tap: .cghidEventTap)
        }
    }
}
