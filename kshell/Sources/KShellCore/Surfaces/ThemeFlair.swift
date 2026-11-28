import SwiftUI

/// The flagship themes' rows in the theme picker carry a taste of the theme
/// itself, whatever theme the picker is wearing: Neon's name splits into
/// chromatic fringes and decodes, with a tube that strikes on beneath it;
/// Arcane's glows, with a sigil turning behind its colours and sparkles
/// winking round it; Stark's types itself out in capitals between closing
/// brackets, with a scan crossing the row; Eclipse's waxes in from behind the
/// moon while a small eclipse behind its colours goes to totality; Abyss's
/// rises into place with bubbles drifting up and a wave drawn beneath it.
/// Quiet at rest, alive when the row is armed or under the pointer.
enum Flagship {
    case neon, arcane, stark, eclipse, abyss

    init?(_ theme: ThemeDefinition) {
        switch theme.name.lowercased() {
        case "neon": self = .neon
        case "arcane": self = .arcane
        case "stark": self = .stark
        case "eclipse": self = .eclipse
        case "abyss": self = .abyss
        default: return nil
        }
    }
}

/// A flagship theme's name, lettered its own way. Each time the row is armed
/// the lettering plays again.
struct FlagshipName: View {
    let flagship: Flagship
    let name: String
    let family: String
    let accent: Color
    let highlight: Color
    let foreground: Color
    let open: Bool

    @State private var armed = 0

    var body: some View {
        lettering
            .id(armed)
            .onChange(of: open) { _, now in if now { armed += 1 } }
    }

    @ViewBuilder private var lettering: some View {
        switch flagship {
        case .neon:
            DecodingChromaText(text: name, font: .custom(family, size: 14.5),
                               magenta: accent, cyan: highlight, foreground: foreground)
        case .arcane:
            Text(name)
                .font(.custom(family, size: 14.5).italic())
                .foregroundColor(foreground)
                .shadow(color: accent.opacity(open ? 0.95 : 0.5), radius: open ? 6 : 3)
                .shadow(color: highlight.opacity(open ? 0.5 : 0), radius: 10)
        case .stark:
            TypedText(text: name.uppercased(), cursor: accent, delay: 0.05, perCharacter: 0.035)
                .font(.system(size: 13, weight: .semibold, design: .monospaced))
                .tracking(2.2)
                .foregroundColor(foreground)
                .shadow(color: accent.opacity(0.6), radius: 3)
        case .eclipse:
            Text(name.uppercased())
                .font(.system(size: 13.5, weight: .medium, design: .serif))
                .tracking(3)
                .foregroundStyle(LinearGradient(colors: [foreground, accent], startPoint: .leading, endPoint: .trailing))
                .shadow(color: accent.opacity(open ? 0.7 : 0.3), radius: open ? 6 : 3)
                .modifier(WaxingOnce())
        case .abyss:
            Text(name)
                .font(.system(size: 15, weight: .semibold, design: .rounded))
                .foregroundStyle(LinearGradient(colors: [foreground, accent], startPoint: .top, endPoint: .bottom))
                .shadow(color: accent.opacity(open ? 0.85 : 0.35), radius: open ? 7 : 3)
                .modifier(RisingOnce(armed: armed > 0))
        }
    }
}

/// Wax in once, as it appears.
private struct WaxingOnce: ViewModifier {
    @State private var shown = false

    func body(content: Content) -> some View {
        content
            .modifier(Waxing(revealed: shown))
            .onAppear {
                guard !Motion.reduced else {
                    shown = true
                    return
                }
                withAnimation(.easeOut(duration: 0.7)) { shown = true }
            }
    }
}

/// Rise into place once, as it appears — but only when it's been armed, so a
/// list scrolling by doesn't bob.
private struct RisingOnce: ViewModifier {
    let armed: Bool
    @State private var risen = false

