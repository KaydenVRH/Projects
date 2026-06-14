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
//!   - State::Shell  — pop‑up shell overlay (Ctrl+J)
//!
//! Multi‑buffer:
//!   Each open file or scratch buffer is stored as a [`Buffer`] in
//!   `Editor::buffers`.  The Editor's own `lines`, `filename`, … fields
//!   are shortcuts that are synced with `buffers[self.current]` via
//!   [`sync_to_buffer`]/[`sync_from_buffer`] whenever the user switches
//!   tabs (`:bn`/`:bp`).  This avoids touching every single field access
//!   in the codebase.

use std::{
    cmp::min,
    fs,
    process::Command as ProcCmd,
    time::{Instant, SystemTime},
};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::highlight;
use crate::shell::ShellProcess;
use crate::config::Config;
use crate::theme::{hsl_to_rgb, Theme, ThemeKind};
use crate::finder::Finder;
use crate::music::MusicPlayer;
use crate::filetree::FileTree;

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
    Visual,  // visual mode: selecting text with movement keys
    Search,  // typing a / search query on the status line
    FileTree,// file tree panel (Ctrl+F)
}

// ── Buffer ───────────────────────────────────────────────────────

/// Per‑buffer state that gets swapped when the user switches tabs.
#[derive(Debug, Clone)]
pub struct Buffer {
    lines: Vec<String>,
    cy: usize,
    cx: usize,
    top: usize,
    left: usize,
    filename: Option<String>,
    modified: bool,
    last_mtime: Option<SystemTime>,
    undo_stack: Vec<Vec<String>>,
    redo_stack: Vec<Vec<String>>,
    selection_anchor: Option<(usize, usize)>,
    clipboard_text: String,
}

impl Buffer {
    fn new(lines: Vec<String>, filename: Option<String>, last_mtime: Option<SystemTime>) -> Self {
        Buffer {
            lines,
            cy: 0, cx: 0, top: 0, left: 0,
            filename,
            modified: false,
            last_mtime,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            selection_anchor: None,
            clipboard_text: String::new(),
        }
    }
}

// ── Editor ───────────────────────────────────────────────────────

/// All editor state.
pub struct Editor {
    // ── buffer (shortcuts – kept in sync via sync_to/from_buffer) ──
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

    // ── search ──
    pub search_query: String,
    pub last_search: Option<String>,

    // ── fuzzy finder ──
    pub finder: Finder,

    // ── file tree (Ctrl+F) ──
    pub filetree: FileTree,
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

    // ── visual mode ──
    pub selection_anchor: Option<(usize, usize)>,

    // ── clipboard (for yy/dd/p/visual yank) ──
    pub clipboard_text: String,

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

    // ── rainbow mode (animated hue cycling from `:3`) ──
    pub rainbow: bool,
    pub rainbow_start: Instant,

    pub filetree_width: u16,
    pub music_dir: String,

    pub buffers: Vec<Buffer>,
    pub current: usize,
}

impl Editor {
    // ═══════════════════════════════════════════════════════════════
    //  Construction & Drop
    // ═══════════════════════════════════════════════════════════════

