import XCTest
@testable import KShellCore

final class ConfigTests: XCTestCase {
    func testHexParsingOpaque() {
        let color = RGBA(hex: "#ff0000")
        XCTAssertEqual(color, RGBA(r: 1, g: 0, b: 0, a: 1))
    }

    func testHexParsingCSSAlphaLast() {
        let color = RGBA(hex: "#ff000080")
        XCTAssertEqual(color?.r, 1)
        XCTAssertEqual(color?.a ?? 0, 128.0 / 255.0, accuracy: 0.001)
    }

    func testHexParsingSketchybarAlphaFirst() {
        let color = RGBA(hex: "0x80ff0000")
        XCTAssertEqual(color?.a ?? 0, 128.0 / 255.0, accuracy: 0.001)
        XCTAssertEqual(color?.r, 1)
        XCTAssertEqual(color?.g, 0)
    }

    func testConfigParse() throws {
        let toml = """
        [bar]
        height = 40
        edge = "bottom"
        background = "#2d353bAA"
        blur = true

        [theme]
        accent = "#83c092"

        [[bar.left]]
        type = "apple"
        icon_hex = "f8ff"

        [[bar.right]]
        type = "clock"
        format = "HH:mm"

        [[bar.right]]
        type = "spacer"
        """
        let config = try ShellConfig.parse(toml: toml)
        XCTAssertEqual(config.bar.height, 40)
        XCTAssertEqual(config.bar.edge, .bottom)
        XCTAssertTrue(config.bar.blur)
        XCTAssertEqual(config.left.count, 1)
        XCTAssertEqual(config.left.first?.type, "apple")
        XCTAssertEqual(config.right.count, 2)
        XCTAssertEqual(config.right.first?.string("format"), "HH:mm")
    }

    func testIconHexResolvesToGlyph() throws {
        let config = try ShellConfig.parse(toml: """
        [[bar.left]]
        type = "apple"
        icon_hex = "f8ff"
        """)
        let spec = try XCTUnwrap(config.left.first)
        XCTAssertEqual(WidgetFactory.resolvedIcon(spec), "\u{f8ff}")
    }

    func testUnknownWidgetTypeProducesNoRuntime() throws {
        let config = try ShellConfig.parse(toml: """
        [[bar.left]]
        type = "does_not_exist"
        """)
        let runtimes = config.left.flatMap {
            WidgetFactory.make(spec: $0, theme: config.theme, bar: config.bar)
        }
        XCTAssertTrue(runtimes.isEmpty)
    }
}
