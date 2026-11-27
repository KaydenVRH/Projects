import XCTest
@testable import KShellCore

final class CalculatorTests: XCTestCase {
    func testArithmetic() {
        XCTAssertEqual(Calculator.evaluate("12*7.5"), 90)
        XCTAssertEqual(Calculator.evaluate("2+3*4"), 14, "precedence")
        XCTAssertEqual(Calculator.evaluate("(2+3)*4"), 20)
        XCTAssertEqual(Calculator.evaluate("2^3^2"), 512, "power is right-associative")
        XCTAssertEqual(Calculator.evaluate("-3 + 5"), 2)
        XCTAssertEqual(Calculator.evaluate("17 % 5"), 2)
        XCTAssertEqual(Calculator.evaluate("sqrt(16) + 1"), 5)
        XCTAssertEqual(Calculator.evaluate("3,5*2"), 7, "a comma works as the decimal point")
        XCTAssertEqual(Calculator.evaluate("2x3"), 6)
        XCTAssertEqual(Calculator.evaluate("pi")!, .pi, accuracy: 1e-12)
    }

    /// Half-typed input is just "not yet" — never a crash.
    func testIncompleteInputIsNil() {
        for text in ["2+", "(2+3", "2**3", "sqrt(", "1/0", "abc", "", ")", "sqrt(-1)", "1..2"] {
            XCTAssertNil(Calculator.evaluate(text), text)
        }
    }

    func testWhatCountsAsMath() {
        XCTAssertTrue(Calculator.looksLikeMath("12*7"))
        XCTAssertTrue(Calculator.looksLikeMath("=sqrt(2)"))
        XCTAssertTrue(Calculator.looksLikeMath("(1+2)"))
        XCTAssertFalse(Calculator.looksLikeMath("12"), "a bare number is a search")
        XCTAssertFalse(Calculator.looksLikeMath("safari"))
        XCTAssertFalse(Calculator.looksLikeMath("="))
    }

    func testFormatting() {
        XCTAssertEqual(Calculator.format(90), "90")
        XCTAssertEqual(Calculator.format(0.1 + 0.2), "0.3")
        XCTAssertEqual(Calculator.format(-2.5), "-2.5")
    }

    func testLauncherRows() {
        XCTAssertEqual(AppLauncher.calculation("=12*7.5").first?.name, "= 90")
        XCTAssertTrue(AppLauncher.calculation("safari").isEmpty)
        XCTAssertTrue(AppLauncher.webSearch("neon city").first?.id.hasSuffix("q=neon%20city") ?? false)
        XCTAssertTrue(AppLauncher.webSearch("2+2").isEmpty, "a sum is never sent to the web")
    }
}
