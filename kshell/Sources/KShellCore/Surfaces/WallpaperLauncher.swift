import AppKit
import ImageIO
import SwiftUI

/// A selectable wallpaper (image, or a video for the live-wallpaper engine).
public struct WallpaperItem: Identifiable, Hashable {
    public let id: String
    public let name: String
    public let url: URL
    public let isVideo: Bool
    public let image: NSImage?

    public static func == (lhs: WallpaperItem, rhs: WallpaperItem) -> Bool { lhs.id == rhs.id }
    public func hash(into hasher: inout Hasher) { hasher.combine(id) }
}

/// Finds wallpapers in a set of directories.
public enum WallpaperCatalog {
    static let imageExtensions: Set<String> = [
        "jpg", "jpeg", "png", "heic", "heif", "gif", "webp", "tiff", "tif", "bmp",
    ]
    static let videoExtensions: Set<String> = ["mp4", "mov", "m4v"]

    /// Decoded previews, keyed by path. Wallpaper files can be tens of MB, so we
    /// downsample once instead of decoding the full image on every open — that
    /// decode was slow enough to swallow the panel's slide-in animation.
    private static let cacheLock = NSLock()
    private static var thumbnails: [String: NSImage?] = [:]

    public static func load(directories: [URL]) -> [WallpaperItem] {
        var seen = Set<String>()
        var items: [WallpaperItem] = []

        for directory in directories {
            guard let entries = try? FileManager.default.contentsOfDirectory(atPath: directory.path) else {
                continue
            }
            for entry in entries {
                let ext = (entry as NSString).pathExtension.lowercased()
                let isVideo = videoExtensions.contains(ext)
                let isImage = imageExtensions.contains(ext)
                guard isVideo || isImage else { continue }

                let url = directory.appendingPathComponent(entry)
                guard seen.insert(url.path).inserted else { continue }
                items.append(WallpaperItem(
                    id: url.path,
                    name: (entry as NSString).deletingPathExtension,
                    url: url,
                    isVideo: isVideo,
                    image: isImage ? thumbnail(for: url) : nil
                ))
            }
        }
        return items.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    /// A downsampled preview for `url`, built once and cached.
    static func thumbnail(for url: URL, maxPixel: Int = 512) -> NSImage? {
        let key = url.path

        cacheLock.lock()
        if let cached = thumbnails[key] {
            cacheLock.unlock()
            return cached
        }
        cacheLock.unlock()

        let image = makeThumbnail(url: url, maxPixel: maxPixel)

        cacheLock.lock()
        thumbnails[key] = image
        cacheLock.unlock()
        return image
    }

    private static func makeThumbnail(url: URL, maxPixel: Int) -> NSImage? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
        ]
        guard let cgImage = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else {
            return nil
        }
        return NSImage(cgImage: cgImage, size: NSSize(width: cgImage.width, height: cgImage.height))
    }
}

/// The wallpaper picker: a vertical sidebar of previews that slides in from the
/// left edge of the screen.
public final class WallpaperLauncher {
    private let overlay: OverlayLauncher

    public init(viewModel: BarViewModel, directories: [URL], margin: CGFloat = 0) {
        // Warm the preview cache in the background so the first open slides in
        // with its rows already decoded rather than stalling on 20 MB PNGs.
        DispatchQueue.global(qos: .utility).async {
            _ = WallpaperCatalog.load(directories: directories)
        }

        overlay = OverlayLauncher(
            viewModel: viewModel,
            edge: .leading,
            margin: margin,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.32, 460),
                    height: min(screen.frame.height * 0.84, 880)
                )
            },
            content: { onClose in
                // Built before the panel is ordered front, so the first frame
                // SwiftUI paints is the finished content.
                AnyView(WallpaperLauncherView(
                    viewModel: viewModel,
                    items: WallpaperCatalog.load(directories: directories),
                    directories: directories,
                    onClose: onClose
                ))
            }
        )
    }

    public func toggle() { overlay.toggle() }
    public func show() { overlay.show() }
    public func hide() { overlay.hide() }
}

