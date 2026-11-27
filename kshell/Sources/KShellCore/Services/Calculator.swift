import Foundation

/// Arithmetic for the app launcher: `+ - * / % ^`, brackets, unary minus,
/// `sqrt(…)`, `pi` and `e`. A small parser of its own rather than
/// `NSExpression`, which raises an Objective-C exception — taking the whole
/// shell down — on half-typed input like `2+`.
public enum Calculator {
    /// The value of `text`, or nil when it isn't complete arithmetic.
    public static func evaluate(_ text: String) -> Double? {
        var parser = Parser(Array(text.lowercased().filter { !$0.isWhitespace }))
        guard let value = parser.expression(), parser.atEnd, value.isFinite else { return nil }
        return value
    }

    /// Whether `query` is meant as a sum: it starts with `=`, or it is digits
    /// with at least one operator between them (`12*7`, not `12` or `cal`).
    public static func looksLikeMath(_ query: String) -> Bool {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("=") { return trimmed.count > 1 }
        guard trimmed.first.map({ $0.isNumber || $0 == "(" || $0 == "-" || $0 == "." }) == true else { return false }
        return trimmed.dropFirst().contains { "+-*/%^".contains($0) }
    }

    /// A result as people write it: no trailing `.0`, no float noise.
    public static func format(_ value: Double) -> String {
        if value == value.rounded(), abs(value) < 1e15 { return String(Int64(value)) }
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.usesGroupingSeparator = false
        formatter.maximumFractionDigits = 10
        formatter.decimalSeparator = "."
        return formatter.string(from: NSNumber(value: value)) ?? String(value)
    }

    private struct Parser {
        let chars: [Character]
        var index = 0

        init(_ chars: [Character]) { self.chars = chars }

        var atEnd: Bool { index == chars.count }
        private var peek: Character? { index < chars.count ? chars[index] : nil }

        private mutating func take(_ c: Character) -> Bool {
            guard peek == c else { return false }
            index += 1
            return true
        }

        // expression := term (('+' | '-') term)*
        mutating func expression() -> Double? {
            guard var value = term() else { return nil }
            while true {
                if take("+") { guard let rhs = term() else { return nil }; value += rhs }
                else if take("-") { guard let rhs = term() else { return nil }; value -= rhs }
                else { return value }
            }
        }

        // term := power (('*' | '/' | '%') power)*
        mutating func term() -> Double? {
            guard var value = power() else { return nil }
            while true {
                if take("*") || take("x") { guard let rhs = power() else { return nil }; value *= rhs }
                else if take("/") { guard let rhs = power(), rhs != 0 else { return nil }; value /= rhs }
                else if take("%") { guard let rhs = power(), rhs != 0 else { return nil }; value = value.truncatingRemainder(dividingBy: rhs) }
                else { return value }
            }
        }

        // power := unary ('^' power)?   (right-associative)
        mutating func power() -> Double? {
            guard let base = unary() else { return nil }
            if take("^") {
                guard let exponent = power() else { return nil }
                return pow(base, exponent)
            }
            return base
        }

        // unary := ('-' | '+') unary | atom
        mutating func unary() -> Double? {
            if take("-") { return unary().map { -$0 } }
            if take("+") { return unary() }
            return atom()
        }

        // atom := number | '(' expression ')' | name | name '(' expression ')'
        mutating func atom() -> Double? {
            if take("(") {
                guard let value = expression(), take(")") else { return nil }
                return value
            }
            if let c = peek, c.isLetter {
                var name = ""
                while let c = peek, c.isLetter { name.append(c); index += 1 }
                switch name {
                case "pi": return .pi
                case "e": return M_E
                case "sqrt":
                    guard take("("), let value = expression(), take(")"), value >= 0 else { return nil }
                    return value.squareRoot()
                default: return nil
                }
            }
            var digits = ""
            while let c = peek, c.isNumber || c == "." || c == "," {
                digits.append(c == "," ? "." : c)
                index += 1
            }
            return Double(digits)
        }
    }
}
