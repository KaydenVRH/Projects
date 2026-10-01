import AppKit
import SwiftUI

/// Applies a theme across the user's tools: kitty, neovim, kshell itself and the
/// desktop wallpaper.
///
/// Each target is a targeted line rewrite, so the config files keep their
/// comments and layout — only the theme values change.
public enum ThemeApplier {
    public struct Result {
        public var changed: [String] = []
        public var failed: [String] = []
        public init() {}
    }

    public static func apply(_ theme: ThemeDefinition, config: ThemeLauncherConfig) -> Result {        var result = Result()

        if let kitty = theme.kitty, !kitty.isEmpty {
            if applyKitty(kitty, theme: theme, config: config, path: KShellPaths.resolve(config.kittyConfig)) {
                result.changed.append("kitty")
                reloadKitty()
            } else {
                result.failed.append("kitty")
            }
        }

        if let nvim = theme.nvim, !nvim.isEmpty {
            if setNvimTheme(nvim, palette: theme, path: KShellPaths.resolve(config.nvimInit)) {
                result.changed.append("nvim")
            } else {
                result.failed.append("nvim")
            }
        }

        if setShellThemeSection(theme, path: KShellPaths.resolve(config.shellConfig)) {
            result.changed.append("kshell")
        } else {
            result.failed.append("kshell")
        }

        if let wallpaper = theme.wallpaper, !wallpaper.isEmpty {
            let url = KShellPaths.resolve(wallpaper)
            var isDirectory: ObjCBool = false
            if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory),
               !isDirectory.boolValue {
                setWallpaper(url)
                result.changed.append("wallpaper")
            }
        }

