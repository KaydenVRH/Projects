import SwiftUI

/// The pieces every kshell menu is built from, so they read as one family: a
/// header with an icon tile, rows on soft cards with one gliding highlight,
/// round icon bubbles, a springy toggle, and key hints drawn as keycaps.
///
/// They take the theme's colours as plain values rather than the view model, so
/// any panel can use them without being rebuilt around them.
struct KitPalette {
    let accent: Color
    let foreground: Color
    let dim: Color
    let font: String
    /// The Stark HUD: brackets, hexagons and hard edges instead of soft cards.
    let hud: Bool
    /// The Arcane theme: glow, gradients and sparkles.
    let arcane: Bool
    /// The theme's second colour (rose, for Arcane).
    let highlight: Color

    init(theme: Theme, bar: BarConfig) {
        self.init(theme: theme, font: bar.fontFamily)
    }

    init(theme: Theme, font: String) {
        accent = theme.accent?.color ?? .accentColor
        foreground = theme.foreground?.color ?? .white
        dim = (theme.dim ?? theme.foreground)?.color ?? .gray
        self.font = font
        hud = Hud.isHud(theme)
        arcane = !hud && Arcane.isArcane(theme)
        highlight = theme.highlight?.color ?? accent
    }
}

// MARK: - header

/// A menu's title: an icon on a tile lit with the accent, the title, a line
/// under it, and room on the right for a control.
struct KitHeader<Trailing: View>: View {
    let icon: String
    let title: String
    var subtitle: String?
    let palette: KitPalette
    @ViewBuilder var trailing: Trailing

    @State private var arrived = false

    var body: some View {
        if palette.hud { hudBody } else if palette.arcane { arcaneBody } else { softBody }
    }

    /// Arcane: the icon at the heart of a turning magic circle, the title lit
    /// with a violet-to-rose gradient, sparkles winking around it.
    private var arcaneBody: some View {
        HStack(spacing: 12) {
            ZStack {
                MagicCircle(violet: palette.accent, rose: palette.highlight, lineWidth: 1, period: 12)
                    .frame(width: 44, height: 44)
                Image(systemName: icon)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(LinearGradient(colors: [palette.highlight, palette.accent],
                                                    startPoint: .top, endPoint: .bottom))
                    .shadow(color: palette.accent, radius: 6)
                    .symbolEffect(.bounce, value: arrived)
            }
            .frame(width: 44, height: 44)
            .scaleEffect(arrived ? 1 : 0.2)
            .rotationEffect(.degrees(arrived ? 0 : -120))
            .opacity(arrived ? 1 : 0)

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.custom(palette.font, size: 19).italic())
                    .foregroundStyle(LinearGradient(colors: [palette.foreground, palette.highlight],
                                                    startPoint: .leading, endPoint: .trailing))
                    .shadow(color: palette.accent.opacity(0.8), radius: 8)
                if let subtitle {
                    Text("✦ " + subtitle)
                        .font(.custom(palette.font, size: 11).italic())
                        .foregroundColor(palette.foreground.opacity(0.6))
                        .contentTransition(.numericText())
                }
            }
            Spacer(minLength: 8)
            trailing
        }
        .padding(.horizontal, 18)
        .padding(.top, 16)
        .padding(.bottom, 12)
        .background(Twinkles(colours: [palette.highlight, palette.accent, .white], count: 5).opacity(0.8))
        .onAppear {
            withAnimation(.kBouncy.delay(0.08)) { arrived = true }
        }
    }

    /// Stark: the icon in a hexagon inside a turning dashed ring, the title in
    /// spaced capitals, and the subtitle as a readout.
    private var hudBody: some View {
        HStack(spacing: 12) {
            ZStack {
                SpinningRing(colour: palette.accent.opacity(0.7), dash: [3, 3], width: 1, period: 6)
                    .frame(width: 42, height: 42)
                HexagonShape()
                    .fill(palette.accent.opacity(0.14))
                    .overlay(HexagonShape().stroke(palette.accent, lineWidth: 1.4))
                    .shadow(color: palette.accent.opacity(0.8), radius: 6)
                    .frame(width: 32, height: 28)
                Image(systemName: icon)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(palette.accent)
                    .symbolEffect(.bounce, value: arrived)
            }
            .frame(width: 42, height: 42)
            .scaleEffect(arrived ? 1 : 0.3)
            .opacity(arrived ? 1 : 0)

            VStack(alignment: .leading, spacing: 2) {
                Text(title.uppercased())
                    .font(.system(size: 15, weight: .semibold, design: .monospaced))
                    .tracking(2.5)
                    .foregroundColor(palette.foreground)
                    .shadow(color: palette.accent.opacity(0.6), radius: 4)
                if let subtitle {
                    Text("// " + subtitle.uppercased())
                        .font(.system(size: 9.5, weight: .medium, design: .monospaced))
                        .tracking(1)
                        .foregroundColor(palette.accent.opacity(0.8))
                        .contentTransition(.numericText())
                }
            }
            Spacer(minLength: 8)
            trailing
        }
        .padding(.horizontal, 18)
        .padding(.top, 16)
        .padding(.bottom, 12)
        .overlay(alignment: .bottom) {
            // a rule with a bright head, drawn across under the title
            HStack(spacing: 0) {
                Rectangle().fill(palette.accent).frame(width: 36, height: 2)
                Rectangle().fill(palette.accent.opacity(0.25)).frame(height: 1)
            }
            .shadow(color: palette.accent.opacity(0.8), radius: 3)
            .padding(.horizontal, 18)
            .scaleEffect(x: arrived ? 1 : 0, anchor: .leading)
        }
        .onAppear {
            withAnimation(.kBouncy.delay(0.08)) { arrived = true }
        }
    }

    private var softBody: some View {
        HStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .fill(LinearGradient(
                        colors: [palette.accent.opacity(0.95), palette.accent.opacity(0.55)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ))
                    .shadow(color: palette.accent.opacity(0.45), radius: 10, y: 3)
                Image(systemName: icon)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(.black.opacity(0.75))
                    .symbolEffect(.bounce, value: arrived)
            }
            .frame(width: 36, height: 36)
            .rotationEffect(.degrees(arrived ? 0 : -14))
            .scaleEffect(arrived ? 1 : 0.6)

            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.custom(palette.font, size: 19))
                    .foregroundColor(palette.foreground)
                if let subtitle {
                    Text(subtitle)
                        .font(.custom(palette.font, size: 11.5))
                        .foregroundColor(palette.foreground.opacity(0.6))
                        .contentTransition(.numericText())
                }
            }
            Spacer(minLength: 8)
            trailing
        }
        .padding(.horizontal, 18)
        .padding(.top, 16)
        .padding(.bottom, 12)
        .onAppear {
            withAnimation(.kBouncy.delay(0.08)) { arrived = true }
        }
    }
}

