//! ked — a vim-like TUI text editor written in Rust.
//!
//! Uses ratatui (crossterm backend) for terminal rendering.
//! Provides vim-style normal/insert editing modes with:
//!   - Python syntax highlighting
//!   - 5 colour themes (default, monokai, solarized, nord, gruvbox)
//!   - Fuzzy file finder (Ctrl+P)
//!   - Run Python in buffer (Ctrl+R)
//!   - Command mode (:w, :q, :q!, :wq, :theme ...)
//!   - Undo / redo (u / Ctrl+r)
//!   - Yank / delete / paste (yy / dd / p / P)
//!   - Scroll wheel support
//!
//! USAGE:
//!   ked [file]
//!   ked main.py          # open a file
//!   ked                  # empty buffer

mod editor;
mod finder;
mod highlight;
mod music;
mod theme;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{poll, read, Event, KeyEventKind},
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
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // ---- panic hook: restore terminal even on crashes ----
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = std::process::Command::new("pkill").arg("-x").arg("afplay").output();
        prev_hook(info);
    }));

    // ---- editor initialisation (load file if given) ----
    let mut editor = editor::Editor::new(filename)?;

    // ---- main event loop ----
    loop {
        // Check for audio-thread events (auto-next-track) and
        // auto-reload the file if it changed on disk.
        editor.tick();

        // render the current editor state
        terminal.draw(|f| editor.render(f))?;

        // Poll with a 100 ms timeout so keyboard feels snappy
        // while the music player thread can still auto-advance.
        let timeout = Duration::from_millis(100);
        if poll(timeout)? {
            if let Event::Key(key) = read()? {
                if key.kind == KeyEventKind::Press {
                    if !editor.handle_key(key) {
                        break;
                    }
                }
            }
        }
    }

    // ---- stop music before cleanup ----
    editor.music_player.stop();

    // ---- cleanup ----
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