        return result
    }

    // MARK: - kitty

    /// Point kitty's `include` line (inside the theme block, when present) at a
    /// different theme file, and set that theme's glass.
    private static func applyKitty(
        _ themeFile: String,
        theme: ThemeDefinition,
        config: ThemeLauncherConfig,
        path: URL
    ) -> Bool {
        guard var lines = readLines(path) else { return false }
        guard setKittyInclude(themeFile, lines: &lines) else { return false }

        if let opacity = theme.opacity ?? config.opacity {
            setKittyValue("background_opacity", formatted(opacity), lines: &lines)
        }
        if let blur = theme.blur ?? config.blur {
            setKittyValue("background_blur", formatted(blur), lines: &lines)
        }
        return writeLines(lines, to: path)
    }

    private static func setKittyInclude(_ theme: String, lines: inout [String]) -> Bool {
        var searchFrom = 0
        if let marker = lines.firstIndex(where: { $0.contains("BEGIN_KITTY_THEME") }) {
            searchFrom = marker + 1
        }
        guard searchFrom < lines.count else { return false }

        let isInclude: (String) -> Bool = {
            $0.trimmingCharacters(in: .whitespaces).hasPrefix("include ")
        }
        guard let index = lines[searchFrom...].firstIndex(where: isInclude) else { return false }

        lines[index] = "include \(theme)"
        return true
    }

    /// Set `key value` in place, or append it when kitty.conf doesn't have it.
    private static func setKittyValue(_ key: String, _ value: String, lines: inout [String]) {
        let matches: (String) -> Bool = {
            $0.trimmingCharacters(in: .whitespaces).hasPrefix(key + " ")
        }
        if let index = lines.firstIndex(where: matches) {
            lines[index] = "\(key) \(value)"
        } else {
            lines.append("\(key) \(value)")
        }
    }

    /// `0.45` stays a fraction; `40.0` prints as `40`.
    private static func formatted(_ value: Double) -> String {
        value == value.rounded() ? String(Int(value)) : String(value)
    }

    /// kitty reloads its config on SIGUSR1.
    private static func reloadKitty() {
        for pid in Shell.capture("pgrep -x kitty").components(separatedBy: .newlines) {
            guard let pid = Int32(pid.trimmingCharacters(in: .whitespaces)) else { continue }
            kill(pid, SIGUSR1)
        }
    }

    // MARK: - neovim

    /// Rewrite `vim.cmd.colorscheme(...)` and the lualine palette table.
    private static func setNvimTheme(_ name: String, palette: ThemeDefinition, path: URL) -> Bool {
        guard var lines = readLines(path) else { return false }
        var touched = false
        var inPalette = false

        for index in lines.indices {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if trimmed.hasPrefix("local c = {") {
                inPalette = true
                continue
            }
            if inPalette {
                if trimmed.hasPrefix("}") {
                    inPalette = false
                    continue
                }
                let values = [
                    ("fg", palette.foreground),
                    ("dim", palette.dim),
                    ("accent", palette.accent),
                    ("mid", palette.mid),
                ]
                for (key, value) in values {
                    guard let value, let updated = replacingValue(of: key, with: value, in: line) else { continue }
                    lines[index] = updated
                    touched = true
                }
                continue
            }

            if trimmed.hasPrefix("vim.cmd.colorscheme(") {
                lines[index] = "vim.cmd.colorscheme(\"\(name)\")"
                touched = true
            }
        }

        return touched && writeLines(lines, to: path)
    }

    /// Replace the quoted value of `key = "…"`, preserving indentation/alignment.
    private static func replacingValue(of key: String, with value: String, in line: String) -> String? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard trimmed.hasPrefix(key) else { return nil }
        let remainder = trimmed.dropFirst(key.count)
        guard remainder.first == " " || remainder.first == "\t" || remainder.first == "=" else { return nil }

        guard let equals = line.firstIndex(of: "="),
              let open = line[line.index(after: equals)...].firstIndex(of: "\""),
              let close = line[line.index(after: open)...].firstIndex(of: "\"") else { return nil }

        return line.replacingCharacters(in: open...close, with: "\"\(value)\"")
    }

    // MARK: - kshell

    /// Replace the `[theme]` section with this theme's palette (and name).
    private static func setShellThemeSection(_ theme: ThemeDefinition, path: URL) -> Bool {
        guard var lines = readLines(path) else { return false }
        let isHeader: (String) -> Bool = {
            $0.trimmingCharacters(in: .whitespaces).hasPrefix("[")
        }
        guard let start = lines.firstIndex(where: { $0.trimmingCharacters(in: .whitespaces) == "[theme]" }) else {
            return false
        }

        var end = lines.count
        for index in (start + 1)..<lines.count {
            let trimmed = lines[index].trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty || isHeader(trimmed) {
                end = index
                break
            }
        }
        // Keep exactly one blank line before whatever follows the section.
        if end < lines.count, lines[end].trimmingCharacters(in: .whitespaces).isEmpty {
            end += 1
        }

        var block = ["[theme]", "name = \"\(theme.name)\""]
        block.append(contentsOf: theme.palette.map { "\($0.key) = \"\($0.value)\"" })
        block.append("")

        lines.replaceSubrange(start..<end, with: block)
        return writeLines(lines, to: path)
    }

    // MARK: - wallpaper

    private static func setWallpaper(_ url: URL) {
        let ext = url.pathExtension.lowercased()
        if ["mp4", "mov", "m4v"].contains(ext) {
            Shell.run("lwp set \(Shell.quote(url.path))")
            return
        }
        for screen in NSScreen.screens {
            try? NSWorkspace.shared.setDesktopImageURL(url, for: screen, options: [:])
        }
    }

    // MARK: - file helpers

    private static func readLines(_ url: URL) -> [String]? {
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return nil }
        return text.components(separatedBy: "\n")
    }

    private static func writeLines(_ lines: [String], to url: URL) -> Bool {
        do {
            try lines.joined(separator: "\n").write(to: url, atomically: true, encoding: .utf8)
            return true
        } catch {
            return false
        }
    }
}

