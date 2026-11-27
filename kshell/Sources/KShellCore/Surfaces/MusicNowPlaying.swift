import AppKit
import SwiftUI

/// Artwork, decoded once, and the colour each cover is tinted by.
///
/// The music panel draws a cover several times over — the hero, the ambient
/// glow behind it, every row — and re-reading it from disk in `body` is what
/// made long lists hitch. The tint is the cover's most vivid colour, which the
/// now-playing card takes as its accent, so the player re-colours itself to
/// whatever is playing.
final class ArtworkCache {
    static let shared = ArtworkCache()

    private var images: [String: NSImage] = [:]
    private var thumbs: [String: NSImage] = [:]
    private var tints: [String: NSColor] = [:]
    private var order: [String] = []
    private let limit = 160

    func image(_ path: String?) -> NSImage? {
        guard let path else { return nil }
        if let cached = images[path] { return cached }
        guard let image = NSImage(contentsOfFile: path) else { return nil }
        remember(path)
        images[path] = image
        return image
    }

    /// A tiny copy for the ambient glow: blurring 64 px costs nothing, and once
    /// it is stretched over the card nobody can tell.
    func thumbnail(_ path: String?) -> NSImage? {
        guard let path else { return nil }
        if let cached = thumbs[path] { return cached }
        guard let image = image(path) else { return nil }
        let size = NSSize(width: 48, height: 48)
        let thumb = NSImage(size: size)
        thumb.lockFocus()
        NSGraphicsContext.current?.imageInterpolation = .medium
        image.draw(in: NSRect(origin: .zero, size: size))
        thumb.unlockFocus()
        thumbs[path] = thumb
        return thumb
    }

    func tint(_ path: String?) -> NSColor? {
        guard let path else { return nil }
        if let cached = tints[path] { return cached }
        guard let image = image(path), let tint = Self.vividColour(of: image) else { return nil }
        tints[path] = tint
        return tint
    }

    private func remember(_ path: String) {
        order.append(path)
        guard order.count > limit else { return }
        let gone = order.removeFirst()
        images[gone] = nil
        thumbs[gone] = nil
    }

    /// The cover's most vivid colour: pixels weighted by how saturated and how
    /// bright they are, so a grey border or a black background doesn't win, then
    /// lifted until it reads on the panel's glass.
    static func vividColour(of image: NSImage) -> NSColor? {
        let side = 24
        guard let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ) else { return nil }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        image.draw(in: NSRect(x: 0, y: 0, width: side, height: side))
        NSGraphicsContext.restoreGraphicsState()

        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, total: CGFloat = 0
        for x in 0..<side {
            for y in 0..<side {
                guard let colour = rep.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) else { continue }
                var h: CGFloat = 0, s: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
                colour.getHue(&h, saturation: &s, brightness: &b, alpha: &a)
                let weight = pow(s, 2) * b * a + 0.002
                red += colour.redComponent * weight
                green += colour.greenComponent * weight
                blue += colour.blueComponent * weight
                total += weight
            }
        }
        guard total > 0 else { return nil }
        let mean = NSColor(deviceRed: red / total, green: green / total, blue: blue / total, alpha: 1)
        var h: CGFloat = 0, s: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        mean.getHue(&h, saturation: &s, brightness: &b, alpha: &a)
        // A nearly grey cover keeps a soft grey; anything coloured gets pushed
        // to a lively, legible version of itself.
        let saturation = s < 0.12 ? s : min(1, max(s * 1.35, 0.45))
        return NSColor(deviceHue: h, saturation: saturation, brightness: max(b, 0.78), alpha: 1)
    }
}

/// Whether text on `colour` should be dark.
private func prefersDarkText(on colour: NSColor) -> Bool {
    guard let rgb = colour.usingColorSpace(.deviceRGB) else { return true }
    let luma = 0.299 * rgb.redComponent + 0.587 * rgb.greenComponent + 0.114 * rgb.blueComponent
    return luma > 0.6
}

// MARK: - the card

/// The left column of the music panel: what is playing, big, and everything
/// you do to it.
///
/// It takes its colour from the cover, glows with it, and moves with the music:
/// the cover breathes on the bass, a spectrum ripples under the controls, and
/// the progress line is a wave that runs while the song plays and lies flat
/// when it is paused.
struct NowPlayingCard: View {
    @ObservedObject var player: MusicPlayer
    let theme: Theme
    let bar: BarConfig

