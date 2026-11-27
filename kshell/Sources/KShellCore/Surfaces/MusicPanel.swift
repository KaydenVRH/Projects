import AppKit
import SwiftUI

/// The music panel: a bottom sheet with a search tab, a library tab and a
/// player bar — the shell's own music player, replacing an external one.
public final class MusicPanel {
    private let viewModel: BarViewModel
    private let library: MusicLibrary
    private let queue: MusicQueue
    private let playlists: MusicPlaylists
    private let overlay: OverlayLauncher

    public init(
        viewModel: BarViewModel,
        library: MusicLibrary,
        queue: MusicQueue,
        playlists: MusicPlaylists,
        margin: CGFloat = 0
    ) {
        self.viewModel = viewModel
        self.library = library
        self.queue = queue
        self.playlists = playlists
        overlay = OverlayLauncher(
            viewModel: viewModel,
            edge: .bottom,
            align: .center,
            margin: margin,
            size: { screen in
                NSSize(
                    width: min(screen.frame.width * 0.8, 1060),
                    height: min(screen.frame.height * 0.66, 640)
                )
            },
            content: { onClose in
                AnyView(MusicPanelView(
                    viewModel: viewModel,
                    library: library,
                    queue: queue,
                    playlists: playlists,
                    downloads: .shared,
                    player: .shared,
                    onClose: onClose
                ))
            }
        )
        MusicDownloads.shared.onDownloadFinished = { [weak library] in library?.refresh() }
    }

    public func toggle() { visible ? hide() : show() }
    public func show(animated: Bool = true) {
        // The card's spectrum needs the analyser, visualiser or not.
        MusicPlayer.shared.spectrumWatched = true
        overlay.show(animated: animated)
    }
    public func hide(animated: Bool = true) {
        MusicPlayer.shared.spectrumWatched = false
        overlay.hide(animated: animated)
    }
    public var visible: Bool { overlay.visible }
}

// MARK: - panel content

struct MusicPanelView: View {
    @ObservedObject var viewModel: BarViewModel
    @ObservedObject var library: MusicLibrary
    @ObservedObject var queue: MusicQueue
    @ObservedObject var playlists: MusicPlaylists
    @ObservedObject var downloads: MusicDownloads
    @ObservedObject var player: MusicPlayer
    let onClose: () -> Void

    enum Tab: String, CaseIterable, Identifiable {
        case search = "Search"
        case library = "Library"
        case queue = "Queue"
        case playlists = "Playlists"
        var id: String { rawValue }
        var icon: String {
            switch self {
            case .search: return "magnifyingglass"
            case .library: return "music.note.list"
            case .queue: return "list.bullet"
            case .playlists: return "square.stack"
            }
        }
    }

    @State private var tab: Tab = .library
    @State private var query = ""
    /// An armed row per tab, moved with the arrow keys like the theme picker.
    @State private var librarySelection = 0
    @State private var resultSelection = 0
    @State private var queueSelection = 0
    @State private var playlistsSelection = 0
    /// Which playlist is drilled into, if any, and the armed track inside it.
    @State private var openedPlaylist: String?
    @State private var trackSelection = 0
    /// Naming a playlist: the field takes the keyboard until Enter or Esc.
    @State private var naming = false
    @State private var newName = ""
    @FocusState private var namingFocused: Bool
    @FocusState private var focusedField: Bool
    @FocusState private var panelFocused: Bool
    @Namespace private var pill
    /// The armed row's highlight glides between rows through this.
    @Namespace private var rowGlide

    private var bar: BarConfig { viewModel.appearance }
    private var theme: Theme { viewModel.theme }
    private var accent: Color { theme.accent?.color ?? .accentColor }
    private var highlight: Color { theme.highlight?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }
    private var palette: KitPalette { KitPalette(theme: theme, bar: bar) }

    var body: some View {
        panel
        .focusable()
        .focusEffectDisabled()
        .focused($panelFocused)
        .onExitCommand { onClose() }
        .modifier(MusicKeys(
            move: move,
            activate: activate,
            togglePlay: { player.toggle() },
            seek: { delta in player.seek(toSeconds: player.position + delta) },
            volume: { delta in player.volume = min(1, max(0, player.volume + delta)) },
            fieldFocused: typing
        ))
        .modifier(VimListKeys(
            move: move,
            jump: jump,
            search: focusSearch,
            fieldFocused: typing
        ))
        .modifier(MusicLetterKeys(
            shiftTab: shiftTab,
            addToQueue: letterA,
            removeFromQueue: letterX,
            savePlaylist: startSavingPlaylist,
            playPlaylist: playArmedPlaylist,
            moveInQueue: shiftArmedInQueue,
            fieldFocused: typing
        ))
        .onExitCommand { onClose() }
        .onAppear {
            library.refresh()
            // Arm the playing track, like the theme picker arming the active theme.
            if let playing = player.current, let at = library.tracks.firstIndex(of: playing) {
                librarySelection = at
            }
            // Claim the keyboard for the panel itself, so the transport keys
            // work before anything is clicked.
            refocusPanel()
        }
        .onChange(of: library.tracks) { _, tracks in
            librarySelection = min(librarySelection, max(tracks.count - 1, 0))
        }
        .onChange(of: downloads.results) { _, results in
            resultSelection = min(resultSelection, max(results.count - 1, 0))
        }
        .onChange(of: queue.tracks) { _, tracks in
            queueSelection = min(queueSelection, max(tracks.count - 1, 0))
        }
        .onChange(of: tab) { _, new in tabChanged(to: new) }
    }

