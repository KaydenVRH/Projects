import SwiftUI

/// The flagship themes' rows in the theme picker carry a taste of the theme
/// itself, whatever theme the picker is wearing: Neon's name splits into
/// chromatic fringes and decodes, with a tube that strikes on beneath it;
/// Arcane's glows, with a sigil turning behind its colours and sparkles
/// winking round it; Stark's types itself out in capitals between closing
/// brackets, with a scan crossing the row. Quiet at rest, alive when the row is
/// armed or under the pointer.
enum Flagship {
    case neon, arcane, stark

    init?(_ theme: ThemeDefinition) {
        switch theme.name.lowercased() {
        case "neon": self = .neon
        case "arcane": self = .arcane
        case "stark": self = .stark
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