    private var fallbackAccent: Color { theme.accent?.color ?? .accentColor }
    private var foreground: Color { theme.foreground?.color ?? .white }
    private var dim: Color { (theme.dim ?? theme.foreground)?.color ?? .gray }
    private var path: String? { player.current?.artworkPath }
    private var tintColour: NSColor? { ArtworkCache.shared.tint(path) }
    private var tint: Color { tintColour.map(Color.init(nsColor:)) ?? fallbackAccent }

    var body: some View {
        VStack(spacing: 0) {
            Group {
                if Hud.isHud(theme) {
                    HudCover(player: player, path: path, tint: tint)
                } else if Neon.isNeon(theme) {
                    NeonCover(player: player, path: path, magenta: theme.accent?.color ?? tint,
                              cyan: theme.highlight?.color ?? tint, yellow: theme.mid?.color ?? .yellow)
                } else if Arcane.isArcane(theme) {
                    ArcaneCover(player: player, path: path, tint: tint,
                                rose: theme.highlight?.color ?? tint)
                } else {
                    ArtworkHero(player: player, path: path, tint: tint, dim: dim)
                }
            }
            .padding(.top, 20)
            titles
                .padding(.top, 18)
            WaveProgress(player: player, tint: tint, dim: dim, font: bar.fontFamily)
                .padding(.top, 12)
            transport
                .padding(.top, 8)
            SpectrumRibbon(spectrum: player.spectrum, tint: tint, playing: player.isPlaying)
                .frame(height: 26)
                .padding(.top, 12)
            VolumeSlider(player: player, tint: tint, dim: dim)
                .padding(.top, 10)
            Spacer(minLength: 8)
            upNext
                .padding(.bottom, 14)
        }
        .padding(.horizontal, 20)
        .frame(maxHeight: .infinity)
        .background { AmbientArt(path: path, tint: tint, playing: player.isPlaying) }
        .clipped()
        .animation(.kLayout, value: path)
    }

    private var titles: some View {
        VStack(spacing: 3) {
            Text(player.current?.displayTitle ?? "Nothing playing")
                .font(.custom(bar.fontFamily, size: 16))
                .foregroundColor(player.current == nil ? dim : foreground)
                .lineLimit(1)
                .id("title-\(player.current?.path ?? "")")
                .transition(.blurSwap)
            Text(subtitle)
                .font(.custom(bar.fontFamily, size: 11.5))
                .foregroundColor(player.current == nil ? dim.opacity(0.7) : tint)
                .lineLimit(1)
                .id("artist-\(player.current?.path ?? "")")
                .transition(.blurSwap)
        }
        .frame(maxWidth: .infinity)
        .animation(.kBouncy, value: player.current?.path)
    }

    private var subtitle: String {
        guard let track = player.current else { return "Pick a song and press ⏎" }
        return track.album.isEmpty ? track.displayArtist : "\(track.displayArtist) · \(track.album)"
    }

    private var transport: some View {
        HStack(spacing: 0) {
            ToggleGlyph(symbol: "shuffle", on: player.shuffle, tint: tint, dim: dim) {
                player.toggleShuffle()
            }
            .help(player.shuffle ? "shuffle on" : "shuffle off")
            Spacer(minLength: 0)
            SkipButton(symbol: "backward.fill", colour: foreground) { player.previous() }
                .disabled(!player.hasTrack)
            Spacer(minLength: 0)
            PlayButton(playing: player.isPlaying, tint: tint, tintColour: tintColour,
                       hud: Hud.isHud(theme), arcane: Arcane.isArcane(theme),
                       neon: Neon.isNeon(theme)) { player.toggle() }
            Spacer(minLength: 0)
            SkipButton(symbol: "forward.fill", colour: foreground) { player.next() }
                .disabled(!player.hasTrack)
            Spacer(minLength: 0)
            ToggleGlyph(
                symbol: player.repeatMode.symbol,
                on: player.repeatMode != .off,
                tint: tint,
                dim: dim
            ) { player.cycleRepeat() }
            .help(player.repeatMode.label)
        }
        .padding(.horizontal, 4)
    }

    @ViewBuilder
    private var upNext: some View {
        if let next = player.upNext {
            Button { player.next() } label: {
                HStack(spacing: 10) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 6).fill(Color.white.opacity(0.06))
                        if let image = ArtworkCache.shared.image(next.artworkPath) {
                            Image(nsImage: image).resizable().aspectRatio(contentMode: .fill)
                        } else {
                            Image(systemName: "music.note").font(.system(size: 10)).foregroundColor(dim)
                        }
                    }
                    .frame(width: 30, height: 30)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    VStack(alignment: .leading, spacing: 1) {
                        Text("UP NEXT")
                            .font(.system(size: 8.5, weight: .bold))
                            .tracking(1.2)
                            .foregroundColor(tint.opacity(0.9))
                        Text(next.displayTitle)
                            .font(.custom(bar.fontFamily, size: 11))
                            .foregroundColor(foreground.opacity(0.85))
                            .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundColor(dim)
                }
                .padding(8)
                .background(RoundedRectangle(cornerRadius: 11).fill(Color.white.opacity(0.05)))
                .contentShape(Rectangle())
            }
            .pressBounce()
            .id(next.path)
            .transition(.asymmetric(
                insertion: .move(edge: .bottom).combined(with: .opacity),
                removal: .move(edge: .top).combined(with: .opacity)
            ))
        }
    }
}

