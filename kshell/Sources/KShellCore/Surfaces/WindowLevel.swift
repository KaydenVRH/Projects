import AppKit

/// Window levels for the shell.
///
/// The shell draws above ordinary windows, but not above everything: macOS puts
/// Notification Center at level 21 (measured on macOS 26), and with the shell at
/// `.statusBar` (25) notification banners were drawn *behind* the bar and the
/// launchers. Everything the shell owns therefore sits in 17…20 — above ordinary
/// and floating windows (0…8), below the notifications and the menu bar (24).
enum WindowLevel {
    /// The desktop visualiser, when it is in overlay mode.
    static let visualiser = NSWindow.Level(rawValue: 17)
    /// The frame's surface: the border itself, and the glass the bar sits on.
    static let surface = NSWindow.Level(rawValue: 18)
    /// The bar's own content window, above the surface.
    static let bar = NSWindow.Level(rawValue: 19)
    /// Launchers and the media island, above the bar.
    static let overlay = NSWindow.Level(rawValue: 20)
    /// Behind every ordinary window but above the desktop and a live
    /// wallpaper: decoration that should show in the gaps between windows and
    /// never cover them.
    static let backdrop = NSWindow.Level(rawValue: -1)
}

/// A click-through, screen-sized window behind ordinary windows, for the
/// trademark themes' decoration that would otherwise sit on top of them — the
/// frame's corner pieces and the particles rising off it. Its frame panel
/// orders it in and out with itself.
final class BackdropPanel: NSPanel {
    let root: NSView

    init(screen: NSScreen) {
        root = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        super.init(contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel],
                   backing: .buffered, defer: false)
        level = WindowLevel.backdrop
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        isMovable = false
        hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]
        root.wantsLayer = true
        contentView = root
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// A frame overlay with a backdrop: ordering one in or out does both.
class FrameOverlayPanel: NSPanel {
    let frameScreen: NSScreen
    let backdrop: BackdropPanel

    init(screen: NSScreen) {
        frameScreen = screen
        backdrop = BackdropPanel(screen: screen)
        super.init(contentRect: screen.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        level = WindowLevel.surface
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        isMovable = false
        hidesOnDeactivate = false
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]
    }

    override func orderFrontRegardless() {
        backdrop.orderFrontRegardless()
        super.orderFrontRegardless()
    }

    override func orderOut(_ sender: Any?) {
        backdrop.orderOut(sender)
        super.orderOut(sender)
    }
}
