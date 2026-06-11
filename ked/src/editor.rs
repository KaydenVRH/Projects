//! Editor — the heart of ked.
//!
//! The [`Editor`] struct owns everything: the text buffer, cursor position,
//! scroll offset, mode/state, undo history, clipboard, theme, and the
//! finder.  Its two key public methods are:
//!
//!   - [`handle_key`] — process a keypress, return `false` to quit
//!   - [`render`]     — draw the current state to a ratatui [`Frame`]
//!
//! Modes & states:
//!   - Mode::Normal  — vi-style motion / editing keys (h/j/k/l, dd, yy, …)
//!   - Mode::Insert  — type text directly
//!   - State::Command — typing a `:` command on the status line
//!   - State::Finder — fuzzy file finder overlay (Ctrl+P)
//!   - State::Run    — Python run output overlay (Ctrl+R)

use std::{
    cmp::min,
    fs,
    process::Command as ProcCmd,
    time::SystemTime,
};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Style, Modifier},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::highlight;
use crate::shell::ShellProcess;
use crate::theme::{Theme, ThemeKind};
use crate::finder::Finder;
use crate::music::MusicPlayer;

// ── Mode & State ─────────────────────────────────────────────────

/// Editing mode: vim-style Normal (motion/command) or Insert (typing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

/// Overlay state: what the user is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Normal,  // editing normally (no overlay)
    Command, // typing a : command on the status line
    Finder,  // fuzzy file finder open
    Run,     // Python run output displayed
    Music,   // music player picker (Ctrl+T)
    Theme,   // theme selector (Ctrl+E)
    Shell,   // pop-up shell (Ctrl+J) — handled in main.rs
}

// ── Editor ───────────────────────────────────────────────────────

/// All editor state.
pub struct Editor {
    // ── buffer ──
    pub lines: Vec<String>,

    // ── cursor (0-based, absolute in buffer) ──
    pub cy: usize,
    pub cx: usize,

    // ── viewport scroll ──
    pub top: usize,  // first visible line
    pub left: usize, // first visible column

    // ── mode (normal/insert) and state (normal/command/finder/run) ──
    pub mode: Mode,
    pub state: State,

    // ── command bar ──
    pub cmd_buf: String,

    // ── fuzzy finder ──
    pub finder: Finder,
    pub finder_query: String,
    pub finder_selection: usize,

    // ── run output ──
    pub run_output: String,

    // ── music player (Ctrl+M) ──
    pub music_player: MusicPlayer,

    // ── file ──
    pub filename: Option<String>,
    pub modified: bool,
    /// Last known mtime of the open file (for auto-reload).
    pub last_mtime: Option<SystemTime>,

    // ── undo / redo ──
    pub undo_stack: Vec<Vec<String>>,
    pub redo_stack: Vec<Vec<String>>,

    // ── clipboard (for yy/dd/p/P) ──
    pub clipboard: Vec<String>,

    // ── theme ──
    pub theme: ThemeKind,
    /// Currently-highlighted row in the theme selector.
    pub theme_selected: usize,

    // ── flash message (shown in status bar, cleared next render) ──
    pub flash: Option<String>,

    // ── cached terminal size (updated every render) ──
    pub cache_w: u16,
    pub cache_h: u16,

    // ── pop-up shell (Ctrl+J) ──
    pub shell: Option<ShellProcess>,
    pub shell_reader: Option<std::thread::JoinHandle<()>>,
}

impl Editor {
    // ═══════════════════════════════════════════════════════════════
    //  Construction & Drop
    // ═══════════════════════════════════════════════════════════════

    /// Create a new editor, optionally loading a file from disk.
    ///
    /// If `filename` is `None`, start with a single empty line
    /// (like vim's empty buffer).
    pub fn new(filename: Option<&str>) -> Result<Self> {
        let (lines, filename, last_mtime) = if let Some(path) = filename {
            let content =
                fs::read_to_string(path).with_context(|| format!("can't read {path}"))?;
            // Split into lines.  If the file ends with `\n`, push an
            // extra empty line so the cursor can sit on the "virtual"
            // line below the last content line (vim behaviour).
            let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
            if content.ends_with('\n') {
                lines.push(String::new());
            }
            if lines.is_empty() {
                lines.push(String::new());
            }
            let mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
            (lines, Some(path.to_string()), mtime)
        } else {
            (vec![String::new()], None, None)
        };

        Ok(Self {
            lines,
            cy: 0,
            cx: 0,
            top: 0,
            left: 0,
            mode: Mode::Normal,
            state: State::Normal,
            cmd_buf: String::new(),
            finder: Finder::new(),
            finder_query: String::new(),
            finder_selection: 0,
            run_output: String::new(),
            music_player: MusicPlayer::new(),
            filename,
            last_mtime,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            clipboard: Vec::new(),
            theme: ThemeKind::Default,
            theme_selected: 0,
            flash: None,
            cache_w: 80,
            cache_h: 24,
            shell: None,
            shell_reader: None,
        })
    }

    // ── cleanup ──