// MARK: - artwork

/// The cover, big. A new song swings in like a card being dealt, the old one
/// swings out the other way, and while music plays the cover swells on the bass
/// and glows in its own colour.
private struct ArtworkHero: View {
    @ObservedObject var player: MusicPlayer
    let path: String?
    let tint: Color
    let dim: Color

    var body: some View {
        BeatPulse(spectrum: player.spectrum, playing: player.isPlaying, tint: tint) {
            ZStack {
                cover
                    .id(path ?? "none")
                    .transition(.asymmetric(
                        insertion: .modifier(active: DealEffect(amount: 1), identity: DealEffect(amount: 0)),
                        removal: .modifier(active: DealEffect(amount: -1), identity: DealEffect(amount: 0))
                    ))
            }
            .frame(width: 196, height: 196)
        }
        .scaleEffect(player.isPlaying || !player.hasTrack ? 1 : 0.9)
        .saturation(player.isPlaying || !player.hasTrack ? 1 : 0.7)
        .animation(.kBouncy, value: player.isPlaying)
        .animation(.kBouncy, value: path)
    }

    @ViewBuilder
    private var cover: some View {
        if let image = ArtworkCache.shared.image(path) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fill)
                .frame(width: 196, height: 196)
                .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
        } else {
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .fill(LinearGradient(
                    colors: [tint.opacity(0.45), tint.opacity(0.12)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                ))
                .overlay(
                    Image(systemName: "music.note")
                        .font(.system(size: 54, weight: .light))
                        .foregroundColor(.white.opacity(0.75))
                )
        }
    }
}

/// A card being dealt: `amount` 1 is arriving from the right, -1 leaving to the
/// left, 0 resting.
private struct DealEffect: ViewModifier {
    let amount: CGFloat

    func body(content: Content) -> some View {
        content
            .rotation3DEffect(.degrees(Double(amount) * -48), axis: (x: 0, y: 1, z: 0), perspective: 0.6)
            .rotationEffect(.degrees(Double(amount) * 8))
            .offset(x: amount * 120, y: abs(amount) * 14)
            .scaleEffect(1 - abs(amount) * 0.3)
            .blur(radius: abs(amount) * 10)
            .opacity(1 - Double(abs(amount)))
    }
}

/// Swells its content on the bass and lights a glow under it.
///
/// Its own view, so the 80-odd spectrum updates a second redraw only this
/// wrapper — the cover inside is unchanged and SwiftUI leaves it alone.
private struct BeatPulse<Content: View>: View {
    @ObservedObject var spectrum: AudioSpectrum
    let playing: Bool
    let tint: Color
    @ViewBuilder let content: Content

    private var bass: CGFloat {
        let bands = spectrum.bands
        guard playing, !bands.isEmpty else { return 0 }
        let low = bands.prefix(max(2, bands.count / 10))
        return CGFloat(low.reduce(0, +) / Float(low.count))
    }

    var body: some View {
        let level = min(1, bass * 1.15)
        content
            .scaleEffect(1 + level * 0.045)
            .shadow(color: tint.opacity(0.28 + level * 0.4), radius: 16 + level * 22, y: 8)
            .shadow(color: .black.opacity(0.35), radius: 10, y: 6)
            .animation(.spring(response: 0.16, dampingFraction: 0.55), value: level)
    }
}

