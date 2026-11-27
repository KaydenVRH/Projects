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

    init(theme: Theme, bar: BarConfig) {
        self.init(theme: theme, font: bar.fontFamily)
    }

    init(theme: Theme, font: String) {
        accent = theme.accent?.color ?? .accentColor
        foreground = theme.foreground?.color ?? .white
        dim = (theme.dim ?? theme.foreground)?.color ?? .gray
        self.font = font
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
        .background(Color.black.opacity(0.12))
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
                Circle().fill(palette.accent.opacity(0.1)).frame(width: 84, height: 84)
                Circle().stroke(palette.accent.opacity(0.18), lineWidth: 1).frame(width: 110, height: 110)
                Image(systemName: icon)
                    .font(.system(size: 30, weight: .light))
                    .foregroundColor(palette.accent)
            }
            .offset(y: float ? -4 : 4)
            Text(title)
                .font(.custom(palette.font, size: 15))
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
