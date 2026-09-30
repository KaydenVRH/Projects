import AppKit
import CoreText
import SwiftUI

/// Renders Nerd Font glyphs to images so their full ink is drawn.
///
/// In the non-`Mono` Nerd Font variants an icon's ink is up to twice as wide as
/// the font's advance width, and SwiftUI clips text to the advance box — which
/// slices the right edge off glyphs like the volume waves, the CPU pins, the
/// clock face and the mug handle. Drawing the glyph into an image and letting it
/// overflow its layout box keeps the whole icon without reserving extra width.
enum IconImage {
    struct Metrics {
        let image: NSImage
        /// Width SwiftUI should reserve for layout — the font's advance.
        let advance: CGFloat
        /// Natural size of the glyph's ink.
        let ink: CGSize
    }

    private static var cache: [String: Metrics?] = [:]
    private static let lock = NSLock()

    static func metrics(symbol: String, font: NSFont) -> Metrics? {
        let key = "\(symbol)|\(font.fontName)|\(font.pointSize)"
        lock.lock()
        if let cached = cache[key] {
            lock.unlock()
            return cached
        }
        lock.unlock()

        let result = make(symbol: symbol, font: font)

        lock.lock()
        cache[key] = result
        lock.unlock()
        return result
    }

    private static func make(symbol: String, font: NSFont) -> Metrics? {
        let attributed = NSAttributedString(string: symbol, attributes: [.font: font])
        let line = CTLineCreateWithAttributedString(attributed)

        var ascent: CGFloat = 0, descent: CGFloat = 0, leading: CGFloat = 0
        let advance = CTLineGetTypographicBounds(line, &ascent, &descent, &leading)
        let ink = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)
        guard advance > 0, ink.width > 0, ink.height > 0 else { return nil }

        // A hair of padding so antialiased edges are not shaved.
        let pad: CGFloat = 1
        let size = NSSize(width: ceil(ink.width) + pad * 2, height: ceil(ink.height) + pad * 2)
        let image = NSImage(size: size)
        image.lockFocus()
        if let context = NSGraphicsContext.current?.cgContext {
            context.setFillColor(NSColor.white.cgColor)
            context.textPosition = CGPoint(x: pad - ink.origin.x, y: pad - ink.origin.y)
            CTLineDraw(line, context)
        }
        image.unlockFocus()
        image.isTemplate = true

        return Metrics(image: image, advance: advance, ink: ink.size)
    }
}

/// A Nerd Font icon that is never clipped: it reserves the font's advance for
/// layout and draws the full glyph, spilling into the surrounding spacing. It
/// only scales down if the ink would otherwise reach its neighbour.
struct NerdIcon: View {
    let symbol: String
    let fontName: String
    let size: Double
    let color: Color
    /// Room the icon may spill into on each side before it is scaled down.
    let slack: Double

    var body: some View {
        if let metrics = IconImage.metrics(
            symbol: symbol,
            font: NSFont(name: fontName, size: size) ?? .systemFont(ofSize: size)
        ) {
            let maxWidth = metrics.advance + slack * 2
            let drawn = min(metrics.ink.width, maxWidth)
            let scale = metrics.ink.width > 0 ? drawn / metrics.ink.width : 1
            Image(nsImage: metrics.image)
                .resizable()
                .interpolation(.high)
                .frame(width: drawn, height: metrics.ink.height * scale)
                .frame(width: metrics.advance, height: metrics.ink.height)
                .foregroundColor(color)
        } else {
            Text(verbatim: symbol)
                .font(.custom(fontName, size: size))
                .foregroundColor(color)
                .fixedSize()
        }
    }
}
