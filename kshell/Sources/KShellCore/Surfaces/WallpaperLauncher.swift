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
    @FocusState private var focused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(accent.opacity(0.25))
            if items.isEmpty {
                empty
            } else {
                list
            }
            footer
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .modifier(PanelSurface(viewModel: viewModel, edge: .leading, fallback: sheet))
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.upArrow) { move(-1); return .handled }
        .onKeyPress(.downArrow) { move(1); return .handled }
        .onKeyPress(.return) { applySelected(); return .handled }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear { refocus() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
    }

    private var sheet: SheetShape { SheetShape(radius: 22, corners: .trailing) }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "photo.on.rectangle.angled")
                .foregroundColor(accent)
            Text("Wallpapers")
                .font(.custom(bar.fontFamily, size: 18))
                .foregroundColor(foreground)
            Spacer()
            Text("\(items.count)")
                .font(.custom(bar.fontFamily, size: 13))
                .foregroundColor(foreground.opacity(0.5))
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
    }

    private var list: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 8) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                        row(item, selected: index == selectedIndex)
                            .id(item.id)
                            .onTapGesture {
                                if selectedIndex == index { apply(item) } else { selectedIndex = index }
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
                guard items.indices.contains(newValue) else { return }
                withAnimation(.snappy(duration: 0.30, extraBounce: 0.12)) {
                    proxy.scrollTo(items[newValue].id, anchor: .center)
                }
            }
        }
    }

    private func row(_ item: WallpaperItem, selected: Bool) -> some View {
        HStack(spacing: 12) {
            thumbnail(item)
                .frame(width: 132, height: 82)
                .clipped()
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .overlay(
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(selected ? accent : Color.white.opacity(0.12), lineWidth: selected ? 2 : 1)
                )

            VStack(alignment: .leading, spacing: 3) {
                Text(item.name)
                    .font(.custom(bar.fontFamily, size: 14))
                    .foregroundColor(selected ? foreground : foreground.opacity(0.7))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(item.isVideo ? "Live wallpaper" : "Image")
                    .font(.custom(bar.fontFamily, size: 11))
                    .foregroundColor(selected ? accent : foreground.opacity(0.4))
            }
            Spacer(minLength: 0)
        }
        .padding(8)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(selected ? accent.opacity(0.18) : Color.white.opacity(0.03))
        )
        .contentShape(Rectangle())
        .animation(.snappy(duration: 0.26, extraBounce: 0.18), value: selected)
    }

    @ViewBuilder
    private func thumbnail(_ item: WallpaperItem) -> some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.06))
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

    private var footer: some View {
        HStack {
            Text(items.indices.contains(selectedIndex) ? items[selectedIndex].name : "—")
                .font(.custom(bar.fontFamily, size: 12))
                .foregroundColor(foreground)
                .lineLimit(1)
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
            Text("No wallpapers found")
                .font(.custom(bar.fontFamily, size: 15))
                .foregroundColor(foreground)
            Text(directories.map(\.path).joined(separator: "\n"))
                .font(.custom(bar.fontFamily, size: 11))
                .foregroundColor(foreground.opacity(0.45))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(30)
    }

    private func move(_ delta: Int) {
        guard !items.isEmpty else { return }
        selectedIndex = max(0, min(items.count - 1, selectedIndex + delta))
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
