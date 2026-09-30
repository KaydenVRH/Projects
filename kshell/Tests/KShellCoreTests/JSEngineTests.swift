import XCTest
@testable import KShellCore

final class JSEngineTests: XCTestCase {
    final class StubRunner: JSRunner {
        var outputs: [String: String] = [:]
        var fired: [String] = []
        var posted: [(String, String)] = []

        func run(command: String, interval: TimeInterval, onOutput: @escaping (String) -> Void) {
            if let output = outputs[command] { onOutput(output) }
        }
        func runOnce(command: String, onOutput: @escaping (String) -> Void) {
            if let output = outputs[command] { onOutput(output) }
        }
        func fire(command: String) { fired.append(command) }
        func postEvent(_ name: String, payload: String) { posted.append((name, payload)) }
    }

    func testRenderReturnsDescriptor() throws {
        let engine = try XCTUnwrap(JSEngine(
            source: "function render(){ return { icon: 'x', label: 'hi' } }",
            runner: StubRunner()
        ))
        let render = try XCTUnwrap(engine.render())
        XCTAssertEqual(render.icon, "x")
        XCTAssertEqual(render.label, "hi")
    }

    func testExecCallbackFeedsRender() throws {
        let runner = StubRunner()
        runner.outputs["echo 42"] = "42"
        let source = """
        var value = "?";
        exec("echo 42", 1, function(out) { value = out; });
        function render() { return { label: value }; }
        """
        let engine = try XCTUnwrap(JSEngine(source: source, runner: runner))
        XCTAssertEqual(engine.render()?.label, "42")
    }

    func testColorsParse() throws {
        let engine = try XCTUnwrap(JSEngine(
            source: "function render(){ return { labelColor: '#ff0000' } }",
            runner: StubRunner()
        ))
        XCTAssertEqual(try XCTUnwrap(engine.render()).labelColor?.r, 1)
    }

    /// JS widgets can use the same `$token` colours as the config, so a widget
    /// follows the active theme instead of being stuck on one palette.
    func testColorsFollowThemeTokens() throws {
        _ = try ShellConfig.parse(toml: """
        [theme]
        accent    = "#ff2a85"
        highlight = "#00d4ff"
        """)
        let engine = try XCTUnwrap(JSEngine(
            source: "function render(){ return { iconColor: '$accent', labelColor: '$highlight' } }",
            runner: StubRunner()
        ))
        let render = try XCTUnwrap(engine.render())
        XCTAssertEqual(render.iconColor, RGBA(hex: "#ff2a85"))
        XCTAssertEqual(render.labelColor, RGBA(hex: "#00d4ff"))

        // …and re-resolve when the theme changes.
        _ = try ShellConfig.parse(toml: """
        [theme]
        accent    = "#fabd2f"
        highlight = "#83a598"
        """)
        let updated = try XCTUnwrap(engine.render())
        XCTAssertEqual(updated.iconColor, RGBA(hex: "#fabd2f"))
        XCTAssertEqual(updated.labelColor, RGBA(hex: "#83a598"))
    }

    func testUnknownTokenColorIsNil() throws {
        let engine = try XCTUnwrap(JSEngine(
            source: "function render(){ return { labelColor: '$nope' } }",
            runner: StubRunner()
        ))
        XCTAssertNil(try XCTUnwrap(engine.render()).labelColor)
    }

    func testMissingRenderReturnsNil() throws {
        let engine = try XCTUnwrap(JSEngine(source: "var x = 1;", runner: StubRunner()))
        XCTAssertNil(engine.render())
    }

    func testOnEventUpdatesState() throws {
        let source = """
        var last = "";
        function onEvent(name, payload) { last = name + ":" + payload; }
        function render() { return { label: last }; }
        """
        let engine = try XCTUnwrap(JSEngine(source: source, runner: StubRunner()))
        engine.callEvent("tick", payload: "5")
        XCTAssertEqual(engine.render()?.label, "tick:5")
    }

    func testPostReachesRunner() throws {
        let runner = StubRunner()
        let engine = try XCTUnwrap(JSEngine(
            source: "post('hello', 'world'); function render(){ return {}; }",
            runner: runner
        ))
        XCTAssertEqual(engine.render()?.label, nil)
        XCTAssertEqual(runner.posted.first?.0, "hello")
        XCTAssertEqual(runner.posted.first?.1, "world")
    }
}
