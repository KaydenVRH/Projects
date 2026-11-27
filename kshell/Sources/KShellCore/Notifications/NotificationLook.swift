import SwiftUI

/// The pieces of a notification card that change with the theme, shared by
/// the toast and the centre so the two always match. Under Neon the tone
/// colours come from the city's tubes, the icon sits in a slanted box, the app
/// is a tag, the title is split into colour fringes and the tone edge is a
/// stack of slanted dashes; otherwise they are the soft originals.
enum NotificationLook {
    static func tint(_ tone: KShellNotification.Tone, accent: Color, neon: Bool) -> Color {
        if neon {
            switch tone {
            case .info: return Color(nsColor: Neon.magenta)
            case .success: return Color(nsColor: Neon.cyan)
            case .warning: return Color(nsColor: Neon.yellow)
            case .error: return Color(red: 1, green: 0.23, blue: 0.3)
            }
        }
        switch tone {
        case .info: return accent
        case .success: return .green
        case .warning: return .orange
        case .error: return .red
        }
    }
}

/// The app's icon on a tile.
struct NotificationIconTile<Icon: View>: View {
    let tint: Color
    let size: CGFloat
    let neon: Bool
    @ViewBuilder let icon: () -> Icon

    var body: some View {
        ZStack {
            if neon {
                SlantShape(slant: 6).fill(tint.opacity(0.16))
                SlantShape(slant: 6).stroke(tint.opacity(0.9), lineWidth: 1.1)
                    .shadow(color: tint.opacity(0.8), radius: 3)
            } else {
                RoundedRectangle(cornerRadius: size * 0.26, style: .continuous).fill(tint.opacity(0.17))
            }
            icon().frame(width: size * 0.68, height: size * 0.68)
        }
        .frame(width: size, height: size)
    }
}

/// Which app it's from: plain text, or under Neon a slanted tag.
struct NotificationAppLabel: View {
    let app: String
    let tint: Color
    let dim: Color
    let font: String
    let neon: Bool

    var body: some View {
        if neon {
            Text(app.uppercased())
                .font(.system(size: 8.5, weight: .heavy, design: .monospaced))
                .tracking(0.8)
                .foregroundColor(Color(red: 0.04, green: 0.02, blue: 0.09))
                .padding(.horizontal, 7)
                .padding(.vertical, 1.5)
                .background(SlantShape(slant: 4).fill(tint))
        } else {
            Text(app).font(.custom(font, size: 10.5)).foregroundColor(dim)
        }
    }
}

/// The title.
struct NotificationHeadline: View {
    let text: String
    let size: CGFloat
    let foreground: Color
    let font: String
    let neon: Bool

    var body: some View {
        if neon {
            ChromaText(text: text, font: .system(size: size, weight: .heavy).italic(),
                       magenta: Color(nsColor: Neon.magenta), cyan: Color(nsColor: Neon.cyan),
                       foreground: foreground)
        } else {
            Text(text).font(.custom(font, size: size)).foregroundColor(foreground)
        }
    }
}

/// The tone as a lit edge down the card's side.
struct NotificationToneEdge: View {
    let tint: Color
    let neon: Bool

    var body: some View {
        if neon {
            // dashes leaning forward, like the tags
            VStack(spacing: 3) {
                ForEach(0..<4, id: \.self) { index in
                    SlantShape(slant: 2.5)
                        .fill(tint.opacity(1 - Double(index) * 0.18))
                        .frame(width: 5)
                }
            }
            .shadow(color: tint.opacity(0.8), radius: 4)
        } else {
            Capsule().fill(tint).frame(width: 3).shadow(color: tint.opacity(0.7), radius: 5)
        }
    }
}
