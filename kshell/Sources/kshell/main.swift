import AppKit
import KShellCore

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
          kshell --diagnose                   print screen + system info
          kshell --version | --help
        """)
        exit(0)

    case "trigger":
        let line = arguments.dropFirst(2).joined(separator: " ")
        guard !line.isEmpty else { fail("usage: kshell trigger <event> [payload]") }
        exit(IPCClient.send(line) ? 0 : 1)

    case "reload":
        exit(IPCClient.send("config_reload") ? 0 : 1)

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
    exit(0)
}

let controller = AppController()
controller.start()

app.run()
