import AppKit
import KShellCore
import SwiftUI

let arguments = CommandLine.arguments
let app = NSApplication.shared
// `.accessory` = no Dock icon, no app menu — a background "shell" process.
app.setActivationPolicy(.accessory)

func fail(_ message: String, code: Int32 = 2) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}

if arguments.count >= 2 {
    switch arguments[1] {
    case "--version", "-v":
        print("kshell \(KShellInfo.version)")
        exit(0)

    case "--help", "-h":
        print("""
        kshell \(KShellInfo.version) — a scriptable shell for macOS

        usage:
          kshell                              run the shell
          kshell trigger <event> [payload]    push an event into the running shell
          kshell reload                       reload the config
          kshell theme                        list configured themes
          kshell theme <name>                 apply a theme (kitty/nvim/kshell/wallpaper)
          kshell --diagnose                   print screen + system info
          kshell --version | --help
        """)
        exit(0)

    case "theme":
        let loader = ConfigLoader()
        let launcher = loader.config.themeLauncher
        let query = arguments.count >= 3 ? arguments[2] : ""

        guard !query.isEmpty else {
            guard !launcher.themes.isEmpty else {
                fail("kshell: no themes configured — add [[theme_launcher.themes]] to \(loader.path.path)")
            }
            for theme in launcher.themes {
                let mark = theme.name == loader.config.theme.name ? "*" : " "
                print("\(mark) \(theme.name)")
            }
            exit(0)
        }
        guard let theme = launcher.themes.first(where: { $0.name.lowercased() == query.lowercased() }) else {
            let names = launcher.themes.map(\.name).joined(separator: ", ")
            fail("kshell: unknown theme '\(query)' — available: \(names)")
        }
        let result = ThemeApplier.apply(theme, config: launcher)
        // Nudge the running shell so the bar re-colours right away.
        _ = IPCClient.send("config_reload")
        print("applied \(theme.name) → \(result.changed.joined(separator: ", "))")
        if !result.failed.isEmpty {
            FileHandle.standardError.write(Data("kshell: could not update: \(result.failed.joined(separator: ", "))\n".utf8))
        }
        exit(0)

    case "trigger":
        let line = arguments.dropFirst(2).joined(separator: " ")
        guard !line.isEmpty else { fail("usage: kshell trigger <event> [payload]") }
        guard IPCClient.send(line) else {
            fail("kshell: no running instance to receive '\(line)' — is the kshell bar running?", code: 1)
        }
        exit(0)

    case "reload":
        guard IPCClient.send("config_reload") else {
            fail("kshell: no running instance — is the kshell bar running?", code: 1)
        }
        exit(0)

    default:
        break
    }
}

if arguments.contains("--diagnose") {
    let config = BarConfig()
    let viewModel = BarViewModel(config: ShellConfig())
    for screen in NSScreen.screens {
        let notch = BarPanel.notchWidth(for: screen, config: config)
        let panel = BarPanel(screen: screen, appearance: config, viewModel: viewModel)
        print("""
        screen      : \(screen.localizedName)
        frame       : \(screen.frame)
        visibleFrame: \(screen.visibleFrame)
        safeTop     : \(screen.safeAreaInsets.top)
        auxTopLeft  : \(String(describing: screen.auxiliaryTopLeftArea))
        auxTopRight : \(String(describing: screen.auxiliaryTopRightArea))
        notchWidth  : \(notch)
        panel.frame : \(panel.frame)   (top=\(panel.frame.maxY) vs screen top=\(screen.frame.maxY))
        """)
    }
    let audio = SystemAudio.defaultOutput()
    let battery = SystemMetrics.battery()
    print("""

    --- system ---
    volume  : \(Int((audio.volume * 100).rounded()))% muted=\(audio.muted)
    battery : \(battery.present ? "\(battery.percent)% charging=\(battery.charging)" : "none")
    ram     : \(Int((SystemMetrics.memoryUsage() * 100).rounded()))%
    """)

    let loaded = ConfigLoader().config
    let scriptsDirectory = KShellPaths.resolve(loaded.scriptDirectory)
    print("""

    --- launchers ---
    scripts     : \(scriptsDirectory.path) -> \(ScriptCatalog.load(directory: scriptsDirectory).count)
    """)
    for directory in loaded.wallpaperDirectories.map({ KShellPaths.resolve($0) }) {
        let items = WallpaperCatalog.load(directories: [directory])
        let names = items.map(\.name).joined(separator: ", ")
        print("wallpapers  : \(directory.path) -> \(items.count) [\(names)]")
    }
    exit(0)
}

