import AppKit
import SwiftUI

/// The music panel: a bottom sheet with a search tab, a library tab and a
/// player bar — the shell's own music player, replacing an external one.
public final class MusicPanel {
    private let viewModel: BarViewModel
    private let library: MusicLibrary
    private let overlay: OverlayLauncher

    public init(viewModel: BarViewModel, library: MusicLibrary, margin: CGFloat = 0) {
        self.viewModel = viewModel
        self.library = library
        overlay = OverlayLauncher(
            viewModel: viewModel,
            edge: .bottom,
            align: .center,
            margin: margin,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.72, 940),
                    height: min(screen.frame.height * 0.64, 620)
                )
            },
            content: { onClose in
                AnyView(MusicPanelView(
                    viewModel: viewModel,
                    library: library,
                    downloads: .shared,
                    player: .shared,
                    onClose: onClose
                ))
            }
        )
        MusicDownloads.shared.onDownloadFinished = { [weak library] in library?.refresh() }
    }

    public func toggle() { overlay.toggle() }
    public func show(animated: Bool = true) { overlay.show(animated: animated) }
    public func hide(animated: Bool = true) { overlay.hide(animated: animated) }
    public var visible: Bool { overlay.visible }
}

// MARK: - panel content

struct MusicPanelView: View {
    @ObservedObject var viewModel: BarViewModel
    @ObservedObject var library: MusicLibrary
    @ObservedObject var downloads: MusicDownloads
    @ObservedObject var player: MusicPlayer
    let onClose: () -> Void

    enum Tab: String, CaseIterable, Identifiable {
        case search = "Search"
        case library = "Library"
        var id: String { rawValue }
        var icon: String { self == .search ? "magnifyingglass" : "music.note.list" }
    }

    @State private var tab: Tab = .library
    @State private var query = ""
    @FocusState private var focusedField: Bool
    @Namespace private var pill

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var highlight: Color { theme.highlight?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }

