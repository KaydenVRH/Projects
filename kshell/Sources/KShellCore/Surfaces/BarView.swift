import SwiftUI

/// The SwiftUI content of a bar: left / center / right runs of widgets over a
/// themed background. One view per display, all sharing the same view model.
///
/// When the display has a notch, the bar still spans it (background goes all
/// the way across), but the left and right widget runs are clamped to the two
/// areas beside the notch so nothing renders behind it.
public struct BarView: View {
    @ObservedObject var viewModel: BarViewModel
    let notchWidth: CGFloat
    /// The focus pill moves between items through this.
    @Namespace private var pill

    public init(viewModel: BarViewModel, notchWidth: CGFloat = 0) {
        self.viewModel = viewModel
        self.notchWidth = notchWidth
    }

    public var body: some View {
        GeometryReader { geometry in
            content(totalWidth: geometry.size.width)
                .padding(.horizontal, viewModel.appearance.paddingX)
                .frame(width: geometry.size.width, height: geometry.size.height)
                .background {
                    ZStack {
                        viewModel.appearance.background.color
                        // Music, behind the widgets.
                        if let config = viewModel.barVisualiser {
                            BarVisualiser(config: config, theme: viewModel.theme)
                                .allowsHitTesting(false)
                        }
                    }
                }
                .overlay(alignment: .bottom) {
                    // Stark: a ruled, glowing baseline with a light sweeping it.
                    if Hud.isHud(viewModel.theme) {
                        HudBarTrim()
                            .frame(height: 6)
                            .allowsHitTesting(false)
                    } else if Neon.isNeon(viewModel.theme) {
                        // Neon: a tube along the bar's foot with a glint running it.
                        NeonBarTrim()
                            .frame(height: 2)
                            .allowsHitTesting(false)
                    } else if Arcane.isArcane(viewModel.theme) {
                        // Arcane: sparkles winking along the bar's foot.
                        ArcaneBarTrim()
                            .frame(height: viewModel.appearance.height)
                            .allowsHitTesting(false)
                    }
                }
        }
    }

    @ViewBuilder
    private func content(totalWidth: CGFloat) -> some View {
        if notchWidth > 0 {
            let available = max(totalWidth - viewModel.appearance.paddingX * 2, 0)
            let half = max((available - notchWidth) / 2, 0)
            HStack(spacing: 0) {
                region(viewModel.left)
                    .frame(width: half, alignment: .leading)
                    .clipped()
                Color.clear.frame(width: notchWidth)
                region(viewModel.right)
                    .frame(width: half, alignment: .trailing)
                    .clipped()
            }
        } else {
            HStack(spacing: 0) {
                region(viewModel.left)
                Spacer(minLength: 0)
                region(viewModel.center)
                Spacer(minLength: 0)
                region(viewModel.right)
            }
        }
    }

    @ViewBuilder
    private func region(_ runtimes: [WidgetRuntime]) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(runtimes.enumerated()), id: \.offset) { _, runtime in
                WidgetItemView(
                    model: runtime.model,
                    appearance: viewModel.appearance,
                    theme: viewModel.theme,
                    pill: pill
                )
            }
        }
    }
}

private struct WidgetItemView: View {
    @ObservedObject var model: WidgetModel
    let appearance: BarConfig
    let theme: Theme
    let pill: Namespace.ID

    @State private var hovered = false

    private var iconSize: Double {
        appearance.iconFontSize ?? appearance.fontSize + 3
    }