extension KitHeader where Trailing == EmptyView {
    init(icon: String, title: String, subtitle: String? = nil, palette: KitPalette) {
        self.init(icon: icon, title: title, subtitle: subtitle, palette: palette) { EmptyView() }
    }
}

// MARK: - key hints

/// The keys a menu answers to, as little keycaps along its foot.
struct KeyHints: View {
    let hints: [(key: String, label: String)]
    let palette: KitPalette

    var body: some View {
        if palette.hud {
            // Stark: [KEY] LABEL, as a readout rather than keycaps.
            HStack(spacing: 12) {
                ForEach(Array(hints.enumerated()), id: \.offset) { _, hint in
                    HStack(spacing: 4) {
                        Text("[\(hint.key.uppercased())]")
                            .foregroundColor(palette.accent)
                        Text(hint.label.uppercased())
                            .foregroundColor(palette.foreground.opacity(0.6))
                    }
                }
            }
            .font(.system(size: 9.5, weight: .medium, design: .monospaced))
            .lineLimit(1)
        } else if palette.arcane {
            // Arcane: glowing runestones and italic labels.
            HStack(spacing: 12) {
                ForEach(Array(hints.enumerated()), id: \.offset) { _, hint in
                    HStack(spacing: 5) {
                        Text(hint.key)
                            .font(.system(size: 10, weight: .semibold, design: .rounded))
                            .foregroundColor(palette.foreground)
                            .padding(.horizontal, 7)
                            .frame(minHeight: 18)
                            .background(
                                Capsule()
                                    .fill(LinearGradient(colors: [palette.accent.opacity(0.35), palette.highlight.opacity(0.2)],
                                                         startPoint: .leading, endPoint: .trailing))
                                    .overlay(Capsule().stroke(palette.accent.opacity(0.6), lineWidth: 0.8))
                                    .shadow(color: palette.accent.opacity(0.6), radius: 4)
                            )
                        Text(hint.label)
                            .font(.custom(palette.font, size: 11).italic())
                            .foregroundColor(palette.foreground.opacity(0.6))
                    }
                }
            }
            .lineLimit(1)
        } else {
            softBody
        }
    }

