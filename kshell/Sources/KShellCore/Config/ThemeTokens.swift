import Foundation
import TOMLKit

/// Theme color tokens.
///
/// Any color value in the config may be written as `$name` (for example
/// `"$accent"`) instead of a literal hex string. Values come from the `[theme]`
/// section, so switching a theme only has to rewrite that section to re-colour
/// the whole bar.
public enum ThemeTokens {
    /// `[theme]` keys that can be referenced as `$tokens`.
    public static let keys = ["accent", "highlight", "foreground", "dim", "mid", "background"]

    private static var values: [String: String] = [:]

    /// Install the literal values from a parsed `[theme]` table. Called while
    /// loading the config, before any widget colors are resolved.
    static func install(from theme: TOMLTable?) {
        var map: [String: String] = [:]
        for key in keys {
            guard let value = theme?.string(key), !value.hasPrefix("$") else { continue }
            map[key] = value
        }
        values = map
    }

    /// Resolve `$name` to its literal value; anything else passes through.
    static func resolve(_ string: String) -> String {
        guard string.hasPrefix("$") else { return string }
        return values[String(string.dropFirst())] ?? string
    }
}