struct WallpaperLauncherView: View {
    @ObservedObject var viewModel: BarViewModel
    let items: [WallpaperItem]
    let directories: [URL]
    let onClose: () -> Void

    @State private var selectedIndex = 0
    @State private var hovered: String?
    @FocusState private var focused: Bool
    @Namespace private var highlight

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var palette: KitPalette { KitPalette(theme: theme, bar: bar) }
    private var selectedItem: WallpaperItem? {
        items.indices.contains(selectedIndex) ? items[selectedIndex] : nil
    }

    /// Thumbnails sit two to a row.
    private let columns = 2

    var body: some View {
        VStack(spacing: 0) {
            KitHeader(
                icon: "photo.on.rectangle.angled",
                title: "Wallpapers",
                subtitle: "\(items.count) in \(directories.count) folder\(directories.count == 1 ? "" : "s")",
                palette: palette
            )
            if items.isEmpty {
                empty
            } else {
                preview
                    .padding(.horizontal, 14)
                    .padding(.bottom, 10)
                grid
            }
            KitFooter(
                hints: [("↑↓←→", "browse"), ("⏎", "apply")],
                palette: palette
            ) { EmptyView() }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .modifier(PanelSurface(viewModel: viewModel, edge: .leading, fallback: sheet))
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .modifier(VimListKeys(move: vimMove, jump: jump))
        .onKeyPress(.upArrow) { vimMove(-columns) }
        .onKeyPress(.downArrow) { vimMove(columns) }
        .onKeyPress(.leftArrow) { vimMove(-1) }
        .onKeyPress(.rightArrow) { vimMove(1) }
        .onKeyPress(.return) { applySelected(); return .handled }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
    }

    private var sheet: SheetShape { SheetShape(radius: 22, corners: .trailing) }

    /// The armed wallpaper, big, drifting slowly like a desktop would show it.
    /// Moving the selection crossfades through a blur to the next one.
    private var preview: some View {
        ZStack(alignment: .bottomLeading) {
            ZStack {
                RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.white.opacity(0.05))
                if let item = selectedItem {
                    KenBurns(seed: item.id) { thumbnail(item) }
                        .id(item.id)
                        .transition(.asymmetric(
                            insertion: .opacity.combined(with: .scale(scale: 1.08)),
                            removal: .opacity
                        ))
                }
            }
            .frame(height: 210)
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))

            LinearGradient(colors: [.clear, .black.opacity(0.65)], startPoint: .center, endPoint: .bottom)
                .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                .allowsHitTesting(false)