    private var softBody: some View {
        HStack(spacing: 12) {
            ForEach(Array(hints.enumerated()), id: \.offset) { _, hint in
                HStack(spacing: 5) {
                    Text(hint.key)
                        .font(.system(size: 10.5, weight: .semibold, design: .rounded))
                        .foregroundColor(palette.foreground.opacity(0.9))
                        .padding(.horizontal, 6)
                        .frame(minWidth: 20, minHeight: 19)
                        .background(
                            RoundedRectangle(cornerRadius: 5, style: .continuous)
                                .fill(Color.white.opacity(0.08))
                                .overlay(
                                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                                        .stroke(Color.white.opacity(0.12), lineWidth: 1)
                                )
                                .shadow(color: .black.opacity(0.25), radius: 0, y: 1.5)
                        )
                    Text(hint.label)
                        .font(.custom(palette.font, size: 11))
                        .foregroundColor(palette.foreground.opacity(0.6))
                }
            }
        }
        .lineLimit(1)
    }
}

/// The bottom strip of a menu: a status on the left, its keys on the right.
struct KitFooter<Leading: View>: View {
    let hints: [(key: String, label: String)]
    let palette: KitPalette
    @ViewBuilder var leading: Leading

    var body: some View {
        HStack(spacing: 10) {
            leading
            Spacer(minLength: 6)
            KeyHints(hints: hints, palette: palette)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        // A hairline above rather than a tinted strip: the foot of a menu is
        // usually the edge that meets the frame, and anything painted there
        // stops at the panel's side while the frame's fillets carry on in the
        // plain glass — a visible seam at the curves.
        .overlay(alignment: .top) {
            Rectangle()
                .fill(palette.foreground.opacity(0.08))
                .frame(height: 1)
                .padding(.horizontal, 12)
        }
    }
}

// MARK: - rows

/// A row's card. The armed row's fill is a single shape that glides between
/// rows (pass the menu's namespace), lit from the left with the accent; the
/// rest are faint cards that brighten under the pointer.
struct KitCard: ViewModifier {
    let selected: Bool
    var hovered = false
    let palette: KitPalette
    let highlight: Namespace.ID
    var radius: CGFloat = 14

    func body(content: Content) -> some View {
        if palette.hud { hudBody(content) } else if palette.arcane { arcaneBody(content) } else { softBody(content) }
    }

    /// Arcane: the armed row is wrapped in a glowing violet-to-rose aura that
    /// glides between rows, with a sparkle twinkling at its corner.
    private func arcaneBody(_ content: Content) -> some View {
        content
            .background {
                ZStack {
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .fill(palette.accent.opacity(hovered ? 0.12 : 0.045))
                    if selected {
                        RoundedRectangle(cornerRadius: 16, style: .continuous)
                            .fill(LinearGradient(colors: [palette.accent.opacity(0.36), palette.highlight.opacity(0.18)],
                                                 startPoint: .leading, endPoint: .trailing))
                            .overlay(
                                RoundedRectangle(cornerRadius: 16, style: .continuous)
                                    .stroke(LinearGradient(colors: [palette.highlight, palette.accent.opacity(0.4)],
                                                           startPoint: .topLeading, endPoint: .bottomTrailing),
                                            lineWidth: 1.2)
                            )
                            .shadow(color: palette.accent.opacity(0.7), radius: 12)
                            .overlay(alignment: .topTrailing) {
                                Image(systemName: "sparkle")
                                    .font(.system(size: 9))
                                    .foregroundColor(.white)
                                    .shadow(color: palette.highlight, radius: 4)
                                    .symbolEffect(.pulse)
                                    .offset(x: 3, y: -4)
                            }
                            .matchedGeometryEffect(id: "kit-armed", in: highlight)
                    }
                }
            }
            .scaleEffect(selected ? 1.015 : (hovered ? 1.008 : 1))
            .animation(.kSelect, value: selected)
            .animation(.kHover, value: hovered)
    }