    var body: some View {
        VStack(spacing: 0) {
            header
            if !downloads.jobs.filter(\.isActive).isEmpty { downloadsStrip }
            Divider().overlay(dim.opacity(0.18))
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            Divider().overlay(dim.opacity(0.18))
            MusicPlayerBar(player: player, theme: theme, bar: bar)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .modifier(PanelSurface(viewModel: viewModel, edge: .bottom, fallback: SheetShape(radius: 22)))
        .focusable()
        .focusEffectDisabled()
        .onExitCommand { onClose() }
        .onAppear { library.refresh() }
    }

    // MARK: header

    private var header: some View {
        HStack(spacing: 14) {
            HStack(spacing: 4) {
                ForEach(Tab.allCases) { item in
                    Button {
                        withAnimation(.snappy(duration: 0.28, extraBounce: 0.16)) { tab = item }
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: item.icon)
                                .font(.system(size: 11, weight: .semibold))
                            Text(item.rawValue)
                                .font(.custom(bar.fontFamily, size: 12))
                        }
                        .foregroundColor(tab == item ? foreground : dim)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background {
                            if tab == item {
                                Capsule()
                                    .fill(accent.opacity(0.22))
                                    .matchedGeometryEffect(id: "tab", in: pill)
                            }
                        }
                        .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }

            Spacer(minLength: 0)

            Equaliser(playing: player.isPlaying, color: accent)
                .opacity(player.hasTrack ? 1 : 0)

            if player.hasTrack {
                VStack(alignment: .trailing, spacing: 1) {
                    Text(player.current?.displayTitle ?? "")
                        .font(.custom(bar.fontFamily, size: 12))
                        .foregroundColor(foreground)
                        .lineLimit(1)
                    Text(player.current?.displayArtist ?? "")
                        .font(.custom(bar.fontFamily, size: 10))
                        .foregroundColor(dim)
                        .lineLimit(1)
                }
                .frame(maxWidth: 220, alignment: .trailing)
                .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .animation(.snappy(duration: 0.3, extraBounce: 0.1), value: player.hasTrack)
    }

    // MARK: downloads

    private var downloadsStrip: some View {
        VStack(spacing: 6) {
            ForEach(downloads.jobs.filter(\.isActive)) { job in
                HStack(spacing: 10) {
                    Image(systemName: "arrow.down.circle.fill")
                        .foregroundColor(accent)
                    Text(job.title)
                        .font(.custom(bar.fontFamily, size: 11))
                        .foregroundColor(foreground)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Text("\(Int(job.progress * 100))%")
                        .font(.custom(bar.fontFamily, size: 11))
                        .foregroundColor(dim)
                        .monospacedDigit()
                }
                .overlay(alignment: .bottomLeading) {
                    GeometryReader { geo in
                        Capsule()
                            .fill(accent.opacity(0.7))
                            .frame(width: max(2, geo.size.width * job.progress), height: 2)
                            .animation(.linear(duration: 0.25), value: job.progress)
                    }
                    .frame(height: 2)
                    .offset(y: 8)
                }
                .padding(.horizontal, 16)
                .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .padding(.bottom, 10)
        .animation(.snappy(duration: 0.3), value: downloads.jobs)
    }

    // MARK: tabs

    @ViewBuilder
    private var content: some View {
        switch tab {
        case .search: searchTab
        case .library: libraryTab
        }
    }

    private var searchTab: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundColor(dim)
                TextField("Search YouTube Music, or paste a link…", text: $query)
                    .textFieldStyle(.plain)
                    .font(.custom(bar.fontFamily, size: 13))
                    .foregroundColor(foreground)
                    .focused($focusedField)
                    .onSubmit { downloads.search(query) }
                if downloads.searching {
                    ProgressView().controlSize(.small)
                } else if !query.isEmpty {
                    Button { downloads.search(query) } label: {
                        Text("Search")
                            .font(.custom(bar.fontFamily, size: 11))
                            .foregroundColor(accent)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color.white.opacity(0.05))
            )
            .padding(16)

            if downloads.results.isEmpty {
                emptyState(
                    icon: "sparkle.magnifyingglass",
                    title: downloads.searching ? "Searching…" : "Find something to keep",
                    subtitle: "Searches YouTube Music. Results download into your library."
                )
            } else {
                ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(downloads.results) { result in
                            SearchRow(
                                result: result,
                                job: downloads.job(for: result),
                                accent: accent, dim: dim, foreground: foreground,
                                font: bar.fontFamily,
                                existing: library.tracks.contains { $0.path.contains("[\(result.id)]") },
                                onDownload: { downloads.download(result) }
                            )
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.bottom, 14)
                }
            }
        }
        .transition(.asymmetric(
            insertion: .move(edge: .leading).combined(with: .opacity),
            removal: .opacity
        ))
    }

    private var libraryTab: some View {
        Group {
            if library.tracks.isEmpty {
                emptyState(
                    icon: "music.note.list",
                    title: "Library is empty",
                    subtitle: "Downloads land in \(library.directory.path)"
                )
            } else {
                ScrollView {
                    LazyVStack(spacing: 4) {
                        ForEach(Array(library.tracks.enumerated()), id: \.element.id) { index, track in
                            LibraryRow(
                                track: track,
                                index: index,
                                playing: player.current == track,
                                isPlaying: player.isPlaying,
                                accent: accent, dim: dim, foreground: foreground,
                                font: bar.fontFamily,
                                onPlay: { player.play(library.tracks, startAt: index) },
                                onRemove: {
                                    library.remove(track)
                                    player.remove(track)
                                }
                            )
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                }
            }
        }
        .transition(.asymmetric(
            insertion: .move(edge: .trailing).combined(with: .opacity),
            removal: .opacity
        ))
    }

    private func emptyState(icon: String, title: String, subtitle: String) -> some View {
        VStack(spacing: 10) {
            Image(systemName: icon)
                .font(.system(size: 34, weight: .light))
                .foregroundColor(accent.opacity(0.7))
            Text(title)
                .font(.custom(bar.fontFamily, size: 14))
                .foregroundColor(foreground)
            Text(subtitle)
                .font(.custom(bar.fontFamily, size: 11))
                .foregroundColor(dim)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .transition(.opacity)
    }
}

// MARK: - rows

private struct SearchRow: View {
    let result: MusicSearchResult
    let job: MusicDownloadJob?
    let accent: Color
    let dim: Color
    let foreground: Color
    let font: String
    let existing: Bool
    let onDownload: () -> Void

    @State private var hovering = false

    private var state: MusicDownloadJob.State? { job?.state }

    var body: some View {
        HStack(spacing: 12) {
            thumbnail
            VStack(alignment: .leading, spacing: 2) {
                Text(result.title)
                    .font(.custom(font, size: 12))
                    .foregroundColor(foreground)
                    .lineLimit(1)
                Text(result.channel.isEmpty ? "YouTube" : result.channel)
                    .font(.custom(font, size: 10))
                    .foregroundColor(dim)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if result.duration > 0 {
                Text(MusicTrack.clock(result.duration))
                    .font(.custom(font, size: 10))
                    .foregroundColor(dim)
                    .monospacedDigit()
            }
            trailing
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color.white.opacity(hovering ? 0.07 : 0.03))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(accent.opacity(hovering ? 0.35 : 0), lineWidth: 1)
        )
        .scaleEffect(hovering ? 1.008 : 1)
        .animation(.snappy(duration: 0.2, extraBounce: 0.2), value: hovering)
        .onHover { hovering = $0 }
    }

    private var thumbnail: some View {
        AsyncImage(url: result.thumbnailURL.flatMap(URL.init(string:))) { image in
            image.resizable().aspectRatio(contentMode: .fill)
        } placeholder: {
            Rectangle().fill(Color.white.opacity(0.06))
        }
        .frame(width: 62, height: 35)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    @ViewBuilder
    private var trailing: some View {
        switch state {
        case .downloading:
            HStack(spacing: 6) {
                ProgressView().controlSize(.small)
                Text("\(Int((job?.progress ?? 0) * 100))%")
                    .font(.custom(font, size: 10))
                    .foregroundColor(accent)
                    .monospacedDigit()
            }
            .frame(width: 72, alignment: .trailing)
        case .done:
            Label("Saved", systemImage: "checkmark.circle.fill")
                .labelStyle(.iconOnly)
                .foregroundColor(accent)
                .frame(width: 72, alignment: .trailing)
        case .failed:
            Image(systemName: "exclamationmark.triangle")
                .foregroundColor(.orange)
                .frame(width: 72, alignment: .trailing)
        case .queued, .none:
            Button(action: onDownload) {
                HStack(spacing: 4) {
                    Image(systemName: existing ? "arrow.down.circle" : "arrow.down.circle.fill")
                    Text(existing ? "Again" : "Get")
                }
                .font(.custom(font, size: 11))
                .foregroundColor(accent)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(Capsule().fill(accent.opacity(0.16)))
            }
            .buttonStyle(.plain)
            .frame(width: 72, alignment: .trailing)
        }
    }
}

private struct LibraryRow: View {
    let track: MusicTrack
    let index: Int
    let playing: Bool
    let isPlaying: Bool
    let accent: Color
    let dim: Color
    let foreground: Color
    let font: String
    let onPlay: () -> Void
    let onRemove: () -> Void

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                if let path = track.artworkPath, let image = NSImage(contentsOfFile: path) {
                    Image(nsImage: image).resizable().aspectRatio(contentMode: .fill)
                } else {
                    Rectangle().fill(Color.white.opacity(0.06))
                    Image(systemName: "music.note")
                        .font(.system(size: 12))
                        .foregroundColor(dim.opacity(0.7))
                }
            }
            .frame(width: 34, height: 34)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(alignment: .bottomTrailing) {
                if playing {
                    Equaliser(playing: isPlaying, color: accent)
                        .scaleEffect(0.7)
                        .offset(x: 6, y: 6)
                }
            }

            Text("\(index + 1)")
                .font(.custom(font, size: 10))
                .foregroundColor(dim.opacity(0.7))
                .monospacedDigit()
                .frame(width: 18, alignment: .trailing)

            VStack(alignment: .leading, spacing: 1) {
                Text(track.displayTitle)
                    .font(.custom(font, size: 12))
                    .foregroundColor(playing ? accent : foreground)
                    .lineLimit(1)
                Text(track.displayArtist)
                    .font(.custom(font, size: 10))
                    .foregroundColor(dim)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if hovering {
                Button(action: onRemove) {
                    Image(systemName: "trash")
                        .font(.system(size: 11))
                        .foregroundColor(dim)
                }
                .buttonStyle(.plain)
                .transition(.opacity)
            }
            Text(MusicTrack.clock(track.duration))
                .font(.custom(font, size: 10))
                .foregroundColor(dim)
                .monospacedDigit()
        }
        .padding(.vertical, 5)
        .padding(.horizontal, 10)
        .background(
            RoundedRectangle(cornerRadius: 9)
                .fill(Color.white.opacity(playing ? 0.08 : (hovering ? 0.06 : 0.02)))
        )
        .overlay(alignment: .leading) {
            if playing {
                Capsule()
                    .fill(accent)
                    .frame(width: 2)
                    .padding(.vertical, 6)
                    .transition(.scale)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: onPlay)
        .onHover { hovering = $0 }
        .animation(.snappy(duration: 0.22, extraBounce: 0.15), value: hovering)
        .animation(.snappy(duration: 0.22), value: playing)
    }
}

// MARK: - player bar

private struct MusicPlayerBar: View {
    @ObservedObject var player: MusicPlayer
    let theme: Theme
    let bar: BarConfig

    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }

    var body: some View {
        HStack(spacing: 14) {
            artwork
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Text(player.current?.displayTitle ?? "Nothing playing")
                        .font(.custom(bar.fontFamily, size: 13))
                        .foregroundColor(player.current == nil ? dim : foreground)
                        .lineLimit(1)
                    if let track = player.current, !track.album.isEmpty {
                        Text("· \(track.album)")
                            .font(.custom(bar.fontFamily, size: 11))
                            .foregroundColor(dim)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    Text(player.current?.displayArtist ?? "")
                        .font(.custom(bar.fontFamily, size: 11))
                        .foregroundColor(accent)
                        .lineLimit(1)
                }
                SeekBar(player: player, accent: accent, dim: dim, font: bar.fontFamily)
            }
            controls
            volume
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private var artwork: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 8).fill(Color.white.opacity(0.06))
            if let path = player.current?.artworkPath, let image = NSImage(contentsOfFile: path) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .transition(.opacity.combined(with: .scale(scale: 0.92)))
            } else {
                Image(systemName: "music.note")
                    .font(.system(size: 16))
                    .foregroundColor(dim.opacity(0.6))
            }
        }
        .frame(width: 44, height: 44)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .shadow(color: .black.opacity(0.35), radius: 6, y: 3)
        .animation(.snappy(duration: 0.32, extraBounce: 0.2), value: player.current?.path)
    }

    private var controls: some View {
        HStack(spacing: 14) {
            button("backward.fill", size: 12) { player.previous() }
            Button { player.toggle() } label: {
                ZStack {
                    Circle()
                        .fill(accent.opacity(player.hasTrack ? 0.9 : 0.3))
                        .frame(width: 30, height: 30)
                    Image(systemName: player.isPlaying ? "pause.fill" : "play.fill")
                        .font(.system(size: 12, weight: .bold))
                        .foregroundColor(theme.background?.color ?? .black)
                        .offset(x: player.isPlaying ? 0 : 1)
                }
            }
            .buttonStyle(.plain)
            .scaleEffect(player.isPlaying ? 1.04 : 1)
            .animation(.snappy(duration: 0.25, extraBounce: 0.35), value: player.isPlaying)
            button("forward.fill", size: 12) { player.next() }
        }
        .disabled(!player.hasTrack)
    }

    private var volume: some View {
        HStack(spacing: 6) {
            Image(systemName: player.volume <= 0.01 ? "speaker.slash.fill" : "speaker.wave.2.fill")
                .font(.system(size: 10))
                .foregroundColor(dim)
            Slider(value: $player.volume, in: 0...1)
                .controlSize(.mini)
                .frame(width: 70)
                .tint(accent)
        }
    }

    private func button(_ symbol: String, size: CGFloat, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: size))
                .foregroundColor(foreground.opacity(0.85))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct SeekBar: View {
    @ObservedObject var player: MusicPlayer
    let accent: Color
    let dim: Color
    let font: String

    var body: some View {
        HStack(spacing: 8) {
            Text(MusicTrack.clock(player.position))
                .font(.custom(font, size: 10))
                .foregroundColor(dim)
                .monospacedDigit()
                .frame(width: 34, alignment: .trailing)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(dim.opacity(0.22))
                    Capsule()
                        .fill(accent)
                        .frame(width: max(0, geo.size.width * player.progress))
                        .animation(.linear(duration: 0.5), value: player.progress)
                }
                .frame(height: 4)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onEnded { value in
                            player.seek(toFraction: value.location.x / max(geo.size.width, 1))
                        }
                )
            }
            .frame(height: 12)
            Text(MusicTrack.clock(player.duration))
                .font(.custom(font, size: 10))
                .foregroundColor(dim)
                .monospacedDigit()
                .frame(width: 34, alignment: .leading)
        }
    }
}

/// A small bouncing-bars indicator, so it is obvious when something is playing.
struct Equaliser: View {
    var playing: Bool
    var color: Color

    @State private var phase = false

    var body: some View {
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(0..<4, id: \.self) { index in
                Capsule()
                    .fill(color)
                    .frame(width: 2.5, height: playing ? (phase ? high(index) : low(index)) : 3)
            }
        }
        .frame(height: 14, alignment: .bottom)
        .onAppear { restart() }
        .onChange(of: playing) { _, _ in restart() }
    }

    private func restart() {
        phase = false
        guard playing else { return }
        withAnimation(.easeInOut(duration: 0.42).repeatForever(autoreverses: true)) {
            phase = true
        }
    }

    private func high(_ index: Int) -> CGFloat { [6, 14, 9, 12][index % 4] }
    private func low(_ index: Int) -> CGFloat { [3, 5, 4, 6][index % 4] }
}
