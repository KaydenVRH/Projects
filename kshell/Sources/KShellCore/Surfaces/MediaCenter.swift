import AppKit
import SwiftUI

/// The media centre: a sheet that slides down out of the bar.
///
/// It is not a panel of its own — its content is installed into the bar's own
/// window and revealed by the bar's mask, so the sheet is literally the bar's
/// material continuing downward: one glass surface, no seam.
public final class MediaCenter {
    static let width: CGFloat = 460
    static let contentHeight: CGFloat = 306

    private let viewModel: BarViewModel
    private let monitor: MediaMonitor
    private let align: OverlayAlign
    /// The bar to unfurl from (the one on the display being used).
    private let barProvider: () -> BarPanel?
    private weak var host: BarPanel?
    private var isOpen = false

    public init(
        viewModel: BarViewModel,
        monitor: MediaMonitor,
        align: OverlayAlign = .leading,
        bar: @escaping () -> BarPanel?
    ) {
        self.viewModel = viewModel
        self.monitor = monitor
        self.align = align
        self.barProvider = bar
    }

    public func show(animated: Bool = true) {
        guard !isOpen, let bar = barProvider() else { return }
        isOpen = true
        host = bar
        // Poll only while the centre is on screen.
        monitor.start()

        bar.configureSheet(width: Self.width, contentHeight: Self.contentHeight, align: align)
        bar.setSheetView(NSHostingView(rootView: MediaCenterView(
            viewModel: viewModel,
            monitor: monitor,
            onClose: { [weak self] in self?.hide() }
        )))
        bar.setSheetHeight(Self.contentHeight, animated: animated)
        bar.makeKey()
    }

    public func hide(animated: Bool = true) {
        guard isOpen, let bar = host else { return }
        isOpen = false
        monitor.stop()
        bar.setSheetHeight(0, animated: animated)
        host = nil
        // Drop the content once it has collapsed back into the bar.
        DispatchQueue.main.asyncAfter(deadline: .now() + (animated ? 0.32 : 0)) { [weak self] in
            guard self?.isOpen != true else { return }
            bar.setSheetView(nil)
        }
    }

    public func toggle() { isOpen ? hide() : show() }

    public var visible: Bool { isOpen }
}

struct MediaCenterView: View {
    @ObservedObject var viewModel: BarViewModel
    @ObservedObject var monitor: MediaMonitor
    let onClose: () -> Void

    @State private var artwork: NSImage?
    @FocusState private var focused: Bool

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }
    private var state: MediaState { monitor.state }

    var body: some View {
        VStack(spacing: 0) {
            if state.hasTrack {
                track
            } else {
                idle
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        // Only the tint — the vibrancy comes from the bar's own material, so the
        // sheet is the same glass as the bar.
        .background(bar.background.color)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.space) { monitor.control(.playPause); return .handled }
        .onKeyPress(.rightArrow) { monitor.control(.next); return .handled }
        .onKeyPress(.leftArrow) { monitor.control(.previous); return .handled }
        .onKeyPress(.escape) { onClose(); return .handled }
        .onExitCommand { onClose() }
        .onAppear {
            loadArtwork(state.artworkPath)
            refocus()
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            refocus()
        }
        .onChange(of: state) { _, new in loadArtwork(new.artworkPath) }
    }

    // MARK: - pieces

    private var track: some View {
        VStack(spacing: 18) {
            HStack(alignment: .top, spacing: 18) {
                cover
                VStack(alignment: .leading, spacing: 5) {
                    Text(state.title)
                        .font(.custom(bar.fontFamily, size: 19))
                        .foregroundColor(foreground)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                    if !state.artist.isEmpty {
                        Text(state.artist)
                            .font(.custom(bar.fontFamily, size: 15))
                            .foregroundColor(accent)
                            .lineLimit(1)
                    }
                    if !state.album.isEmpty {
                        Text(state.album)
                            .font(.custom(bar.fontFamily, size: 13))
                            .foregroundColor(dim)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    Text(state.source == "termusic" ? "termusic"
                         : (state.source == "kshell" ? "kshell library" : "now playing"))
                        .font(.custom(bar.fontFamily, size: 11))
                        .foregroundColor(dim.opacity(0.7))
                }
                Spacer(minLength: 0)
            }
            .frame(height: 132)

            progress
            controls
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 16)
    }

    private var cover: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 12)
                .fill(Color.white.opacity(0.06))
            if let artwork {
                Image(nsImage: artwork)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
            } else {
                Image(systemName: "music.note")
                    .font(.system(size: 34))
                    .foregroundColor(dim.opacity(0.6))
            }
        }
        .frame(width: 132, height: 132)
        .clipped()
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color.white.opacity(0.10), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.45), radius: 12, y: 5)
    }

    private var progress: some View {
        VStack(spacing: 6) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.12))
                    Capsule()
                        .fill(accent)
                        .frame(width: max(2, geo.size.width * state.progress))
                }
            }
            .frame(height: 4)
            HStack {
                Text(clock(state.position))
                Spacer()
                Text(state.duration > 0 ? clock(state.duration) : "--:--")
            }
            .font(.custom(bar.fontFamily, size: 11))
            .foregroundColor(dim)
        }
    }

    private var controls: some View {
        HStack(spacing: 26) {
            button("backward.end.fill", size: 20) { monitor.control(.previous) }
            button(state.isPlaying ? "pause.circle.fill" : "play.circle.fill", size: 40) {
                monitor.control(.playPause)
            }
            button("forward.end.fill", size: 20) { monitor.control(.next) }
        }
        .frame(maxWidth: .infinity)
    }

    private func button(_ symbol: String, size: CGFloat, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size, weight: .regular))
                .foregroundColor(foreground)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            if hovering { NSCursor.pointingHand.push() } else { NSCursor.pop() }
        }
    }

    private var idle: some View {
        VStack(spacing: 8) {
            Image(systemName: "music.note.list")
                .font(.system(size: 30))
                .foregroundColor(dim.opacity(0.6))
            Text("Nothing playing")
                .font(.custom(bar.fontFamily, size: 15))
                .foregroundColor(foreground)
            Text("Start something in termusic, Spotify or Music")
                .font(.custom(bar.fontFamily, size: 11))
                .foregroundColor(dim)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: - helpers

    private func clock(_ seconds: Double) -> String {
        guard seconds.isFinite, seconds >= 0 else { return "0:00" }
        let total = Int(seconds.rounded())
        let h = total / 3600, m = (total % 3600) / 60, s = total % 60
        return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
    }

    private func loadArtwork(_ path: String?) {
        guard let path, !path.isEmpty else {
            artwork = nil
            return
        }
        artwork = NSImage(contentsOfFile: path)
    }

    private func refocus() {
        DispatchQueue.main.async { focused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { focused = true }
    }
}