    /// Join the shell reader thread and drop the shell process.
    pub fn kill_shell(&mut self) {
        self.shell = None;
        if let Some(handle) = self.shell_reader.take() {
            let _ = handle.join();
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Key dispatch
    // ═══════════════════════════════════════════════════════════════

    /// Handle a key press.  Returns `true` to keep running, `false` to quit.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // If the shell overlay is active, forward every key to the
        // shell (including Ctrl+J, Ctrl+C, etc.).  The only way out
        // is to type `exit` in the shell.
        if self.state == State::Shell {
            return self.handle_shell_state(key);
        }

        // Clear any flash message on the next keypress.
        self.flash = None;

        // Ctrl+T = music player (opens from any state).  We can't use
        // Ctrl+M — it sends the same byte (0x0D) as Enter in most
        // terminals so crossterm never sees it as Ctrl+M.
        // Check this BEFORE overlay dispatch so it works everywhere.
        if key.code == KeyCode::Char('t') && key.modifiers == KeyModifiers::CONTROL {
            self.state = State::Music;
            self.music_player.selected = 0;
            // Scan ~/Music (recursively).  Fall back to cwd if HOME
            // isn't set (very unlikely on macOS).
            let music_dir = std::env::var("HOME")
                .map(|h| format!("{h}/Music"))
                .unwrap_or_else(|_| ".".to_string());
            self.music_player.scan(&music_dir);
            return true;
        }

        // Ctrl+E = theme selector (opens from any state).
        if key.code == KeyCode::Char('e') && key.modifiers == KeyModifiers::CONTROL {
            self.state = State::Theme;
            self.theme_selected = 0;
            return true;
        }

        // Ctrl+J = pop-up shell (opens from any state).
        if key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::CONTROL {
            match ShellProcess::spawn() {
                Ok((shell, reader)) => {
                    shell.force_raw_mode();
                    self.shell = Some(shell);
                    self.shell_reader = Some(reader);
                    self.state = State::Shell;
                    // Set initial terminal size for the shell.
                    if let Some(ref s) = self.shell {
                        // pick a reasonable default — will be
                        // updated on first render.
                        s.resize(40, 100);
                    }
                }
                Err(e) => {
                    self.flash = Some(format!("shell failed: {e}"));
                }
            }
            return true;
        }

        // If an overlay is active (command bar, finder, run, music,
        // theme), dispatch to the overlay handler first.
        match self.state {
            State::Command => return self.handle_cmd_state(key),
            State::Finder => return self.handle_finder_state(key),
            State::Run => return self.handle_run_state(key),
            State::Music => return self.handle_music_state(key),
            State::Theme => return self.handle_theme_state(key),
            State::Normal => {}
            State::Shell => {} // handled in main.rs by spawning $SHELL
        }

                // Global keybindings that work in both normal and insert modes:
        // Ctrl+P = finder, Ctrl+R = run Python, Ctrl+T = music player,
        // Ctrl+C/D = quit.
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('p') => {
                    self.state = State::Finder;
                    self.finder_query.clear();
                    self.finder_selection = 0;
                    self.finder.collect_files();
                    self.finder.search("");
                    return true;
                }
                KeyCode::Char('r') => {
                    if let Some(ref fname) = self.filename.clone() {
                        let _ = self.save_to_disk(fname);
                        self.run_output = self.run_python(fname);
                    } else {
                        self.run_output =
                            "No filename set.  Use :w filename.py first.".to_string();
                    }
                    self.state = State::Run;
                    return true;
                }
                KeyCode::Char('c') | KeyCode::Char('d') => return false,
                _ => {}
            }
        }

