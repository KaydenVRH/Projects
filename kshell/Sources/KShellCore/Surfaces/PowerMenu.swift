import AppKit
import SwiftUI

/// Lock, sleep, restart, shut down, log out. Restart, shut down and log out go
/// through macOS's own confirmation (the dialog the Apple menu shows), so a
/// stray Enter can't take the machine down.
public final class PowerMenu {
    private let overlay: OverlayLauncher

    enum Action: String, CaseIterable {
        case lock, sleep, displayOff, restart, shutDown, logOut

        var title: String {
            switch self {
            case .lock: return "Lock"
            case .sleep: return "Sleep"
            case .displayOff: return "Display off"
            case .restart: return "Restart…"
            case .shutDown: return "Shut down…"
            case .logOut: return "Log out…"
            }
        }

        var symbol: String {
            switch self {
            case .lock: return "lock.fill"
            case .sleep: return "moon.zzz.fill"
            case .displayOff: return "display"
            case .restart: return "arrow.clockwise"
            case .shutDown: return "power"
            case .logOut: return "rectangle.portrait.and.arrow.right"
            }
        }

        func run() {
            switch self {
            case .lock: PowerMenu.lockScreen()
            case .sleep: Shell.run("pmset sleepnow")
            case .displayOff: Shell.run("pmset displaysleepnow")
            // loginwindow's own events: each puts up the system's "Are you
            // sure…?" dialog with its countdown.
            case .restart: Shell.run(#"osascript -e 'tell application "loginwindow" to «event aevtrrst»'"#)
            case .shutDown: Shell.run(#"osascript -e 'tell application "loginwindow" to «event aevtrsdn»'"#)
            case .logOut: Shell.run(#"osascript -e 'tell application "loginwindow" to «event aevtlogo»'"#)
            }
        }
    }

    public init(viewModel: BarViewModel, margin: CGFloat = 0) {
        overlay = OverlayLauncher(
            viewModel: viewModel,
            margin: margin,
            size: { screen in
                NSSize(width: min(screen.frame.width * 0.5, 600), height: min(screen.frame.height * 0.6, 410))
            },
            content: { onClose in
                let items = Action.allCases.map { LauncherItem(id: $0.rawValue, name: $0.title, symbol: $0.symbol) }
                return AnyView(LauncherView(
                    viewModel: viewModel,
                    placeholder: "Power…",
                    style: .grid,
                    items: items,
                    onSelect: { item in
                        onClose()
                        guard let action = Action(rawValue: item.id) else { return }
                        // Let the panel get out of the way first.
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { action.run() }
                    },
                    onClose: onClose
                ))
            }
        )
    }

    public func toggle() { overlay.toggle() }
    public func show() { overlay.show() }
    public func hide() { overlay.hide() }

    /// Lock straight away — what the Apple menu's Lock Screen does.
    static func lockScreen() {
        let path = "/System/Library/PrivateFrameworks/login.framework/Versions/Current/login"
        if let handle = dlopen(path, RTLD_NOW), let symbol = dlsym(handle, "SACLockScreenImmediate") {
            typealias Lock = @convention(c) () -> Int32
            _ = unsafeBitCast(symbol, to: Lock.self)()
            return
        }
        // Fallback: put the display to sleep, which locks when a password is
        // required straight after sleep.
        Shell.run("pmset displaysleepnow")
    }
}