/// Live theme list for the picker. A store (rather than rebuilding the whole
/// panel on every config reload) means applying a theme never re-creates the
/// window, so the picker can't blink or re-animate underneath the pointer.
final class ThemeStore: ObservableObject {
    @Published var themes: [ThemeDefinition] = []
    @Published var activeName: String?
    var config = ThemeLauncherConfig()

    func apply(_ theme: ThemeDefinition, then completion: () -> Void) {
        _ = ThemeApplier.apply(theme, config: config)
        activeName = theme.name
        completion()
    }
}

/// The theme picker: a sidebar that slides in from the right edge. Applying a
/// theme re-colours kitty, neovim, kshell and the wallpaper, and the menu stays
/// open so themes can be tried one after another.
public final class ThemeLauncher {
    private let store = ThemeStore()
    private let overlay: OverlayLauncher

    public init(viewModel: BarViewModel, margin: CGFloat = 0, onApplied: @escaping () -> Void) {
        let store = self.store
        overlay = OverlayLauncher(
            viewModel: viewModel,
            edge: .trailing,
            margin: margin,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.30, 420),
                    height: min(screen.frame.height * 0.72, 700)
                )
            },
            content: { onClose in
                AnyView(ThemeLauncherView(
                    viewModel: viewModel,
                    store: store,
                    onApply: { theme in store.apply(theme, then: onApplied) },
                    onClose: onClose
                ))
            }
        )
    }

    func update(config: ThemeLauncherConfig, activeName: String?) {
        store.config = config
        // Only republish when something actually changed — an unrelated config
        // reload shouldn't re-run the rows' animations.
        if store.themes.map(\.name) != config.themes.map(\.name) {
            store.themes = config.themes
        }
        if store.activeName != activeName {
            store.activeName = activeName
        }
    }

    public func toggle() { overlay.toggle() }
    public func show(animated: Bool = true) { overlay.show(animated: animated) }
    public func hide(animated: Bool = true) { overlay.hide(animated: animated) }
    public var visible: Bool { overlay.visible }
}

struct ThemeLauncherView: View {
    @ObservedObject var viewModel: BarViewModel
    @ObservedObject var store: ThemeStore
    let onApply: (ThemeDefinition) -> Void
    let onClose: () -> Void

    @State private var selectedIndex: Int
    @FocusState private var focused: Bool

