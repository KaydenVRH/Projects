//! ked — a vim-like TUI text editor written in Rust.
//!
//! Uses ratatui (crossterm backend) for terminal rendering.
//! Provides vim-style normal/insert editing modes with:
//!   - tree-sitter syntax highlighting (Rust, Python, C/C++, JS/TS,
//!     HTML, CSS, TOML, JSON, Bash, Go, Markdown) with shebang /
//!     filename / extension language detection
//!   - 30 colour themes (dark and light)
//!   - Fuzzy file finder (Ctrl+P)
//!   - Run code in buffer (Ctrl+E: .py, .rs, .c, .h, .go)
//!   - Command mode (:w, :q, :q!, :wq, :theme ...)
//!   - Memories / reminders / journal (Ctrl+Shift+N / Ctrl+Shift+M, :mem, :today)
//!   - Undo / redo (u / Ctrl+R)
//!   - Yank / delete / paste (yy / dd / p / P)
//!
//! USAGE:
//!   ked [OPTIONS] [FILE]
//!   ked main.rs          # open a file
//!   ked --help           # list every option
//!   ked --due            # today's reminders, no TUI

mod config;
mod editor;
mod filetree;
mod finder;
mod graphics;
mod highlight;
mod keymap;
mod memory;
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

// ── command line ─────────────────────────────────────────────────

/// What the command line asked for.
#[derive(Debug, PartialEq, Eq)]
enum Cli {
    /// Start the editor (optionally opening a file).
    Run(Option<String>),
    Help,
    Version,
    Remember(String),
    Remind,
    InstallAgent,
    UninstallAgent,
    PrintConfig,
    PrintKeys,
    Due,
    DueCount,
    NextDue,
}

/// Parse `argv` (including the program name at index 0).
///
/// Unknown options are rejected *before* the terminal is touched, so a
/// typo like `ked --hlep` can never leave the shell in raw mode.
fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let mut files: Vec<String> = Vec::new();
    let mut only_files = false;
    let mut i = 1; // skip argv[0]
    while i < args.len() {
        let a = args[i].as_str();
        if only_files {
            files.push(a.to_string());
            i += 1;
            continue;
        }
        match a {
            "--" => only_files = true,
            "-h" | "--help" => return Ok(Cli::Help),
            "-V" | "--version" => return Ok(Cli::Version),
            "-r" | "--remember" => {
                let text = args[i + 1..].join(" ");
                if text.trim().is_empty() {
                    return Err("`--remember` needs some text".to_string());
                }
                return Ok(Cli::Remember(text));
            }
            "--remind" => return Ok(Cli::Remind),
            "--install-agent" => return Ok(Cli::InstallAgent),
            "--uninstall-agent" => return Ok(Cli::UninstallAgent),
            "--print-config" | "--config-template" | "--example-config" => {
                return Ok(Cli::PrintConfig)
            }
            "--print-keys" | "--keys-template" => return Ok(Cli::PrintKeys),
            "--due" | "--agenda" => return Ok(Cli::Due),
            "--due-count" => return Ok(Cli::DueCount),
            "--next-due" => return Ok(Cli::NextDue),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => files.push(other.to_string()),
        }
        i += 1;
    }
    if files.len() > 1 {
        return Err(format!(
            "too many files (expected one, got {}); try `ked --help`",
            files.len()
        ));
    }
    Ok(Cli::Run(files.into_iter().next()))
}

fn print_help() {
    println!(
        r#"ked {version} — a vim-like terminal editor

USAGE:
    ked [OPTIONS] [FILE]

OPTIONS:
    -h, --help              show this help and exit
    -V, --version           show the version and exit
    -r, --remember <TEXT>   save a note/reminder and exit
        --due, --agenda     print overdue and today's reminders
        --due-count         print how many reminders are due
        --next-due          print the next due time (HH:MM)
        --remind            run one reminder pass (used by the agent)
        --install-agent     install the macOS reminder LaunchAgent
        --uninstall-agent   remove it
        --print-config      print a commented config template
        --print-keys        print the [keys] template
        --                  stop option parsing (for names starting with '-')

FILE:
    Open FILE.  A file that doesn't exist starts as a new empty buffer,
    like vim.  With no FILE, ked opens an empty buffer.

INSIDE KED:
    :keys     list the effective keybindings
    :cal      month planner / calendar
    :mem      quick capture       :today   daily journal

EXAMPLES:
    ked src/main.rs
    ked --remember "call mom @tomorrow 9am #family"
    ked --next-due          # e.g. for a shell prompt
"#,
        version = env!("CARGO_PKG_VERSION")
    );
}