    func body(content: Content) -> some View {
        content
            .offset(y: risen || !armed ? 0 : 9)
            .opacity(risen || !armed ? 1 : 0)
            .onAppear {
                guard armed, !Motion.reduced else { return }
                withAnimation(.spring(response: 0.5, dampingFraction: 0.65)) { risen = true }
            }
    }
}

/// The flagship's motion round its row.
struct FlagshipFlair: ViewModifier {
    let flagship: Flagship
    let accent: Color
    let highlight: Color
    let open: Bool

    func body(content: Content) -> some View {
        switch flagship {
        case .neon:
            content.overlay(alignment: .bottom) { NeonRowTube(magenta: accent, cyan: highlight, open: open) }
        case .arcane:
            content
                .background(alignment: .leading) { ArcaneRowSigil(violet: accent, rose: highlight, open: open) }
                .overlay {
                    if open {
                        Twinkles(colours: [accent, highlight, .white], count: 5)
                            .transition(.opacity)
                    }
                }
        case .stark:
            content.overlay { StarkRowScan(accent: accent, ice: highlight, open: open) }
        case .eclipse:
            content
                .background(alignment: .leading) { EclipseRowSky(gold: accent, silver: highlight, open: open) }
        case .abyss:
            content
                .overlay(alignment: .bottom) { AbyssRowTide(teal: accent, jelly: highlight, open: open) }
                .overlay {
                    if open {
                        RisingBubbles(colours: [accent, highlight, .white], count: 5)
                            .transition(.opacity)
                    }
                }
        }
    }
}

extension View {
    /// Dress a theme picker row if its theme is a flagship.
    @ViewBuilder
    func flagshipFlair(_ flagship: Flagship?, accent: Color, highlight: Color, open: Bool) -> some View {
        if let flagship {
            modifier(FlagshipFlair(flagship: flagship, accent: accent, highlight: highlight, open: open))
        } else {
            self
        }
    }
}

// MARK: - Neon

/// A tube along the foot of the row: a dim, unlit glass line at rest that
/// stutters on when the row is armed, the way a sign catches.
private struct NeonRowTube: View {
    let magenta: Color
    let cyan: Color
    let open: Bool

    @State private var lit: Double = 0.18
    /// Bumped by each strike and dim, so a strike's late steps can tell they
    /// have been overtaken.
    @State private var generation = 0

    var body: some View {
        Capsule()
            .fill(LinearGradient(colors: [magenta, magenta, cyan], startPoint: .leading, endPoint: .trailing))
            .frame(height: 1.5)
            .shadow(color: magenta.opacity(lit), radius: 4)
            .shadow(color: cyan.opacity(lit * 0.6), radius: 7)
            .opacity(lit)
            .padding(.horizontal, 14)
            .padding(.bottom, 3)
            .allowsHitTesting(false)
            .onAppear { if open { strike() } }
            .onChange(of: open) { _, now in now ? strike() : dim() }
    }

    private func strike() {
        guard !Motion.reduced else {
            lit = 1
            return
        }
        generation += 1
        let strike = generation
        for (delay, value) in [(0.0, 0.9), (0.05, 0.15), (0.11, 1.0), (0.17, 0.35), (0.24, 1.0)] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
                guard generation == strike else { return }
                lit = value
            }
        }
    }

    private func dim() {
        generation += 1
        withAnimation(.easeOut(duration: 0.25)) { lit = 0.18 }
    }
}

// MARK: - Arcane

/// A faint seven-pointed sigil behind the theme's colours, turning slowly and
/// brightening while the row is armed.
private struct ArcaneRowSigil: View {
    let violet: Color
    let rose: Color
    let open: Bool

    var body: some View {
        ZStack {
            Circle()
                .stroke(violet.opacity(0.5), lineWidth: 0.8)
            StarPolygon(points: 7, skip: 3)
                .stroke(LinearGradient(colors: [violet, rose], startPoint: .top, endPoint: .bottom), lineWidth: 0.9)
                .padding(4)
        }
        .frame(width: 46, height: 46)
        .shadow(color: violet.opacity(open ? 0.8 : 0), radius: 5)
        .modifier(Turning(on: open, period: 14))
        .opacity(open ? 0.85 : 0.28)
        .scaleEffect(open ? 1.08 : 0.92)
        .padding(.leading, 40)
        .allowsHitTesting(false)
        .animation(.easeInOut(duration: 0.45), value: open)
    }
}