    init(
        viewModel: BarViewModel,
        store: ThemeStore,
        onApply: @escaping (ThemeDefinition) -> Void,
        onClose: @escaping () -> Void
    ) {
        self.viewModel = viewModel
        self.store = store
        self.onApply = onApply
        self.onClose = onClose
        // Start on the active theme: changing the selection in `onAppear` would
        // run the row/scroll animations while the panel is still sliding in.
        let active = store.activeName ?? viewModel.theme.name
        _selectedIndex = State(initialValue: store.themes.firstIndex { $0.name == active } ?? 0)
    }

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var sheet: SheetShape { SheetShape(radius: 22, corners: .leading) }
    private var activeName: String? { store.activeName ?? theme.name }
    private var themes: [ThemeDefinition] { store.themes }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(accent.opacity(0.25))
            if themes.isEmpty {
                empty
            } else {
                list
            }
            footer
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .modifier(PanelSurface(viewModel: viewModel, edge: .trailing, fallback: sheet))
        .focusable()
        .focusEffectDisabled()   // the system focus ring is drawn as a rectangle,
                                 // which leaves red line stubs where the sheet's
                                 // rounded corners clip it
        .focused($focused)
        .onKeyPress(.upArrow) { move(-1); return .handled }
        .onKeyPress(.downArrow) { move(1); return .handled }
        .onKeyPress(.return) { applySelection(); return .handled }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "paintpalette.fill")
                .foregroundColor(accent)
            Text("Themes")
                .font(.custom(bar.fontFamily, size: 18))
                .foregroundColor(foreground)
            Spacer()
            if let activeName {
                Text(activeName)
                    .font(.custom(bar.fontFamily, size: 12))
                    .foregroundColor(accent)
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
    }

    private var list: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 8) {
                    ForEach(Array(themes.enumerated()), id: \.element.name) { index, item in
                        row(item, selected: index == selectedIndex, active: item.name == activeName)
                            .id(item.name)
                            .onTapGesture {
                                selectedIndex = index
                                applySelection()
                            }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .scrollTargetLayout()
            }
            .scrollTargetBehavior(.viewAligned)
            .frame(maxHeight: .infinity)
            .onChange(of: selectedIndex) { _, newValue in
                guard themes.indices.contains(newValue) else { return }
                withAnimation(.snappy(duration: 0.30, extraBounce: 0.12)) {
                    proxy.scrollTo(themes[newValue].name, anchor: .center)
                }
            }
        }
    }

    private func row(_ item: ThemeDefinition, selected: Bool, active: Bool) -> some View {
        HStack(spacing: 12) {
            swatch(item)
                .frame(width: 76, height: 36)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(selected ? accent : Color.white.opacity(0.12), lineWidth: selected ? 2 : 1)
                )

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(item.name)
                        .font(.custom(bar.fontFamily, size: 15))
                        .foregroundColor(selected ? foreground : foreground.opacity(0.75))
                    if active {
                        Image(systemName: "checkmark")
                            .font(.system(size: 10, weight: .bold))
                            .foregroundColor(accent)
                    }
                }
                Text(targets(item))
                    .font(.custom(bar.fontFamily, size: 11))
                    .foregroundColor(foreground.opacity(0.4))
            }
            Spacer(minLength: 0)
        }
        .padding(9)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(selected ? accent.opacity(0.18) : Color.white.opacity(0.03))
        )
        .contentShape(Rectangle())
        .animation(.snappy(duration: 0.26, extraBounce: 0.18), value: selected)
    }

    private func swatch(_ item: ThemeDefinition) -> some View {
        HStack(spacing: 0) {
            chip(item.background)
            chip(item.accent)
            chip(item.highlight)
            chip(item.dim)
        }
    }

    private func chip(_ hex: String?) -> some View {
        let color = hex.flatMap { RGBA(hex: $0) }?.color ?? Color.white.opacity(0.08)
        return color.frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Which tools this theme touches.
    private func targets(_ item: ThemeDefinition) -> String {
        var parts: [String] = []
        if item.kitty?.isEmpty == false { parts.append("kitty") }
        if item.nvim?.isEmpty == false { parts.append("nvim") }
        parts.append("kshell")
        if item.wallpaper?.isEmpty == false { parts.append("wallpaper") }
        return parts.joined(separator: " · ")
    }

    private var footer: some View {
        HStack {
            Text("\(themes.count) themes")
                .font(.custom(bar.fontFamily, size: 12))
                .foregroundColor(foreground.opacity(0.55))
            Spacer()
            Text("↑ ↓  browse     ⏎  apply")
                .font(.custom(bar.fontFamily, size: 11))
                .foregroundColor(foreground.opacity(0.5))
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
    }

    private var empty: some View {
        VStack(spacing: 8) {
            Text("No themes configured")
                .font(.custom(bar.fontFamily, size: 15))
                .foregroundColor(foreground)
            Text("Add [[theme_launcher.themes]] entries to config.toml")
                .font(.custom(bar.fontFamily, size: 11))
                .foregroundColor(foreground.opacity(0.45))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(30)
    }

    private func move(_ delta: Int) {
        guard !themes.isEmpty else { return }
        selectedIndex = max(0, min(themes.count - 1, selectedIndex + delta))
    }

    private func applySelection() {
        guard themes.indices.contains(selectedIndex) else { return }
        onApply(themes[selectedIndex])
    }

    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }
}
