//! ked — a vim-like TUI text editor written in Rust.
//!
//! Uses ratatui (crossterm backend) for terminal rendering.
//! Provides vim-style normal/insert editing modes with:
//!   - tree-sitter syntax highlighting (Rust, Python, C/C++, JS/TS,
//!     HTML, CSS, TOML, JSON, Bash, Go, Markdown) with shebang /
//!     filename / extension language detection
//!   - 20 colour themes
//!   - Fuzzy file finder (Ctrl+P)
//!   - Run code in buffer (Ctrl+E: .py, .rs, .c, .h, .go)
//!   - Command mode (:w, :q, :q!, :wq, :theme ...)
//!   - Undo / redo (u / Ctrl+R)
//!   - Yank / delete / paste (yy / dd / p / P)
//!
//! USAGE:
//!   ked [file]
//!   ked main.rs          # open a file
//!   ked                  # empty buffer

mod config;
mod editor;
mod filetree;
mod finder;
mod highlight;
mod music;
mod shell;
mod theme;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{
        poll, read, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

fn main() -> Result<()> {
    // ---- command-line argument: optional filename to open ----
    let args: Vec<String> = std::env::args().collect();
    let filename = args.get(1).map(|s| s.as_str());

    // ---- terminal setup (raw mode + alternate screen) ----
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    // Ask for the kitty keyboard protocol so Ctrl+M is distinguishable
    // from Enter (used by the music player binding).  Terminals without
    // support simply ignore the request.
    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    // Bracketed paste: terminal pastes arrive as a single Paste event
    // instead of a burst of keystrokes.
    execute!(stdout, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // ---- panic hook: restore terminal even on crashes ----
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let _ = std::process::Command::new("pkill").arg("-x").arg("mpv").output();
        prev_hook(info);
    }));

    // ---- load config + editor initialisation ----
    let config = config::Config::load();
    let mut editor = editor::Editor::new(filename, &config)?;

    // ---- main event loop ----
    loop {
        // Check for audio-thread events (auto-next-track) and
        // auto-reload the file if it changed on disk.
        editor.tick();

        // render the current editor state
        // Hide the terminal cursor while the frame is painted — the
        // diff repaints cells by moving the physical cursor around,
        // which makes it strobe across the screen (and sends kitty's
        // cursor trail into a frenzy).  ratatui re-shows the cursor
        // at the final editor position at the end of draw().
        terminal.hide_cursor()?;
        terminal.draw(|f| editor.render(f))?;
        // Apply cursor style set by render
        let _ = execute!(io::stdout(), editor.cursor_style);

        // Poll with a 100 ms timeout so keyboard feels snappy
        // while the music player thread can still auto-advance.
        let timeout = Duration::from_millis(16);
        if poll(timeout)? {
            match read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        if !editor.handle_key(key) {
                            break;
                        }
                        // Immediately pump shell output so typed
                        // characters appear without a frame of delay.
                        editor.tick();
                    }
                }
                Event::Paste(text) => {
                    // Terminal pasted something — insert it literally
                    // (never as vim commands).
                    editor.paste_from_terminal(&text);
                    editor.tick();
                }
                _ => {}
            }
        }
    }

    // ---- stop music and shell before cleanup ----
    editor.music_player.stop();
    editor.kill_shell();

    // ---- cleanup ----
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableBracketedPaste)?;
    execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