        // Otherwise dispatch by editing mode.
        match self.mode {
            Mode::Normal => self.handle_normal_mode(key),
            Mode::Insert => self.handle_insert_mode(key),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Normal-mode key handling
    // ═══════════════════════════════════════════════════════════════

    fn handle_normal_mode(&mut self, key: KeyEvent) -> bool {
        // Emergency quit: Ctrl+C or Ctrl+D (check BEFORE matching
        // on key.code so we don't conflict with 'c'/'d' motions).
        if key.modifiers == KeyModifiers::CONTROL
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
        {
            return false;
        }

        match key.code {
            // ── cursor motion ──
            KeyCode::Char('h') | KeyCode::Left => {
                if self.cx > 0 {
                    self.cx -= 1;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cy + 1 < self.lines.len() {
                    self.cy += 1;
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let max_cx = self.lines[self.cy].len();
                if self.cx < max_cx {
                    self.cx += 1;
                }
            }
            KeyCode::Char('0') | KeyCode::Home => self.cx = 0,
            KeyCode::Char('$') | KeyCode::End => {
                self.cx = self.lines[self.cy].len();
            }

            // ── word motion ──
            KeyCode::Char('w') => {
                self.cx = self.next_word_pos(self.cy, self.cx);
            }
            KeyCode::Char('b') => {
                self.cx = self.prev_word_pos(self.cy, self.cx);
            }

            // ── page motion ──
            KeyCode::PageUp => {
                let page = (self.terminal_height() as usize).saturating_sub(2).max(1);
                self.cy = self.cy.saturating_sub(page / 2);
            }
            KeyCode::PageDown => {
                let page = (self.terminal_height() as usize).saturating_sub(2).max(1);
                self.cy = min(self.cy + page / 2, self.lines.len().saturating_sub(1));
            }

            // ── jump to first / last line ──
            KeyCode::Char('g') => {
                if self.cmd_buf == "g" {
                    // gg: reset pending buffer first
                } else {
                    self.cmd_buf.clear();
                }
                self.cy = 0;
                self.cx = 0;
            }
            KeyCode::Char('G') => {
                self.cmd_buf.clear();
                if !self.lines.is_empty() {
                    self.cy = self.lines.len() - 1;
                    self.cx = 0;
                }
            }

            // ── enter insert mode ──
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('a') => {
                if self.cx < self.lines[self.cy].len() {
                    self.cx += 1;
                }
                self.mode = Mode::Insert;
            }
            KeyCode::Char('I') => {
                self.cx = 0;
                self.mode = Mode::Insert;
            }
            KeyCode::Char('A') => {
                self.cx = self.lines[self.cy].len();
                self.mode = Mode::Insert;
            }

            // ── open new lines ──
            KeyCode::Char('o') => {
                self.save_undo();
                self.lines.insert(self.cy + 1, String::new());
                self.cy += 1;
                self.cx = 0;
                self.mode = Mode::Insert;
                self.modified = true;
            }
            KeyCode::Char('O') => {
                self.save_undo();
                self.lines.insert(self.cy, String::new());
                self.cx = 0;
                self.mode = Mode::Insert;
                self.modified = true;
            }

            // ── delete char (x) ──
            KeyCode::Char('x') => {
                self.save_undo();
                let line = &mut self.lines[self.cy];
                if self.cx < line.len() {
                    line.remove(self.cx);
                    self.modified = true;
                }
            }

            // ── delete line (dd) ──
            KeyCode::Char('d') => {
                self.save_undo();
                if !self.lines.is_empty() {
                    let removed = self.lines.remove(self.cy);
                    self.clipboard = vec![removed];
                    self.modified = true;
                    if self.lines.is_empty() {
                        self.lines.push(String::new());
                    }
                    self.cy = min(self.cy, self.lines.len().saturating_sub(1));
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }

            // ── yank line (yy) ──
            KeyCode::Char('y') => {
                if !self.lines.is_empty() {
                    self.clipboard = vec![self.lines[self.cy].clone()];
                }
            }

            // ── paste below / above (p / P) ──
            KeyCode::Char('p') => {
                if !self.clipboard.is_empty() {
                    self.save_undo();
                    for (i, line) in self.clipboard.iter().enumerate() {
                        self.lines.insert(self.cy + 1 + i, line.clone());
                    }
                    self.cy += self.clipboard.len();
                    self.cx = 0;
                    self.modified = true;
                }
            }
            KeyCode::Char('P') => {
                if !self.clipboard.is_empty() {
                    self.save_undo();
                    for (i, line) in self.clipboard.iter().enumerate() {
                        self.lines.insert(self.cy + i, line.clone());
                    }
                    self.cx = 0;
                    self.modified = true;
                }
            }

            // ── undo / redo ──
            KeyCode::Char('u') => {
                if let Some(prev) = self.undo_stack.pop() {
                    self.redo_stack.push(self.lines.clone());
                    self.lines = prev;
                    self.cy = min(self.cy, self.lines.len().saturating_sub(1));
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }

            // ── enter command mode ──
            KeyCode::Char(':') => {
                self.state = State::Command;
                self.cmd_buf.clear();
            }

            _ => {}
        }

        // Re-clamp scroll after any motion.
        self.clamp_scroll();
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Insert-mode key handling
    // ═══════════════════════════════════════════════════════════════

    fn handle_insert_mode(&mut self, key: KeyEvent) -> bool {
        // Save undo snapshot before the first modification in insert
        // mode (best-effort: we save once per insert session).
        // We track this by checking if undo_stack's last entry matches.
        // This is a simplistic approach — a real editor would use a
        // coalescing undo mechanism.
        if self.undo_stack.last().map(|s| *s != self.lines).unwrap_or(true) {
            self.save_undo();
        }

        match key.code {
            // Escape returns to normal mode.
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                // Move cursor one left if past end of line (vim does this
                // so the block cursor sits on the last character).
                if self.cx > 0 && self.cx > self.lines[self.cy].len() {
                    self.cx = self.lines[self.cy].len();
                }
            }

            // Enter: split the line at the cursor.
            KeyCode::Enter => {
                let right = self.lines[self.cy].split_off(self.cx);
                self.lines.insert(self.cy + 1, right);
                self.cy += 1;
                self.cx = 0;
                self.modified = true;
            }

            // Backspace: delete character before cursor.
            KeyCode::Backspace => {
                if self.cx > 0 {
                    let line = &mut self.lines[self.cy];
                    line.remove(self.cx - 1);
                    self.cx -= 1;
                    self.modified = true;
                } else if self.cy > 0 {
                    // Join with previous line.
                    let prev_len = self.lines[self.cy - 1].len();
                    let current = self.lines.remove(self.cy);
                    self.lines[self.cy - 1].push_str(&current);
                    self.cy -= 1;
                    self.cx = prev_len;
                    self.modified = true;
                }
            }

            // Delete: remove character at cursor.
            KeyCode::Delete => {
                let line = &mut self.lines[self.cy];
                if self.cx < line.len() {
                    line.remove(self.cx);
                    self.modified = true;
                } else if self.cy + 1 < self.lines.len() {
                    let next = self.lines.remove(self.cy + 1);
                    self.lines[self.cy].push_str(&next);
                    self.modified = true;
                }
            }

            // Tab: insert 4 spaces.
            KeyCode::Tab => {
                let line = &mut self.lines[self.cy];
                line.insert_str(self.cx, "    ");
                self.cx += 4;
                self.modified = true;
            }

            // Arrow keys: move cursor (don't exit insert mode).
            KeyCode::Left => {
                if self.cx > 0 { self.cx -= 1; }
            }
            KeyCode::Right => {
                if self.cx < self.lines[self.cy].len() { self.cx += 1; }
            }
            KeyCode::Up => {
                if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = self.cx.min(self.lines[self.cy].len());
                }
            }
            KeyCode::Down => {
                if self.cy + 1 < self.lines.len() {
                    self.cy += 1;
                    self.cx = self.cx.min(self.lines[self.cy].len());
                }
            }

            // Regular character: insert at cursor.
            KeyCode::Char(ch) => {
                let line = &mut self.lines[self.cy];
                line.insert(self.cx, ch);
                self.cx += 1;
                self.modified = true;
            }

            _ => {}
        }

        self.clamp_scroll();
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Command-state key handling (:command)
    // ═══════════════════════════════════════════════════════════════

    fn handle_cmd_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc or Enter: process or cancel command.
            KeyCode::Esc => {
                self.state = State::Normal;
                self.cmd_buf.clear();
            }
            KeyCode::Enter => {
                let cmd = self.cmd_buf.clone();
                self.cmd_buf.clear();
                self.state = State::Normal;
                return self.exec_cmd(&cmd);
            }

            // Backspace: remove last character.
            KeyCode::Backspace => {
                self.cmd_buf.pop();
            }

            // Regular char: append to command buffer.
            KeyCode::Char(ch) => {
                self.cmd_buf.push(ch);
            }

            _ => {}
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Finder-state key handling (Ctrl+P overlay)
    // ═══════════════════════════════════════════════════════════════

    fn handle_finder_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc or Ctrl+P: close finder.
            KeyCode::Esc => {
                self.state = State::Normal;
            }
            KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                self.state = State::Normal;
            }

            // Enter: open the selected file.
            KeyCode::Enter => {
                if let Some((path, _)) = self.finder.results.get(self.finder_selection) {
                    let full_path = std::env::current_dir()
                        .unwrap_or_default()
                        .join(path);
                    if let Ok(content) = fs::read_to_string(&full_path) {
                        self.lines = content.lines().map(|l| l.to_string()).collect();
                        if content.ends_with('\n') {
                            self.lines.push(String::new());
                        }
                        self.filename = Some(full_path.to_string_lossy().to_string());
                        self.modified = false;
                        self.cy = 0;
                        self.cx = 0;
                        self.top = 0;
                        self.left = 0;
                        self.undo_stack.clear();
                        self.redo_stack.clear();
                    }
                }
                self.state = State::Normal;
            }

            // Navigation: up/down in the result list.
            KeyCode::Up | KeyCode::Char('k') => {
                self.finder_selection = self.finder_selection.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.finder.results.len().saturating_sub(1);
                self.finder_selection = min(self.finder_selection + 1, max);
            }
            // Page up/down in finder results.
            KeyCode::PageUp => {
                self.finder_selection = self.finder_selection.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = self.finder.results.len().saturating_sub(1);
                self.finder_selection = min(self.finder_selection + 10, max);
            }

            // Backspace: remove last query character.
            KeyCode::Backspace => {
                self.finder_query.pop();
                self.finder.search(&self.finder_query);
                self.finder_selection = 0;
            }

            // Regular char: append to query and re-search.
            KeyCode::Char(ch) => {
                self.finder_query.push(ch);
                self.finder.search(&self.finder_query);
                self.finder_selection = 0;
            }

            _ => {}
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Run-state key handling (Ctrl+R overlay)
    // ═══════════════════════════════════════════════════════════════

    fn handle_run_state(&mut self, key: KeyEvent) -> bool {
        // Any key dismisses the run output.
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('d')
                if key.modifiers == KeyModifiers::CONTROL =>
            {
                return false;
            }
            _ => {
                self.state = State::Normal;
            }
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Music-state key handling (Ctrl+M overlay)
    // ═══════════════════════════════════════════════════════════════

    /// Handle keys while the music-player picker is open.
    fn handle_music_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc closes the picker (playback continues).
            KeyCode::Esc => {
                self.state = State::Normal;
            }
            // Enter: start playing the selected file.
            KeyCode::Enter => {
                self.music_player.play(self.music_player.selected);
            }
            // 's' or Backspace: stop playback.
            KeyCode::Char('s') | KeyCode::Backspace => {
                self.music_player.stop();
            }
            // Navigation.
            KeyCode::Up | KeyCode::Char('k') => {
                self.music_player.selected =
                    self.music_player.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.music_player.files.len().saturating_sub(1);
                self.music_player.selected =
                    (self.music_player.selected + 1).min(max);
            }
            KeyCode::PageUp => {
                self.music_player.selected =
                    self.music_player.selected.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = self.music_player.files.len().saturating_sub(1);
                self.music_player.selected =
                    (self.music_player.selected + 10).min(max);
            }

            KeyCode::Char('c') | KeyCode::Char('d')
                if key.modifiers == KeyModifiers::CONTROL =>
            {
                return false;
            }

            _ => {}
        }
        true
    }

    // ── theme-selector state ────────────────────────────────────

    fn handle_theme_state(&mut self, key: KeyEvent) -> bool {
        let themes = ThemeKind::all();
        match key.code {
            // Esc / Enter: close selector, apply selection.
            KeyCode::Esc => self.state = State::Normal,
            KeyCode::Enter => {
                self.theme = themes[self.theme_selected];
                self.state = State::Normal;
            }
            // Navigation.
            KeyCode::Up | KeyCode::Char('k') => {
                self.theme_selected = self.theme_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = themes.len().saturating_sub(1);
                self.theme_selected = (self.theme_selected + 1).min(max);
            }
            // Ctrl+C/D: quit.
            KeyCode::Char('c') | KeyCode::Char('d')
                if key.modifiers == KeyModifiers::CONTROL =>
            {
                return false;
            }
            _ => {}
        }
        true
    }

    fn handle_shell_state(&mut self, key: KeyEvent) -> bool {
        let shell = match self.shell.as_ref() {
            Some(s) => s,
            None => {
                self.state = State::Normal;
                return true;
            }
        };
        // Force raw mode on the slave side so the terminal driver
        // never echoes (even if the shell re-enabled it).
        shell.force_raw_mode();
        let bytes = key_to_bytes(key);
        shell.write(&bytes);
        true
    }

    /// Poll the music-player thread for events (auto-next-track).
    /// Called every frame: polls music events and auto-reloads the
    /// file if it changed on disk (only when there are no unsaved edits).
    pub fn tick(&mut self) {
        self.music_player.poll();
        self.auto_reload();
        // Pump shell output and detect shell death.
        if let Some(ref mut s) = self.shell {
            s.tick();
            s.force_raw_mode();
            if !s.is_alive() {
                self.shell = None;
                if let Some(handle) = self.shell_reader.take() {
                    let _ = handle.join();
                }
                self.state = State::Normal;
            }
        }
    }

    /// If the open file was modified externally (mtime changed) and
    /// we have no unsaved changes, reload it in-place.
    fn auto_reload(&mut self) {
        let fname = match &self.filename {
            Some(f) => f.clone(),
            None => return,
        };
        // Don't clobber unsaved edits.
        if self.modified {
            return;
        }
        let current_mtime =
            match fs::metadata(&fname).ok().and_then(|m| m.modified().ok()) {
                Some(m) => m,
                None => return,
            };
        let prev = match self.last_mtime {
            Some(p) => p,
            None => {
                self.last_mtime = Some(current_mtime);
                return;
            }
        };
        if current_mtime == prev {
            return;
        }
        // File changed — reload.
        let content = match fs::read_to_string(&fname) {
            Ok(c) => c,
            Err(_) => return,
        };
        let mut new_lines: Vec<String> =
            content.lines().map(|l| l.to_string()).collect();
        if content.ends_with('\n') {
            new_lines.push(String::new());
        }
        if new_lines.is_empty() {
            new_lines.push(String::new());
        }
        // Preserve cursor line if possible.
        let saved_cy = self.cy.min(new_lines.len().saturating_sub(1));
        let saved_cx = self.cx.min(new_lines[saved_cy].len());
        self.lines = new_lines;
        self.cy = saved_cy;
        self.cx = saved_cx;
        self.top = self.top.min(self.lines.len().saturating_sub(1));
        self.last_mtime = Some(current_mtime);
    }

    // ═══════════════════════════════════════════════════════════════
    //  Command execution
    // ═══════════════════════════════════════════════════════════════

    /// Parse and execute a command string (without the leading `:`).
    ///
    /// Returns `false` if the command should quit the editor.
    fn exec_cmd(&mut self, cmd: &str) -> bool {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        if parts.is_empty() {
            return true;
        }
        let cmd_name = parts[0];

        match cmd_name {
            // :q — quit (refuse if modified)
            "q" => {
                if self.modified {
                    self.flash = Some("No write since last change (add ! to override)".to_string());
                    return true;
                }
                return false;
            }
            // :q! — force quit
            "q!" => return false,
            // :w — save
            "w" => {
                if let Some(ref fname) = self.filename.clone() {
                    match self.save_to_disk(fname) {
                        Ok(_) => {
                            self.modified = false;
                            self.flash = Some(format!("'{}' written", fname));
                        }
                        Err(e) => {
                            self.flash = Some(format!("Error: {e}"));
                        }
                    }
                } else {
                    self.flash = Some("No filename.  Use :w <name>".to_string());
                }
            }
            // :w <filename> — save as
            cmd if cmd.starts_with("w ") => {
                let fname = cmd[2..].trim();
                if !fname.is_empty() {
                    match self.save_to_disk(fname) {
                        Ok(_) => {
                            self.filename = Some(fname.to_string());
                            self.modified = false;
                            self.flash = Some(format!("'{}' written", fname));
                        }
                        Err(e) => {
                            self.flash = Some(format!("Error: {e}"));
                        }
                    }
                }
            }
            // :wq — save and quit
            "wq" => {
                if let Some(ref fname) = self.filename.clone() {
                    if self.save_to_disk(fname).is_ok() {
                        self.modified = false;
                        return false;
                    }
                } else {
                    self.flash = Some("No filename".to_string());
                    return true;
                }
                return false;
            }
            // :theme <name> — switch theme
            cmd if cmd.starts_with("theme ") || cmd == "theme" => {
                let name = cmd.trim_start_matches("theme ").trim();
                if name.is_empty() || name == "theme" {
                    self.flash = Some(format!("Current theme: {}", self.theme.name()));
                } else if let Some(t) = ThemeKind::from_str(name) {
                    self.theme = t;
                    self.flash = Some(format!("Theme: {}", t.name()));
                } else {
                    self.flash = Some(format!("Unknown theme '{name}'"));
                }
            }
            // :themes — list available themes
            "themes" => {
                let names: Vec<&str> = ThemeKind::all().iter().map(|t| t.name()).collect();
                self.flash = Some(format!("Themes: {}", names.join(", ")));
            }
            // :e <filename> — open a file
            cmd if cmd.starts_with("e ") => {
                let fname = cmd[2..].trim();
                if !fname.is_empty() {
                    match fs::read_to_string(fname) {
                        Ok(content) => {
                            let mut lines: Vec<String> =
                                content.lines().map(|l| l.to_string()).collect();
                            if content.ends_with('\n') {
                                lines.push(String::new());
                            }
                            self.lines = lines;
                            self.filename = Some(fname.to_string());
                            self.modified = false;
                            self.cy = 0;
                            self.cx = 0;
                            self.top = 0;
                            self.left = 0;
                            self.undo_stack.clear();
                            self.redo_stack.clear();
                        }
                        Err(e) => {
                            self.flash = Some(format!("Can't open {fname}: {e}"));
                        }
                    }
                }
            }
            // Unknown command.
            _ => {
                self.flash = Some(format!("Unknown command: {cmd_name}"));
            }
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  File operations
    // ═══════════════════════════════════════════════════════════════

    /// Save the current buffer to `path`.
    fn save_to_disk(&mut self, path: &str) -> Result<()> {
        let content = self.lines.join("\n");
        fs::write(path, &content).with_context(|| format!("can't write {path}"))?;
        self.last_mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
        Ok(())
    }

    /// Run a Python file and capture its stdout + stderr.
    fn run_python(&self, path: &str) -> String {
        let output = ProcCmd::new("python3")
            .arg(path)
            .output();
        match output {
            Ok(out) => {
                let mut result = String::new();
                if !out.stdout.is_empty() {
                    result.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                if result.is_empty() {
                    result = "(no output)".to_string();
                }
                result
            }
            Err(e) => format!("Error running python3: {e}"),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Scroll helpers
    // ═══════════════════════════════════════════════════════════════

    /// Clamp `top` and `left` so the cursor stays visible.
    fn clamp_scroll(&mut self) {
        // The gutter width (line number string + "| ") we need to
        // account for in horizontal scrolling.
        let gutter = self.gutter_width() + 2; // "│ " after the number

        // Vertical: make sure cursor line is on screen.
        let height = self.terminal_height() as usize;
        let page_lines = height.saturating_sub(1); // reserve 1 for status bar
        if self.cy < self.top {
            self.top = self.cy;
        }
        if self.cy >= self.top + page_lines {
            self.top = self.cy.saturating_sub(page_lines) + 1;
        }

        // Horizontal: make sure cursor column is on screen.
        let width = self.terminal_width() as usize;
        let visible_cols = width.saturating_sub(gutter);
        if visible_cols == 0 {
            return;
        }
        if self.cx < self.left {
            self.left = self.cx;
        }
        if self.cx >= self.left + visible_cols {
            self.left = self.cx.saturating_sub(visible_cols) + 1;
        }
    }

    /// Number of chars needed to print the highest line number.
    fn gutter_width(&self) -> usize {
        let total = self.lines.len();
        if total == 0 { 1 } else { total.to_string().len().max(2) }
    }

    /// Height of the terminal (from the last rendered frame or default).
    fn terminal_height(&self) -> u16 {
        self.cache_h
    }

    /// Width of the terminal (from the last rendered frame or default).
    fn terminal_width(&self) -> u16 {
        self.cache_w
    }

    // ═══════════════════════════════════════════════════════════════
    //  Word-motion helpers
    // ═══════════════════════════════════════════════════════════════

    /// Find the start position of the next word on or after `col`.
    fn next_word_pos(&self, row: usize, col: usize) -> usize {
        let line = &self.lines[row];
        let mut pos = col;
        // Skip whitespace.
        while pos < line.len() && line.as_bytes()[pos] == b' ' {
            pos += 1;
        }
        // Skip word characters.
        while pos < line.len() && line.as_bytes()[pos] != b' ' {
            pos += 1;
        }
        if pos >= line.len() && row + 1 < self.lines.len() {
            // Wrap to next line.
            return 0;
        }
        pos
    }

    /// Find the start position of the previous word before `col`.
    #[allow(dead_code)]
    fn prev_word_pos(&self, row: usize, col: usize) -> usize {
        if col == 0 {
            if row > 0 {
                return self.lines[row - 1].len();
            }
            return 0;
        }
        let line = &self.lines[row];
        let mut pos = col;
        // Skip backwards over whitespace.
        while pos > 0 && line.as_bytes()[pos.saturating_sub(1)] == b' ' {
            pos -= 1;
        }
        // Skip backwards over word characters (stop before the word).
        while pos > 0 && line.as_bytes()[pos.saturating_sub(1)] != b' ' {
            pos -= 1;
        }
        pos
    }

    /// Save the current buffer state onto the undo stack.
    fn save_undo(&mut self) {
        self.undo_stack.push(self.lines.clone());
        // Keep undo stack manageable (max 100 entries).
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        // Redo is invalidated on new changes.
        self.redo_stack.clear();
    }

    // ═══════════════════════════════════════════════════════════════
    //  Rendering
    // ═══════════════════════════════════════════════════════════════

    /// Draw the editor into the terminal frame.
    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        if area.width == 0 || area.height == 0 {
            return; // terminal too small, don't draw anything
        }

        // Full-area clear so shell-popup border artifacts don't
        // linger when the shell closes.
        f.render_widget(ratatui::widgets::Clear, area);

        // Cache dimensions for scroll clamping (called outside render).
        self.cache_w = area.width;
        self.cache_h = area.height;

        // Cache dimensions.
        let _height = area.height as usize;
        let width  = area.width as usize;

        // Split the screen: main content + status bar (1 line at the bottom).
        let [content_area, status_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        // ── 1. main content ──────────────────────────────────────
        let gutter = self.gutter_width() + 1; // e.g. " 12│"
        let theme = self.theme.theme();
        let mut lines_vec: Vec<Line> = Vec::new();
        let visible_lines = content_area.height as usize;

        for row in 0..visible_lines {
            let buf_row = self.top + row;
            let style = Style::new().fg(theme.fg).bg(theme.bg);

            if buf_row >= self.lines.len() {
                // Past EOF: show "~" filler lines (like vim).
                let tilde = Span::styled("~", theme.tilde);
                lines_vec.push(Line::from(vec![
                    Span::raw(" ".repeat(gutter + 1)),
                    tilde,
                ]));
                continue;
            }

            // (a) line number gutter
            let line_num = format!("{:>width$}│", buf_row + 1, width = gutter - 1);
            let num_span = Span::styled(line_num, theme.line_number);

            // (b) highlighted content
            let raw = &self.lines[buf_row];
            let lang = self.filename.as_deref()
                .and_then(|f| {
                    let ext = f.rsplit('.').next()?;
                    match ext {
                        "rs" => Some(highlight::Lang::Rust),
                        "py" => Some(highlight::Lang::Python),
                        "conf" | "ini" | "cfg" => Some(highlight::Lang::Conf),
                        _ => None,
                    }
                })
                .unwrap_or(highlight::Lang::Plain);
            let highlighted = if buf_row < self.lines.len() {
                highlight::highlight_line(raw, &theme, lang)
            } else {
                vec![Span::styled(raw.clone(), style)]
            };

            // Build the line: gutter + content.
            let mut spans = vec![num_span];
            spans.extend(highlighted);

            // If the cursor is on this line and we're in insert mode,
            // we can add a visual cursor hint (but ratatui handles
            // the real cursor via f.set_cursor_position).
            lines_vec.push(Line::from(spans));
        }

        // Wrap in Paragraph and render.  We explicitly set the width
        // so long lines don't wrap (we handle horiz scroll ourselves).
        let content = Text::from(lines_vec);
        // Clear the entire content area first so shell-overlay artifacts
        // (text that rendered past the line-number gutter) don't ghost.
        f.render_widget(ratatui::widgets::Clear, content_area);
        let paragraph = Paragraph::new(content)
            .style(Style::new().bg(theme.bg));
        f.render_widget(paragraph, content_area);

        // ── 2. status bar ────────────────────────────────────────
        self.render_status(f, status_area, width);

        // ── 3. cursor position ───────────────────────────────────
        let cy_screen = (self.cy.saturating_sub(self.top)) as u16;
        let cx_screen = (self.cx.saturating_sub(self.left)) as u16 + gutter as u16;
        if cy_screen < content_area.height {
            let cursor_x = content_area.x + cx_screen;
            let cursor_y = content_area.y + cy_screen;
            f.set_cursor_position(Position::new(cursor_x, cursor_y));
        }

        // ── 4. overlays ──────────────────────────────────────────
        match self.state {
            State::Command if !self.cmd_buf.is_empty() => {
                // Command is shown in the status bar already;
                // no separate overlay needed.
            }
            State::Finder => self.render_finder(f, area),
            State::Music => self.render_music(f, area),
            State::Theme => self.render_theme(f, area),
            State::Run => self.render_run(f, area, &theme),
            State::Shell => self.render_shell(f, area),
            _ => {}
        }
    }

    // ── status bar ───────────────────────────────────────────────

    /// Render the status bar at the bottom of the screen.
    fn render_status(&self, f: &mut Frame, area: Rect, _width: usize) {
        let theme = self.theme.theme();

        // Build the status text depending on state.
        let text: String = if let Some(ref msg) = self.flash {
            format!(" {msg} ")
        } else if self.state == State::Command {
            // Show the command being typed.
            format!(":{}", self.cmd_buf)
        } else {
            // Normal status:  mode | filename | modified | line:col | theme
            let mode_str = match self.mode {
                Mode::Normal => "NORMAL",
                Mode::Insert => "INSERT",
            };
            let fname = self
                .filename
                .as_deref()
                .unwrap_or("[No Name]");
            let mod_str = if self.modified { " [+]" } else { "" };
            let location = format!("{}:{}", self.cy + 1, self.cx + 1);
            let tname = self.theme.name();
            // Append " ♫ song.mp3" if playing music.
            let now_playing = self.music_player.current_song.as_ref()
                .filter(|_| self.music_player.playing)
                .map(|s| {
                    let name = s.rsplit('/').next().unwrap_or(s);
                    format!("  ♫ {name}")
                })
                .unwrap_or_default();
            format!(
                " {mode_str}  {fname}{mod_str}  ─ {location}  ─ {tname}{now_playing} "
            )
        };

        let bar = Paragraph::new(Line::from(Span::styled(
            text.clone(),
            Style::new().fg(theme.status_fg).bg(theme.status_bg),
        )))
        .style(Style::new().bg(theme.status_bg));
        f.render_widget(ratatui::widgets::Clear, area);
        f.render_widget(bar, area);
    }

    // ── finder overlay ───────────────────────────────────────────

    fn render_finder(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.theme();

        // Centered popup: ~60% width, ~50% height.
        let popup_w = (area.width as f32 * 0.6) as u16;
        let popup_h = (area.height as f32 * 0.5) as u16;
        let popup_x = (area.width - popup_w) / 2;
        let popup_y = (area.height - popup_h) / 3;
        let popup = Rect::new(
            popup_x,
            popup_y,
            popup_w.min(area.width),
            popup_h.min(area.height),
        );

        // Clear the area so the popup stands out.
        f.render_widget(Clear, popup);

        // Outer block with "Find File" title.
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Find File ")
            .title_style(Style::new().fg(theme.fg))
            .border_style(Style::new().fg(theme.fg));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        // Split inner area: query bar (1 line) + results list (rest).
        if inner.height < 2 {
            return;
        }
        let [query_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);

        // Query bar (status-bar style line).
        let query_text = if self.finder_query.is_empty() {
            " type to search…".to_string()
        } else {
            format!(" {}", self.finder_query)
        };
        let query_span = Span::styled(
            query_text,
            Style::new().fg(theme.status_fg).bg(theme.status_bg),
        );
        f.render_widget(
            Paragraph::new(Line::from(query_span))
                .style(Style::new().bg(theme.status_bg)),
            query_area,
        );

        // Results list.
        let results: Vec<ListItem> = self
            .finder
            .results
            .iter()
            .enumerate()
            .map(|(i, (path, _score))| {
                let style = if i == self.finder_selection {
                    Style::new()
                        .fg(theme.status_bg)
                        .bg(theme.status_fg)
                } else {
                    Style::new().fg(theme.fg).bg(theme.bg)
                };
                ListItem::new(Line::from(Span::styled(path.clone(), style)))
            })
            .collect();

        if results.is_empty() {
            let no_files = vec![ListItem::new(Line::from(Span::styled(
                " (no files found)",
                Style::new().fg(theme.fg).bg(theme.bg),
            )))];
            f.render_widget(
                List::new(no_files).style(Style::new().bg(theme.bg)),
                list_area,
            );
        } else {
            f.render_widget(
                List::new(results).style(Style::new().bg(theme.bg)),
                list_area,
            );
        }
    }

    // ── music player overlay (Ctrl+M) ──────────────────────────

    fn render_music(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.theme();

        let popup_w = (area.width as f32 * 0.6) as u16;
        let popup_h = (area.height as f32 * 0.5) as u16;
        let popup_x = (area.width - popup_w) / 2;
        let popup_y = (area.height - popup_h) / 3;
        let popup = Rect::new(
            popup_x,
            popup_y,
            popup_w.min(area.width),
            popup_h.min(area.height),
        );

        f.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Music Player ")
            .title_style(Style::new().fg(theme.fg))
            .border_style(Style::new().fg(theme.fg));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        if inner.height < 2 {
            return;
        }
        let [status_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);

        // Status line: now-playing or hint.
        let status_text = if let Some(ref song) = self.music_player.current_song {
            let name = song.rsplit('/').next().unwrap_or(song);
            format!(" Now Playing: {name}")
        } else if self.music_player.files.is_empty() {
            " No MP3 files found in this directory.".to_string()
        } else {
            format!(" {} files — Enter=play  s=stop  Esc=close", self.music_player.files.len())
        };
        let status_span = Span::styled(
            status_text,
            Style::new().fg(theme.status_fg).bg(theme.status_bg),
        );
        f.render_widget(
            Paragraph::new(Line::from(status_span))
                .style(Style::new().bg(theme.status_bg)),
            status_area,
        );

        // File list.
        let results: Vec<ListItem> = self
            .music_player
            .files
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let name = path.rsplit('/').next().unwrap_or(path);
                let is_playing = self.music_player.playing
                    && self.music_player.current_index == i;
                let style = if i == self.music_player.selected {
                    Style::new()
                        .fg(theme.status_bg)
                        .bg(theme.status_fg)
                } else if is_playing {
                    Style::new()
                        .fg(theme.fg)
                        .bg(theme.bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(theme.fg).bg(theme.bg)
                };
                let prefix = if is_playing { " ▶ " } else { "   " };
                ListItem::new(Line::from(Span::styled(
                    format!("{prefix}{name}"),
                    style,
                )))
            })
            .collect();

        if results.is_empty() {
            let no_files = vec![ListItem::new(Line::from(Span::styled(
                " (no mp3 files)",
                Style::new().fg(theme.fg).bg(theme.bg),
            )))];
            f.render_widget(
                List::new(no_files).style(Style::new().bg(theme.bg)),
                list_area,
            );
        } else {
            f.render_widget(
                List::new(results).style(Style::new().bg(theme.bg)),
                list_area,
            );
        }
    }

    // ── theme-selector overlay ─────────────────────────────────

    fn render_theme(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.theme();

        let popup_w = (area.width as f32 * 0.45) as u16;
        let popup_h = (area.height as f32 * 0.4) as u16;
        let popup_x = (area.width - popup_w) / 2;
        let popup_y = (area.height - popup_h) / 3;
        let popup = Rect::new(
            popup_x,
            popup_y,
            popup_w.min(area.width),
            popup_h.min(area.height),
        );

        f.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Theme Selector ")
            .title_style(Style::new().fg(theme.fg))
            .border_style(Style::new().fg(theme.fg));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        if inner.height < 1 {
            return;
        }

        let list_items: Vec<ListItem> = ThemeKind::all()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let name = t.name();
                let is_current = *t == self.theme;
                let style = if i == self.theme_selected {
                    Style::new()
                        .fg(theme.status_bg)
                        .bg(theme.status_fg)
                } else if is_current {
                    Style::new()
                        .fg(theme.fg)
                        .bg(theme.bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(theme.fg).bg(theme.bg)
                };
                let prefix = if is_current { " ✓ " } else { "   " };
                ListItem::new(Line::from(Span::styled(
                    format!("{prefix}{name}"),
                    style,
                )))
            })
            .collect();

        f.render_widget(
            List::new(list_items).style(Style::new().bg(theme.bg)),
            inner,
        );
    }

    // ── run output overlay ───────────────────────────────────────

    fn render_run(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Centered popup: ~70% width, ~60% height.
        let popup_w = (area.width as f32 * 0.7) as u16;
        let popup_h = (area.height as f32 * 0.6) as u16;
        let popup_x = (area.width - popup_w) / 2;
        let popup_y = (area.height - popup_h) / 3;
        let popup = Rect::new(
            popup_x,
            popup_y,
            popup_w.min(area.width),
            popup_h.min(area.height),
        );

        f.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Run Output ")
            .title_style(Style::new().fg(theme.fg))
            .border_style(Style::new().fg(theme.fg));

        let output_paragraph = Paragraph::new(Text::from(self.run_output.clone()))
            .block(block)
            .style(Style::new().fg(theme.fg).bg(theme.bg))
            .wrap(Wrap { trim: false });

        f.render_widget(output_paragraph, popup);
    }

    fn render_shell(&self, f: &mut Frame, area: Rect) {
        let theme = self.theme.theme();
        let popup_w = (area.width as f32 * 0.92) as u16;
        let popup_h = (area.height as f32 * 0.85) as u16;
        let popup_x = (area.width - popup_w) / 2;
        let popup_y = (area.height - popup_h) / 2;
        let popup = Rect::new(
            popup_x,
            popup_y,
            popup_w.min(area.width),
            popup_h.min(area.height),
        );

        f.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Shell — type exit to close ")
            .title_style(Style::new().fg(theme.fg))
            .border_style(Style::new().fg(theme.fg));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        // Tell the shell about the popup dimensions (for column width, etc.).
        if let Some(ref s) = self.shell {
            s.resize(inner.height, inner.width);
        }

        if inner.height < 1 {
            return;
        }

        let output_str = self.shell.as_ref().map(|s| s.output());
        let text = output_str.as_deref().unwrap_or("");
        let lines: Vec<&str> = text.lines().collect();
        let max_rows = inner.height as usize;
        let start = lines.len().saturating_sub(max_rows);
        let visible: Vec<&str> = lines[start..].to_vec();

        let mut styled_lines: Vec<Line> = Vec::with_capacity(visible.len());
        for line in &visible {
            styled_lines.push(Line::from(Span::styled(
                *line,
                Style::new().fg(theme.fg).bg(theme.bg),
            )));
        }
        // Pad empty lines so the cursor stays in place visually.
        while styled_lines.len() < max_rows {
            styled_lines.push(Line::from(Span::styled(
                "~",
                Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)).bg(theme.bg),
            )));
        }

        let paragraph = Paragraph::new(Text::from(styled_lines))
            .style(Style::new().bg(theme.bg));
        f.render_widget(paragraph, inner);
    }
}

/// Convert a crossterm key event into the byte sequence to send to
/// the shell's PTY.  Handles regular chars, Ctrl+letter, arrows,
/// Home/End, Delete, PageUp/Down, Esc, Tab, Enter, Backspace.
fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers == KeyModifiers::CONTROL && c.is_ascii_lowercase() {
                vec![(c as u8) - b'a' + 1] // Ctrl+A → 0x01, etc.
            } else if key.modifiers == KeyModifiers::CONTROL && c.is_ascii_uppercase() {
                // Ctrl+Shift+A → 0x01 as well (same byte)
                vec![(c as u8) - b'A' + 1]
            } else if key.modifiers == KeyModifiers::ALT {
                let mut v = vec![0x1b]; // ESC prefix
                v.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
                v
            } else {
                c.to_string().into_bytes()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        _ => vec![],
    }
}