    /// Stark: hard-edged cells with cut corners; the armed one is locked on by a
    /// targeting reticle that glides between rows, with a lit edge and a glow.
    private func hudBody(_ content: Content) -> some View {
        content
            .background {
                ZStack {
                    ChamferShape(cut: 6)
                        .fill(palette.accent.opacity(hovered ? 0.08 : 0.03))
                        .overlay(ChamferShape(cut: 6).stroke(palette.accent.opacity(hovered ? 0.35 : 0.14), lineWidth: 1))
                    if selected {
                        ZStack {
                            ChamferShape(cut: 6)
                                .fill(LinearGradient(
                                    colors: [palette.accent.opacity(0.24), palette.accent.opacity(0.04)],
                                    startPoint: .leading,
                                    endPoint: .trailing
                                ))
                            BracketShape(arm: 9)
                                .stroke(palette.accent, lineWidth: 1.8)
                                .shadow(color: palette.accent, radius: 5)
                                .padding(-3)
                            HStack {
                                Rectangle()
                                    .fill(palette.accent)
                                    .frame(width: 2)
                                    .padding(.vertical, 7)
                                    .shadow(color: palette.accent, radius: 4)
                                Spacer()
                            }
                        }
                        .matchedGeometryEffect(id: "kit-armed", in: highlight)
                    }
                }
            }
            .animation(.kSelect, value: selected)
            .animation(.kHover, value: hovered)
    }

    private func softBody(_ content: Content) -> some View {
        content
            .background {
                ZStack {
                    RoundedRectangle(cornerRadius: radius, style: .continuous)
                        .fill(Color.white.opacity(hovered ? 0.075 : 0.035))
                    if selected {
                        RoundedRectangle(cornerRadius: radius, style: .continuous)
                            .fill(LinearGradient(
                                colors: [palette.accent.opacity(0.32), palette.accent.opacity(0.12)],
                                startPoint: .leading,
                                endPoint: .trailing
                            ))
                            .overlay(
                                RoundedRectangle(cornerRadius: radius, style: .continuous)
                                    .stroke(
                                        LinearGradient(
                                            colors: [palette.accent.opacity(0.85), palette.accent.opacity(0.2)],
                                            startPoint: .topLeading,
                                            endPoint: .bottomTrailing
                                        ),
                                        lineWidth: 1.2
                                    )
                            )
                            .shadow(color: palette.accent.opacity(0.3), radius: 12)
                            .matchedGeometryEffect(id: "kit-armed", in: highlight)
                    }
                }
            }
            .scaleEffect(selected ? 1.015 : (hovered ? 1.008 : 1))
            .animation(.kSelect, value: selected)
            .animation(.kHover, value: hovered)
    }
}

extension View {
    func kitCard(
        selected: Bool,
        hovered: Bool = false,
        palette: KitPalette,
        highlight: Namespace.ID,
        radius: CGFloat = 14
    ) -> some View {
        modifier(KitCard(selected: selected, hovered: hovered, palette: palette, highlight: highlight, radius: radius))
    }
}

/// A round icon: a faint disc normally, filled with the accent (and ringed
/// with a slow pulse) when the thing it stands for is live.
struct IconBubble: View {
    let symbol: String
    let active: Bool
    let palette: KitPalette
    var size: CGFloat = 38

    @State private var pulse = false

    var body: some View {
        if palette.hud { hudBody } else if palette.arcane { arcaneBody } else { softBody }
    }

    /// Arcane: a glowing orb, with a spark orbiting it while live.
    private var arcaneBody: some View {
        ZStack {
            Circle()
                .fill(active
                      ? AnyShapeStyle(RadialGradient(colors: [palette.highlight, palette.accent, palette.accent.opacity(0.4)],
                                                      center: .topLeading, startRadius: 1, endRadius: size))
                      : AnyShapeStyle(palette.accent.opacity(0.1)))
                .overlay(Circle().stroke(palette.accent.opacity(active ? 0.9 : 0.3), lineWidth: 1))
                .shadow(color: palette.accent.opacity(active ? 0.9 : 0), radius: 8)
            if active {
                Spinning(period: 2.6) {
                    Circle().fill(Color.white).frame(width: 4, height: 4)
                        .shadow(color: palette.highlight, radius: 4)
                        .offset(y: -size / 2 - 3)
                }
            }
            Image(systemName: symbol)
                .font(.system(size: size * 0.38, weight: .medium))
                .foregroundColor(active ? .white : palette.foreground.opacity(0.75))
                .symbolEffect(.bounce, value: active)
        }
        .frame(width: size + 8, height: size + 8)
        .animation(.kBouncy, value: active)
    }

