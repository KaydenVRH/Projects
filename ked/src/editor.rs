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
};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::highlight;
use crate::theme::{Theme, ThemeKind};
use crate::finder::Finder;

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

    // ── file ──
    pub filename: Option<String>,
    pub modified: bool,

    // ── undo / redo ──
    pub undo_stack: Vec<Vec<String>>,
    pub redo_stack: Vec<Vec<String>>,

    // ── clipboard (for yy/dd/p/P) ──
    pub clipboard: Vec<String>,

    // ── theme ──
    pub theme: ThemeKind,

    // ── cached terminal size (updated every render) ──
    pub cache_w: u16,
    pub cache_h: u16,
}

impl Editor {
    // ═══════════════════════════════════════════════════════════════
    //  Construction
    // ═══════════════════════════════════════════════════════════════

    /// Create a new editor, optionally loading a file from disk.
    ///
    /// If `filename` is `None`, start with a single empty line
    /// (like vim's empty buffer).
    pub fn new(filename: Option<&str>) -> Result<Self> {
        let (lines, filename) = if let Some(path) = filename {
            let content =
                fs::read_to_string(path).with_context(|| format!("can't read {path}"))?;
            // Split into lines.  If the file ends with `\n`, push an
            // extra empty line so the cursor can sit on the "virtual"
            // line below the last content line (vim behaviour).
            let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
            if content.ends_with('\n') {
                lines.push(String::new());
            }
            (lines, Some(path.to_string()))
        } else {
            (vec![String::new()], None)
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
            filename,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            clipboard: Vec::new(),
            theme: ThemeKind::Default,
            cache_w: 80,
            cache_h: 24,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    //  Key dispatch
    // ═══════════════════════════════════════════════════════════════

    /// Handle a key press.  Returns `true` to keep running, `false` to quit.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // If an overlay is active (command bar, finder, run output),
        // dispatch to the overlay handler first.
        match self.state {
            State::Command => return self.handle_cmd_state(key),
            State::Finder => return self.handle_finder_state(key),
            State::Run => return self.handle_run_state(key),
            State::Normal => {}
        }

        // Global keybindings that work in both normal and insert modes:
        // Ctrl+P = fuzzy file finder, Ctrl+R = run Python, Ctrl+C/D = quit.
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
                    // Show error in status line — we set state back
                    // to Command with an error message for one frame.
                    self.cmd_buf = "No write since last change (add ! to override)".to_string();
                    self.state = State::Command;
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
                            self.cmd_buf = format!("'{}' written", fname);
                            self.state = State::Command;
                        }
                        Err(e) => {
                            self.cmd_buf = format!("Error: {e}");
                            self.state = State::Command;
                        }
                    }
                } else {
                    self.cmd_buf = "No filename.  Use :w <name>".to_string();
                    self.state = State::Command;
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
                            self.cmd_buf = format!("'{}' written", fname);
                            self.state = State::Command;
                        }
                        Err(e) => {
                            self.cmd_buf = format!("Error: {e}");
                            self.state = State::Command;
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
                    self.cmd_buf = "No filename".to_string();
                    self.state = State::Command;
                    return true;
                }
                return false;
            }
            // :theme <name> — switch theme
            cmd if cmd.starts_with("theme ") || cmd == "theme" => {
                let name = cmd.trim_start_matches("theme ").trim();
                if name.is_empty() || name == "theme" {
                    self.cmd_buf = format!("Current theme: {}", self.theme.name());
                    self.state = State::Command;
                } else if let Some(t) = ThemeKind::from_str(name) {
                    self.theme = t;
                    self.cmd_buf = format!("Theme: {}", t.name());
                    self.state = State::Command;
                } else {
                    self.cmd_buf = format!("Unknown theme '{name}'");
                    self.state = State::Command;
                }
            }
            // :themes — list available themes
            "themes" => {
                let names: Vec<&str> = ThemeKind::all().iter().map(|t| t.name()).collect();
                self.cmd_buf = format!("Themes: {}", names.join(", "));
                self.state = State::Command;
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
                            self.cmd_buf = format!("Can't open {fname}: {e}");
                            self.state = State::Command;
                        }
                    }
                }
            }
            // Unknown command.
            _ => {
                self.cmd_buf = format!("Unknown command: {cmd_name}");
                self.state = State::Command;
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
            let highlighted = if buf_row < self.lines.len() {
                highlight::highlight_line(raw, &theme)
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
            State::Run => self.render_run(f, area, &theme),
            _ => {}
        }
    }

    // ── status bar ───────────────────────────────────────────────

    /// Render the status bar at the bottom of the screen.
    fn render_status(&self, f: &mut Frame, area: Rect, _width: usize) {
        let theme = self.theme.theme();

        // Build the status text depending on state.
        let text: String = if self.state == State::Command {
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
            format!(
                " {mode_str}  {fname}{mod_str}  ─ {location}  ─ {tname} "
            )
        };

        let bar = Paragraph::new(Line::from(Span::styled(
            text.clone(),
            Style::new().fg(theme.status_fg).bg(theme.status_bg),
        )))
        .style(Style::new().bg(theme.status_bg));
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
}