    /// Create a new editor, optionally loading a file from disk.
    ///
    /// If `filename` is `None`, start with a single empty line
    /// (like vim's empty buffer).
    pub fn new(filename: Option<&str>, cfg: &Config) -> Result<Self> {
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

        let theme = ThemeKind::from_str(&cfg.theme).unwrap_or(ThemeKind::Default);

        let initial_buf = Buffer::new(lines.clone(), filename.clone(), last_mtime);

        Ok(Self {
            buffers: vec![initial_buf],
            current: 0,
            lines,
            cy: 0,
            cx: 0,
            top: 0,
            left: 0,
            mode: Mode::Normal,
            state: State::Normal,
            cmd_buf: String::new(),
            search_query: String::new(),
            last_search: None,
            finder: Finder::new(),
            finder_query: String::new(),
            finder_selection: 0,
            filetree: FileTree::new(),
            run_output: String::new(),
            music_player: MusicPlayer::new(),
            filename,
            last_mtime,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            selection_anchor: None,
            clipboard_text: String::new(),
            theme,
            theme_selected: ThemeKind::all().iter().position(|t| *t == theme).unwrap_or(0),
            flash: None,
            cache_w: 80,
            cache_h: 24,
            shell: None,
            shell_reader: None,
            rainbow: false,
            rainbow_start: Instant::now(),
            filetree_width: cfg.filetree_width.max(10),
            music_dir: if cfg.music_dir.is_empty() {
                std::env::var("HOME")
                    .map(|h| format!("{h}/Music"))
                    .unwrap_or_else(|_| ".".to_string())
            } else {
                cfg.music_dir.clone()
            },
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

    // ── multi‑buffer helpers ──

    /// Copy the current Editor fields into `buffers[self.current]`.
    fn sync_to_buffer(&mut self) {
        if let Some(buf) = self.buffers.get_mut(self.current) {
            buf.lines = std::mem::take(&mut self.lines);
            buf.cy = self.cy;
            buf.cx = self.cx;
            buf.top = self.top;
            buf.left = self.left;
            buf.filename = self.filename.take();
            buf.modified = self.modified;
            buf.last_mtime = self.last_mtime.take();
            buf.undo_stack = std::mem::take(&mut self.undo_stack);
            buf.redo_stack = std::mem::take(&mut self.redo_stack);
            buf.selection_anchor = self.selection_anchor.take();
            buf.clipboard_text = std::mem::take(&mut self.clipboard_text);
        }
    }

    /// Copy `buffers[idx]` into the Editor fields.
    fn sync_from_buffer(&mut self, idx: usize) {
        if let Some(buf) = self.buffers.get(idx) {
            self.lines = buf.lines.clone();
            self.cy = buf.cy;
            self.cx = buf.cx;
            self.top = buf.top;
            self.left = buf.left;
            self.filename = buf.filename.clone();
            self.modified = buf.modified;
            self.last_mtime = buf.last_mtime;
            self.undo_stack = buf.undo_stack.clone();
            self.redo_stack = buf.redo_stack.clone();
            self.selection_anchor = buf.selection_anchor;
            self.clipboard_text = buf.clipboard_text.clone();
        }
    }

    /// Switch to a new buffer index after saving the current state.
    fn switch_buffer(&mut self, idx: usize) {
        if idx >= self.buffers.len() || idx == self.current {
            return;
        }
        self.sync_to_buffer();
        self.current = idx;
        self.sync_from_buffer(idx);
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
            self.music_player.scan(&self.music_dir);
            return true;
        }

        // Ctrl+E = theme selector (opens from any state).
        if key.code == KeyCode::Char('e') && key.modifiers == KeyModifiers::CONTROL {
            self.state = State::Theme;
            self.theme_selected = 0;
            return true;
        }

        // Ctrl+F = file tree (opens from any state).
        if key.code == KeyCode::Char('f') && key.modifiers == KeyModifiers::CONTROL {
            if self.state == State::FileTree {
                self.state = State::Normal;
            } else {
                self.filetree.refresh();
                self.state = State::FileTree;
            }
            return true;
        }

        // Ctrl+J = pop-up shell (opens from any state *except* shell).
        if key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::CONTROL {
            if self.state == State::Shell {
                // Already in the shell — forward Ctrl+J (0x0A, line feed)
                // so it works as a regular key inside the shell.
                if let Some(ref s) = self.shell {
                    s.force_raw_mode();
                    s.write(&[0x0a]);
                }
                return true;
            }
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
        // Visual is handled after global keybindings (below).
        match self.state {
            State::Command => return self.handle_cmd_state(key),
            State::Finder => return self.handle_finder_state(key),
            State::Run => return self.handle_run_state(key),
            State::Music => return self.handle_music_state(key),
            State::Theme => return self.handle_theme_state(key),
            State::Search => return self.handle_search_state(key),
            State::FileTree => return self.handle_filetree_state(key),
            State::Normal => {}
            State::Shell => {}
            State::Visual => {}
        }

        // Global keybindings that work in both normal and insert modes
        // (and also visual mode, via the fall-through above):
        // Ctrl+P = finder, Ctrl+R = run Python, Ctrl+C/D = quit.
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
                        self.run_output = self.run_file(fname);
                    } else {
                        self.run_output =
                            "No filename set.  Use :w <file> first.".to_string();
                    }
                    self.state = State::Run;
                    return true;
                }
                KeyCode::Char('c') | KeyCode::Char('d') => return false,
                _ => {}
            }
        }

        // Dispatch by visual state or editing mode.
        if self.state == State::Visual {
            return self.handle_visual_mode(key);
        }
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
                    self.cx = self.left_cx();
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
                    self.cx = self.right_cx();
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

            // ── enter visual mode ──
            KeyCode::Char('v') => {
                self.state = State::Visual;
                self.selection_anchor = Some((self.cy, self.cx));
            }

            // ── enter insert mode ──
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('a') => {
                if self.cx < self.lines[self.cy].len() {
                    self.cx = self.right_cx();
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
                    let pos = line.floor_char_boundary(self.cx);
                    let ch = line[pos..].chars().next().unwrap();
                    for _ in 0..ch.len_utf8() {
                        line.remove(pos);
                    }
                    self.modified = true;
                }
            }

            // ── delete line (dd) ──
            KeyCode::Char('d') => {
                self.save_undo();
                if !self.lines.is_empty() {
                    let removed = self.lines.remove(self.cy);
                    self.clipboard_text = removed;
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
                    self.clipboard_text = self.lines[self.cy].clone();
                }
            }