    /// Stark: a hexagon, lit and ringed by a turning dashed circle when live.
    private var hudBody: some View {
        ZStack {
            if active {
                SpinningRing(colour: palette.accent.opacity(0.8), dash: [2, 3], width: 1, period: 4)
                    .frame(width: size + 6, height: size + 6)
            }
            HexagonShape()
                .fill(palette.accent.opacity(active ? 0.28 : 0.06))
                .overlay(HexagonShape().stroke(palette.accent.opacity(active ? 1 : 0.35), lineWidth: 1.2))
                .shadow(color: palette.accent.opacity(active ? 0.8 : 0), radius: 5)
                .frame(width: size * 0.92, height: size * 0.8)
            Image(systemName: symbol)
                .font(.system(size: size * 0.36, weight: .medium))
                .foregroundColor(active ? palette.accent : palette.foreground.opacity(0.7))
                .symbolEffect(.bounce, value: active)
        }
        .frame(width: size + 6, height: size + 6)
        .animation(.kBouncy, value: active)
    }

    private var softBody: some View {
        ZStack {
            if active {
                Circle()
                    .stroke(palette.accent.opacity(pulse ? 0 : 0.55), lineWidth: 2)
                    .scaleEffect(pulse ? 1.45 : 1)
            }
            Circle()
                .fill(active
                      ? AnyShapeStyle(LinearGradient(
                          colors: [palette.accent, palette.accent.opacity(0.7)],
                          startPoint: .top,
                          endPoint: .bottom))
                      : AnyShapeStyle(Color.white.opacity(0.07)))
            Image(systemName: symbol)
                .font(.system(size: size * 0.4, weight: .medium))
                .foregroundColor(active ? .black.opacity(0.78) : palette.foreground.opacity(0.75))
                .symbolEffect(.bounce, value: active)
        }
        .frame(width: size, height: size)
        .animation(.kBouncy, value: active)
        .onAppear { restartPulse() }
        .onChange(of: active) { _, _ in restartPulse() }
    }

    private func restartPulse() {
        pulse = false
        guard active else { return }
        withAnimation(.easeOut(duration: 1.6).repeatForever(autoreverses: false)) { pulse = true }
    }
}

/// An on/off switch whose knob stretches as it travels, then lands with a
/// bounce.
struct KitToggle: View {
    let isOn: Bool
    let palette: KitPalette
    let action: () -> Void

    @State private var travelling = false

    var body: some View {
        Button {
            withAnimation(.kHover) { travelling = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) {
                withAnimation(.kBouncy) { travelling = false }
            }
            action()
        } label: {
            if palette.hud {
                // Stark: a hard switch with a lit block that slams across.
                ZStack(alignment: isOn ? .trailing : .leading) {
                    ChamferShape(cut: 4)
                        .stroke(palette.accent.opacity(isOn ? 0.9 : 0.35), lineWidth: 1.2)
                        .background(ChamferShape(cut: 4).fill(palette.accent.opacity(isOn ? 0.16 : 0.03)))
                    ChamferShape(cut: 3)
                        .fill(isOn ? palette.accent : palette.foreground.opacity(0.4))
                        .shadow(color: isOn ? palette.accent : .clear, radius: 6)
                        .frame(width: travelling ? 26 : 18, height: 14)
                        .padding(4)
                }
                .frame(width: 46, height: 22)
                .contentShape(Rectangle())
            } else if palette.arcane {
                // Arcane: a glowing track and a knob of light.
                ZStack(alignment: isOn ? .trailing : .leading) {
                    Capsule()
                        .fill(isOn
                              ? AnyShapeStyle(LinearGradient(colors: [palette.accent, palette.highlight],
                                                             startPoint: .leading, endPoint: .trailing))
                              : AnyShapeStyle(palette.accent.opacity(0.15)))
                        .shadow(color: isOn ? palette.accent : .clear, radius: 10)
                    Circle()
                        .fill(Color.white)
                        .shadow(color: isOn ? palette.highlight : .clear, radius: 6)
                        .frame(width: travelling ? 22 : 16, height: 16)
                        .padding(3)
                }
                .frame(width: 42, height: 22)
                .contentShape(Capsule())
            } else {
                ZStack(alignment: isOn ? .trailing : .leading) {
                    Capsule()
                        .fill(isOn ? palette.accent : Color.white.opacity(0.14))
                        .shadow(color: isOn ? palette.accent.opacity(0.5) : .clear, radius: 8)
                    Capsule()
                        .fill(isOn ? Color.black.opacity(0.7) : palette.foreground.opacity(0.85))
                        .frame(width: travelling ? 24 : 16, height: 16)
                        .padding(3)
                }
                .frame(width: 42, height: 22)
                .contentShape(Capsule())
            }
        }
        .buttonStyle(.plain)
        .animation(.kBouncy, value: isOn)
    }
}

/// A small status word in a capsule: tinted when it is good news.
struct StatusPill: View {
    let text: String
    let lit: Bool
    let palette: KitPalette

