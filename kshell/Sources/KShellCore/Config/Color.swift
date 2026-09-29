import AppKit
import SwiftUI
import TOMLKit

/// An sRGB color parsed from config strings.
///
/// Accepted forms:
///   - `#RRGGBB`      (opaque)
///   - `#RRGGBBAA`    (CSS-style, alpha last)
///   - `0xAARRGGBB`   (sketchybar-style, alpha first)
public struct RGBA: Equatable {
    public var r: Double
    public var g: Double
    public var b: Double
    public var a: Double

    public init(r: Double, g: Double, b: Double, a: Double = 1) {
        self.r = r; self.g = g; self.b = b; self.a = a
    }

    public init?(hex string: String) {
        var s = string.trimmingCharacters(in: .whitespaces)
        guard !s.isEmpty else { return nil }

        let alphaFirst: Bool
        if s.hasPrefix("0x") || s.hasPrefix("0X") {
            s = String(s.dropFirst(2))
            alphaFirst = (s.count == 8)
        } else {
            if s.hasPrefix("#") { s = String(s.dropFirst()) }
            alphaFirst = false
        }

        guard let value = UInt64(s, radix: 16) else { return nil }

        switch s.count {
        case 6:
            r = Double((value >> 16) & 0xFF) / 255
            g = Double((value >> 8) & 0xFF) / 255
            b = Double(value & 0xFF) / 255
            a = 1
        case 8:
            if alphaFirst {
                a = Double((value >> 24) & 0xFF) / 255
                r = Double((value >> 16) & 0xFF) / 255
                g = Double((value >> 8) & 0xFF) / 255
                b = Double(value & 0xFF) / 255
            } else {
                r = Double((value >> 24) & 0xFF) / 255
                g = Double((value >> 16) & 0xFF) / 255
                b = Double((value >> 8) & 0xFF) / 255
                a = Double(value & 0xFF) / 255
            }
        default:
            return nil
        }
    }

    public static func parse(_ string: String?) -> RGBA? {
        guard let string else { return nil }
        return RGBA(hex: string)
    }

    public var nsColor: NSColor {
        NSColor(srgbRed: r, green: g, blue: b, alpha: a)
    }

    public var color: Color {
        Color(.sRGB, red: r, green: g, blue: b, opacity: a)
    }
}

// MARK: - TOML convenience accessors

extension TOMLTable {
    func string(_ key: String) -> String? { self[key]?.string }
    func int(_ key: String) -> Int? { self[key]?.int }
    func double(_ key: String) -> Double? { self[key]?.double }
    func bool(_ key: String) -> Bool? { self[key]?.bool }
    func table(_ key: String) -> TOMLTable? { self[key]?.table }
    func array(_ key: String) -> TOMLArray? { self[key]?.array }

    /// Numeric value as a Double, accepting TOML integers too.
    func number(_ key: String) -> Double? {
        if let double = self[key]?.double { return double }
        if let int = self[key]?.int { return Double(int) }
        return nil
    }
}