/// The cover again, tiny and blurred to a glow that fills the card and turns
/// slowly while the song plays.
struct AmbientArt: View {
    let path: String?
    let tint: Color
    let playing: Bool

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: !playing)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            ZStack {
                tint.opacity(0.10)
                if let thumb = ArtworkCache.shared.thumbnail(path) {
                    Image(nsImage: thumb)
                        .resizable()
                        .interpolation(.medium)
                        .aspectRatio(contentMode: .fill)
                        .frame(width: 420, height: 420)
                        .rotationEffect(.degrees(t.truncatingRemainder(dividingBy: 360) * 7))
                        .scaleEffect(1.25 + 0.08 * sin(t * 0.5))
                        .blur(radius: 46)
                        .saturation(1.5)
                        .opacity(0.42)
                        .id(path)
                        .transition(.opacity)
                }
            }
            // Fade the glow out before the bottom edge: that edge meets the
            // frame, whose fillets are plain glass, so anything still painted
            // there shows as a seam at the curves.
            .mask(LinearGradient(
                stops: [
                    .init(color: .black, location: 0),
                    .init(color: .black, location: 0.45),
                    .init(color: .clear, location: 0.92),
                ],
                startPoint: .top,
                endPoint: .bottom
            ))
        }
        .animation(.easeInOut(duration: 0.8), value: path)
    }
}

// MARK: - progress

/// The progress line: a running wave over the part already played, a flat line
/// over what is left, and a handle between them that can be dragged to scrub.
/// Pausing lets the wave settle flat; playing whips it back up.
private struct WaveProgress: View {
    @ObservedObject var player: MusicPlayer
    let tint: Color
    let dim: Color
    let font: String

    @State private var scrub: Double?
    @State private var hovering = false

    var body: some View {
        VStack(spacing: 4) {
            TimelineView(.animation(paused: !player.isPlaying || scrub != nil)) { context in
                let phase = context.date.timeIntervalSinceReferenceDate * 7
                GeometryReader { geo in
                    let fraction = scrub ?? liveFraction
                    let width = geo.size.width
                    let x = width * fraction
                    ZStack(alignment: .leading) {
                        Capsule()
                            .fill(dim.opacity(0.25))
                            .frame(width: max(0, width - x - 6), height: 3)
                            .offset(x: x + 6)
                        Wave(progress: fraction, amplitude: player.isPlaying && scrub == nil ? 3.2 : 0, phase: phase)
                            .stroke(tint, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                            .animation(.kBouncy, value: player.isPlaying)
                            .animation(.kBouncy, value: scrub == nil)
                        Capsule()
                            .fill(tint)
                            .frame(width: scrub != nil ? 7 : (hovering ? 5 : 4), height: scrub != nil ? 22 : (hovering ? 18 : 14))
                            .shadow(color: tint.opacity(0.6), radius: scrub != nil ? 8 : 3)
                            .offset(x: x - 2)
                            .animation(.kHover, value: hovering)
                            .animation(.kHover, value: scrub != nil)
                    }
                    .frame(height: geo.size.height)
                    .contentShape(Rectangle())
                    .gesture(
                        DragGesture(minimumDistance: 0)
                            .onChanged { value in
                                scrub = min(1, max(0, value.location.x / max(width, 1)))
                            }
                            .onEnded { value in
                                player.seek(toFraction: value.location.x / max(width, 1))
                                scrub = nil
                            }
                    )
                }
            }
            .frame(height: 24)
            .onHover { hovering = $0 }

            HStack {
                Text(MusicTrack.clock(scrub.map { $0 * player.duration } ?? player.position))
                Spacer()
                Text("-" + MusicTrack.clock(max(0, player.duration - (scrub.map { $0 * player.duration } ?? player.position))))
            }
            .font(.custom(font, size: 10))
            .foregroundColor(scrub != nil ? tint : dim)
            .monospacedDigit()
        }
    }

    private var liveFraction: Double {
        guard player.duration > 0 else { return 0 }
        return min(1, max(0, player.livePosition() / player.duration))
    }
}

/// A sine wave from the left edge to `progress`, tapering in at the start so it
/// grows out of the line rather than starting mid-swing.
struct Wave: Shape {
    var progress: Double
    var amplitude: CGFloat
    var phase: Double

    var animatableData: AnimatablePair<Double, CGFloat> {
        get { AnimatablePair(progress, amplitude) }
        set {
            progress = newValue.first
            amplitude = newValue.second
        }
    }