    var body: some View {
        if palette.hud {
            Text("[ \(text.uppercased()) ]")
                .font(.system(size: 9.5, weight: .semibold, design: .monospaced))
                .foregroundColor(lit ? palette.accent : palette.dim)
                .shadow(color: lit ? palette.accent.opacity(0.8) : .clear, radius: 4)
                .animation(.kBouncy, value: lit)
        } else if palette.arcane {
            HStack(spacing: 4) {
                Image(systemName: lit ? "sparkles" : "moon.stars")
                    .font(.system(size: 8.5))
                Text(text).font(.custom(palette.font, size: 10).italic())
            }
            .foregroundColor(lit ? .white : palette.dim)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(
                Capsule().fill(lit
                    ? AnyShapeStyle(LinearGradient(colors: [palette.accent.opacity(0.7), palette.highlight.opacity(0.5)],
                                                   startPoint: .leading, endPoint: .trailing))
                    : AnyShapeStyle(palette.accent.opacity(0.08)))
            )
            .shadow(color: lit ? palette.accent.opacity(0.7) : .clear, radius: 6)
            .animation(.kBouncy, value: lit)
        } else {
            softBody
        }
    }

    private var softBody: some View {
        HStack(spacing: 5) {
            Circle()
                .fill(lit ? palette.accent : palette.dim.opacity(0.6))
                .frame(width: 6, height: 6)
            Text(text)
                .font(.custom(palette.font, size: 10))
        }
        .foregroundColor(lit ? palette.accent : palette.dim)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Capsule().fill(lit ? palette.accent.opacity(0.14) : Color.white.opacity(0.06)))
        .animation(.kBouncy, value: lit)
    }
}

/// A big, calm empty state.
struct KitEmpty: View {
    let icon: String
    let title: String
    let detail: String
    let palette: KitPalette

    @State private var float = false

    var body: some View {
        VStack(spacing: 12) {
            ZStack {
                if palette.hud {
                    SpinningRing(colour: palette.accent.opacity(0.6), dash: [2, 5], width: 2, period: 7)
                        .frame(width: 112, height: 112)
                    SpinningRing(colour: palette.accent.opacity(0.35), dash: [24, 10], width: 1, period: 11, clockwise: false)
                        .frame(width: 140, height: 140)
                    HexagonShape().stroke(palette.accent.opacity(0.7), lineWidth: 1.2).frame(width: 84, height: 74)
                } else if palette.arcane {
                    MagicCircle(violet: palette.accent, rose: palette.highlight, lineWidth: 1.4, period: 18)
                        .frame(width: 130, height: 130)
                    Twinkles(colours: [palette.highlight, palette.accent, .white], count: 7)
                        .frame(width: 170, height: 150)
                } else {
                    Circle().fill(palette.accent.opacity(0.1)).frame(width: 84, height: 84)
                    Circle().stroke(palette.accent.opacity(0.18), lineWidth: 1).frame(width: 110, height: 110)
                }
                Image(systemName: icon)
                    .font(.system(size: 30, weight: .light))
                    .foregroundColor(palette.accent)
            }
            .offset(y: float ? -4 : 4)
            Text(palette.hud ? title.uppercased() : title)
                .font(palette.hud ? .system(size: 13, weight: .semibold, design: .monospaced) : .custom(palette.font, size: 15))
                .tracking(palette.hud ? 2 : 0)
                .foregroundColor(palette.foreground)
            Text(detail)
                .font(.custom(palette.font, size: 11))
                .foregroundColor(palette.dim)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 280)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(30)
        .onAppear {
            withAnimation(.easeInOut(duration: 2.2).repeatForever(autoreverses: true)) { float = true }
        }
    }
}