    /// Search wants the field; Library leaves the keys to the panel.
    private func tabChanged(to new: Tab) {
        // The new tab's rows cascade in, as they do when the panel opens.
        Motion.reveal()
        if new == .search { focusedField = true } else { refocusPanel() }
    }

    /// Any text field holds the keyboard: the search box, or the playlist name
    /// while it is being typed. Every list and transport binding stands down so
    /// letters and Enter reach the field.
    private var typing: Bool { focusedField || namingFocused }

    /// Focus the panel itself (not the search field), so transport keys land.
    private func refocusPanel() {
        guard !naming else { return }
        focusedField = false
        DispatchQueue.main.async { panelFocused = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { panelFocused = true }
    }

    /// The panel's layout, kept separate from its keyboard so the two type-check
    /// independently.
    private var panel: some View {
        HStack(spacing: 0) {
            // What is playing, big, on the left; the lists on the right.
            NowPlayingCard(player: player, theme: theme, bar: bar)
                .frame(width: 300)
            Rectangle()
                .fill(dim.opacity(0.18))
                .frame(width: 1)
                .padding(.vertical, 14)
            VStack(spacing: 0) {
                header
                if naming { namingRow }
                if !downloads.jobs.filter(\.isActive).isEmpty { downloadsStrip }
                Divider().overlay(dim.opacity(0.18))
                ZStack {
                    content
                        .id(tab)
                        .transition(.asymmetric(
                            insertion: .opacity.combined(with: .offset(y: 18)).combined(with: .scale(scale: 0.97)),
                            removal: .opacity.combined(with: .scale(scale: 1.02))
                        ))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .clipped()
                KitFooter(hints: hints, palette: palette) {
                    Text(status)
                        .font(.custom(bar.fontFamily, size: 10.5))
                        .foregroundColor(foreground.opacity(0.5))
                        .contentTransition(.numericText())
                }
                .animation(.kLayout, value: tab)
                .animation(.kLayout, value: openedPlaylist)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .modifier(PanelSurface(viewModel: viewModel, edge: .bottom, fallback: SheetShape(radius: 22)))
    }

    /// The index the arrow keys are moving, per tab.
    private var selection: Int {
        switch tab {
        case .search: return resultSelection
        case .library: return librarySelection
        case .queue: return queueSelection
        case .playlists: return openedPlaylist == nil ? playlistsSelection : trackSelection
        }
    }

    private var selectionCount: Int {
        switch tab {
        case .search: return downloads.results.count
        case .library: return library.tracks.count
        case .queue: return queue.tracks.count
        case .playlists:
            if let name = openedPlaylist {
                return playlists.playlist(named: name)?.tracks.count ?? 0
            }
            return playlists.count
        }
    }

    private func setSelection(_ value: Int) {
        switch tab {
        case .search: resultSelection = value
        case .library: librarySelection = value
        case .queue: queueSelection = value
        case .playlists:
            if openedPlaylist == nil { playlistsSelection = value } else { trackSelection = value }
        }
    }

    /// `g`/`G`: the ends of whichever list is armed.
    private func jump(_ edge: VimEdge) -> KeyPress.Result {
        let count = selectionCount
        guard count > 0 else { return .ignored }
        setSelection(edge == .first ? 0 : count - 1)
        return .handled
    }

    private func move(_ delta: Int) -> KeyPress.Result {
        let count = selectionCount
        guard count > 0 else { return .ignored }
        let next = min(max(0, selection + delta), count - 1)
        guard next != selection else { return .handled }
        withAnimation(.kSelect) { setSelection(next) }
        return .handled
    }

    /// Run the armed row: play it in the library, download it in search, play the
    /// playlist in playlists.
    private func activate() -> KeyPress.Result {
        switch tab {
        case .library:
            guard library.tracks.indices.contains(librarySelection) else { return .ignored }
            player.play(library.tracks, startAt: librarySelection)
        case .search:
            guard downloads.results.indices.contains(resultSelection) else { return .ignored }
            downloads.download(downloads.results[resultSelection])
        case .queue:
            guard queue.tracks.indices.contains(queueSelection) else { return .ignored }
            player.play(queue.tracks, startAt: queueSelection)
        case .playlists:
            if let name = openedPlaylist {
                // Inside a playlist: play from the armed track.
                let tracks = playlists.playlist(named: name)?.tracks ?? []
                guard tracks.indices.contains(trackSelection) else { return .ignored }
                queue.replace(tracks)
                player.play(tracks, startAt: trackSelection)
            } else {
                // On the list: drill in, so tracks can be played or removed.
                guard playlists.playlists.indices.contains(playlistsSelection) else { return .ignored }
                let playlist = playlists.playlists[playlistsSelection]
                guard !playlist.tracks.isEmpty else { return .ignored }
                withAnimation(.kLayout) {
                    openedPlaylist = playlist.name
                    trackSelection = 0
                }
            }
        }
        return .handled
    }

    /// `a` — queue the armed track, or in Playlists add the playing track to the
    /// playlist being looked at.
    private func letterA() -> KeyPress.Result {
        guard tab == .playlists else { return addArmedToQueue() }
        guard let name = openedPlaylist ?? playlists.playlists[safe: playlistsSelection]?.name,
              let playing = player.current
        else { return .ignored }
        playlists.add(playing, to: name)
        return .handled
    }

    /// `x` — dequeue the armed track; in Playlists remove the armed track from
    /// the playlist, or the playlist itself when the list is armed.
    private func letterX() -> KeyPress.Result {
        if let name = openedPlaylist {
            let tracks = playlists.playlist(named: name)?.tracks ?? []
            guard tracks.indices.contains(trackSelection) else { return .ignored }
            playlists.remove(tracks[trackSelection], from: name)
            // Keep the arm on a real row after the list shrinks.
            trackSelection = min(trackSelection, max(tracks.count - 2, 0))
            return .handled
        }
        guard tab == .playlists else { return removeArmedFromQueue() }
        guard playlists.playlists.indices.contains(playlistsSelection) else { return .ignored }
        playlists.remove(named: playlists.playlists[playlistsSelection].name)
        playlistsSelection = min(playlistsSelection, max(playlists.count - 1, 0))
        return .handled
    }

    /// `p` — play the armed playlist without drilling in.
    private func playArmedPlaylist() -> KeyPress.Result {
        guard tab == .playlists, openedPlaylist == nil else { return .ignored }
        guard playlists.playlists.indices.contains(playlistsSelection) else { return .ignored }
        let playlist = playlists.playlists[playlistsSelection]
        guard !playlist.tracks.isEmpty else { return .ignored }
        // A playlist replaces the queue: it is a thing you listen to, not a set
        // of suggestions to merge into whatever was lined up.
        queue.replace(playlist.tracks)
        player.play(playlist.tracks, startAt: 0)
        return .handled
    }

    /// `s` — save the queue as a playlist; the name is typed in the field above.
    private func startSavingPlaylist() -> KeyPress.Result {
        guard !queue.isEmpty else { return .ignored }
        newName = playlists.uniqueName()
        naming = true
        focusedField = false
        DispatchQueue.main.async { namingFocused = true }
        return .handled
    }

    private func commitPlaylistName() {
        let name = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        naming = false
        namingFocused = false
        guard !name.isEmpty, !queue.isEmpty else {
            refocusPanel()
            return
        }
        playlists.save(name: name, tracks: queue.tracks)
        tab = .playlists
        if let at = playlists.playlists.firstIndex(where: { $0.name == name }) {
            playlistsSelection = at
        }
        refocusPanel()
    }

    private func cancelNaming() {
        naming = false
        namingFocused = false
        refocusPanel()
    }

    /// Scroll a list so the armed row stays in view.
    private func keepVisible(_ proxy: ScrollViewProxy, id: String, when value: Int) {
        DispatchQueue.main.async { proxy.scrollTo(id, anchor: .center) }
    }

    /// Run a transport action unless the search field has the keyboard.
    private func transport(_ action: () -> Void) -> KeyPress.Result {
        guard !focusedField else { return .ignored }
        action()
        return .handled
    }

    // MARK: header

    /// The tabs: a segmented track with one lit pill that springs from tab to
    /// tab, its icon hopping as it lands.
    private var header: some View {
        HStack(spacing: 14) {
            HStack(spacing: 2) {
                ForEach(Tab.allCases) { item in
                    let on = tab == item
                    Button {
                        withAnimation(.kBouncy) { tab = item }
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: item.icon)
                                .font(.system(size: 11, weight: .semibold))
                                .symbolEffect(.bounce, value: on)
                            Text(item.rawValue)
                                .font(.custom(bar.fontFamily, size: 12))
                        }
                        .foregroundColor(on ? .black.opacity(0.8) : foreground.opacity(0.65))
                        .padding(.horizontal, 13)
                        .padding(.vertical, 7)
                        .background {
                            if on {
                                AnyShape(palette.hud ? AnyShape(ChamferShape(cut: 6)) : AnyShape(Capsule()))
                                    .fill(accent)
                                    .shadow(color: accent.opacity(0.5), radius: 8)
                                    .matchedGeometryEffect(id: "tab", in: pill)
                            }
                        }
                        .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(3)
            .background(Capsule().fill(Color.white.opacity(0.06)))

            Spacer(minLength: 0)

            if tab == .queue, !queue.isEmpty {
                Button(action: playQueue) {
                    HStack(spacing: 5) {
                        Image(systemName: "play.fill").font(.system(size: 9))
                        Text("Play queue").font(.custom(bar.fontFamily, size: 11))
                    }
                    .foregroundColor(accent)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 5)
                    .background(Capsule().fill(accent.opacity(0.15)))
                }
                .pressBounce()
                .transition(.scale.combined(with: .opacity))
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 14)
        .padding(.bottom, 10)
        .animation(.kBouncy, value: tab)
    }

    /// The search box's outline: a capsule, or cut corners under the HUD.
    private var fieldShape: AnyShape {
        if palette.hud { return AnyShape(ChamferShape(cut: 8)) }
        if palette.neon { return AnyShape(SlantShape(slant: 12)) }
        return AnyShape(Capsule(style: .continuous))
    }

    /// What the footer says on the left, per tab.
    private var status: String {
        switch tab {
        case .search: return downloads.results.isEmpty ? "YouTube Music" : "\(downloads.results.count) results"
        case .library: return "\(library.tracks.count) tracks"
        case .queue: return "\(queue.count) queued"
        case .playlists:
            if let name = openedPlaylist { return name }
            return "\(playlists.count) playlists"
        }
    }

    /// The keys that matter on this tab.
    private var hints: [(key: String, label: String)] {
        if naming { return [("⏎", "save"), ("esc", "cancel")] }
        switch tab {
        case .search: return [("⏎", "download"), ("/", "search"), ("h l", "tabs")]
        case .library: return [("⏎", "play"), ("a", "queue"), ("space", "pause"), ("h l", "tabs")]
        case .queue: return [("⏎", "play"), ("x", "remove"), ("J K", "move"), ("s", "save")]
        case .playlists:
            if openedPlaylist != nil { return [("⏎", "play"), ("x", "remove"), ("h", "back")] }
            return [("⏎", "open"), ("p", "play"), ("x", "delete")]
        }
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
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
                .background {
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            RoundedRectangle(cornerRadius: 11, style: .continuous)
                                .fill(Color.white.opacity(0.05))
                            // The card fills with the accent as it downloads.
                            RoundedRectangle(cornerRadius: 11, style: .continuous)
                                .fill(LinearGradient(
                                    colors: [accent.opacity(0.08), accent.opacity(0.28)],
                                    startPoint: .leading, endPoint: .trailing))
                                .frame(width: max(22, geo.size.width * job.progress))
                                .animation(.kLayout, value: job.progress)
                        }
                    }
                }
                .padding(.horizontal, 14)
                .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .padding(.bottom, 10)
        .animation(.kLayout, value: downloads.jobs)
    }

    // MARK: tabs

    @ViewBuilder
    private var content: some View {
        switch tab {
        case .search: searchTab
        case .library: libraryTab
        case .queue: queueTab
        case .playlists: playlistsTab
        }
    }

    private var searchTab: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(accent)
                    .symbolEffect(.bounce, value: query.count)
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
                            .foregroundColor(.black.opacity(0.8))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 4)
                            .background(Capsule().fill(accent))
                    }
                    .pressBounce()
                    .transition(.scale.combined(with: .opacity))
                }
            }
            .padding(.horizontal, 15)
            .padding(.vertical, 10)
            .background(
                fieldShape
                    .fill(Color.white.opacity(focusedField ? 0.09 : 0.05))
                    .overlay(
                        fieldShape
                            .stroke(accent.opacity(focusedField ? 0.6 : 0.15), lineWidth: 1.2)
                    )
                    .shadow(color: accent.opacity(focusedField ? 0.3 : 0), radius: 12)
            )
            .animation(.kHover, value: focusedField)
            .animation(.kBouncy, value: query.isEmpty)
            .padding(.horizontal, 14)
            .padding(.vertical, 12)

            if downloads.results.isEmpty {
                emptyState(
                    icon: "sparkle.magnifyingglass",
                    title: downloads.searching ? "Searching…" : "Find something to keep",
                    subtitle: "Searches YouTube Music. Results download into your library."
                )
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(downloads.results) { result in
                            SearchRow(
                                result: result,
                                job: downloads.job(for: result),
                                selected: result.id == downloads.results[safe: resultSelection]?.id,
                                accent: accent, dim: dim, foreground: foreground,
                                font: bar.fontFamily,
                                palette: palette,
                                highlight: rowGlide,
                                existing: library.tracks.contains { $0.path.contains("[\(result.id)]") },
                                onDownload: { downloads.download(result) }
                            )
                            .id(result.id)
                            .cascade(downloads.results.firstIndex(of: result) ?? 0)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.bottom, 14)
                    }
                    .onChange(of: resultSelection) { _, new in
                        guard downloads.results.indices.contains(new) else { return }
                        withAnimation(.kLayout) {
                            proxy.scrollTo(downloads.results[new].id, anchor: .center)
                        }
                    }
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
                ScrollViewReader { proxy in
                    ScrollView {
                    LazyVStack(spacing: 4) {
                        ForEach(Array(library.tracks.enumerated()), id: \.element.id) { index, track in
                            libraryRow(index: index, track: track).id(track.id).cascade(index)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    }
                    .onChange(of: librarySelection) { _, new in
                        guard library.tracks.indices.contains(new) else { return }
                        withAnimation(.kLayout) {
                            proxy.scrollTo(library.tracks[new].id, anchor: .center)
                        }
                    }
                }
            }
        }
        .transition(.asymmetric(
            insertion: .move(edge: .trailing).combined(with: .opacity),
            removal: .opacity
        ))
    }

    /// The library row as its own function: the `ForEach` closure above was
    /// heavy enough that the type-checker gave up on the surrounding `Group`.
    private func libraryRow(index: Int, track: MusicTrack) -> LibraryRow {
        LibraryRow(
            track: track,
            index: index,
            selected: index == librarySelection,
            playing: player.current == track,
            isPlaying: player.isPlaying,
            queued: queue.contains(track),
            onQueue: { _ = queue.add(track) },
            accent: accent,
            dim: dim,
            foreground: foreground,
            font: bar.fontFamily,
            palette: palette,
            highlight: rowGlide,
            onPlay: { player.play(library.tracks, startAt: index) },
            onRemove: {
                library.remove(track)
                player.remove(track)
                queue.remove(track)
            }
        )
    }

    // MARK: playlists

    /// Naming a playlist: the queue becomes a named list.
    private var namingRow: some View {
        HStack(spacing: 8) {
            Image(systemName: "square.stack.badge.plus")
                .font(.system(size: 11, weight: .semibold))
                .foregroundColor(accent)
            TextField("Playlist name", text: $newName)
                .textFieldStyle(.plain)
                .font(.custom(bar.fontFamily, size: 12))
                .foregroundColor(foreground)
                .focused($namingFocused)
                .onSubmit { commitPlaylistName() }
                .onExitCommand { cancelNaming() }
            Text(queue.count == 1 ? "1 track" : "\(queue.count) tracks")
                .font(.custom(bar.fontFamily, size: 10))
                .foregroundColor(dim)
        }
        .padding(.horizontal, 15)
        .padding(.vertical, 9)
        .background(
            Capsule(style: .continuous)
                .fill(accent.opacity(0.1))
                .overlay(Capsule(style: .continuous).stroke(accent.opacity(0.5), lineWidth: 1.2))
        )
        .padding(.horizontal, 14)
        .padding(.bottom, 8)
        .transition(.move(edge: .top).combined(with: .opacity))
    }

    private var playlistsTab: some View {
        Group {
            if let name = openedPlaylist {
                playlistDetail(name)
            } else if playlists.isEmpty {
                emptyState(
                    icon: "square.stack",
                    title: "No playlists yet",
                    subtitle: "Line tracks up in the queue, then press s to save it here."
                )
            } else {
                playlistsList
            }
        }
        .transition(.asymmetric(
            insertion: .move(edge: .trailing).combined(with: .opacity),
            removal: .opacity
        ))
    }

    /// Inside one playlist: play from a track, or take a track out of it.
    private func playlistDetail(_ name: String) -> some View {
        let tracks = playlists.playlist(named: name)?.tracks ?? []
        return VStack(spacing: 0) {
            HStack(spacing: 8) {
                Button {
                    withAnimation(.kLayout) { openedPlaylist = nil }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 10, weight: .bold))
                            .foregroundColor(dim)
                        PlaylistMosaic(tracks: tracks, accent: accent, size: 40)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(name)
                                .font(.custom(bar.fontFamily, size: 15))
                                .foregroundColor(foreground)
                                .lineLimit(1)
                            Text(tracks.count == 1 ? "1 track" : "\(tracks.count) tracks")
                                .font(.custom(bar.fontFamily, size: 10.5))
                                .foregroundColor(foreground.opacity(0.55))
                        }
                    }
                }
                .pressBounce()

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)

            if tracks.isEmpty {
                emptyState(
                    icon: "music.note",
                    title: "Playlist is empty",
                    subtitle: "Press a on a playlist to add the playing track, or save the queue with s."
                )
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 2) {
                            ForEach(Array(tracks.enumerated()), id: \.element.id) { index, track in
                                detailRow(index: index, track: track, playlist: name)
                                    .id(track.id)
                                    .cascade(index)
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                    }
                    .onChange(of: trackSelection) { _, new in
                        guard tracks.indices.contains(new) else { return }
                        withAnimation(.kLayout) {
                            proxy.scrollTo(tracks[new].id, anchor: .center)
                        }
                    }
                }
            }
        }
        // Esc steps out of the playlist instead of closing the whole panel.
        .onExitCommand {
            withAnimation(.kLayout) { openedPlaylist = nil }
        }
    }

    private func detailRow(index: Int, track: MusicTrack, playlist: String) -> some View {
        let armed = index == trackSelection
        let playing = player.current == track
        return HStack(spacing: 10) {
            Text("\(index + 1)")
                .font(.custom(bar.fontFamily, size: 10))
                .foregroundColor(dim.opacity(0.7))
                .frame(width: 22, alignment: .trailing)
            ZStack {
                RoundedRectangle(cornerRadius: 6).fill(Color.white.opacity(0.06))
                if let image = ArtworkCache.shared.image(track.artworkPath) {
                    Image(nsImage: image).resizable().aspectRatio(contentMode: .fill)
                } else {
                    Image(systemName: "music.note").font(.system(size: 11)).foregroundColor(dim.opacity(0.7))
                }
            }
            .frame(width: 32, height: 32)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 1) {
                Text(track.displayTitle)
                    .font(.custom(bar.fontFamily, size: 12))
                    .foregroundColor(playing ? accent : foreground)
                    .lineLimit(1)
                Text(track.displayArtist)
                    .font(.custom(bar.fontFamily, size: 10))
                    .foregroundColor(dim)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if playing {
                Image(systemName: player.isPlaying ? "speaker.wave.2.fill" : "speaker.fill")
                    .font(.system(size: 10))
                    .foregroundColor(accent)
            }
            Button {
                playlists.remove(track, from: playlist)
                trackSelection = min(trackSelection, max(index - 1, 0))
            } label: {
                Image(systemName: "minus.circle")
                    .font(.system(size: 11))
                    .foregroundColor(dim.opacity(0.8))
            }
            .buttonStyle(.plain)
            .help("remove from playlist")
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .kitCard(selected: armed, palette: palette, highlight: rowGlide, radius: 11)
        .contentShape(RoundedRectangle(cornerRadius: 11))
        .onTapGesture {
            withAnimation(.kSelect) { trackSelection = index }
            _ = activate()
        }
    }

    private var playlistsList: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 4) {
                        ForEach(Array(playlists.playlists.enumerated()), id: \.element.id) { index, playlist in
                            playlistRow(index: index, playlist: playlist)
                                .id(playlist.id)
                                .cascade(index)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                }
                .onChange(of: playlistsSelection) { _, new in
                    guard playlists.playlists.indices.contains(new) else { return }
                    withAnimation(.kLayout) {
                        proxy.scrollTo(playlists.playlists[new].id, anchor: .center)
                    }
                }
            }
        }
    }

    private func playlistRow(index: Int, playlist: Playlist) -> some View {
        let armed = index == playlistsSelection
        let playing: Bool = {
            guard let current = player.current else { return false }
            return playlist.tracks.contains(current)
        }()
        return HStack(spacing: 12) {
            PlaylistMosaic(tracks: playlist.tracks, accent: accent, size: 46)
                .overlay(alignment: .bottomTrailing) {
                    if playing {
                        Equaliser(playing: player.isPlaying, color: accent)
                            .scaleEffect(0.7)
                            .padding(3)
                            .background(Circle().fill(Color.black.opacity(0.55)))
                            .offset(x: 6, y: 6)
                    }
                }
            VStack(alignment: .leading, spacing: 2) {
                Text(playlist.name)
                    .font(.custom(bar.fontFamily, size: 13.5))
                    .foregroundColor(foreground)
                    .lineLimit(1)
                Text(playlist.subtitle)
                    .font(.custom(bar.fontFamily, size: 10))
                    .foregroundColor(dim)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Text("\(playlist.count)")
                .font(.custom(bar.fontFamily, size: 10))
                .foregroundColor(dim)
            Image(systemName: "chevron.right")
                .font(.system(size: 9, weight: .semibold))
                .foregroundColor(dim.opacity(0.6))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .kitCard(selected: armed, palette: palette, highlight: rowGlide)
        .contentShape(RoundedRectangle(cornerRadius: 14))
        .onTapGesture {
            withAnimation(.kSelect) { playlistsSelection = index }
            _ = activate()
        }
    }

    private var queueTab: some View {
        Group {
            if queue.tracks.isEmpty {
                emptyState(
                    icon: "list.bullet",
                    title: "Queue is empty",
                    subtitle: "Press a on a library track to line it up for later."
                )
            } else {
                queueList
            }
        }
        .transition(.asymmetric(
            insertion: .move(edge: .trailing).combined(with: .opacity),
            removal: .opacity
        ))
    }

    private var queueList: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 4) { queueRows }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                }
                .onChange(of: queueSelection) { _, new in
                    guard queue.tracks.indices.contains(new) else { return }
                    withAnimation(.kLayout) {
                        proxy.scrollTo(queue.tracks[new].id, anchor: .center)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var queueRows: some View {
        ForEach(Array(queue.tracks.enumerated()), id: \.element.id) { index, track in
            QueueRow(
                track: track,
                index: index,
                selected: index == queueSelection,
                playing: player.current == track,
                accent: accent,
                dim: dim,
                foreground: foreground,
                font: bar.fontFamily,
                palette: palette,
                highlight: rowGlide,
                onPlay: { player.play(queue.tracks, startAt: index) },
                onRemove: { removeFromQueue(track) },
                onMove: { delta in shiftQueue(index, by: delta) }
            )
            .id(track.id)
            .cascade(index)
        }
    }

    // MARK: queue

    private func playQueue() {
        guard !queue.tracks.isEmpty else { return }
        player.play(queue.tracks, startAt: min(queueSelection, queue.tracks.count - 1))
    }

    private func removeFromQueue(_ track: MusicTrack) {
        queue.remove(track)
        queueSelection = min(queueSelection, max(queue.count - 1, 0))
    }

    private func shiftQueue(_ index: Int, by delta: Int) {
        guard queue.tracks.indices.contains(index) else { return }
        queue.move(index, by: delta)
        queueSelection = min(max(index + delta, 0), max(queue.count - 1, 0))
    }

    /// `a` — line the armed library track up for later.
    private func addArmedToQueue() -> KeyPress.Result {
        guard tab == .library, library.tracks.indices.contains(librarySelection) else { return .ignored }
        queue.add(library.tracks[librarySelection])
        return .handled
    }

    /// `x` — take the armed song back out.
    private func removeArmedFromQueue() -> KeyPress.Result {
        guard tab == .queue, queue.tracks.indices.contains(queueSelection) else { return .ignored }
        removeFromQueue(queue.tracks[queueSelection])
        return .handled
    }

    /// `J`/`K` — slide the armed song down or up the queue.
    private func shiftArmedInQueue(_ delta: Int) -> KeyPress.Result {
        guard tab == .queue, queue.tracks.indices.contains(queueSelection) else { return .ignored }
        shiftQueue(queueSelection, by: delta)
        return .handled
    }

    /// `h`/`l` — walk the tabs.
    private func shiftTab(_ delta: Int) -> KeyPress.Result {
        // Inside a playlist, `h` steps back to the list rather than the previous tab.
        if openedPlaylist != nil {
            guard delta < 0 else { return .handled }
            withAnimation(.kLayout) { openedPlaylist = nil }
            return .handled
        }
        let all = Tab.allCases
        guard let current = all.firstIndex(of: tab) else { return .ignored }
        let next = min(max(current + delta, 0), all.count - 1)
        guard next != current else { return .handled }
        withAnimation(.kLayout) { tab = all[next] }
        return .handled
    }

    /// `/` — hand the keyboard to the search field, opening that tab if needed.
    private func focusSearch() -> KeyPress.Result {
        if tab != .search {
            withAnimation(.kLayout) { tab = .search }
            focusedField = true
        } else {
            tabChanged(to: .search)
        }
        return .handled
    }

    private func emptyState(icon: String, title: String, subtitle: String) -> some View {
        KitEmpty(icon: icon, title: title, detail: subtitle, palette: palette)
            .transition(.opacity)
    }
}

// MARK: - rows

private extension Array {
    /// Nil instead of trapping when a list shrinks under an armed index.
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

private struct SearchRow: View {
    let result: MusicSearchResult
    let job: MusicDownloadJob?
    let selected: Bool
    let accent: Color
    let dim: Color
    let foreground: Color
    let font: String
    let palette: KitPalette
    let highlight: Namespace.ID
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
        .padding(.vertical, 7)
        .padding(.horizontal, 10)
        .kitCard(selected: selected, hovered: hovering, palette: palette, highlight: highlight)
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
    let selected: Bool
    let playing: Bool
    let isPlaying: Bool
    let queued: Bool
    let onQueue: () -> Void
    let accent: Color
    let dim: Color
    let foreground: Color
    let font: String
    let palette: KitPalette
    let highlight: Namespace.ID
    let onPlay: () -> Void
    let onRemove: () -> Void

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                if let image = ArtworkCache.shared.image(track.artworkPath) {
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
            Button(action: onQueue) {
                Image(systemName: queued ? "checkmark.circle.fill" : "plus.circle")
                    .font(.system(size: 12))
                    .foregroundColor(queued ? accent : dim)
            }
            .buttonStyle(.plain)
            .opacity(hovering || queued ? 1 : 0)
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
        .padding(.vertical, 6)
        .padding(.horizontal, 10)
        .overlay(alignment: .leading) {
            if playing {
                Capsule()
                    .fill(accent)
                    .frame(width: 3)
                    .padding(.vertical, 9)
                    .shadow(color: accent.opacity(0.7), radius: 4)
                    .transition(.scale)
            }
        }
        .kitCard(selected: selected, hovered: hovering, palette: palette, highlight: highlight, radius: 12)
        .contentShape(Rectangle())
        .onTapGesture(perform: onPlay)
        .onHover { hovering = $0 }
        .animation(.kLayout, value: playing)
    }
}

/// A queued song: the same shape as a library row, with the controls a running
/// order wants — nudge it up or down, or take it out.
private struct QueueRow: View {
    let track: MusicTrack
    let index: Int
    let selected: Bool
    let playing: Bool
    let accent: Color
    let dim: Color
    let foreground: Color
    let font: String
    let palette: KitPalette
    let highlight: Namespace.ID
    let onPlay: () -> Void
    let onRemove: () -> Void
    let onMove: (Int) -> Void

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                if let image = ArtworkCache.shared.image(track.artworkPath) {
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
                HStack(spacing: 8) {
                    control("chevron.up") { onMove(-1) }
                    control("chevron.down") { onMove(1) }
                    control("xmark") { onRemove() }
                }
                .transition(.opacity)
            }
            Text(MusicTrack.clock(track.duration))
                .font(.custom(font, size: 10))
                .foregroundColor(dim)
                .monospacedDigit()
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 10)
        .overlay(alignment: .leading) {
            if playing {
                Capsule()
                    .fill(accent)
                    .frame(width: 3)
                    .padding(.vertical, 9)
                    .shadow(color: accent.opacity(0.7), radius: 4)
            }
        }
        .kitCard(selected: selected, hovered: hovering, palette: palette, highlight: highlight, radius: 12)
        .contentShape(Rectangle())
        .onTapGesture(perform: onPlay)
        .onHover { hovering = $0 }
    }

    private func control(_ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(dim)
        }
        .buttonStyle(.plain)
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

/// The panel's keyboard, kept out of the panel's body so the type checker has an
/// easier time of it.
///
/// Arrow keys walk the armed row, return acts on it (play, or download), and
/// holding command turns the arrows into transport — volume on up/down, seek on
/// left/right. Space plays or pauses unless the search field has the keyboard.
private struct MusicKeys: ViewModifier {
    let move: (Int) -> KeyPress.Result
    let activate: () -> KeyPress.Result
    let togglePlay: () -> Void
    let seek: (Double) -> Void
    let volume: (Double) -> Void
    let fieldFocused: Bool

    func body(content: Content) -> some View {
        content
            .onKeyPress(.upArrow, phases: .down) { press in
                if press.modifiers.contains(.command) {
                    volume(0.05)
                    return .handled
                }
                return typed { move(-1) }
            }
            .onKeyPress(.downArrow, phases: .down) { press in
                if press.modifiers.contains(.command) {
                    volume(-0.05)
                    return .handled
                }
                return typed { move(1) }
            }
            .onKeyPress(.leftArrow, phases: .down) { press in
                guard press.modifiers.contains(.command) else { return .ignored }
                seek(-5)
                return .handled
            }
            .onKeyPress(.rightArrow, phases: .down) { press in
                guard press.modifiers.contains(.command) else { return .ignored }
                seek(5)
                return .handled
            }
            .onKeyPress(.return, phases: .down) { _ in typed(activate) }
            .onKeyPress(.space, phases: .down) { _ in
                typed {
                    togglePlay()
                    return .handled
                }
            }
    }

    /// A text field with the keyboard (the search box, or the playlist name)
    /// gets every key: Enter submits the field, arrows move its caret, and the
    /// transport must not act on them.
    private func typed(_ action: () -> KeyPress.Result) -> KeyPress.Result {
        fieldFocused ? .ignored : action()
    }
}

/// The letters the music panel adds on top of plain movement: the tabs on
/// `h`/`l`, and the queue on `a`/`x`/`J`/`K`. All of them stand down while the
/// search field has the keyboard, where they are just letters.
private struct MusicLetterKeys: ViewModifier {
    let shiftTab: (Int) -> KeyPress.Result
    let addToQueue: () -> KeyPress.Result
    let removeFromQueue: () -> KeyPress.Result
    let savePlaylist: () -> KeyPress.Result
    let playPlaylist: () -> KeyPress.Result
    let moveInQueue: (Int) -> KeyPress.Result
    let fieldFocused: Bool

    func body(content: Content) -> some View {
        content
            .onKeyPress("h", phases: .down) { _ in typed { shiftTab(-1) } }
            .onKeyPress("l", phases: .down) { _ in typed { shiftTab(1) } }
            .onKeyPress("a", phases: .down) { _ in typed(addToQueue) }
            .onKeyPress("x", phases: .down) { _ in typed(removeFromQueue) }
            .onKeyPress("s", phases: .down) { _ in typed(savePlaylist) }
            .onKeyPress("p", phases: .down) { _ in typed(playPlaylist) }
            .onKeyPress("J", phases: .down) { _ in typed { moveInQueue(1) } }
            .onKeyPress("K", phases: .down) { _ in typed { moveInQueue(-1) } }
    }

    private func typed(_ action: () -> KeyPress.Result) -> KeyPress.Result {
        fieldFocused ? .ignored : action()
    }
}

/// A playlist's cover: the first four distinct covers in a 2×2 grid, or one
/// cover, or a tinted tile when it has none.
struct PlaylistMosaic: View {
    let tracks: [MusicTrack]
    let accent: Color
    let size: CGFloat

    private var covers: [NSImage] {
        var seen = Set<String>()
        var images: [NSImage] = []
        for track in tracks {
            guard let path = track.artworkPath, !seen.contains(path),
                  let image = ArtworkCache.shared.image(path) else { continue }
            seen.insert(path)
            images.append(image)
            if images.count == 4 { break }
        }
        return images
    }

    var body: some View {
        let covers = self.covers
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
                .fill(LinearGradient(colors: [accent.opacity(0.4), accent.opacity(0.12)],
                                     startPoint: .topLeading, endPoint: .bottomTrailing))
            if covers.count >= 4 {
                let half = size / 2
                VStack(spacing: 0) {
                    HStack(spacing: 0) { cell(covers[0], half); cell(covers[1], half) }
                    HStack(spacing: 0) { cell(covers[2], half); cell(covers[3], half) }
                }
            } else if let first = covers.first {
                cell(first, size)
            } else {
                Image(systemName: "square.stack.fill")
                    .font(.system(size: size * 0.36))
                    .foregroundColor(.white.opacity(0.8))
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size * 0.22, style: .continuous))
        .shadow(color: .black.opacity(0.3), radius: 4, y: 2)
    }

    private func cell(_ image: NSImage, _ side: CGFloat) -> some View {
        Image(nsImage: image).resizable().aspectRatio(contentMode: .fill)
            .frame(width: side, height: side).clipped()
    }
}