    func path(in rect: CGRect) -> Path {
        var path = Path()
        let end = rect.width * progress
        let mid = rect.midY
        let wavelength: CGFloat = 26
        path.move(to: CGPoint(x: 0, y: mid))
        guard end > 1 else { return path }
        var x: CGFloat = 0
        while x <= end {
            let taper = min(1, x / 18, (end - x) / 10)
            let y = mid + sin(x / wavelength * 2 * .pi - phase) * amplitude * max(0, taper)
            path.addLine(to: CGPoint(x: x, y: y))
            x += 1.5
        }
        path.addLine(to: CGPoint(x: end, y: mid))
        return path
    }
}

// MARK: - controls

/// The big play button: a squircle that rounds into a circle while it plays,
/// with the glyph morphing between play and pause.
struct PlayButton: View {
    let playing: Bool
    let tint: Color
    let tintColour: NSColor?
    /// Stark: a hexagon in a turning ring.
    var hud = false
    /// Arcane: an orb of light with a halo.
    var arcane = false
    /// Neon: a slanted magenta block with a cyan echo.
    var neon = false
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            ZStack {
                if neon {
                    SlantShape(slant: 10)
                        .stroke(Color(nsColor: Neon.cyan), lineWidth: 1.4)
                        .frame(width: 66, height: 48)
                        .offset(x: 3, y: -2.5)
                    SlantShape(slant: 10)
                        .fill(tint)
                        .shadow(color: tint.opacity(hovering ? 1 : 0.7), radius: hovering ? 16 : 10)
                        .frame(width: 66, height: 48)
                } else if arcane {
                    Circle()
                        .stroke(tint.opacity(playing ? 0.5 : 0.2), lineWidth: 1)
                        .frame(width: 70, height: 70)
                        .scaleEffect(playing ? 1.06 : 0.95)
                    Circle()
                        .fill(RadialGradient(colors: [.white.opacity(0.9), tint, tint.opacity(0.6)],
                                             center: .topLeading, startRadius: 2, endRadius: 60))
                        .shadow(color: tint.opacity(hovering ? 1 : 0.75), radius: hovering ? 20 : 12)
                        .frame(width: 56, height: 56)
                } else if hud {
                    SpinningRing(colour: tint.opacity(playing ? 0.8 : 0.3), dash: [3, 4], width: 1.2, period: playing ? 3 : 12)
                        .frame(width: 66, height: 66)
                    HexagonShape()
                        .fill(tint.opacity(playing ? 0.9 : 0.25))
                        .overlay(HexagonShape().stroke(tint, lineWidth: 1.5))
                        .shadow(color: tint.opacity(hovering ? 0.9 : 0.6), radius: hovering ? 14 : 8)
                        .frame(width: 58, height: 50)
                } else {
                    RoundedRectangle(cornerRadius: playing ? 28 : 16, style: .continuous)
                        .fill(tint)
                        .shadow(color: tint.opacity(hovering ? 0.7 : 0.45), radius: hovering ? 16 : 10, y: 4)
                }
                Image(systemName: playing ? "pause.fill" : "play.fill")
                    .font(.system(size: 20, weight: .bold))
                    .foregroundColor(hud && !playing ? tint : (tintColour.map(prefersDarkText) ?? true ? .black.opacity(0.82) : .white))
                    .contentTransition(.symbolEffect(.replace.downUp))
                    .offset(x: playing ? 0 : 1.5)
            }
            .frame(width: playing || arcane || neon ? 56 : 62, height: 56)
            .contentShape(Rectangle())
        }
        .pressBounce()
        .scaleEffect(hovering ? 1.06 : 1)
        .rotationEffect(.degrees(playing || hud || arcane || neon ? 0 : -6))
        .onHover { hovering = $0 }
        .animation(.kBouncy, value: playing)
        .animation(.kHover, value: hovering)
    }
}

/// Previous / next: the glyph kicks in the direction it skips when pressed.
struct SkipButton: View {
    let symbol: String
    let colour: Color
    let action: () -> Void

    @State private var kicks = 0
    @State private var hovering = false

    var body: some View {
        Button {
            kicks += 1
            action()
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 17))
                .foregroundColor(colour.opacity(hovering ? 1 : 0.8))
                .symbolEffect(.bounce.down, value: kicks)
                .frame(width: 36, height: 36)
                .background(Circle().fill(Color.white.opacity(hovering ? 0.1 : 0)))
                .contentShape(Circle())
        }
        .pressBounce()
        .onHover { hovering = $0 }
        .animation(.kHover, value: hovering)
    }
}

/// Shuffle and repeat: dim when off, tinted with a dot under them when on.
private struct ToggleGlyph: View {
    let symbol: String
    let on: Bool
    let tint: Color
    let dim: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 3) {
                Image(systemName: symbol)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(on ? tint : dim.opacity(0.55))
                    .contentTransition(.symbolEffect(.replace))
                    .symbolEffect(.bounce, value: on)
                Circle()
                    .fill(tint)
                    .frame(width: 4, height: 4)
                    .scaleEffect(on ? 1 : 0.01)
                    .opacity(on ? 1 : 0)
            }
            .frame(width: 30, height: 30)
            .contentShape(Rectangle())
        }
        .pressBounce()
        .animation(.kBouncy, value: on)
        .animation(.kBouncy, value: symbol)
    }
}