if let index = arguments.firstIndex(of: "--render") {
    let path = arguments.count > index + 1 ? arguments[index + 1] : "/tmp/kshell.png"
    let loader = ConfigLoader()
    let config = loader.config
    let viewModel = BarViewModel(config: config)
    guard let screen = NSScreen.main ?? NSScreen.screens.first else { exit(1) }
    let appearance = config.bar
    let renderHeight = arguments.count > index + 2
        ? (Double(arguments[index + 2]) ?? appearance.height)
        : appearance.height
    let size = NSSize(
        width: screen.frame.width - appearance.marginX * 2,
        height: renderHeight
    )
    let notch = BarPanel.notchWidth(for: screen, config: appearance)
    // Fill empty labels with representative text so the render reflects real widths.
    func sampleLabel(for runtime: WidgetRuntime) -> String? {
        let spec = runtime.model.spec
        switch spec.type {
        case "clock":     return "Fri 5:30 PM"
        case "cpu":       return "5%"
        case "ram":       return "95%"
        case "battery":   return "100%"
        case "volume":    return "69%"
        case "wifi":      return nil
        case "bluetooth": return "off"
        case "js":        return "main"
        case "front_app": return "kitty"
        case "now_playing": return "Artist — Title"
        case "script":
            let command = spec.string("command") ?? ""
            if command.contains("now_playing") { return "Artist — Title" }
            return "0"
        default:
            return nil
        }
    }
    for runtime in viewModel.left + viewModel.center + viewModel.right {
        if runtime.model.label.isEmpty, let label = sampleLabel(for: runtime) {
            runtime.model.label = label
        }
    }
    let host = NSHostingView(rootView: BarView(viewModel: viewModel, notchWidth: notch))
    host.frame = NSRect(origin: .zero, size: size)
    guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { exit(1) }
    host.cacheDisplay(in: host.bounds, to: rep)
    if let data = rep.representation(using: NSBitmapImageRep.FileType.png, properties: [:]) {
        try? data.write(to: URL(fileURLWithPath: path))
        print("wrote \(path) (\(Int(size.width))x\(Int(size.height)), notch=\(Int(notch)))")
    }
    exit(0)
}

if arguments.contains("--glyphs") {
    let font = NSFont(name: "Hack Nerd Font", size: 16) ?? NSFont.systemFont(ofSize: 16)
    let checks: [(String, UInt32)] = [
        ("apple", 0xF8FF), ("chevron", 0xF054), ("music", 0xF001), ("clock", 0xF017),
        ("volume-off", 0xF026), ("volume-down", 0xF027), ("volume-up", 0xF028),
        ("battery-full", 0xF240), ("battery-empty", 0xF244), ("bolt", 0xF0E7),
        ("microchip", 0xF2DB), ("memory", 0xF538), ("ram-efc1", 0xEFC1),
        ("server", 0xF233), ("database", 0xF1C0), ("hdd", 0xF0A0), ("sdcard", 0xF7C2),
        ("wifi", 0xF1EB), ("wifi-off-f05e", 0xF05E),
        ("bluetooth-b", 0xF293), ("bluetooth", 0xF294),
        ("beer", 0xF0FC), ("code-fork", 0xF126), ("chevron-default-ea9c", 0xEA9C),
    ]
    for (name, codepoint) in checks {
        var characters = [UniChar](repeating: 0, count: 1)
        characters[0] = UniChar(codepoint)
        var glyphs = [CGGlyph](repeating: 0, count: 1)
        let found = CTFontGetGlyphsForCharacters(font as CTFont, &characters, &glyphs, 1)
        let ok = found && glyphs[0] != 0
        print("\(ok ? "OK  " : "MISS")  U+\(String(codepoint, radix: 16, uppercase: true))  \(name)")
    }
    exit(0)
}

if arguments.contains("--icons") {
    let codepoints: [UInt32] = [
        0xF8FF, 0xF054, 0xF001, 0xF017, 0xF026, 0xF027, 0xF028,
        0xF240, 0xF244, 0xF0E7, 0xF2DB, 0xEFC1, 0xF1EB, 0xF293, 0xF294, 0xF0FC, 0xF126,
    ]
    let glyphs = codepoints
        .compactMap { UnicodeScalar($0) }
        .map(String.init)
        .joined(separator: "  ")
    let view = Text(verbatim: glyphs)
        .font(.custom("Hack Nerd Font", size: 44))
        .foregroundColor(.white)
        .padding(24)
        .background(Color.black)
    let host = NSHostingView(rootView: view)
    host.frame = NSRect(x: 0, y: 0, width: CGFloat(codepoints.count) * 60 + 48, height: 110)
    if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
        host.cacheDisplay(in: host.bounds, to: rep)
        if let data = rep.representation(using: NSBitmapImageRep.FileType.png, properties: [:]) {
            let path = "/tmp/kshell-icons.png"
            try? data.write(to: URL(fileURLWithPath: path))
            print("wrote \(path)")
        }
    }
    exit(0)
}

