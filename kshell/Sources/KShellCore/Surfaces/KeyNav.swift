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
            .onKeyPress("j", phases: .down) { _ in fieldFocused ? .ignored : move(1) }
            .onKeyPress("k", phases: .down) { _ in fieldFocused ? .ignored : move(-1) }
            .onKeyPress("g", phases: .down) { _ in fieldFocused ? .ignored : jump(.first) }
            .onKeyPress("G", phases: .down) { _ in fieldFocused ? .ignored : jump(.last) }
            .onKeyPress("/", phases: .down) { _ in
                guard !fieldFocused else { return .ignored }
                return search?() ?? .ignored
            }
    }
}