/// A slim volume line with a speaker that fills its waves as it gets louder.
private struct VolumeSlider: View {
    @ObservedObject var player: MusicPlayer
    let tint: Color
    let dim: Color

    @State private var dragging = false
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: player.volume <= 0.001 ? "speaker.slash.fill" : "speaker.wave.3.fill",
                  variableValue: player.volume)
                .font(.system(size: 11))
                .foregroundColor(dim)
                .frame(width: 18)
                .contentTransition(.symbolEffect(.replace))
                .onTapGesture { player.volume = player.volume > 0 ? 0 : 0.6 }
            GeometryReader { geo in
                let thick: CGFloat = dragging || hovering ? 7 : 4
                ZStack(alignment: .leading) {
                    Capsule().fill(dim.opacity(0.22))
                    Capsule()
                        .fill(LinearGradient(colors: [tint.opacity(0.6), tint], startPoint: .leading, endPoint: .trailing))
                        .frame(width: max(thick, geo.size.width * player.volume))
                }
                .frame(height: thick)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            dragging = true
                            player.volume = min(1, max(0, value.location.x / max(geo.size.width, 1)))
                        }
                        .onEnded { _ in dragging = false }
                )
                .animation(.kHover, value: thick)
            }
            .frame(height: 16)
            .onHover { hovering = $0 }
            Text("\(Int((player.volume * 100).rounded()))")
                .font(.system(size: 9.5, weight: .medium).monospacedDigit())
                .foregroundColor(dragging ? tint : dim)
                .frame(width: 22, alignment: .trailing)
        }
    }
}

// MARK: - spectrum

/// A mirrored ribbon of the song's spectrum: bass in the middle, highs at the
/// edges, each bar rounded and fading from the cover's colour. It sinks to a
/// dotted line while nothing plays.
private struct SpectrumRibbon: View {
    @ObservedObject var spectrum: AudioSpectrum
    let tint: Color
    let playing: Bool

    private let half = 22

    var body: some View {
        Canvas { context, size in
            let bands = spectrum.bands
            let count = half * 2
            let slot = size.width / CGFloat(count)
            let barWidth = max(2, slot * 0.56)
            for i in 0..<count {
                // Mirror: the middle two bars are the lowest band.
                let distance = i < half ? half - 1 - i : i - half
                let value: CGFloat
                if playing, !bands.isEmpty {
                    // Use the lower two-thirds of the spectrum: the top is mostly
                    // air and would leave the edges dead.
                    let at = min(bands.count - 1, distance * (bands.count * 2 / 3) / half)
                    value = CGFloat(bands[at])
                } else {
                    value = 0
                }
                let height = max(barWidth, value * size.height)
                let rect = CGRect(
                    x: CGFloat(i) * slot + (slot - barWidth) / 2,
                    y: (size.height - height) / 2,
                    width: barWidth,
                    height: height
                )
                let fade = 1 - Double(distance) / Double(half) * 0.65
                context.fill(
                    Path(roundedRect: rect, cornerRadius: barWidth / 2),
                    with: .color(tint.opacity((playing ? 0.85 : 0.3) * fade))
                )
            }
        }
    }
}

// MARK: - transitions

extension AnyTransition {
    /// Text swapping through a blur, rising in and falling away.
    static var blurSwap: AnyTransition {
        .asymmetric(
            insertion: .modifier(active: BlurSwap(amount: 1), identity: BlurSwap(amount: 0)),
            removal: .modifier(active: BlurSwap(amount: -1), identity: BlurSwap(amount: 0))
        )
    }
}

private struct BlurSwap: ViewModifier {
    let amount: CGFloat

    func body(content: Content) -> some View {
        content
            .blur(radius: abs(amount) * 8)
            .offset(y: amount * 10)
            .scaleEffect(1 - abs(amount) * 0.08)
            .opacity(1 - Double(abs(amount)))
    }
}

/// A wave progress line for anything with a progress and a playing state — the
/// media island's, where the music panel's own is bound to the player.
struct WaveLine: View {
    let progress: Double
    let playing: Bool
    let tint: Color
    let dim: Color

    var body: some View {
        TimelineView(.animation(paused: !playing)) { context in
            let phase = context.date.timeIntervalSinceReferenceDate * 7
            GeometryReader { geo in
                let x = geo.size.width * min(1, max(0, progress))
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(dim.opacity(0.25))
                        .frame(width: max(0, geo.size.width - x - 6), height: 3)
                        .offset(x: x + 6)
                    Wave(progress: min(1, max(0, progress)), amplitude: playing ? 3 : 0, phase: phase)
                        .stroke(tint, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                        .animation(.kBouncy, value: playing)
                    Capsule()
                        .fill(tint)
                        .frame(width: 4, height: 14)
                        .shadow(color: tint.opacity(0.6), radius: 3)
                        .offset(x: x - 2)
                }
                .frame(height: geo.size.height)
            }
        }
        .frame(height: 18)
    }
}

