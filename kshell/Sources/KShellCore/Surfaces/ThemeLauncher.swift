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

        if applyTmux(theme) {
            result.changed.append("tmux")
        } else {
            result.failed.append("tmux")
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

        if applyBorders(theme, path: KShellPaths.resolve(config.bordersConfig)) { result.changed.append("borders") }

        if let wallpaper = theme.wallpaper, !wallpaper.isEmpty {
            let url = KShellPaths.resolve(wallpaper)
            var isDirectory: ObjCBool = false
            if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory),
               !isDirectory.boolValue {
                setWallpaper(url)
                result.changed.append("wallpaper")
            }
        }

        // A shader wallpaper replaces the desktop entirely — no video file, the
        // GPU draws it every frame.
        if let shader = theme.shader, !shader.isEmpty {
            Shell.run("lwp set-builtin \(shader)")
            result.changed.append("shader wallpaper")
        } else if let live = theme.liveWallpaper, !live.isEmpty,
           FileManager.default.fileExists(atPath: KShellPaths.resolve(live).path) {
            setWallpaper(KShellPaths.resolve(live))
            result.changed.append("live wallpaper")
        } else {
            Shell.run("lwp stop")
        }

        return result
    }

    // MARK: - window borders

    /// JankyBorders, when it's set up: the focused window's outline runs from
    /// the theme's accent to its highlight, the others take its dim colour.
    /// The running instance is updated in place (`borders` with arguments
    /// talks to it), and `bordersrc` is rewritten so the next start matches.
    /// Nothing happens without a `bordersrc`.
    private static func applyBorders(_ theme: ThemeDefinition, path rc: URL) -> Bool {
        guard var text = try? String(contentsOf: rc, encoding: .utf8),
              let accent = argb(theme.accent, alpha: 0xff) else { return false }
        let highlight = argb(theme.highlight, alpha: 0xff) ?? accent
        let active = accent == highlight ? accent : "gradient(top_left=\(accent),bottom_right=\(highlight))"
        let inactive = argb(theme.dim, alpha: 0x44) ?? "0x44494949"

        // bordersrc is a bash array of key=value words.
        for (key, value) in [("active_color", "\"\(active)\""), ("inactive_color", inactive)] {
            let lines = text.components(separatedBy: "\n").map { line -> String in
                let trimmed = line.trimmingCharacters(in: .whitespaces)
                guard trimmed.hasPrefix(key + "=") else { return line }
                let indent = line.prefix { $0 == " " || $0 == "\t" }
                return indent + key + "=" + value
            }
            text = lines.joined(separator: "\n")
        }
        try? text.write(to: rc, atomically: true, encoding: .utf8)

        if !Shell.capture("pgrep -x borders", timeout: 2).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            Shell.run("borders active_color=\(Shell.quote(active)) inactive_color=\(inactive)")
        }
        return true
    }

    /// `#RRGGBB` (or `#RRGGBBAA`) as JankyBorders' `0xAARRGGBB`.
    private static func argb(_ hex: String?, alpha: UInt8) -> String? {
        guard var digits = hex?.trimmingCharacters(in: .whitespaces) else { return nil }
        if digits.hasPrefix("#") { digits.removeFirst() }
        guard digits.count >= 6, UInt32(digits.prefix(6), radix: 16) != nil else { return nil }
        return String(format: "0x%02x", alpha) + digits.prefix(6).lowercased()
    }

    // MARK: - tmux

    /// Rewrite `~/.config/tmux/theme.conf` from the palette and reload tmux.
    ///
    /// tmux cannot read kshell's config, so the shared writer in the dotfiles
    /// repo owns the mapping — `theme-switch.sh` calls the same script, and the
    /// two switchers cannot drift apart. A missing script is reported as a
    /// failure rather than silently skipped, so the panel says which targets it
    /// did not reach.
    private static func applyTmux(_ theme: ThemeDefinition) -> Bool {
        let script = KShellPaths.resolve("~/dotfiles/scripts/tmux-theme.sh")
        guard FileManager.default.isExecutableFile(atPath: script.path) else { return false }

        let palette = [
            theme.name,
            theme.accent, theme.highlight, theme.foreground,
            theme.dim, theme.mid, theme.background,
        ].map { $0 ?? "" }

        let command = ([script.path] + palette.map(Shell.quote)).joined(separator: " ")
        return Shell.capture(command, timeout: 10).contains("theme.conf written")
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
    @State private var hovered: String?
    @FocusState private var focused: Bool
    @Namespace private var highlight

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
    private var palette: KitPalette { KitPalette(theme: theme, bar: bar) }

    var body: some View {
        VStack(spacing: 0) {
            header
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
        .modifier(VimListKeys(move: vimMove, jump: jump))
        .onKeyPress(.upArrow) { vimMove(-1) }
        .onKeyPress(.downArrow) { vimMove(1) }
        .onKeyPress(.return) { applySelection(); return .handled }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
    }

    private var header: some View {
        KitHeader(
            icon: "paintpalette.fill",
            title: "Themes",
            subtitle: activeName.map { "Wearing \($0)" } ?? "\(themes.count) themes",
            palette: palette
        )
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
                            .cascade(index, from: .trailing)
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
                withAnimation(.kLayout) {
                    proxy.scrollTo(themes[newValue].name, anchor: .center)
                }
            }
        }
    }

    private func row(_ item: ThemeDefinition, selected: Bool, active: Bool) -> some View {
        let open = selected || hovered == item.name
        return HStack(spacing: 14) {
            orbs(item, fanned: open)
                .frame(width: 100, height: 40, alignment: .leading)

            VStack(alignment: .leading, spacing: 5) {
                Text(item.name)
                    .font(.custom(bar.fontFamily, size: 14.5))
                    .foregroundColor(selected ? foreground : foreground.opacity(0.78))
                HStack(spacing: 4) {
                    ForEach(targets(item), id: \.self) { target in
                        Text(target)
                            .font(.custom(bar.fontFamily, size: 9))
                            .foregroundColor(foreground.opacity(0.55))
                            .padding(.horizontal, 5)
                            .padding(.vertical, 2)
                            .background(Capsule().fill(Color.white.opacity(0.07)))
                    }
                }
            }
            Spacer(minLength: 0)
            if active {
                StatusPill(text: "Active", lit: true, palette: palette)
                    .transition(.scale.combined(with: .opacity))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .kitCard(selected: selected, hovered: hovered == item.name, palette: palette, highlight: highlight)
        .contentShape(Rectangle())
        .onHover { inside in hovered = inside ? item.name : (hovered == item.name ? nil : hovered) }
        .animation(.kBouncy, value: open)
        .animation(.kBouncy, value: active)
    }

    /// The theme's colours as overlapping discs that fan out when the row is
    /// armed or under the pointer — the background disc carries the others.
    private func orbs(_ item: ThemeDefinition, fanned: Bool) -> some View {
        let colours = [item.background, item.accent, item.highlight, item.dim]
        return HStack(spacing: fanned ? 3 : -9) {
            ForEach(Array(colours.enumerated()), id: \.offset) { index, hex in
                Circle()
                    .fill(hex.flatMap { RGBA(hex: $0) }?.color ?? Color.white.opacity(0.08))
                    .overlay(Circle().stroke(Color.white.opacity(0.22), lineWidth: 1))
                    .frame(width: 22, height: 22)
                    .shadow(color: .black.opacity(0.3), radius: 3, y: 1)
                    .zIndex(Double(colours.count - index))
                    .offset(y: fanned ? (index % 2 == 0 ? -3 : 3) : 0)
                    .rotationEffect(.degrees(fanned ? Double(index) * 6 - 9 : 0))
            }
        }
    }

    /// Which tools this theme touches.
    private func targets(_ item: ThemeDefinition) -> [String] {
        var parts: [String] = []
        if item.kitty?.isEmpty == false { parts.append("kitty") }
        if item.nvim?.isEmpty == false { parts.append("nvim") }
        if item.wallpaper?.isEmpty == false { parts.append("wallpaper") }
        return parts
    }

    private var footer: some View {
        KitFooter(hints: [("↑↓", "browse"), ("⏎", "apply")], palette: palette) {
            Text("\(themes.count) themes")
                .font(.custom(bar.fontFamily, size: 10.5))
                .foregroundColor(foreground.opacity(0.5))
        }
    }

    private var empty: some View {
        KitEmpty(
            icon: "paintpalette",
            title: "No themes configured",
            detail: "Add [[theme_launcher.themes]] entries to config.toml",
            palette: palette
        )
    }

    private func vimMove(_ delta: Int) -> KeyPress.Result {
        move(delta)
        return .handled
    }

    private func jump(_ edge: VimEdge) -> KeyPress.Result {
        guard !themes.isEmpty else { return .ignored }
        withAnimation(.kSelect) { selectedIndex = edge == .first ? 0 : themes.count - 1 }
        return .handled
    }

    private func move(_ delta: Int) {
        guard !themes.isEmpty else { return }
        withAnimation(.kSelect) { selectedIndex = max(0, min(themes.count - 1, selectedIndex + delta)) }
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