/// Spins only while `on`, so a resting row costs nothing.
private struct Turning: ViewModifier {
    let on: Bool
    let period: Double

    func body(content: Content) -> some View {
        if on && !Motion.reduced {
            Spinning(period: period) { content }
        } else {
            content
        }
    }
}

// MARK: - Stark

/// Corner brackets round the row that close in and brighten when it's armed,
/// with a scan line crossing it once.
private struct StarkRowScan: View {
    let accent: Color
    let ice: Color
    let open: Bool

    @State private var scan: CGFloat = -0.2

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                BracketShape(arm: 9)
                    .stroke(accent.opacity(open ? 0.95 : 0.35), lineWidth: open ? 1.4 : 1)
                    .shadow(color: accent.opacity(open ? 0.8 : 0), radius: 4)
                    .padding(open ? 3 : 7)

                LinearGradient(colors: [ice.opacity(0), ice.opacity(0.22), ice.opacity(0)],
                               startPoint: .leading, endPoint: .trailing)
                    .frame(width: 70)
                    .offset(x: scan * geo.size.width - 35)
                    .opacity(scan > -0.2 && scan < 1.2 ? 1 : 0)
            }
        }
        .allowsHitTesting(false)
        .animation(.spring(response: 0.3, dampingFraction: 0.7), value: open)
        .onChange(of: open) { _, now in if now { sweep() } }
    }

    private func sweep() {
        guard !Motion.reduced else { return }
        // back to the start unseen, then across
        var reset = Transaction()
        reset.disablesAnimations = true
        withTransaction(reset) { scan = -0.2 }
        DispatchQueue.main.async {
            withAnimation(.easeInOut(duration: 0.6)) { scan = 1.2 }
        }
    }
}

// MARK: - Eclipse

/// A small eclipse behind the theme's colours: at rest a faint crescent; armed,
/// the moon slides over to totality and the corona blooms round it.
private struct EclipseRowSky: View {
    let gold: Color
    let silver: Color
    let open: Bool

    var body: some View {
        ZStack {
            CoronaGlow(radius: 21)
                .opacity(open ? 0.95 : 0)
                .scaleEffect(open ? 1 : 0.7)
            Circle()
                .fill(silver.opacity(0.55))
                .frame(width: 40, height: 40)
                .opacity(open ? 0 : 1)
            Circle()
                .fill(Color(nsColor: Eclipse.night))
                .overlay(Circle().stroke(silver.opacity(open ? 0.4 : 0.15), lineWidth: 0.8))
                .frame(width: 41, height: 41)
                .offset(x: open ? 0 : 11, y: open ? 0 : -6)
        }
        .frame(width: 46, height: 46)
        .opacity(open ? 1 : 0.4)
        .padding(.leading, 40)
        .allowsHitTesting(false)
        .animation(.easeInOut(duration: 0.6), value: open)
    }
}

// MARK: - Abyss

/// A wave along the foot of the row, drawn in as it's armed.
private struct AbyssRowTide: View {
    let teal: Color
    let jelly: Color
    let open: Bool

    var body: some View {
        TideLine(amplitude: 1.5, wavelength: 24)
            .trim(from: 0, to: open ? 1 : 0.0)
            .stroke(LinearGradient(colors: [teal, jelly], startPoint: .leading, endPoint: .trailing), lineWidth: 1.2)
            .frame(height: 5)
            .shadow(color: teal, radius: 4)
            .padding(.horizontal, 16)
            .padding(.bottom, 2)
            .allowsHitTesting(false)
            .animation(.easeOut(duration: 0.55), value: open)
    }
}