// MARK: - the HUD cover

/// Stark: the cover as a disc at the heart of a projection. Two dashed rings
/// turn around it in opposite directions, a ring of spectrum bars flares out
/// from it with the music, and four brackets lock on to it.
private struct HudCover: View {
    @ObservedObject var player: MusicPlayer
    let path: String?
    let tint: Color

    var body: some View {
        ZStack {
            RadialSpectrum(spectrum: player.spectrum, tint: tint, playing: player.isPlaying, inner: 84)
                .frame(width: 236, height: 236)
            SpinningRing(colour: tint.opacity(0.75), dash: [2, 4], width: 2, period: player.isPlaying ? 9 : 30)
                .frame(width: 186, height: 186)
            SpinningRing(colour: tint.opacity(0.45), dash: [40, 14], width: 1, period: player.isPlaying ? 14 : 40, clockwise: false)
                .frame(width: 204, height: 204)
            BracketShape(arm: 16)
                .stroke(tint, lineWidth: 1.6)
                .shadow(color: tint, radius: 5)
                .frame(width: 214, height: 214)
            disc
                .id(path ?? "none")
                .transition(.asymmetric(
                    insertion: .scale(scale: 0.2).combined(with: .opacity),
                    removal: .scale(scale: 1.4).combined(with: .opacity)
                ))
        }
        .frame(width: 236, height: 236)
        .padding(.vertical, -20)
        .saturation(player.isPlaying || !player.hasTrack ? 1 : 0.6)
        .animation(.kBouncy, value: path)
        .animation(.kBouncy, value: player.isPlaying)
    }

    private var disc: some View {
        ZStack {
            Circle().fill(tint.opacity(0.12))
            if let image = ArtworkCache.shared.image(path) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
            } else {
                HexagonShape()
                    .stroke(tint, lineWidth: 1.5)
                    .frame(width: 60, height: 52)
                Image(systemName: "music.note")
                    .font(.system(size: 24, weight: .light))
                    .foregroundColor(tint)
            }
        }
        .frame(width: 160, height: 160)
        .clipShape(Circle())
        .overlay(Circle().stroke(tint.opacity(0.8), lineWidth: 1.5))
        .shadow(color: tint.opacity(0.6), radius: 14)
    }
}

/// The spectrum as bars standing out from a circle, mirrored left and right
/// with the bass at the top.
private struct RadialSpectrum: View {
    @ObservedObject var spectrum: AudioSpectrum
    let tint: Color
    let playing: Bool
    let inner: CGFloat

    var body: some View {
        Canvas { context, size in
            let bands = spectrum.bands
            let centre = CGPoint(x: size.width / 2, y: size.height / 2)
            let count = 72
            let reach = min(size.width, size.height) / 2 - inner
            var path = Path()
            for index in 0..<count {
                let angle = Double(index) / Double(count) * 2 * .pi - .pi / 2
                let fold = index <= count / 2 ? index : count - index
                var value: CGFloat = 0
                if playing, !bands.isEmpty {
                    let at = min(bands.count - 1, fold * (bands.count * 2 / 3) / (count / 2))
                    value = CGFloat(bands[at])
                }
                let length = 3 + value * reach
                let from = CGPoint(x: centre.x + cos(angle) * inner, y: centre.y + sin(angle) * inner)
                let to = CGPoint(x: centre.x + cos(angle) * (inner + length), y: centre.y + sin(angle) * (inner + length))
                path.move(to: from)
                path.addLine(to: to)
            }
            context.stroke(path, with: .color(tint.opacity(playing ? 0.85 : 0.3)),
                           style: StrokeStyle(lineWidth: 2.2, lineCap: .round))
        }
    }
}

// MARK: - the Arcane cover

/// Arcane: the cover as a glowing disc at the centre of a magic circle, the
/// spectrum flaring round it as a soft aura, sparks orbiting it while it plays.
private struct ArcaneCover: View {
    @ObservedObject var player: MusicPlayer
    let path: String?
    let tint: Color
    let rose: Color

