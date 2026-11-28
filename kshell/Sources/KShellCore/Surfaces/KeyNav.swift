import SwiftUI

/// Which end of a list a jump goes to.
enum VimEdge {
    case first
    case last
}

/// Vim movement for anything walked with the arrow keys.
///
/// `j`/`k` step, `g`/`G` jump to the ends, and `/` takes the keyboard to a
/// search field where there is one. `g` goes straight to the top rather than
/// waiting for a second `g`, so the habit of typing `gg` still lands on the
/// first row.
///
/// The arrow keys keep working — this is an addition, not a replacement — and so
/// does everything else the view already binds.
struct VimListKeys: ViewModifier {
    /// Step the armed row, `+1` down or `-1` up.
    let move: (Int) -> KeyPress.Result
    /// Jump the armed row to an end.
    let jump: (VimEdge) -> KeyPress.Result
    /// Hand the keyboard to the search field, where the view has one.
    var search: (() -> KeyPress.Result)?
    /// Whether a text field holds the keyboard — a `j` typed into a search box
    /// has to stay a `j`, so none of these fire while it does.
    var fieldFocused = false

    func body(content: Content) -> some View {
        content
            .onKeyPress("j", phases: .held) { _ in fieldFocused ? .ignored : move(1) }
            .onKeyPress("k", phases: .held) { _ in fieldFocused ? .ignored : move(-1) }
            .onKeyPressOnce("g", passThrough: fieldFocused) { jump(.first) }
            .onKeyPressOnce("G", passThrough: fieldFocused) { jump(.last) }
            .onKeyPressOnce("/", passThrough: fieldFocused) { search?() ?? .ignored }
    }
}

extension KeyPress.Phases {
    /// The first press and the repeats that follow while the key is held, for
    /// keys that walk something and should keep walking.
    static let held: KeyPress.Phases = [.down, .repeat]
}

extension View {
    /// A key that acts once per press, for keys that do something rather than
    /// walk somewhere.
    ///
    /// Holding a key down sends repeats, and a repeat nobody claims makes macOS
    /// beep — so the repeats are claimed here and dropped, and so is a press
    /// with nothing to act on (return on an empty list): a key the panel binds
    /// is never an error. While `passThrough` holds (a text field has the
    /// keyboard) every phase is left for the field.
    func onKeyPressOnce(_ key: KeyEquivalent, passThrough: Bool = false,
                        action: @escaping () -> KeyPress.Result) -> some View {
        onKeyPress(key, phases: .held) { press in
            if passThrough { return .ignored }
            if press.phase == .down { _ = action() }
            return .handled
        }
    }
}
