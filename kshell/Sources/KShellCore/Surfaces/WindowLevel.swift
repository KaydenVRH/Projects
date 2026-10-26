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
}