    var body: some View {
        ZStack {
            RadialSpectrum(spectrum: player.spectrum, tint: tint, playing: player.isPlaying, inner: 92)
                .frame(width: 236, height: 236)
                .opacity(0.8)
            MagicCircle(violet: tint, rose: rose, lineWidth: 1.2, period: player.isPlaying ? 16 : 40)
                .frame(width: 210, height: 210)
            ForEach(0..<3, id: \.self) { index in
                Spinning(period: 4 + Double(index) * 1.7, clockwise: index != 1) {
                    Image(systemName: "sparkle")
                        .font(.system(size: 9 - CGFloat(index)))
                        .foregroundColor(.white)
                        .shadow(color: index == 1 ? rose : tint, radius: 5)
                        .offset(y: -96 + CGFloat(index) * 8)
                }
                .opacity(player.isPlaying ? 1 : 0.25)
            }
            disc
                .id(path ?? "none")
                .transition(.asymmetric(
                    insertion: .scale(scale: 0.1).combined(with: .opacity),
                    removal: .scale(scale: 1.3).combined(with: .opacity)
                ))
        }
        .frame(width: 236, height: 236)
        .padding(.vertical, -20)
        .animation(.kBouncy, value: path)
        .animation(.kBouncy, value: player.isPlaying)
    }

    private var disc: some View {
        ZStack {
            Circle().fill(RadialGradient(colors: [rose.opacity(0.5), tint.opacity(0.25)],
                                         center: .center, startRadius: 4, endRadius: 80))
            if let image = ArtworkCache.shared.image(path) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fill)
            } else {
                Image(systemName: "wand.and.stars")
                    .font(.system(size: 34, weight: .light))
                    .foregroundStyle(LinearGradient(colors: [.white, rose], startPoint: .top, endPoint: .bottom))
            }
        }
        .frame(width: 150, height: 150)
        .clipShape(Circle())
        .overlay(Circle().stroke(LinearGradient(colors: [rose, tint], startPoint: .top, endPoint: .bottom), lineWidth: 1.5))
        .shadow(color: tint, radius: 16)
        .shadow(color: rose.opacity(0.5), radius: 30)
    }
}

// MARK: - the Neon cover

/// Neon: the cover in front of a synthwave sun, a grid underneath, and two
/// colour-fringe copies — cyan and magenta — that split away from it on the
/// bass, like a tape losing its registration.
private struct NeonCover: View {
    @ObservedObject var player: MusicPlayer
    let path: String?
    let magenta: Color
    let cyan: Color
    let yellow: Color

    var body: some View {
        ZStack {
            SynthSun(yellow: yellow, magenta: magenta)
                .frame(width: 190, height: 190)
                .offset(y: -18)
            NeonSplit(spectrum: player.spectrum, playing: player.isPlaying, magenta: magenta, cyan: cyan) {
                cover
            }
            .id(path ?? "none")
            .transition(.asymmetric(
                insertion: .move(edge: .trailing).combined(with: .opacity),
                removal: .move(edge: .leading).combined(with: .opacity)
            ))
        }
        .frame(width: 236, height: 236)
        .padding(.vertical, -20)
        .animation(.kBouncy, value: path)
    }

    private var cover: some View {
        ZStack {
            Rectangle().fill(Color.black.opacity(0.6))
            if let image = ArtworkCache.shared.image(path) {
                Image(nsImage: image).resizable().interpolation(.high).aspectRatio(contentMode: .fill)
            } else {
                Image(systemName: "waveform")
                    .font(.system(size: 40, weight: .heavy))
                    .foregroundStyle(LinearGradient(colors: [yellow, magenta], startPoint: .top, endPoint: .bottom))
            }
        }
        .frame(width: 140, height: 140)
        .clipped()
        .overlay(Rectangle().stroke(magenta, lineWidth: 1.5))
        .shadow(color: magenta.opacity(0.8), radius: 12)
    }
}

/// Splits its content into cyan and magenta copies by the bass level.
private struct NeonSplit<Content: View>: View {
    @ObservedObject var spectrum: AudioSpectrum
    let playing: Bool
    let magenta: Color
    let cyan: Color
    @ViewBuilder let content: Content

    private var bass: CGFloat {
        let bands = spectrum.bands
        guard playing, !bands.isEmpty else { return 0 }
        let low = bands.prefix(max(2, bands.count / 10))
        return CGFloat(low.reduce(0, +) / Float(low.count))
    }

    var body: some View {
        let split = 1.5 + min(1, bass * 1.2) * 7
        ZStack {
            content.colorMultiply(cyan).opacity(0.7).offset(x: -split, y: split * 0.3).blendMode(.screen)
            content.colorMultiply(magenta).opacity(0.7).offset(x: split, y: -split * 0.3).blendMode(.screen)
            content
        }
        .animation(.spring(response: 0.14, dampingFraction: 0.6), value: split)
    }
}