if arguments.contains("--render-icons") {
    // Diagnostic: Nerd Font glyphs have ink wider than the font's advance, so
    // SwiftUI clips them to the text's layout box. Draw a variants x glyphs grid
    // and count ink per cell to find the smallest fix at the bar's real size.
    let size: CGFloat = arguments.count > 2 ? (Double(arguments[2]).map { CGFloat($0) } ?? 19) : 19
    let glyphs: [(String, UInt32)] = [
        ("apple", 0xf8ff), ("cpu", 0xf4bc), ("ram", 0xf0c9), ("clock", 0xf43a),
        ("brew", 0xf487), ("music", 0xf001), ("volume", 0xf028), ("wifi", 0xf1eb),
        ("bt", 0xf293),
    ]
    let font = "Hack Nerd Font"
    func glyph(_ code: UInt32) -> String {
        guard let scalar = UnicodeScalar(code) else { return "?" }
        return String(Character(scalar))
    }
    let cell: CGFloat = 30

    @ViewBuilder
    func variant<V: View>(_ text: V, _ kind: Int) -> some View {
        if kind == 0 {
            text.fixedSize()                                   // bar's current recipe
        } else if kind == 1 {
            text.fixedSize().padding(.trailing, size * 0.05)
        } else if kind == 2 {
            text.fixedSize().padding(.trailing, size * 0.10)
        } else if kind == 3 {
            text.fixedSize().padding(.trailing, size * 0.15)
        } else if kind == 4 {
            text.fixedSize().padding(.horizontal, size * 0.06)
        } else {
            text                                               // no fixedSize
        }
    }

    let view = VStack(alignment: .leading, spacing: 2) {
        ForEach(0..<6, id: \.self) { kind in
            HStack(spacing: 0) {
                Text("v\(kind)")
                    .font(.system(size: 9))
                    .foregroundColor(.yellow)
                    .frame(width: 22, alignment: .leading)
                ForEach(glyphs, id: \.0) { _, code in
                    variant(Text(glyph(code)).font(.custom(font, size: size)).lineLimit(1), kind)
                        .frame(width: cell, height: cell)
                        .background(Color.black)
                }
            }
        }
    }
    .padding(10)
    .background(Color.black)
    let hostSize = NSSize(width: cell * CGFloat(glyphs.count) + 42, height: cell * 6 + 20)
    let host = NSHostingView(rootView: view)
    host.frame = NSRect(origin: .zero, size: hostSize)
    host.layoutSubtreeIfNeeded()
    host.displayIfNeeded()
    if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
        host.cacheDisplay(in: host.bounds, to: rep)
        if let data = rep.representation(using: NSBitmapImageRep.FileType.png, properties: [:]) {
            try? data.write(to: URL(fileURLWithPath: "/tmp/kshell-icons.png"))
            print("wrote /tmp/kshell-icons.png (size \(size)pt)")
        }
    }
    exit(0)
}

if arguments.contains("--render-sheet") {
    let size = NSSize(width: 560, height: 340)
    // Three sheets exactly like the launchers draw them: fill + 1px stroke,
    // clipped to the shape — so the corners can be checked at high zoom.
    let view = VStack(spacing: 20) {
        ForEach([SheetShape.Corners.top, .leading, .trailing], id: \.rawValue) { corners in
            let shape = SheetShape(radius: 34, corners: corners)
            ZStack {
                Color(red: 0.10, green: 0.10, blue: 0.12)
                Color(red: 0.55, green: 0.1, blue: 0.7)
                    .clipShape(shape)
                    .overlay(shape.stroke(Color(red: 1, green: 0.16, blue: 0.52), lineWidth: 1))
                    .clipShape(shape)
            }
            .frame(width: size.width, height: 90)
        }
    }
    .padding(20)
    .background(Color.black)
    .frame(width: size.width + 40, height: 3 * 110 + 40)
    let host = NSHostingView(rootView: view)
    let hostSize = NSSize(width: size.width + 40, height: 3 * 110 + 40)
    host.frame = NSRect(origin: .zero, size: hostSize)
    if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
        host.cacheDisplay(in: host.bounds, to: rep)
        if let data = rep.representation(using: NSBitmapImageRep.FileType.png, properties: [:]) {
            let path = "/tmp/kshell-sheet.png"
            try? data.write(to: URL(fileURLWithPath: path))
            print("wrote \(path)")
        }
    }
    exit(0)
}

// Refuse to start a second instance (a stale one would leave the socket orphaned).
if IPCClient.isInstanceRunning() {
    FileHandle.standardError.write(Data("kshell: already running\n".utf8))
    exit(0)
}

let controller = AppController()
controller.start()

// Clean up the socket on exit so triggers never hit a stale socket.
var signalSources: [DispatchSourceSignal] = []
for sig in [SIGTERM, SIGINT, SIGHUP] {
    signal(sig, SIG_IGN)
    let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
    source.setEventHandler {
        controller.stop()
        exit(0)
    }
    source.resume()
    signalSources.append(source)
}

app.run()