    var body: some View {
        if !model.hidden {
            if model.flexible {
                Spacer(minLength: 0)
            } else {
                HStack(spacing: model.spacing) {
                    if let icon = model.icon, !icon.isEmpty {
                        NerdIcon(
                            symbol: icon,
                            fontName: iconFontName,
                            size: iconSize,
                            color: iconColor,
                            slack: model.spacing
                        )
                        .layoutPriority(1)
                    }
                    if !model.label.isEmpty {
                        Text(model.label)
                            .font(.custom(appearance.fontFamily, size: appearance.fontSize))
                            .foregroundColor(labelColor)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .minimumScaleFactor(0.9)
                            .background {
                                // Neon: hovered, the label splits into its
                                // colour fringes, like a mistracked tape.
                                if neonHover {
                                    ZStack {
                                        label.foregroundColor(Color(nsColor: Neon.cyan).opacity(0.8)).offset(x: -1.6)
                                        label.foregroundColor(Color(nsColor: Neon.magenta).opacity(0.8)).offset(x: 1.6)
                                    }
                                }
                            }
                    }
                }
                .padding(.horizontal, model.paddingX)
                .padding(.horizontal, model.active ? (Hud.isHud(theme) || Neon.isNeon(theme) ? 7 : 4) : 0)
                .background {
                    if model.active {
                        if Neon.isNeon(theme) {
                            // Neon: a slanted tag, outlined in the other tube colour.
                            SlantShape(slant: 5)
                                .fill(labelColor.opacity(0.22))
                                .overlay(SlantShape(slant: 5).stroke(labelColor, lineWidth: 1.2).offset(x: 1, y: -0.5))
                                .shadow(color: labelColor.opacity(0.8), radius: 4)
                                .padding(.vertical, 5)
                                .modifier(NeonStrikeUp())
                                .matchedGeometryEffect(id: "active", in: pill)
                        } else if Hud.isHud(theme) {
                            // Stark: a targeting bracket instead of a pill.
                            ZStack {
                                ChamferShape(cut: 4).fill(labelColor.opacity(0.14))
                                BracketShape(arm: 5)
                                    .stroke(labelColor, lineWidth: 1.5)
                                    .shadow(color: labelColor.opacity(0.9), radius: 3)
                            }
                            .padding(.vertical, 5)
                            .matchedGeometryEffect(id: "active", in: pill)
                        } else {
                            Capsule()
                                .fill(labelColor.opacity(0.2))
                                .overlay(Capsule().stroke(labelColor.opacity(0.35), lineWidth: 1))
                                .padding(.vertical, 5)
                                .matchedGeometryEffect(id: "active", in: pill)
                        }
                    }
                }
                .contentShape(Rectangle())
                .scaleEffect(hovered && model.action != nil ? 1.08 : 1)
                .offset(x: jolt)
                .onHover { inside in
                    hovered = inside
                    if inside && Neon.isNeon(theme) { glitch() }
                }
                .animation(.kHover, value: hovered)
                .onTapGesture { model.action?() }
                .transition(.scale(scale: 0.4).combined(with: .opacity))
            }
        }
    }

    private var neonHover: Bool { hovered && Neon.isNeon(theme) }

    private var label: Text {
        Text(model.label).font(.custom(appearance.fontFamily, size: appearance.fontSize))
    }

    @State private var jolt: CGFloat = 0

    /// A quick sideways jolt, the way a glitching picture jumps.
    private func glitch() {
        let steps: [(Double, CGFloat)] = [(0, -2.5), (0.05, 2), (0.1, -1), (0.15, 0)]
        for (delay, x) in steps {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { jolt = x }
        }
    }

    private var iconFontName: String {
        model.iconFont ?? appearance.iconFontFamily ?? appearance.fontFamily
    }

    private var iconColor: Color {
        model.iconColor?.color ?? theme.accent?.color ?? .white
    }

    private var labelColor: Color {
        model.labelColor?.color ?? theme.highlight?.color ?? .white
    }
}

/// A neon tube striking up: on appearing, a stutter of dark and light before
/// it holds steady. The bar's active pill does this as it lands somewhere new.
private struct NeonStrikeUp: ViewModifier {
    @State private var level: Double = 0

    func body(content: Content) -> some View {
        content
            .opacity(level)
            .onAppear {
                let steps: [(Double, Double)] = [(0, 1), (0.05, 0.25), (0.1, 0.9), (0.16, 0.4), (0.22, 1)]
                for (delay, value) in steps {
                    DispatchQueue.main.asyncAfter(deadline: .now() + delay) { level = value }
                }
            }
    }
}