            if let item = selectedItem {
                HStack(alignment: .bottom) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(item.name)
                            .font(.custom(bar.fontFamily, size: 16))
                            .foregroundColor(.white)
                            .lineLimit(1)
                        Text(item.isVideo ? "Live wallpaper" : "Still image")
                            .font(.custom(bar.fontFamily, size: 10.5))
                            .foregroundColor(.white.opacity(0.7))
                    }
                    .id("label-\(item.id)")
                    .transition(.blurSwap)
                    Spacer()
                    Button { apply(item) } label: {
                        HStack(spacing: 5) {
                            Image(systemName: "checkmark")
                            Text("Apply")
                        }
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundColor(.black.opacity(0.8))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(Capsule().fill(accent))
                        .shadow(color: accent.opacity(0.5), radius: 8)
                    }
                    .pressBounce()
                }
                .padding(14)
            }
        }
        .frame(height: 210)
        .shadow(color: .black.opacity(0.35), radius: 14, y: 6)
        .animation(.kLayout, value: selectedIndex)
    }

    private var grid: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                LazyVGrid(
                    columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: columns),
                    spacing: 10
                ) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                        tile(item, selected: index == selectedIndex)
                            .id(item.id)
                            .onTapGesture {
                                if selectedIndex == index {
                                    apply(item)
                                } else {
                                    withAnimation(.kSelect) { selectedIndex = index }
                                }
                            }
                            .cascade(index, from: .leading)
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 6)
            }
            .frame(maxHeight: .infinity)
            .onChange(of: selectedIndex) { _, newValue in
                guard items.indices.contains(newValue) else { return }
                withAnimation(.kLayout) {
                    proxy.scrollTo(items[newValue].id, anchor: .center)
                }
            }
        }
    }

    private func tile(_ item: WallpaperItem, selected: Bool) -> some View {
        let isHovered = hovered == item.id
        return ZStack(alignment: .bottomLeading) {
            thumbnail(item)
                .frame(height: 92)
                .frame(maxWidth: .infinity)
                .clipped()
                .scaleEffect(isHovered ? 1.08 : 1)
            LinearGradient(colors: [.clear, .black.opacity(0.55)], startPoint: .center, endPoint: .bottom)
            HStack(spacing: 4) {
                if item.isVideo {
                    Image(systemName: "play.fill").font(.system(size: 7))
                }
                Text(item.name)
                    .font(.custom(bar.fontFamily, size: 10.5))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .foregroundColor(.white.opacity(0.92))
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
        }
        .frame(height: 92)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            if selected {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(accent, lineWidth: 2.5)
                    .shadow(color: accent.opacity(0.7), radius: 8)
                    .matchedGeometryEffect(id: "wall-armed", in: highlight)
            }
        }
        .scaleEffect(selected ? 0.96 : 1)
        .offset(y: isHovered && !selected ? -2 : 0)
        .contentShape(Rectangle())
        .onHover { inside in hovered = inside ? item.id : (hovered == item.id ? nil : hovered) }
        .animation(.kBouncy, value: selected)
        .animation(.kHover, value: isHovered)
    }

    @ViewBuilder
    private func thumbnail(_ item: WallpaperItem) -> some View {
        ZStack {
            Rectangle().fill(Color.white.opacity(0.06))
            if let image = item.image {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                VStack(spacing: 6) {
                    Image(systemName: "play.rectangle.fill")
                        .font(.system(size: 26))
                    Text("Live")
                        .font(.system(size: 11, weight: .semibold))
                }
                .foregroundColor(accent)
            }
        }
    }

    private var empty: some View {
        KitEmpty(
            icon: "photo.on.rectangle.angled",
            title: "No wallpapers found",
            detail: "Looked in " + directories.map(\.path).joined(separator: ", "),
            palette: palette
        )
    }

    private func vimMove(_ delta: Int) -> KeyPress.Result {
        move(delta)
        return .handled
    }

    private func jump(_ edge: VimEdge) -> KeyPress.Result {
        guard !items.isEmpty else { return .ignored }
        withAnimation(.kSelect) { selectedIndex = edge == .first ? 0 : items.count - 1 }
        return .handled
    }

    private func move(_ delta: Int) {
        guard !items.isEmpty else { return }
        withAnimation(.kSelect) { selectedIndex = max(0, min(items.count - 1, selectedIndex + delta)) }
    }

    private func applySelected() {
        guard items.indices.contains(selectedIndex) else { return }
        apply(items[selectedIndex])
    }

    private func apply(_ item: WallpaperItem) {
        if item.isVideo {
            Shell.run("lwp set \(Shell.quote(item.url.path))")
        } else {
            for screen in NSScreen.screens {
                try? NSWorkspace.shared.setDesktopImageURL(item.url, for: screen, options: [:])
            }
        }
        onClose()
    }

    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }
}

/// A slow pan and zoom over a still, so a preview feels alive. Each picture
/// drifts its own way.
struct KenBurns<Content: View>: View {
    let seed: String
    @ViewBuilder let content: Content

    @State private var drifted = false

    var body: some View {
        let direction: CGFloat = seed.hashValue % 2 == 0 ? 1 : -1
        content
            .scaleEffect(drifted ? 1.14 : 1.02)
            .offset(x: drifted ? 10 * direction : -10 * direction, y: drifted ? -6 : 4)
            .onAppear {
                withAnimation(.easeInOut(duration: 9).repeatForever(autoreverses: true)) { drifted = true }
            }
    }
}