            // ── paste (p / P) ──
            KeyCode::Char('p') => {
                if !self.clipboard_text.is_empty() {
                    self.save_undo();
                    // If clipboard contains newlines, paste as lines below.
                    if self.clipboard_text.contains('\n') {
                        let pasted_lines: Vec<&str> = self.clipboard_text.split('\n').collect();
                        for (i, line) in pasted_lines.iter().enumerate() {
                            self.lines.insert(self.cy + 1 + i, line.to_string());
                        }
                        self.cy += pasted_lines.len();
                        self.cx = 0;
                    } else {
                        // Single-line: insert at cursor.
                        let line = &mut self.lines[self.cy];
                        line.insert_str(self.cx, &self.clipboard_text);
                        self.cx += self.clipboard_text.len();
                    }
                    self.modified = true;
                }
            }
            KeyCode::Char('P') => {
                if !self.clipboard_text.is_empty() {
                    self.save_undo();
                    if self.clipboard_text.contains('\n') {
                        let pasted_lines: Vec<&str> = self.clipboard_text.split('\n').collect();
                        for (i, line) in pasted_lines.iter().enumerate() {
                            self.lines.insert(self.cy + i, line.to_string());
                        }
                        self.cx = 0;
                    } else {
                        let line = &mut self.lines[self.cy];
                        line.insert_str(self.cx, &self.clipboard_text);
                        self.cx += self.clipboard_text.len();
                    }
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

            // ── enter search mode ──
            KeyCode::Char('/') => {
                self.state = State::Search;
                self.search_query.clear();
            }

            // ── repeat last search ──
            KeyCode::Char('n') => {
                if self.last_search.is_some() {
                    self.search_next();
                }
            }
            KeyCode::Char('N') => {
                if self.last_search.is_some() {
                    self.search_prev();
                }
            }

            // ── buffer switching ──
            KeyCode::Tab => {
                if self.buffers.len() > 1 {
                    let next = (self.current + 1) % self.buffers.len();
                    self.switch_buffer(next);
                }
            }
            KeyCode::BackTab => {
                if self.buffers.len() > 1 {
                    let prev = if self.current == 0 {
                        self.buffers.len() - 1
                    } else {
                        self.current - 1
                    };
                    self.switch_buffer(prev);
                }
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
                    let left = self.left_cx();
                    let char_len = self.cx - left;
                    let line = &mut self.lines[self.cy];
                    for _ in 0..char_len {
                        line.remove(left);
                    }
                    self.cx = left;
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
                    let pos = line.floor_char_boundary(self.cx);
                    let ch = line[pos..].chars().next().unwrap();
                    for _ in 0..ch.len_utf8() {
                        line.remove(pos);
                    }
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
                if self.cx > 0 { self.cx = self.left_cx(); }
            }
            KeyCode::Right => {
                if self.cx < self.lines[self.cy].len() { self.cx = self.right_cx(); }
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

            // Ctrl+A: paste clipboard text.
            KeyCode::Char('a') if key.modifiers == KeyModifiers::CONTROL => {
                if !self.clipboard_text.is_empty() {
                    let line = &mut self.lines[self.cy];
                    line.insert_str(self.cx, &self.clipboard_text);
                    self.cx += self.clipboard_text.len();
                    self.modified = true;
                }
            }

            // Regular character: insert at cursor.
            KeyCode::Char(ch) => {
                let line = &mut self.lines[self.cy];
                let pos = line.floor_char_boundary(self.cx);
                line.insert(pos, ch);
                self.cx = pos + ch.len_utf8();
                self.modified = true;
            }

            _ => {}
        }

        self.clamp_scroll();
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Visual-mode key handling
    // ═══════════════════════════════════════════════════════════════

    fn handle_visual_mode(&mut self, key: KeyEvent) -> bool {
        // Ctrl+C/D: quit.
        if key.modifiers == KeyModifiers::CONTROL
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
        {
            return false;
        }

        match key.code {
            // Esc or v: exit visual mode.
            KeyCode::Esc => {
                self.state = State::Normal;
                self.selection_anchor = None;
            }
            KeyCode::Char('v') => {
                self.state = State::Normal;
                self.selection_anchor = None;
            }

            // Cursor motion extends (or shrinks) the selection.
            KeyCode::Char('h') | KeyCode::Left => {
                if self.cx > 0 {
                    self.cx = self.left_cx();
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
                    self.cx = self.right_cx();
                }
            }
            KeyCode::Char('0') | KeyCode::Home => self.cx = 0,
            KeyCode::Char('$') | KeyCode::End => {
                self.cx = self.lines[self.cy].len();
            }
            KeyCode::Char('w') => {
                self.cx = self.next_word_pos(self.cy, self.cx);
            }
            KeyCode::Char('b') => {
                self.cx = self.prev_word_pos(self.cy, self.cx);
            }

            // Yank selection (y or c both copy).
            KeyCode::Char('y') | KeyCode::Char('c') => {
                self.clipboard_text = self.selection_text();
                self.state = State::Normal;
                self.selection_anchor = None;
            }

            // Delete selection.
            KeyCode::Char('d') => {
                if self.selection_anchor.is_some() {
                    self.save_undo();
                    self.clipboard_text = self.selection_text();
                    let (start_y, end_y, start_x, end_x) = self.selection_bounds();
                    if start_y == end_y {
                        let line = &mut self.lines[start_y];
                        let to_remove = end_x.saturating_sub(start_x);
                        for _ in 0..to_remove {
                            line.remove(start_x);
                        }
                    } else {
                        // First line: remove from start_x to end.
                        {
                            let line = &mut self.lines[start_y];
                            line.truncate(start_x);
                        }
                        // Middle lines: remove.
                        for _ in (start_y + 1)..end_y {
                            self.lines.remove(start_y + 1);
                        }
                        // Last line: remove from 0 to end_x.
                        if end_y > start_y {
                            let last = &mut self.lines[start_y + 1];
                            let remaining = last.split_off(end_x);
                            self.lines[start_y].push_str(&remaining);
                            self.lines.remove(start_y + 1);
                        }
                    }
                    self.cy = start_y;
                    self.cx = start_x;
                    self.modified = true;
                }
                self.state = State::Normal;
                self.selection_anchor = None;
            }

            // Enter insert mode (exits visual mode like vim).
            KeyCode::Char('i') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.mode = Mode::Insert;
            }
            KeyCode::Char('a') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                if self.cx < self.lines[self.cy].len() {
                    self.cx = self.right_cx();
                }
                self.mode = Mode::Insert;
            }
            KeyCode::Char('I') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.cx = 0;
                self.mode = Mode::Insert;
            }
            KeyCode::Char('A') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.cx = self.lines[self.cy].len();
                self.mode = Mode::Insert;
            }

