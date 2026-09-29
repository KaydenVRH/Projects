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

    public init(viewModel: BarViewModel, notchWidth: CGFloat = 0) {
        self.viewModel = viewModel
        self.notchWidth = notchWidth
    }

    public var body: some View {
        GeometryReader { geometry in
            content(totalWidth: geometry.size.width)
                .padding(.horizontal, viewModel.appearance.paddingX)
                .frame(width: geometry.size.width, height: geometry.size.height)
                .background(viewModel.appearance.background.color)
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
                    theme: viewModel.theme
                )
            }
        }
    }
}

private struct WidgetItemView: View {
    @ObservedObject var model: WidgetModel
    let appearance: BarConfig
    let theme: Theme

    var body: some View {
        if model.flexible {
            Spacer(minLength: 0)
        } else {
            HStack(spacing: model.spacing) {
                if let icon = model.icon, !icon.isEmpty {
                    Text(icon)
                        .font(.custom(iconFontName, size: appearance.iconFontSize ?? appearance.fontSize + 3))
                        .foregroundColor(iconColor)
                        .fixedSize()
                }
                if !model.label.isEmpty {
                    Text(model.label)
                        .font(.custom(appearance.fontFamily, size: appearance.fontSize))
                        .foregroundColor(labelColor)
                        .lineLimit(1)
                        .fixedSize()
                }
            }
            .padding(.horizontal, model.paddingX)
            .contentShape(Rectangle())
            .onTapGesture { model.action?() }
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