/// Restores the terminal on every exit path — normal return, `?`, or a
/// panic unwinding.  Without this a startup error after raw mode left
/// the shell in the alternate screen / raw mode (breaking prompts like
/// starship).
struct TuiGuard;

impl Drop for TuiGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(out, DisableBracketedPaste);
        let _ = execute!(out, PopKeyboardEnhancementFlags);
        let _ = execute!(out, LeaveAlternateScreen);
    }
}

// ── non-TUI commands ─────────────────────────────────────────────

fn print_due(config: &config::Config) {
    let store = memory::MemoryStore::load(memory::dir_for(&config.memories));
    let now = memory::now();
    let mut rows: Vec<(&memory::Memory, &'static str)> = Vec::new();
    for m in &store.entries {
        if m.done {
            continue;
        }
        match m.due {
            Some(d) if d.date() <= now.date() => {
                rows.push((m, if d < now { "overdue" } else { "today" }));
            }
            None if m.created.date() == now.date() => rows.push((m, "note")),
            _ => {}
        }
    }
    if rows.is_empty() {
        println!("Nothing due today.");
        return;
    }
    rows.sort_by_key(|(m, _)| m.due.unwrap_or(m.created));
    for (m, kind) in rows {
        let when = m
            .due
            .map(|d| d.format("%H:%M").to_string())
            .unwrap_or_else(|| "--:--".to_string());
        println!("{when}  {kind:<7}  {}", m.summary(80));
    }
}

// ── main ─────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cli = match parse_cli(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ked: {e}");
            eprintln!("Try `ked --help`.");
            std::process::exit(2);
        }
    };

    // Help/version don't need the config or a terminal.
    match cli {
        Cli::Help => {
            print_help();
            return Ok(());
        }
        Cli::Version => {
            println!("ked {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    let config = config::Config::load();

    match cli {
        Cli::Help | Cli::Version => {}
        Cli::Remember(text) => {
            let dir = memory::dir_for(&config.memories);
            let mut store = memory::MemoryStore::load(dir);
            match store.add(&text, memory::now(), None) {
                Ok(p) => println!("{}", p.display()),
                Err(e) => {
                    eprintln!("ked: {e}");
                    std::process::exit(1);
                }
            }
        }
        Cli::Remind => memory::run_reminder_pass(&config.memories),
        Cli::InstallAgent => match memory::install_launch_agent() {
            Ok(()) => println!(
                "installed LaunchAgent — `ked --remind` now runs every 60s"
            ),
            Err(e) => {
                eprintln!("ked: {e}");
                std::process::exit(1);
            }
        },
        Cli::UninstallAgent => match memory::uninstall_launch_agent() {
            Ok(()) => println!("removed ked LaunchAgent"),
            Err(e) => {
                eprintln!("ked: {e}");
                std::process::exit(1);
            }
        },
        Cli::PrintConfig => print!("{}", config::example()),
        Cli::PrintKeys => print!("{}", keymap::template()),
        Cli::Due => print_due(&config),
        Cli::DueCount => {
            let store = memory::MemoryStore::load(memory::dir_for(&config.memories));
            println!("{}", store.due_count(memory::now()));
        }
        Cli::NextDue => {
            let store = memory::MemoryStore::load(memory::dir_for(&config.memories));
            if let Some(d) = store.next_due(memory::now()) {
                println!("{}", d.format("%H:%M"));
            }
        }
        Cli::Run(file) => {
            // Build the editor *before* touching the terminal, so a bad
            // path or permission error prints normally and leaves the
            // shell exactly as it was.
            let mut editor = editor::Editor::new(file.as_deref(), &config)?;
            run_tui(&mut editor)?;
        }
    }
    Ok(())
}

// ── the TUI ──────────────────────────────────────────────────────

/// Map a cursor shape to a small id so the loop can detect changes.
fn cursor_style_code(s: crossterm::cursor::SetCursorStyle) -> u8 {
    use crossterm::cursor::SetCursorStyle as S;
    match s {
        S::DefaultUserShape => 0,
        S::BlinkingBlock => 1,
        S::SteadyBlock => 2,
        S::BlinkingUnderScore => 3,
        S::SteadyUnderScore => 4,
        S::BlinkingBar => 5,
        S::SteadyBar => 6,
    }
}

fn run_tui(editor: &mut editor::Editor) -> Result<()> {
    enable_raw_mode()?;
    // From here on, any `?` (or panic) restores the terminal.
    let _guard = TuiGuard;

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

    // Probe the terminal for kitty-graphics support (inline images).
    // Do this before the first clear: non-supporting terminals may
    // echo the probe bytes, and `terminal.clear()` wipes that.
    editor.set_graphics(graphics::detect());
    terminal.clear()?;

    // ---- main event loop ----
    // Track the last cursor shape so it is only sent when it changes.
    // Re-sending `\x1b[N q` every frame resets the blink timer, which
    // makes the cursor look like it is flickering.
    let mut last_cursor_style: Option<u8> = None;
    loop {
        // Check for audio-thread events (auto-next-track) and
        // auto-reload the file if it changed on disk.
        editor.tick();

        // ratatui hides/shows and repositions the cursor itself at the
        // end of draw().  Do NOT hide it here every frame: the
        // hide/show toggle at ~60 fps is what made the cursor flicker.
        terminal.draw(|f| editor.render(f))?;
        // Flush kitty-graphics placements queued during render, then
        // restore the cursor (the placement commands move it around).
        editor.flush_graphics()?;
        // Apply the cursor shape only when the mode/state changed.
        let style = editor.cursor_style;
        let code = cursor_style_code(style);
        if last_cursor_style != Some(code) {
            last_cursor_style = Some(code);
            let _ = execute!(io::stdout(), style);
        }

        // Poll with a 16 ms timeout so keyboard feels snappy while the
        // music player thread can still auto-advance.
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
    // Remove any inline images from the terminal.
    editor.clear_graphics();
    terminal.show_cursor()?;
    // `TuiGuard` restores raw mode / alt screen / paste / keyboard
    // flags when it drops, on both success and error paths.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_common_commands() {
        assert_eq!(parse_cli(&argv(&["ked"])), Ok(Cli::Run(None)));
        assert_eq!(
            parse_cli(&argv(&["ked", "src/main.rs"])),
            Ok(Cli::Run(Some("src/main.rs".into())))
        );
        assert_eq!(parse_cli(&argv(&["ked", "--version"])), Ok(Cli::Version));
        assert_eq!(parse_cli(&argv(&["ked", "-h"])), Ok(Cli::Help));
        assert_eq!(parse_cli(&argv(&["ked", "--due"])), Ok(Cli::Due));
        assert_eq!(
            parse_cli(&argv(&["ked", "--due-count"])),
            Ok(Cli::DueCount)
        );
        assert_eq!(
            parse_cli(&argv(&["ked", "--next-due"])),
            Ok(Cli::NextDue)
        );
    }

    #[test]
    fn remember_takes_the_rest_of_the_line() {
        assert_eq!(
            parse_cli(&argv(&["ked", "--remember", "call", "mom", "#family"])),
            Ok(Cli::Remember("call mom #family".into()))
        );
        assert!(parse_cli(&argv(&["ked", "--remember"])).is_err());
    }

    #[test]
    fn unknown_options_are_rejected_before_the_tui() {
        assert!(parse_cli(&argv(&["ked", "--hlep"])).is_err());
        assert!(parse_cli(&argv(&["ked", "-x"])).is_err());
        assert!(parse_cli(&argv(&["ked", "a", "b"])).is_err());
    }

    #[test]
    fn dashdash_allows_dashed_filenames() {
        assert_eq!(
            parse_cli(&argv(&["ked", "--", "--weird-name"])),
            Ok(Cli::Run(Some("--weird-name".into())))
        );
    }
}