            _ => {}
        }

        self.clamp_scroll();
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Search-state key handling (/)
    // ═══════════════════════════════════════════════════════════════

    fn handle_search_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc: cancel search.
            KeyCode::Esc => {
                self.state = State::Normal;
                self.search_query.clear();
            }
            // Enter: execute search.
            KeyCode::Enter => {
                self.last_search = Some(self.search_query.clone());
                self.search_query.clear();
                self.state = State::Normal;
                self.search_next();
            }
            // Backspace: remove last char.
            KeyCode::Backspace => {
                self.search_query.pop();
            }
            // Regular char: append to query.
            KeyCode::Char(ch) => {
                self.search_query.push(ch);
            }
            _ => {}
        }
        true
    }

    /// Jump to the next occurrence of the last search query.
    fn search_next(&mut self) {
        let query = match &self.last_search {
            Some(q) => q.clone(),
            None => return,
        };
        if query.is_empty() {
            return;
        }
        if let Some((y, x)) = self.find_next(self.cy, self.cx + 1, &query) {
            self.cy = y;
            self.cx = x;
        } else {
            self.flash = Some("search: no more matches".to_string());
        }
    }

    /// Jump to the previous occurrence of the last search query.
    fn search_prev(&mut self) {
        let query = match &self.last_search {
            Some(q) => q.clone(),
            None => return,
        };
        if query.is_empty() {
            return;
        }
        // Search backward from just before the cursor.
        let from_x = self.cx.saturating_sub(1);
        if let Some((y, x)) = self.find_prev(self.cy, from_x, &query) {
            self.cy = y;
            self.cx = x;
        } else {
            self.flash = Some("search: no more matches".to_string());
        }
    }

    /// Find the first occurrence of `query` at or after (from_y, from_x),
    /// wrapping around to the top of the buffer if necessary.
    fn find_next(&self, from_y: usize, from_x: usize, query: &str) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        // Forward from current position to end.
        for y in from_y..self.lines.len() {
            let line = &self.lines[y];
            let start = if y == from_y { from_x.min(line.len()) } else { 0 };
            if let Some(x) = line[start..].find(query) {
                return Some((y, start + x));
            }
        }
        // Wrap: search from beginning to (from_y, from_x).
        for y in 0..=from_y {
            let line = &self.lines[y];
            let limit = if y == from_y { from_x.min(line.len()) } else { line.len() };
            if limit == 0 { continue; }
            if let Some(x) = line[..limit].find(query) {
                return Some((y, x));
            }
        }
        None
    }

    /// Find the last occurrence of `query` before (from_y, from_x),
    /// wrapping around to the bottom of the buffer if necessary.
    fn find_prev(&self, from_y: usize, from_x: usize, query: &str) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        // Backward from current position to beginning.
        for y in (0..=from_y).rev() {
            let line = &self.lines[y];
            let limit = if y == from_y { from_x.min(line.len()) } else { line.len() };
            if limit == 0 { continue; }
            if let Some(x) = line[..limit].rfind(query) {
                return Some((y, x));
            }
        }
        // Wrap: search from end backward to (from_y, from_x).
        for y in (from_y + 1..self.lines.len()).rev() {
            let line = &self.lines[y];
            if line.is_empty() { continue; }
            if let Some(x) = line.rfind(query) {
                return Some((y, x));
            }
        }
        None
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
                        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                        let fname = full_path.to_string_lossy().to_string();
                        let mtime = fs::metadata(&fname).ok().and_then(|m| m.modified().ok());
                        self.sync_to_buffer();
                        self.buffers.push(Buffer::new(lines, Some(fname.to_string()), mtime));
                        self.current = self.buffers.len() - 1;
                        self.sync_from_buffer(self.current);
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

    // ── file-tree state (Ctrl+F) ───────────────────────────────

    fn handle_filetree_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
                self.state = State::Normal;
            }
            KeyCode::Enter => {
                let entry = self.filetree.selected_entry().cloned();
                if let Some(e) = entry {
                    if e.is_dir {
                        self.filetree.toggle_expand();
                    } else {
                        // Open file in new buffer
                        if let Ok(content) = fs::read_to_string(&e.path) {
                            let lines: Vec<String> =
                                content.lines().map(|l| l.to_string()).collect();
                            let mtime =
                                fs::metadata(&e.path).ok().and_then(|m| m.modified().ok());
                            self.sync_to_buffer();
                            self.buffers.push(Buffer::new(
                                lines,
                                Some(e.path.clone()),
                                mtime,
                            ));
                            self.current = self.buffers.len() - 1;
                            self.sync_from_buffer(self.current);
                        } else {
                            self.flash = Some(format!("can't read {}", e.path));
                        }
                        self.state = State::Normal;
                    }
                }
            }
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                self.filetree.selected = self.filetree.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.filetree.entries.len().saturating_sub(1);
                self.filetree.selected = (self.filetree.selected + 1).min(max);
            }
            KeyCode::PageUp => {
                self.filetree.selected = self.filetree.selected.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = self.filetree.entries.len().saturating_sub(1);
                self.filetree.selected = (self.filetree.selected + 10).min(max);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let entry = self.filetree.selected_entry().cloned();
                if let Some(e) = entry {
                    if e.is_dir && e.expanded {
                        self.filetree.toggle_expand();
                    }
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let entry = self.filetree.selected_entry().cloned();
                if let Some(e) = entry {
                    if e.is_dir && !e.expanded {
                        self.filetree.toggle_expand();
                    }
                }
            }
            // Ctrl+C/D: quit
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
            // :q — close current buffer (quit if last)
            "q" => {
                if self.modified {
                    self.flash = Some("No write since last change (add ! to override)".to_string());
                    return true;
                }
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() {
                    return false;
                }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
            // :q! — force close (quit if last)
            "q!" => {
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() {
                    return false;
                }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
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
            // :wq — save and close buffer (quit if last)
            "wq" => {
                let fname = self.filename.clone().unwrap_or_default();
                if !fname.is_empty() && self.save_to_disk(&fname).is_err() {
                    self.flash = Some("Error saving".to_string());
                    return true;
                }
                if fname.is_empty() {
                    self.flash = Some("No filename".to_string());
                    return true;
                }
                self.modified = false;
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() {
                    return false;
                }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
            // :bn — next buffer
            "bn" | "bnext" => {
                if self.buffers.len() > 1 {
                    let next = (self.current + 1) % self.buffers.len();
                    self.switch_buffer(next);
                } else {
                    self.flash = Some("Only one buffer open".to_string());
                }
            }
            // :bp — previous buffer
            "bp" | "bprev" => {
                if self.buffers.len() > 1 {
                    let prev = if self.current == 0 {
                        self.buffers.len() - 1
                    } else {
                        self.current - 1
                    };
                    self.switch_buffer(prev);
                } else {
                    self.flash = Some("Only one buffer open".to_string());
                }
            }
            // :3 — toggle rainbow mode
            "3" => {
                self.rainbow = !self.rainbow;
                self.rainbow_start = Instant::now();
                self.flash = Some(if self.rainbow {
                    "🌈 rainbow on".to_string()
                } else {
                    "rainbow off".to_string()
                });
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

    /// Run a file and capture its stdout + stderr.
    /// Python files (`.py`) run via `python3`, C files (`.c`/`.h`)
    /// compile with `cc` then run the resulting binary.
    fn run_file(&self, path: &str) -> String {
        let ext = path.rsplit('.').next().unwrap_or("");
        match ext {
            "c" | "h" => self.run_c(path),
            "rs" => self.run_rust(path),
            "py" => run_python(path),
            _ => format!("Don't know how to run .{ext} files.  Use .py, .rs, .c, or .h."),
        }
    }

    /// Compile and run a C source file.
    fn run_c(&self, path: &str) -> String {
        let bin = "/tmp/ked_c_bin";

        // Compile
        let comp = ProcCmd::new("cc")
            .args(["-o", bin, "-Wall", "-Wextra", path])
            .output();
        let comp_out = match comp {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => return format!("Error running cc: {e}"),
        };

        // Check if compilation succeeded (binary exists).
        if !std::path::Path::new(bin).exists() {
            let mut result = String::from("── Compilation failed ──\n");
            result.push_str(&comp_out);
            return result;
        }

        // Run
        let run = ProcCmd::new(bin).output();
        let run_out = match run {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => format!("Error running binary: {e}"),
        };

        let mut result = String::new();
        if !comp_out.is_empty() {
            result.push_str("── Compilation ──\n");
            result.push_str(&comp_out);
            result.push('\n');
        }
        result.push_str("── Output ──\n");
        result.push_str(&run_out);
        result
    }

    /// Compile and run a Rust source file.
    fn run_rust(&self, path: &str) -> String {
        let bin = "/tmp/ked_rust_bin";

        let comp = ProcCmd::new("rustup")
            .args(["run", "stable", "rustc", "-o", bin, path])
            .output();
        let comp_out = match comp {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => return format!("Error running rustc: {e}"),
        };

        if !std::path::Path::new(bin).exists() {
            let mut result = String::from("── Compilation failed ──\n");
            result.push_str(&comp_out);
            return result;
        }

        let run = ProcCmd::new(bin).output();
        let run_out = match run {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => format!("Error running binary: {e}"),
        };

        let mut result = String::new();
        if !comp_out.is_empty() {
            result.push_str("── Compilation ──\n");
            result.push_str(&comp_out);
            result.push('\n');
        }
        result.push_str("── Output ──\n");
        result.push_str(&run_out);
        result
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

    /// Advance `cx` one full UTF‑8 character to the right (or to end of line).
    fn right_cx(&self) -> usize {
        let line = &self.lines[self.cy];
        if self.cx >= line.len() { return self.cx; }
        let pos = if line.is_char_boundary(self.cx) { self.cx } else { line.floor_char_boundary(self.cx) };
        let ch = line[pos..].chars().next().unwrap();
        (pos + ch.len_utf8()).min(line.len())
    }

    /// Retreat `cx` one full UTF‑8 character to the left (or to column 0).
    fn left_cx(&self) -> usize {
        if self.cx == 0 { return 0; }
        let line = &self.lines[self.cy];
        let pos = if line.is_char_boundary(self.cx) { self.cx } else { line.floor_char_boundary(self.cx) };
        if let Some(ch) = line[..pos].chars().next_back() {
            pos - ch.len_utf8()
        } else {
            0
        }
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

    // ── visual mode helpers ─────────────────────────────────────

    /// Return the text within the current selection.
    fn selection_text(&self) -> String {
        let Some((ay, ax)) = self.selection_anchor else {
            return String::new();
        };
        let (start_y, end_y, start_x, end_x) = self.order_bounds(ay, ax, self.cy, self.cx);
        if start_y == end_y {
            self.lines[start_y][start_x..end_x].to_string()
        } else {
            let mut parts = Vec::new();
            // First line: from start_x to end.
            parts.push(self.lines[start_y][start_x..].to_string());
            // Middle lines: entire lines.
            for y in (start_y + 1)..end_y {
                parts.push(self.lines[y].clone());
            }
            // Last line: from 0 to end_x.
            parts.push(self.lines[end_y][..end_x].to_string());
            parts.join("\n")
        }
    }

    /// Return the ordered bounds (start_y, end_y, start_x, end_x)
    /// of the current selection.
    fn selection_bounds(&self) -> (usize, usize, usize, usize) {
        let (ay, ax) = self.selection_anchor.unwrap_or((self.cy, self.cx));
        self.order_bounds(ay, ax, self.cy, self.cx)
    }

    /// Order two cursor positions into (start_y, end_y, start_x, end_x).
    fn order_bounds(
        &self,
        ay: usize, ax: usize,
        by: usize, bx: usize,
    ) -> (usize, usize, usize, usize) {
        if ay < by || (ay == by && ax <= bx) {
            (ay, by, ax, bx)
        } else {
            (by, ay, bx, ax)
        }
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

        // Split the screen: buffer bar (top) + main content + status bar (bottom).
        let [bufbar_area, content_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        // When the file tree is open, split content area horizontally.
        let (editor_area, tree_area) = if self.state == State::FileTree {
            let [t, e] = Layout::horizontal([
                Constraint::Length(self.filetree_width),
                Constraint::Min(1),
            ])
            .areas(content_area);
            (e, Some(t))
        } else {
            (content_area, None)
        };

        // ── 1. main content / splash ────────────────────────────
        let is_default = self.theme == ThemeKind::Default;
        let mut theme = self.theme.theme();
        if self.rainbow {
            let elapsed = self.rainbow_start.elapsed();
            let hue = (elapsed.as_millis() as f64 * 0.04) % 360.0;
            theme = theme.with_rainbow(hue);
            if is_default {
                // Cycle the status bar too (Default uses named colours
                // like DarkGray which rotate_hue skips).
                let (r, g, b) = hsl_to_rgb(hue, 0.5, 0.3);
                theme.status_bg = Color::Rgb(r, g, b);
                let (r, g, b) = hsl_to_rgb(hue, 0.8, 0.9);
                theme.status_fg = Color::Rgb(r, g, b);
            }
        }
        let is_splash = self.filename.is_none()
            && self.lines.len() == 1
            && self.lines[0].is_empty();
        let is_visual = self.state == State::Visual;
        let sel_bounds = if is_visual {
            self.selection_anchor.filter(|&(ay, ax)| ay != self.cy || ax != self.cx)
                .map(|_| self.selection_bounds())
        } else {
            None
        };
        if is_splash {
            self.render_splash(f, editor_area, &theme);
        } else {
        let gutter = self.gutter_width() + 1;
        let mut lines_vec: Vec<Line> = Vec::new();
        let visible_lines = editor_area.height as usize;

        for row in 0..visible_lines {
            let buf_row = self.top + row;
            let style = Style::new().fg(theme.fg).bg(theme.bg);
            let line_style = Style::new().bg(theme.bg);

            if buf_row >= self.lines.len() {
                // Past EOF: show "~" filler lines (like vim).
                let tilde = Span::styled("~", theme.tilde);
                lines_vec.push(
                    Line::from(vec![
                        Span::raw(" ".repeat(gutter + 1)),
                        tilde,
                    ]).style(line_style),
                );
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
                        "c" | "h" => Some(highlight::Lang::C),
                        "md" | "markdown" => Some(highlight::Lang::Md),
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

            // Build the line: gutter + content, with search-match
            // highlighting if there's an active query.
            let mut spans = vec![num_span];
            if let Some(ref query) = self.last_search {
                if !query.is_empty() && raw.contains(query.as_str()) {
                    let mut content_spans = Vec::new();
                    for span in highlighted {
                        let text = span.content.to_string();
                        let mut last = 0;
                        for (start, m) in text.match_indices(query.as_str()) {
                            if start > last {
                                content_spans.push(Span::styled(
                                    text[last..start].to_string(),
                                    span.style,
                                ));
                            }
                            let match_style = Style::default()
                                .bg(theme.selection_bg)
                                .patch(span.style);
                            content_spans.push(Span::styled(m.to_string(), match_style));
                            last = start + m.len();
                        }
                        if last < text.len() {
                            content_spans.push(Span::styled(
                                text[last..].to_string(),
                                span.style,
                            ));
                        }
                    }
                    spans.extend(content_spans);
                } else {
                    spans.extend(highlighted);
                }
            } else {
                spans.extend(highlighted);
            }

            // Visual mode selection highlighting (character-granular).
            if let Some((sy, ey, sx, ex)) = sel_bounds {
                if buf_row >= sy && buf_row <= ey {
                    let fully = buf_row > sy && buf_row < ey;
                    let sel_start = if buf_row == sy { sx } else { 0 };
                    let sel_end = if buf_row == ey { ex } else { raw.len() };
                    if fully || (sel_start <= 0 && sel_end >= raw.len()) {
                        // Entire content row selected — paint every span.
                        for span in spans.iter_mut().skip(1) {
                            span.style = span.style.patch(
                                Style::new().bg(theme.selection_bg));
                        }
                    } else {
                        // Partially selected row — split at selection edges.
                        let mut rebuilt = vec![spans[0].clone()];
                        let mut offset = 0usize;
                        for span in spans.drain(1..) {
                            let text = span.content.to_string();
                            let span_end = offset + text.len();
                            if span_end <= sel_start || offset >= sel_end {
                                rebuilt.push(span);
                            } else if offset >= sel_start && span_end <= sel_end {
                                let mut s = span;
                                s.style = s.style.patch(
                                    Style::new().bg(theme.selection_bg));
                                rebuilt.push(s);
                            } else {
                                // Span straddles a selection edge — split.
                                let s = span;
                                // part before selection
                                if sel_start > offset {
                                    let end = sel_start - offset;
                                    rebuilt.push(Span::styled(
                                        text[..end].to_string(), s.style));
                                }
                                // selected part
                                let m_start = offset.max(sel_start) - offset;
                                let m_end = span_end.min(sel_end) - offset;
                                if m_start < m_end {
                                    rebuilt.push(Span::styled(
                                        text[m_start..m_end].to_string(),
                                        s.style.patch(Style::new().bg(theme.selection_bg))));
                                }
                                // part after selection
                                if sel_end < span_end {
                                    rebuilt.push(Span::styled(
                                        text[(sel_end - offset)..].to_string(), s.style));
                                }
                            }
                            offset = span_end;
                        }
                        spans = rebuilt;
                    }
                }
            }

            lines_vec.push(Line::from(spans).style(line_style));
        }

        // Wrap in Paragraph and render.  We explicitly set the width
        // so long lines don't wrap (we handle horiz scroll ourselves).
        let content = Text::from(lines_vec);
        // Clear the entire content area first so shell-overlay artifacts
        // (text that rendered past the line-number gutter) don't ghost.
        f.render_widget(ratatui::widgets::Clear, editor_area);
        let paragraph = Paragraph::new(content)
            .style(Style::new().bg(theme.bg));
        f.render_widget(paragraph, editor_area);
        } // end else (normal content)

        // ── 2. buffer bar ──────────────────────────────────────────
        self.render_buffer_bar(f, bufbar_area, &theme);

        // ── 3. status bar ────────────────────────────────────────
        self.render_status(f, status_area, width, &theme);

        // ── 4. cursor position ───────────────────────────────────
        if is_splash {
            f.set_cursor_position(Position::new(0, 0));
        } else if let Some(gutter) = self.gutter_width().checked_add(1) {
            let cy_screen = (self.cy.saturating_sub(self.top)) as u16;
            let cx_screen = (self.cx.saturating_sub(self.left)) as u16 + gutter as u16;
            if cy_screen < editor_area.height {
                let cursor_x = editor_area.x + cx_screen;
                let cursor_y = editor_area.y + cy_screen;
                f.set_cursor_position(Position::new(cursor_x, cursor_y));
            }
        }

        // ── 4. overlays ──────────────────────────────────────────
        match self.state {
            State::Command if !self.cmd_buf.is_empty() => {
                // Command is shown in the status bar already;
                // no separate overlay needed.
            }
            State::Finder => self.render_finder(f, area, &theme),
            State::Music => self.render_music(f, area, &theme),
            State::Theme => self.render_theme(f, area, &theme),
            State::Run => self.render_run(f, area, &theme),
            State::Shell => self.render_shell(f, area),
            State::FileTree => {
                if let Some(tree_area) = tree_area {
                    self.render_filetree(f, tree_area, &theme);
                }
            }
            _ => {}
        }
    }

    // ── status bar ───────────────────────────────────────────────

    /// Render a one‑line buffer (tab) bar at the top of the screen.
    fn render_buffer_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<Span> = self
            .buffers
            .iter()
            .enumerate()
            .flat_map(|(i, buf)| {
                let is_current = i == self.current;
                let name = buf
                    .filename
                    .as_deref()
                    .map(|p| p.rsplit('/').next().unwrap_or(p))
                    .unwrap_or("[No Name]");
                let mod_flag = if buf.modified { " +" } else { "" };
                let label = format!(" {name}{mod_flag} ");
                let style = if is_current {
                    Style::new().fg(theme.status_fg).bg(theme.status_bg)
                } else {
                    Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)).bg(theme.bg)
                };
                vec![Span::styled(label, style), Span::raw("│")]
            })
            .collect::<Vec<_>>();

        // Remove the trailing "│".
        let last_idx = items.len().saturating_sub(1);
        let spans: Vec<Span> = items.into_iter().take(last_idx).collect();

        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::new().bg(theme.bg)),
            area,
        );
    }

    /// Render the status bar at the bottom of the screen.
    fn render_status(&self, f: &mut Frame, area: Rect, _width: usize, theme: &Theme) {

        // Build the status text depending on state.
        let text: String = if let Some(ref msg) = self.flash {
            format!(" {msg} ")
        } else if self.state == State::Command {
            // Show the command being typed.
            format!(":{}", self.cmd_buf)
        } else if self.state == State::Search {
            // Show the search query being typed.
            format!("/{}", self.search_query)
        } else {
            // Normal status:  mode | filename | modified | line:col | theme
            let mode_str = match self.state {
                State::Visual => "VISUAL",
                _ => match self.mode {
                    Mode::Normal => "NORMAL",
                    Mode::Insert => "INSERT",
                },
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

    fn render_finder(&self, f: &mut Frame, area: Rect, theme: &Theme) {

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

    fn render_music(&self, f: &mut Frame, area: Rect, theme: &Theme) {

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

    fn render_theme(&self, f: &mut Frame, area: Rect, theme: &Theme) {

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

    fn render_shell(&mut self, f: &mut Frame, area: Rect) {
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
        let max_rows = inner.height as usize;

        let output_lines = self.shell.as_mut().map(|s| {
            let default_style = Style::new().fg(theme.fg).bg(theme.bg);
            let lines: &[Line] = s.output_styled(default_style);
            let start = lines.len().saturating_sub(max_rows);
            lines[start..].to_vec()
        });
        let mut styled_lines = output_lines.unwrap_or_default();
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

    fn render_splash(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        f.render_widget(Clear, area);

        let figlet = [
            " _            _ ",
            "| |          | |",
            "| | _____  __| |",
            "| |/ / _ \\/ _` |",
            "|   <  __/ (_| |",
            "|_|\\_\\___|\\__,_|",
        ];

        let keybinds = [
            ("Ctrl+P", "find file"),
            ("Ctrl+F", "file tree"),
            ("Ctrl+J", "shell"),
            ("Ctrl+R", "run python"),
            ("Ctrl+T", "music"),
            ("Ctrl+E", "theme"),
            ("/",      "search"),
            ("Tab",    "switch buf"),
            ("Ctrl+S", "save"),
            (":wq",    "save & quit"),
            (":q!",    "quit"),
        ];

        // Compute column widths for alignment.
        let key_w = keybinds.iter().map(|(k, _)| k.len()).max().unwrap_or(6);

        let mut lines: Vec<Line> = Vec::new();

        let total_h = figlet.len() + 3 + (keybinds.len() + 1) / 2;
        let pad_top = (area.height as usize).saturating_sub(total_h) / 2;
        for _ in 0..pad_top {
            lines.push(Line::from(""));
        }

        for line in &figlet {
            lines.push(Line::from(Span::styled(*line, theme.keyword)));
        }

        lines.push(Line::from(""));

        // Subtitle
        lines.push(Line::from(Span::styled(
            "kayden's editor",
            Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)),
        )));

        lines.push(Line::from(""));

        for chunk in keybinds.chunks(2) {
            let mut spans = Vec::new();
            for (i, (key, desc)) in chunk.iter().enumerate() {
                if i > 0 {
                    // Gap between left cell and right cell.
                    spans.push(Span::raw(" ".repeat(3)));
                }
                spans.push(Span::styled(*key, theme.builtin));
                let pad = key_w + 2 - key.len();
                spans.push(Span::raw(" ".repeat(pad)));
                spans.push(Span::styled(*desc, Style::new().fg(theme.fg)));
            }
            lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .alignment(Alignment::Center)
            .style(Style::new().bg(theme.bg));
        f.render_widget(paragraph, area);
    }

    // ── file tree panel ─────────────────────────────────────────

    fn render_filetree(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::RIGHT)
            .title(" Files ")
            .title_style(Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)))
            .border_style(Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if inner.height < 1 {
            return;
        }

        let max_idx = self.filetree.entries.len().saturating_sub(1);
        let selected = self.filetree.selected.min(max_idx);

        // Auto-scroll: keep selected in view.
        self.filetree.scroll = if selected < self.filetree.scroll {
            selected
        } else if selected >= self.filetree.scroll + inner.height as usize {
            selected.saturating_sub(inner.height as usize - 1)
        } else {
            self.filetree.scroll
        };
        let scroll = self.filetree.scroll;

        let visible: Vec<ListItem> = self.filetree.entries
            .iter()
            .enumerate()
            .skip(scroll)
            .take(inner.height as usize)
            .map(|(i, entry)| {
                let prefix = if entry.is_dir {
                    if entry.expanded { "▾ " } else { "▸ " }
                } else {
                    "  "
                };
                let indent = "  ".repeat(entry.depth);
                let label = format!("{indent}{prefix}{}", entry.name);
                let style = if i == selected {
                    Style::new().fg(theme.status_bg).bg(theme.status_fg)
                } else if entry.is_dir {
                    Style::new().fg(theme.function.fg.unwrap_or(theme.fg))
                } else {
                    Style::new().fg(theme.fg)
                };
                ListItem::new(Line::from(Span::styled(label, style)))
            })
            .collect();

        f.render_widget(
            List::new(visible).style(Style::new().bg(theme.bg)),
            inner,
        );
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

/// Run a Python file and capture its stdout + stderr.
fn run_python(path: &str) -> String {
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
