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
//!   - State::Run    — run output overlay (Ctrl+E)
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
use crossterm::cursor::SetCursorStyle;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::highlight;
use crate::shell::ShellProcess;
use crate::config::Config;
use crate::theme::{Theme, ThemeKind};
use crate::finder::Finder;
use crate::music::MusicPlayer;
use crate::filetree::{FileTree, file_icon as ft_icon};

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
    Run,     // run output displayed (Ctrl+E)
    Music,   // music player picker (Ctrl+M)
    Theme,   // theme selector (Ctrl+T)
    Shell,   // pop-up shell (Ctrl+J) — handled in main.rs
    Visual,  // visual mode: selecting text with movement keys
    Search,  // typing a / search query on the status line
    FileTree,// file tree panel (Ctrl+F)
    SysInfo, // system dashboard (:sys)
    Help,    // keybind manual (Ctrl+K)
}

// ── Buffer ───────────────────────────────────────────────────────

/// A full-buffer undo/redo snapshot: the text plus the cursor
/// position at the moment the snapshot was taken.
#[derive(Debug, Clone)]
pub struct Snapshot {
    lines: Vec<String>,
    cy: usize,
    cx: usize,
}

impl Snapshot {
    fn of(lines: Vec<String>, cy: usize, cx: usize) -> Self {
        Snapshot { lines, cy, cx }
    }
}

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
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    /// Has the undo snapshot for the current insert session been
    /// taken?  (We snapshot once per insert session, not per key.)
    insert_saved: bool,
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
            insert_saved: false,
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

    // ── syntax highlighter (tree-sitter, re-parsed on edits) ──
    pub highlight: Option<highlight::Highlighter>,

    // ── fuzzy finder ──
    pub finder: Finder,
    pub finder_query: String,
    pub finder_selection: usize,
    pub finder_scroll: usize,

    // ── file tree (Ctrl+F) ──
    pub filetree: FileTree,

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
    pub undo_stack: Vec<Snapshot>,
    pub redo_stack: Vec<Snapshot>,
    /// Has the undo snapshot for the current insert session been taken?
    pub insert_saved: bool,

    // ── visual mode ──
    pub selection_anchor: Option<(usize, usize)>,

    // ── clipboard (for yy/dd/p/visual yank) ──
    pub clipboard_text: String,

    // ── theme ──
    pub theme: ThemeKind,
    /// Currently-highlighted row in the theme selector.
    pub theme_selected: usize,
    /// Scroll offset for the theme selector list.
    pub theme_scroll: usize,

    // ── flash message (shown in status bar, cleared next render) ──
    pub flash: Option<String>,

    // ── cached terminal size (updated every render) ──
    pub cache_w: u16,
    pub cache_h: u16,

    // ── pop-up shell (Ctrl+J) ──
    pub shell: Option<ShellProcess>,
    pub shell_reader: Option<std::thread::JoinHandle<()>>,

    // ── colour fx mode (animated theme) ──
    pub fx_mode: u8,  // 0=off, 1=gentle, 2=breathing, 3=warm
    pub fx_start: Instant,

    pub filetree_width: u16,
    pub music_dir: String,
    pub transparent: bool,
    pub status_bar_top: bool,
    pub animations: bool,
    pub bar_stats: bool,
    pub alt_scroll: usize,
    pub opacity: f32,
    pub _frame: u64,
    pub _mode_switched: u64,
    pub _last_mode: Mode,
    /// Cursor style to apply after rendering (set by render, consumed by main).
    pub cursor_style: SetCursorStyle,

    /// Cached system stats for the buffer bar (refreshed periodically)
    pub sys_updated: Instant,
    pub sys_cpu: f64,
    pub sys_mem_used: u64,
    pub sys_mem_total: u64,
    pub sys_batt_pct: Option<u8>,
    pub sys_batt_status: Option<String>,
    /// Background thread for non-blocking stat collection
    pub sys_task: Option<std::thread::JoinHandle<(f64, u64, u64, Option<u8>, Option<String>)>>,

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

        let theme = ThemeKind::from_str(&cfg.theme).unwrap_or(ThemeKind::Oxocarbon);

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
            highlight: None,
            finder: Finder::new(),
            finder_query: String::new(),
            finder_selection: 0,
            finder_scroll: 0,
            filetree: FileTree::new(),
            run_output: String::new(),
            music_player: MusicPlayer::new(),
            filename,
            last_mtime,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            insert_saved: false,
            selection_anchor: None,
            clipboard_text: String::new(),
            theme,
            theme_selected: ThemeKind::all().iter().position(|t| *t == theme).unwrap_or(0),
            theme_scroll: 0,
            flash: None,
            cache_w: 80,
            cache_h: 24,
            shell: None,
            shell_reader: None,
            fx_mode: cfg.fx_mode.min(3),
            fx_start: Instant::now(),
            filetree_width: cfg.filetree_width.max(10),
            music_dir: if cfg.music_dir.is_empty() {
                std::env::var("HOME")
                    .map(|h| format!("{h}/Music"))
                    .unwrap_or_else(|_| ".".to_string())
            } else {
                cfg.music_dir.clone()
            },
            transparent: cfg.transparent,
            status_bar_top: cfg.status_bar_top,
            animations: cfg.animations,
            bar_stats: cfg.bar_stats,
            alt_scroll: cfg.alt_scroll.max(1),
            opacity: cfg.opacity.max(0.0).min(1.0),
            _frame: 0,
            _mode_switched: 0,
            _last_mode: Mode::Normal,
            cursor_style: SetCursorStyle::SteadyBlock,
            sys_updated: Instant::now(),
            sys_cpu: 0.0,
            sys_mem_used: 0,
            sys_mem_total: 0,
            sys_batt_pct: None,
            sys_batt_status: None,
            sys_task: None,
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
            buf.insert_saved = self.insert_saved;
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
            self.insert_saved = buf.insert_saved;
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

        // Ctrl+M = music player (opens from any state).  This only
        // works on terminals with kitty keyboard protocol support
        // (kitty, foot, wezterm, iTerm2, ghostty, …) — we push the
        // enhancement flags in main.rs so crossterm can tell Ctrl+M
        // apart from Enter.  Check BEFORE overlay dispatch so it
        // works everywhere.
        if key.code == KeyCode::Char('m') && key.modifiers == KeyModifiers::CONTROL {
            self.state = State::Music;
            self.music_player.selected = 0;
            self.music_player.scan(&self.music_dir);
            return true;
        }

        // Ctrl+T = theme selector (opens from any state).
        if key.code == KeyCode::Char('t') && key.modifiers == KeyModifiers::CONTROL {
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
            State::SysInfo => return self.handle_sysinfo_state(key),
            State::Help => return self.handle_help_state(key),
            State::Normal => {}
            State::Shell => {}
            State::Visual => {}
        }

        // Global keybindings that work in both normal and insert modes
        // (and also visual mode, via the fall-through above):
        // Ctrl+P = finder, Ctrl+E = run, Ctrl+S = save, Ctrl+C = quit.
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
                KeyCode::Char('e') => {
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
                KeyCode::Char('s') => {
                    if let Some(ref fname) = self.filename.clone() {
                        match self.save_to_disk(fname) {
                            Ok(()) => {
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
                    return true;
                }
                // Ctrl+C: exit visual mode → normal, exit insert mode
                // → normal (like vim), quit everywhere else.
                KeyCode::Char('c') => {
                    if self.state == State::Visual {
                        self.state = State::Normal;
                        self.selection_anchor = None;
                        return true;
                    }
                    if self.mode == Mode::Insert {
                        self.mode = Mode::Normal;
                        self.insert_saved = false;
                        if self.cx > 0 && self.cx > self.lines[self.cy].len() {
                            self.cx = self.lines[self.cy].len();
                        }
                        return true;
                    }
                    return false;
                }
                KeyCode::Char('k') => {
                    self.state = State::Help;
                    return true;
                }
                KeyCode::Char('l') => {
                    self.state = State::Command;
                    self.cmd_buf.clear();
                    return true;
                }
                _ => {}
            }
        }

        // Cmd/Option+arrows / Cmd/Option+hjkl: jump cursor (works in any mode).
        let mods = key.modifiers;
        if mods.intersects(KeyModifiers::SUPER) || mods.intersects(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    for _ in 0..self.alt_scroll {
                        if self.cx == 0 { break; }
                        self.cx = self.left_cx();
                    }
                    self.clamp_scroll();
                    return true;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    for _ in 0..self.alt_scroll {
                        let max = self.lines[self.cy].len();
                        if self.cx >= max { break; }
                        self.cx = self.right_cx();
                    }
                    self.clamp_scroll();
                    return true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.cy = self.cy.saturating_sub(self.alt_scroll);
                    self.cx = self.cx.min(self.lines[self.cy].len());
                    self.clamp_scroll();
                    return true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.cy = (self.cy + self.alt_scroll).min(self.lines.len().saturating_sub(1));
                    self.cx = self.cx.min(self.lines[self.cy].len());
                    self.clamp_scroll();
                    return true;
                }
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
                let (r, c) = self.next_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
            }
            KeyCode::Char('b') => {
                let (r, c) = self.prev_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
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
            // vim-style half-page scroll.
            KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = min(self.cy + page, self.lines.len().saturating_sub(1));
                self.cx = self.cx.min(self.lines[self.cy].len());
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = self.cy.saturating_sub(page);
                self.cx = self.cx.min(self.lines[self.cy].len());
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
            KeyCode::Char('i') => {
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('a') => {
                if self.cx < self.lines[self.cy].len() {
                    self.cx = self.right_cx();
                }
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('I') => {
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('A') => {
                self.cx = self.lines[self.cy].len();
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }

            // ── open new lines ──
            KeyCode::Char('o') => {
                self.save_undo();
                self.lines.insert(self.cy + 1, String::new());
                self.cy += 1;
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = true; // snapshot already taken above
                self.modified = true;
            }
            KeyCode::Char('O') => {
                self.save_undo();
                self.lines.insert(self.cy, String::new());
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = true; // snapshot already taken above
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
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => self.redo(),

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
        // Take one undo snapshot per insert session — right before the
        // first modification — so `u` undoes the whole typed run like
        // vim, instead of one character at a time.
        let modifies = matches!(key.code, KeyCode::Char(_))
            || matches!(key.code, KeyCode::Enter | KeyCode::Backspace | KeyCode::Delete | KeyCode::Tab);
        if modifies {
            self.save_undo_if_needed();
        }

        match key.code {
            // Escape returns to normal mode.
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.insert_saved = false;
                // Move cursor one left if past end of line (vim does this
                // so the block cursor sits on the last character).
                if self.cx > 0 && self.cx > self.lines[self.cy].len() {
                    self.cx = self.lines[self.cy].len();
                }
            }

            // Enter: split the line at the cursor, preserving indent.
            KeyCode::Enter => {
                let indent: String = {
                    let line = &self.lines[self.cy];
                    line.chars().take_while(|c| *c == ' ' || *c == '\t').collect()
                };
                let right = self.lines[self.cy].split_off(self.cx);
                self.lines.insert(self.cy + 1, indent.clone() + &right);
                self.cy += 1;
                self.cx = indent.len();
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

            // Regular character: insert at cursor (Ctrl-modified
            // chars are reserved for editor actions, so ignore them).
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
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
                let (r, c) = self.next_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
            }
            KeyCode::Char('b') => {
                let (r, c) = self.prev_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
            }
            // vim-style half-page scroll (extends the selection).
            KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = min(self.cy + page, self.lines.len().saturating_sub(1));
                self.cx = self.cx.min(self.lines[self.cy].len());
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = self.cy.saturating_sub(page);
                self.cx = self.cx.min(self.lines[self.cy].len());
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
                self.insert_saved = false;
            }
            KeyCode::Char('a') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                if self.cx < self.lines[self.cy].len() {
                    self.cx = self.right_cx();
                }
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('I') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('A') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.cx = self.lines[self.cy].len();
                self.mode = Mode::Insert;
                self.insert_saved = false;
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
                self.clamp_scroll();
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
                        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                        if content.ends_with('\n') { lines.push(String::new()); }
                        if lines.is_empty() { lines.push(String::new()); }
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
    //  Run-state key handling (Ctrl+E overlay)
    // ═══════════════════════════════════════════════════════════════

    fn handle_run_state(&mut self, _key: KeyEvent) -> bool {
        // Any key dismisses the run output (Ctrl+C/D included — they
        // shouldn't quit the whole editor while the overlay is open).
        self.state = State::Normal;
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
            // 'l': toggle looping.
            KeyCode::Char('l') => {
                self.music_player.looping = !self.music_player.looping;
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

            // Ctrl+C: quit.
            KeyCode::Char('c')
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
            // Ctrl+C: quit.
            KeyCode::Char('c')
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
                            let mut lines: Vec<String> =
                                content.lines().map(|l| l.to_string()).collect();
                            if content.ends_with('\n') { lines.push(String::new()); }
                            if lines.is_empty() { lines.push(String::new()); }
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
            // Ctrl+C: quit.
            KeyCode::Char('c')
                if key.modifiers == KeyModifiers::CONTROL =>
            {
                return false;
            }
            _ => {}
        }
        true
    }

    fn handle_sysinfo_state(&mut self, _key: KeyEvent) -> bool {
        self.state = State::Normal;
        true
    }

    fn handle_help_state(&mut self, _key: KeyEvent) -> bool {
        self.state = State::Normal;
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
        // Non-blocking system stats: poll completed task or spawn new one
        if let Some(ref task) = self.sys_task {
            if task.is_finished() {
                if let Ok(task) = self.sys_task.take().unwrap().join() {
                    self.sys_cpu = task.0;
                    self.sys_mem_used = task.1;
                    self.sys_mem_total = task.2;
                    self.sys_batt_pct = task.3;
                    self.sys_batt_status = task.4;
                    self.sys_updated = Instant::now();
                }
            }
        } else if self.sys_updated.elapsed().as_secs() >= 3 {
            self.sys_task = Some(std::thread::spawn(collect_sys_stats));
        }
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

    /// Re-parse the buffer with tree-sitter if its content or language
    /// changed since the last highlight pass.  Called once per render;
    /// cheap because it only does a hash check when nothing changed.
    fn ensure_highlight(&mut self) {
        let lang = highlight::detect_lang(
            self.filename.as_deref(),
            self.lines.first().map(|s| s.as_str()),
        );
        let stale = match &self.highlight {
            Some(h) => h.lang() != lang || h.hash() != hash_lines(&self.lines),
            None => true,
        };
        if stale {
            let hash = hash_lines(&self.lines);
            let source = self.lines.join("\n");
            self.highlight = Some(highlight::Highlighter::parse(lang, &source, hash));
        }
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
            // :w [file] — save (optionally to a new name)
            "w" => {
                let fname = if parts.len() > 1 {
                    let name = parts[1..].join(" ");
                    self.filename = Some(name.clone());
                    name
                } else {
                    match self.filename.clone() {
                        Some(f) => f,
                        None => {
                            self.flash = Some("No filename.  Use :w <name>".to_string());
                            return true;
                        }
                    }
                };
                match self.save_to_disk(&fname) {
                    Ok(_) => {
                        self.modified = false;
                        self.flash = Some(format!("'{}' written", fname));
                    }
                    Err(e) => {
                        self.flash = Some(format!("Error: {e}"));
                    }
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
            // :3 — hidden easter egg: cycle colour fx mode
            "3" => {
                self.fx_mode = (self.fx_mode + 1) % 4;
                self.fx_start = Instant::now();
                let names = ["off", "gentle", "breathing", "warm"];
                self.flash = Some(format!("fx: {}", names[self.fx_mode as usize]));
            }
            // :sys — system dashboard
            "sys" => {
                self.state = State::SysInfo;
            }
            // :e <file> — open file
            "e" if parts.len() > 1 => {
                let path = parts[1..].join(" ");
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                        if content.ends_with('\n') { lines.push(String::new()); }
                        if lines.is_empty() { lines.push(String::new()); }
                        let mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                        self.sync_to_buffer();
                        self.buffers.push(Buffer::new(lines, Some(path.clone()), mtime));
                        self.current = self.buffers.len() - 1;
                        self.sync_from_buffer(self.current);
                        self.flash = Some(format!("Opened {path}"));
                    }
                    Err(e) => {
                        self.flash = Some(format!("Can't open {path}: {e}"));
                    }
                }
            }
            // :theme <name> — switch theme
            "theme" if parts.len() > 1 => {
                let name = parts[1].to_lowercase();
                if let Some(tk) = ThemeKind::from_str(&name) {
                    self.theme = tk;
                    self.flash = Some(format!("Theme: {}", tk.name()));
                } else {
                    self.flash = Some(format!("Unknown theme: {name}"));
                }
            }
            // :wq! — save and force close
            "wq!" => {
                let fname = self.filename.clone().unwrap_or_default();
                if !fname.is_empty() && self.save_to_disk(&fname).is_err() {
                    self.flash = Some("Error saving".to_string());
                    return true;
                }
                self.modified = false;
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() { return false; }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
            // :<number> — jump to line
            cmd_name if cmd_name.parse::<usize>().is_ok() => {
                let num: usize = cmd_name.parse().unwrap();
                let line = num.saturating_sub(1);
                self.cy = line.min(self.lines.len().saturating_sub(1));
                self.cx = 0;
                self.left = 0;
                let page = (self.terminal_height() as usize).saturating_sub(2).max(1);
                self.top = self.cy.saturating_sub(page / 2);
                self.clamp_scroll();
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
            "go" => run_go(path),
            _ => format!("Don't know how to run .{ext} files.  Use .py, .rs, .c, .h, or .go."),
        }
    }

    /// Compile and run a C source file.
    fn run_c(&self, path: &str) -> String {
        let bin = "/tmp/ked_c_bin";

        // Remove any stale binary so a failed compile can't leave an
        // old executable behind that "succeeds" on the next run.
        let _ = fs::remove_file(bin);

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

        // Remove any stale binary so a failed compile can't leave an
        // old executable behind that "succeeds" on the next run.
        let _ = fs::remove_file(bin);

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
    ///
    /// Horizontal math is done in terminal *cells* (not bytes): `cx`
    /// and `left` are UTF-8 byte offsets, but the screen is laid out
    /// in columns — wide characters (CJK/emoji) take 2 cells and tabs
    /// advance to the next multiple of 8.
    fn clamp_scroll(&mut self) {
        // The gutter width (line number string + "│") we need to
        // account for in horizontal scrolling.
        let gutter = self.gutter_width() + 1; // number + "│"

        // Vertical: make sure the cursor line is on screen.  Reserve
        // one row for the buffer bar and one for the status bar.
        let height = self.terminal_height() as usize;
        let page_lines = height.saturating_sub(2).max(1);
        if self.cy < self.top {
            self.top = self.cy;
        }
        if self.cy >= self.top + page_lines {
            self.top = self.cy.saturating_sub(page_lines) + 1;
        }

        // Horizontal: make sure the cursor column is on screen.
        let width = self.terminal_width() as usize;
        let visible_cols = width.saturating_sub(gutter);
        if visible_cols == 0 {
            return;
        }
        let line = &self.lines[self.cy];
        let left = line.floor_char_boundary(self.left.min(line.len()));
        let cx = line.floor_char_boundary(self.cx.min(line.len()));
        let left_col = col_width(&line[..left]);
        let cx_col = col_width(&line[..cx]);
        if cx_col < left_col {
            self.left = cx;
        } else if cx_col >= left_col + visible_cols {
            self.left = byte_at_col(line, cx_col.saturating_sub(visible_cols) + 1);
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

    /// Find the end of the next word on or after `col`, wrapping to
    /// the start of the next line when the word runs off the end.
    fn next_word_pos(&self, row: usize, col: usize) -> (usize, usize) {
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
            return (row + 1, 0);
        }
        (row, pos)
    }

    /// Find the start of the word before `col`, wrapping to the end
    /// of the previous line when already at column 0.
    fn prev_word_pos(&self, row: usize, col: usize) -> (usize, usize) {
        if col == 0 {
            if row > 0 {
                return (row - 1, self.lines[row - 1].len());
            }
            return (0, 0);
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
        (row, pos)
    }

    /// Save the current buffer state (text + cursor) onto the undo
    /// stack.  Called before every edit in normal/visual mode.
    fn save_undo(&mut self) {
        self.undo_stack.push(Snapshot::of(self.lines.clone(), self.cy, self.cx));
        // Keep undo stack manageable (max 100 entries).
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        // Redo is invalidated on new changes.
        self.redo_stack.clear();
    }

    /// Save the undo snapshot for this insert session, unless it was
    /// already taken (we snapshot once per insert session so `u`
    /// undoes the whole typed run like vim).
    fn save_undo_if_needed(&mut self) {
        if self.insert_saved {
            return;
        }
        self.save_undo();
        self.insert_saved = true;
    }

    /// Restore a snapshot into the editor, clamping the cursor to the
    /// restored text.
    fn restore(&mut self, snap: Snapshot) {
        self.lines = snap.lines;
        self.cy = snap.cy.min(self.lines.len().saturating_sub(1));
        self.cx = snap.cx.min(self.lines[self.cy].len());
    }

    /// Undo one step (`u`).
    fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(Snapshot::of(self.lines.clone(), self.cy, self.cx));
            self.restore(prev);
        }
    }

    /// Redo one step (Ctrl+R).
    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(Snapshot::of(self.lines.clone(), self.cy, self.cx));
            self.restore(next);
        }
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
        self._frame = self._frame.wrapping_add(1);
        if self.mode != self._last_mode {
            self._mode_switched = self._frame;
            self._last_mode = self.mode;
        }

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

        // Split the screen: buffer bar + main content + status bar.
        // With `status_bar_top` the status bar sits above the buffer bar.
        let (bufbar_area, content_area, status_area) = if self.status_bar_top {
            let [status_area, bufbar_area, content_area] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .areas(area);
            (bufbar_area, content_area, status_area)
        } else {
            let [bufbar_area, content_area, status_area] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .areas(area);
            (bufbar_area, content_area, status_area)
        };

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
        let mut theme = self.theme.theme();
        if self.fx_mode > 0 {
            // Quantise time to ~30 steps/sec: a full 60fps shift
            // recolours every token every frame, forcing a full
            // repaint (and cursor strobe) every frame.
            let elapsed = (self.fx_start.elapsed().as_secs_f64() * 30.0).floor() / 30.0;
            match self.fx_mode {
                1 => { // gentle: very slow ±15° hue oscillation
                    let hue = (elapsed * 3.0).sin() * 15.0;
                    theme = soft_shift_theme(&theme, hue, 0.0);
                }
                2 => { // breathing: very slow lightness pulse
                    let lightness = ((elapsed * 0.3).sin() * 0.5 + 0.5) * 0.08;
                    theme = soft_shift_theme(&theme, 0.0, lightness);
                }
                3 => { // warm: very slow drift between amber and purple
                    let hue = (elapsed * 2.0).sin() * 25.0 + 270.0;
                    theme = soft_shift_theme(&theme, hue, 0.0);
                }
                _ => {}
            }
        }
        // Transparent content background
        if self.transparent {
            theme.bg = Color::Reset;
        }
        // Apply opacity to UI chrome
        if self.opacity < 1.0 {
            theme.status_bg = Color::Reset;
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
        // Re-parse the buffer for highlighting if it changed.
        self.ensure_highlight();
        let gutter = self.gutter_width() + 1;
        let mut lines_vec: Vec<Line> = Vec::new();
        let visible_lines = editor_area.height as usize;
        let visible_content_cols = editor_area.width.saturating_sub(gutter as u16) as usize;

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

            // (b) highlighted content (tree-sitter, cached per edit)
            let raw = &self.lines[buf_row];
            let highlighted = match &self.highlight {
                Some(h) => h.spans(buf_row, &theme),
                None => vec![Span::styled(raw.to_string(), style)],
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

            // Clip content spans to the visible horizontal window
            // (in terminal cells), expanding tabs to spaces so the
            // frame never desynchronises.
            lines_vec.push(
                Line::from(visible_spans(raw, spans, self.left, visible_content_cols))
                    .style(line_style),
            );
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
            // Cursor column in cells: convert byte offsets through
            // visual widths so wide chars / tabs don't push the
            // cursor off its column.
            let line = &self.lines[self.cy];
            let cx = line.floor_char_boundary(self.cx.min(line.len()));
            let left = line.floor_char_boundary(self.left.min(line.len()));
            let cx_screen = col_width(&line[..cx])
                .saturating_sub(col_width(&line[..left])) as u16
                + gutter as u16;
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
            State::SysInfo => self.render_sysinfo(f, area, &theme),
            State::Help => self.render_help(f, area, &theme),
            _ => {}
        }

        // Cursor shape per mode (applied in main.rs after draw)
        self.cursor_style = match self.mode {
            Mode::Insert => SetCursorStyle::BlinkingBar,
            Mode::Normal if self.state == State::Visual => SetCursorStyle::SteadyUnderScore,
            _ => SetCursorStyle::SteadyBlock,
        };
    }

    // ── status bar ───────────────────────────────────────────────

    /// Render a one‑line buffer (tab) bar at the top of the screen.
    fn render_buffer_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Split: tabs on the left, system stats on the right (if enabled)
        let stat_w = if self.bar_stats {
            56u16.min(area.width.saturating_sub(20))
        } else {
            0
        };
        let [tabs_area, stats_area] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(stat_w),
        ])
        .areas(area);

        // Tab shimmer
        let active_bg = if self.animations {
            let pulse = 0.5 + 0.5 * (self._frame as f64 * 0.12).sin();
            blend_colors(theme.status_bg, theme.status_fg, 0.0, (pulse * 0.10) as f32)
        } else {
            theme.status_bg
        };

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
                let tab_bg = if is_current { active_bg } else { theme.bg };
                let tab_fg = if is_current { theme.status_fg }
                    else { theme.comment.fg.unwrap_or(theme.fg) };
                let tab_style = Style::new().fg(tab_fg).bg(tab_bg);
                let edge_style = Style::new().fg(tab_bg).bg(theme.bg);
                let show_pills = is_current && tab_bg != Color::Reset;
                if show_pills {
                    vec![
                        Span::styled("", edge_style),
                        Span::styled(label, tab_style),
                        Span::styled("", edge_style),
                    ]
                } else if is_current {
                    vec![Span::styled(label, tab_style)]
                } else {
                    vec![
                        Span::styled(label, tab_style),
                    ]
                }
            })
            .collect::<Vec<_>>();

        f.render_widget(
            Paragraph::new(Line::from(items)).style(Style::new().bg(theme.bg)),
            tabs_area,
        );

        // ── right-side system stats (only when bar_stats = true) ──
        if !self.bar_stats {
            return;
        }
        let now = chrono::Local::now();
        let time_str = now.format("%H:%M:%S").to_string();
        let date_str = now.format("%Y-%m-%d").to_string();

        // Build stats line: CPU | MEM | BATT | time | holy pulse
        let cpu_str = format!("CPU:{:3.0}%", self.sys_cpu.min(99.9));
        let mem_str = if self.sys_mem_total > 0 {
            let used_gb = self.sys_mem_used as f64 / (1 << 30) as f64;
            let total_gb = self.sys_mem_total as f64 / (1 << 30) as f64;
            format!("MEM:{:.1}/{:.1}G", used_gb, total_gb)
        } else {
            String::from("MEM:?")
        };
        let batt_str = if let Some(pct) = self.sys_batt_pct {
            let icon = match self.sys_batt_status.as_deref() {
                Some("charging") => "+",
                Some("full") | Some("on AC") => "=",
                _ => "",
            };
            format!("BAT:{}{}%", icon, pct)
        } else {
            String::from("BAT:?")
        };

        // TempleOS "holy pulse" indicator if animations are on
        let holy = if self.animations {
            let chars = ['▁','▂','▃','▄','▅','▆','▇','█'];
            let idx = (self._frame as usize / 6) % chars.len();
            format!("{}", chars[idx])
        } else {
            String::from("█")
        };

        let stats_text = format!(
            " {cpu_str}  {mem_str}  {batt_str}  {date_str} {time_str} {holy} "
        );
        // Split stats area: separator char on left, stats text on right
        let [sep_area, text_area] = Layout::horizontal([
            Constraint::Length(1),
            Constraint::Min(1),
        ]).areas(stats_area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                if theme.status_bg == Color::Reset { " " } else { "" },
                Style::new().fg(theme.status_bg).bg(theme.bg))))
            .style(Style::new().bg(theme.status_bg)),
            sep_area,
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(stats_text,
                Style::new().fg(theme.status_fg).bg(theme.status_bg))))
                .alignment(Alignment::Right)
                .style(Style::new().bg(theme.status_bg)),
            text_area,
        );
    }

    /// Render the status bar at the bottom of the screen.
    fn render_status(&self, f: &mut Frame, area: Rect, _width: usize, theme: &Theme) {

        // Build the status text depending on state.
        let text: String = if let Some(ref msg) = self.flash {
            format!(" {msg} ")
        } else if self.state == State::Command {
            format!(":{}", self.cmd_buf)
        } else if self.state == State::Search {
            format!("/{}", self.search_query)
        } else {
            // Spinner (braille rotation) when music is playing
            let spinner_chars: &[char] = &['⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏'];
            let spinner = if self.animations && self.music_player.playing {
                format!("{} ", spinner_chars[self._frame as usize % spinner_chars.len()])
            } else {
                String::new()
            };

            // Mode text in solid colour while active
            let (mode_text, mode_color) = if self.state == State::Visual {
                ("VISUAL", Color::Rgb(100, 160, 255))
            } else {
                match self.mode {
                    Mode::Insert => ("INSERT", Color::Rgb(100, 220, 100)),
                    Mode::Normal => ("NORMAL", theme.status_fg),
                }
            };
            let mode_style = Style::new().fg(mode_color).bg(theme.status_bg);

            let fname = self.filename.as_deref().unwrap_or("[No Name]");
            let mod_str = if self.modified { " [+]" } else { "" };
            let location = format!("{}:{}", self.cy + 1, self.cx + 1);
            let tname = self.theme.name();
            // Clock
            let now = chrono::Local::now().format("%H:%M").to_string();

            let now_playing = self.music_player.current_song.as_ref()
                .filter(|_| self.music_player.playing)
                .map(|s| {
                    let name = s.rsplit('/').next().unwrap_or(s);
                    let max_len = (_width.saturating_sub(60)).max(10);
                    if name.len() > max_len {
                        // Scroll at ~3 chars/sec (frame / 20 at 60fps)
                        let pos_raw = (self._frame as usize / 20) % (name.len() + 7);
                        let scrolled = format!("{name}  ♫  {name}");
                        let end_raw = (pos_raw + max_len).min(scrolled.len());
                        // Snap to char boundaries so we never slice mid-char
                        let pos = if scrolled.is_char_boundary(pos_raw) { pos_raw }
                            else { scrolled.floor_char_boundary(pos_raw) };
                        let end = scrolled.ceil_char_boundary(end_raw).min(scrolled.len());
                        let slice = &scrolled[pos..end];
                        format!("  ♫ {slice}")
                    } else {
                        format!("  ♫ {name}")
                    }
                })
                .unwrap_or_default();

            // Build spans with animated dash colours
            let dash_color = if self.animations {
                let dash_colors = [theme.keyword.fg, theme.builtin.fg, theme.rstype.fg, theme.string.fg, theme.number.fg];
                let dash_idx = (self._frame / 3) as usize % dash_colors.len();
                dash_colors[dash_idx].unwrap_or(theme.status_fg)
            } else {
                theme.status_fg
            };
            let dash_style = Style::new().fg(dash_color).bg(theme.status_bg);
            let plain_style = Style::new().fg(theme.status_fg).bg(theme.status_bg);

            let spans = vec![
                Span::styled(format!(" {spinner}"), plain_style),
                Span::styled(mode_text.to_string(), mode_style),
                Span::styled(format!("  {fname}{mod_str}  "), plain_style),
                Span::styled("─", dash_style),
                Span::styled(format!(" {location}  "), plain_style),
                Span::styled("─", dash_style),
                Span::styled(format!(" {tname}  "), plain_style),
                Span::styled("─", dash_style),
                Span::styled(format!(" {now}{now_playing} "), plain_style),
            ];

            let bar = Paragraph::new(Line::from(spans))
                .style(Style::new().bg(theme.status_bg));
            f.render_widget(ratatui::widgets::Clear, area);
            f.render_widget(bar, area);
            return;
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

    fn render_finder(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {

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

        // Outer block with scrolling title (border shows through on sides)
        let title_text = scrolled_title("Find File", self._frame, self.animations);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .title_alignment(Alignment::Center)
            .title_style(Style::new().fg(theme.fg).bg(theme.status_bg))
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

        let total = self.finder.results.len();
        let visible_h = list_area.height as usize;

        // Auto-scroll
        if self.finder_selection < self.finder_scroll {
            self.finder_scroll = self.finder_selection;
        }
        if self.finder_selection >= self.finder_scroll + visible_h {
            self.finder_scroll = self.finder_selection - visible_h + 1;
        }

        // Results list.
        let results: Vec<ListItem> = self
            .finder
            .results
            .iter()
            .enumerate()
            .skip(self.finder_scroll)
            .take(visible_h)
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
                if total == 0 { " (no files found)" } else { "" },
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

    fn render_music(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {

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

        let title_text = scrolled_title("Music Player", self._frame, self.animations);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .title_alignment(Alignment::Center)
            .title_style(Style::new().fg(theme.fg).bg(theme.status_bg))
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
        let loop_indicator = if self.music_player.looping { " ↻" } else { "" };
        let status_text = if let Some(ref song) = self.music_player.current_song {
            let name = song.rsplit('/').next().unwrap_or(song);
            format!(" Now Playing: {name}{loop_indicator}")
        } else if self.music_player.files.is_empty() {
            " No MP3 files found in this directory.".to_string()
        } else {
            format!(" {} files — Enter=play  s=stop  l=loop{}  Esc=close",
                self.music_player.files.len(), loop_indicator)
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

        let total = self.music_player.files.len();
        let visible_h = list_area.height as usize;

        // Auto-scroll to keep selected visible
        let scroll = if self.music_player.selected < visible_h {
            0
        } else {
            (self.music_player.selected - visible_h / 2).min(total.saturating_sub(visible_h))
        };

        // File list.
        let results: Vec<ListItem> = self
            .music_player
            .files
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible_h)
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

    fn render_theme(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {

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

        let title_text = scrolled_title("Theme Selector", self._frame, self.animations);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .title_alignment(Alignment::Center)
            .title_style(Style::new().fg(theme.fg).bg(theme.status_bg))
            .border_style(Style::new().fg(theme.fg));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        if inner.height < 1 {
            return;
        }

        let themes = ThemeKind::all();
        let visible_h = inner.height as usize;

        // Auto-scroll to keep selected item visible
        if self.theme_selected < self.theme_scroll {
            self.theme_scroll = self.theme_selected;
        }
        if self.theme_selected >= self.theme_scroll + visible_h {
            self.theme_scroll = self.theme_selected - visible_h + 1;
        }

        let list_items: Vec<ListItem> = themes
            .iter()
            .enumerate()
            .skip(self.theme_scroll)
            .take(visible_h)
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

    // ── system dashboard (:sys) ──────────────────────────────────

    fn render_sysinfo(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Cached sys stats + fresh dashboard data
        let cpu_pct = self.sys_cpu.min(100.0);
        let mem_used = self.sys_mem_used;
        let mem_total = self.sys_mem_total.max(1);
        let mem_pct = ((mem_used as f64 / mem_total as f64) * 100.0).min(100.0);

        let host = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "?".into());
        let user = std::env::var("USER").unwrap_or_else(|_| "?".into());
        let kernel = std::process::Command::new("uname").arg("-r").output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "?".into());

        let uptime = std::process::Command::new("uptime").output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "?".into());

        let disk_line = std::process::Command::new("df").args(["-h", "/"]).output()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                let parts: Vec<&str> = s.lines().last().unwrap_or("").split_whitespace().collect();
                if parts.len() >= 5 {
                    format!("Disk: {} used / {}  ({} free)", parts[2], parts[1], parts[3])
                } else { "Disk: ?".into() }
            })
            .unwrap_or_else(|_| "Disk: ?".into());

        let batt_line = {
            #[cfg(target_os = "macos")]
            { self.sys_batt_pct.map(|pct| {
                let status = self.sys_batt_status.as_deref().unwrap_or("?");
                format!("Battery: {}% ({})", pct, status)
            })}
            #[cfg(not(target_os = "macos"))]
            { None::<String> }
        };

        // Log tail
        let log_text = {
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("log")
                    .args(["show", "--last", "5m", "--style", "compact", "--predicate",
                        "eventMessage contains[c] 'error' or messageType == 16 or messageType == 17"])
                    .output().ok().map(|out| {
                        let s = String::from_utf8_lossy(&out.stdout);
                        let lines: Vec<&str> = s.lines().rev().take(15).collect();
                        lines.into_iter().rev().collect::<Vec<_>>().join("\n")
                    }).unwrap_or_else(|| "(no log data)".into())
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("journalctl")
                    .args(["--no-pager", "-n", "15", "-p", "3..4", "-o", "short-iso"])
                    .output().ok().map(|out| {
                        String::from_utf8_lossy(&out.stdout).trim().to_string()
                    }).filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "(no log data)".into())
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            { String::new() }
        };

        // ── Layout ────────────────────────────────────────────────
        // header (1) + info (1) + battery? (1) + gap (1) + log-header (1) + logs (rest)
        let has_batt = batt_line.is_some();
        let mut constraints = vec![
            Constraint::Length(1),          // header
            Constraint::Length(1),          // info
        ];
        if has_batt {
            constraints.push(Constraint::Length(1)); // battery
        }
        constraints.push(Constraint::Length(1));     // gap
        constraints.push(Constraint::Length(1));     // log header
        constraints.push(Constraint::Min(4));       // logs
        let areas = Layout::vertical(constraints).split(area);
        let mut idx = 0;
        let head_area      = areas[idx]; idx += 1;
        let info_area      = areas[idx]; idx += 1;
        let batt_area      = if has_batt { let r = areas[idx]; idx += 1; Some(r) } else { None };
        let log_head_area  = areas[idx + 1]; // skip gap
        let log_area       = areas[idx + 2];

        // ── Header bar ────────────────────────────────────────────
        let header = format!(" System Dashboard ─ {host} ─ {user} ─ kernel {kernel} ");
        f.render_widget(Clear, head_area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(header,
                Style::new().fg(theme.status_fg).bg(theme.status_bg))))
            .style(Style::new().bg(theme.status_bg)),
            head_area,
        );

        // ── Info row ──────────────────────────────────────────────
        let info_text = format!(
            "  CPU: {:.1}%     Memory: {:.1}/{:.1} GB ({:.0}%)     {}     {}  ",
            cpu_pct,
            mem_used as f64 / (1<<30) as f64,
            mem_total as f64 / (1<<30) as f64,
            mem_pct,
            disk_line,
            uptime,
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(info_text,
                Style::new().fg(theme.fg).bg(theme.bg))))
            .style(Style::new().bg(theme.bg)),
            info_area,
        );

        // ── Battery row (optional) ────────────────────────────────
        if let (Some(area), Some(bl)) = (batt_area, &batt_line) {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(format!("  {bl}  "),
                    Style::new().fg(theme.fg).bg(theme.bg))))
                .style(Style::new().bg(theme.bg)),
                area,
            );
        }

        // ── Log header ────────────────────────────────────────────
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("  System Log ─ recent errors  ",
                Style::new().fg(theme.status_fg).bg(theme.status_bg))))
            .style(Style::new().bg(theme.status_bg)),
            log_head_area,
        );

        // ── Log content ───────────────────────────────────────────
        let log_lines: Vec<Line> = if log_text.is_empty() {
            vec![Line::from(Span::styled("  (no log data)",
                Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)).bg(theme.bg)))]
        } else {
            log_text.lines().map(|l|
                Line::from(Span::styled(format!("  {l}"),
                    Style::new().fg(theme.fg).bg(theme.bg)))
            ).collect()
        };
        f.render_widget(
            Paragraph::new(Text::from(log_lines))
                .style(Style::new().bg(theme.bg)),
            log_area,
        );
    }

    // ── keybind manual (Ctrl+K) ──────────────────────────────────

    fn render_help(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Keybinds ")
            .title_style(Style::new().fg(theme.fg))
            .border_style(Style::new().fg(theme.fg));
        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);

        let help_text = vec![
            Line::from(Span::styled("Movement & Editing", theme.keyword)),
            Line::from("  h/j/k/l    move cursor          w/b      next/prev word"),
            Line::from("  0/$/gg/G    line start/end, first/last"),
            Line::from("  i/a/I/A     insert mode           o/O      open line below/above"),
            Line::from("  x/dd        delete char/line      yy/p/P   yank/paste"),
            Line::from("  u           undo                  Ctrl+R   redo"),
            Line::from("  v           visual mode           Ctrl+C   exit to normal"),
            Line::from(""),
            Line::from(Span::styled("Buffers & Files", theme.keyword)),
            Line::from("  Tab/S-Tab   next/prev buffer      :bn/:bp  next/prev buffer"),
            Line::from("  Ctrl+P      find file             Ctrl+F   file tree"),
            Line::from("  Ctrl+S      save                  :e <f>   open file"),
            Line::from(""),
            Line::from(Span::styled("Tools", theme.keyword)),
            Line::from("  Ctrl+E      run file              Ctrl+M   music player"),
            Line::from("  Ctrl+T      theme selector        Ctrl+K   this manual"),
            Line::from("  :sys        system dashboard      :wq      save & quit"),
            Line::from(""),
            Line::from(Span::styled("Commands", theme.keyword)),
            Line::from("  /           search                n/N      next/prev match"),
            Line::from("  :w/:wq      save / save & quit    :q/:q!   quit / force quit"),
            Line::from("  :theme <n>  switch theme          Ctrl+J   shell popup"),
        ];

        f.render_widget(
            Paragraph::new(Text::from(help_text))
                .style(Style::new().fg(theme.fg).bg(theme.bg)),
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

        let title_text = scrolled_title("Run Output", self._frame, self.animations);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_text)
            .title_alignment(Alignment::Center)
            .title_style(Style::new().fg(theme.fg).bg(theme.status_bg))
            .border_style(Style::new().fg(theme.fg));
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        // Output text — show last inner_h lines.
        let inner_h = inner.height as usize;
        let lines: Vec<&str> = self.run_output.split('\n').collect();
        let visible = if lines.len() > inner_h { &lines[lines.len() - inner_h..] } else { &lines[..] };
        let text: String = visible.join("\n");
        f.render_widget(
            Paragraph::new(Text::from(text))
                .style(Style::new().fg(theme.fg).bg(theme.bg)),
            inner,
        );
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

        // ── visible cursor ───────────────────────────────────────
        let cur_row = self.shell.as_ref().map(|s| s.cursor_row).unwrap_or(0);
        let cur_col = self.shell.as_ref().map(|s| s.cursor_col).unwrap_or(0);
        if cur_row < max_rows {
            let cy = inner.y + cur_row as u16;
            let cx = inner.x + (cur_col as u16).min(inner.width.saturating_sub(1));
            f.set_cursor_position(Position::new(cx, cy));
        }
    }

    fn render_splash(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        f.render_widget(Clear, area);

        let figlet = [
            "   __          __",
            "  / /_____ ___/ /",
            " /  '_/ -_) _  / ",
            "/_/\\_\\\\__/\\_,_/  ",
            "                 ",
        ];

        // Rainbow colours for the logo — cycle through theme highlights
        let splash_colors: [(Color, f64); 5] = [
            (theme.keyword.fg.unwrap_or(theme.fg), 0.0),
            (theme.builtin.fg.unwrap_or(theme.fg), 0.2),
            (theme.rstype.fg.unwrap_or(theme.fg), 0.4),
            (theme.string.fg.unwrap_or(theme.fg), 0.6),
            (theme.number.fg.unwrap_or(theme.fg), 0.8),
        ];
        let anim_offset = self._frame as f64 * 0.04;

        let keybinds = [
            ("Ctrl+P", "find file"),
            ("Ctrl+F", "file tree"),
            ("Ctrl+J", "terminal"),
            ("Ctrl+M", "music"),
            ("Ctrl+T", "theme"),
            ("Ctrl+E", "run"),
            ("Ctrl+R", "redo"),
            ("Ctrl+K", "keybinds"),
            ("Tab",    "switch buf"),
            (":wq",    "save & quit"),
        ];

        // Compute column widths for alignment: keys in one column,
        // descriptions in the next, and a fixed-width first column so
        // the second column lines up on every row.
        let key_w = keybinds.iter().map(|(k, _)| k.len()).max().unwrap_or(6);
        let first_w = keybinds
            .chunks(2)
            .filter_map(|c| c.first())
            .map(|(k, d)| key_w + 2 + d.len())
            .max()
            .unwrap_or(key_w + 2);

        let mut lines: Vec<Line> = Vec::new();

        let total_h = figlet.len() + 3 + (keybinds.len() + 1) / 2;
        let pad_top = (area.height as usize).saturating_sub(total_h) / 2;
        for _ in 0..pad_top {
            lines.push(Line::from(""));
        }

        for (li, line) in figlet.iter().enumerate() {
            let color = if self.animations {
                splash_colors[(li as f64 + anim_offset) as usize % splash_colors.len()].0
            } else {
                theme.keyword.fg.unwrap_or(theme.fg)
            };
            lines.push(Line::from(Span::styled(*line, Style::new().fg(color))));
        }

        lines.push(Line::from(""));

        // Subtitle
        lines.push(Line::from(Span::styled(
            "kayden's editor (now in rust!)",
            Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)),
        )));

        lines.push(Line::from(""));

        for chunk in keybinds.chunks(2) {
            let mut spans = Vec::new();
            for (i, (key, desc)) in chunk.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" ".repeat(3)));
                }
                spans.push(Span::styled(*key, theme.builtin));
                spans.push(Span::raw(" ".repeat(key_w + 2 - key.len())));
                spans.push(Span::styled(*desc, Style::new().fg(theme.fg)));
                if i == 0 && chunk.len() > 1 {
                    // Pad the first column so the second lines up.
                    spans.push(Span::raw(" ".repeat(first_w - (key_w + 2 + desc.len()))));
                }
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
                let icon = ft_icon(&entry.path, entry.is_dir);
                let indent = "  ".repeat(entry.depth);
                let label = format!("{indent}{prefix}{icon} {}", entry.name);
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

/// TempleOS-style scrolling title bar text.
/// Pads with `═` and scrolls left based on frame counter.
/// When `animated` is false, returns a static centered title.
fn scrolled_title(label: &str, frame: u64, animated: bool) -> String {
    let label = &label.to_uppercase();
    if !animated {
        return format!("═══ {label} ═══");
    }
    let pad = 6usize;
    let scrolled = format!("{}═══ {} ═══{}",
        "═".repeat(pad), label, "═".repeat(pad));
    let chars: Vec<char> = scrolled.chars().collect();
    let visible = 24usize.min(chars.len());
    let offset = (frame as usize / 5) % chars.len();
    if offset + visible <= chars.len() {
        chars[offset..offset + visible].iter().collect()
    } else {
        let mut s: String = chars[offset..].iter().collect();
        let remain = visible - (chars.len() - offset);
        s.push_str(&chars[..remain].iter().collect::<String>());
        s
    }
}

/// Blend two ratatui Colors by interpolating their RGB values.
/// `factor` controls how much of `b` is mixed in: 0.0 = pure `a`, 1.0 = pure `b`.
fn blend_colors(a: Color, b: Color, min_factor: f32, max_factor: f32) -> Color {
    let ar = color_to_rgb(a);
    let br = color_to_rgb(b);
    if let (Some((ar, ag, ab)), Some((br, bg, bb))) = (ar, br) {
        let t = min_factor.max(max_factor).min(1.0);
        Color::Rgb(
            (ar as f32 + (br as f32 - ar as f32) * t) as u8,
            (ag as f32 + (bg as f32 - ag as f32) * t) as u8,
            (ab as f32 + (bb as f32 - ab as f32) * t) as u8,
        )
    } else {
        a
    }
}

/// Softly shift a theme's colours.  `hue_offset` rotates hues in degrees
/// (small values = subtle).  `lightness` blends toward white (positive) or
/// black (negative) — used for breathing effects.
fn soft_shift_theme(t: &Theme, hue_offset: f64, lightness: f64) -> Theme {
    use crate::theme::hsl_to_rgb;

    let shift = |c: Color| -> Color {
        let rgb = color_to_rgb(c);
        let (r, g, b) = match rgb {
            Some(x) => x,
            None => return c,
        };
        let rn = r as f64 / 255.0;
        let gn = g as f64 / 255.0;
        let bn = b as f64 / 255.0;
        let mx = rn.max(gn).max(bn);
        let mn = rn.min(gn).min(bn);
        let l = (mx + mn) / 2.0;
        let d = mx - mn;

        let s = if d == 0.0 { 0.0 }
            else if l > 0.5 { d / (2.0 - mx - mn) }
            else { d / (mx + mn) };

        let h = if d == 0.0 { 0.0 } else if mx == rn {
            ((gn - bn) / d * 60.0 + 360.0) % 360.0
        } else if mx == gn {
            ((bn - rn) / d * 60.0 + 120.0) % 360.0
        } else {
            ((rn - gn) / d * 60.0 + 240.0) % 360.0
        };

        let h = (h + hue_offset + 360.0) % 360.0;
        let l = (l + lightness).clamp(0.05, 0.95);
        let (r2, g2, b2) = hsl_to_rgb(h, s as f64, l);
        Color::Rgb(r2, g2, b2)
    };

    let shift_style = |s: &Style| -> Style {
        Style {
            fg: s.fg.map(shift),
            bg: s.bg.map(shift),
            underline_color: s.underline_color.map(shift),
            add_modifier: s.add_modifier,
            sub_modifier: s.sub_modifier,
        }
    };

    Theme {
        fg: shift(t.fg),
        bg: t.bg,  // keep background static
        selection_bg: shift(t.selection_bg),
        line_number: shift_style(&t.line_number),
        tilde: shift_style(&t.tilde),
        status_bg: shift(t.status_bg),
        status_fg: shift(t.status_fg),
        keyword: shift_style(&t.keyword),
        builtin: shift_style(&t.builtin),
        rstype: shift_style(&t.rstype),
        function: shift_style(&t.function),
        lifetime: shift_style(&t.lifetime),
        string: shift_style(&t.string),
        fstring_prefix: shift_style(&t.fstring_prefix),
        comment: shift_style(&t.comment),
        number: shift_style(&t.number),
        constant: shift_style(&t.constant),
        decorator: shift_style(&t.decorator),
        property: shift_style(&t.property),
        operator: shift_style(&t.operator),
        punctuation: shift_style(&t.punctuation),
    }
}
fn color_to_rgb(c: Color) -> Option<(u8, u8, u8)> {
    match c {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((255, 0, 0)),
        Color::Green => Some((0, 255, 0)),
        Color::Yellow => Some((255, 255, 0)),
        Color::Blue => Some((0, 0, 255)),
        Color::Magenta => Some((255, 0, 255)),
        Color::Cyan => Some((0, 255, 255)),
        Color::Gray => Some((192, 192, 192)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::LightRed => Some((255, 128, 128)),
        Color::LightGreen => Some((128, 255, 128)),
        Color::LightYellow => Some((255, 255, 128)),
        Color::LightBlue => Some((128, 128, 255)),
        Color::LightMagenta => Some((255, 128, 255)),
        Color::LightCyan => Some((128, 255, 255)),
        Color::White => Some((255, 255, 255)),
        _ => None,
    }
}
// ── terminal-cell (visual column) helpers ───────────────────────

/// Approximate terminal cell width of a character (wcwidth).
/// ASCII is 1 cell, combining marks are 0, East Asian wide/fullwidth
/// characters and common emoji are 2.  Good enough to keep the
/// cursor aligned with the text it edits.
pub fn char_width(c: char) -> usize {
    if c == '\0' {
        return 0;
    }
    if c.is_ascii() {
        return 1;
    }
    match c as u32 {
        // Combining diacritics — zero width.
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20FF | 0xFE20..=0xFE2F => 0,
        // East Asian wide & fullwidth, Hangul, emoji, CJK extensions.
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE10..=0xFE19
        | 0xFE30..=0xFE6F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6
        | 0x16FE0..=0x16FFF | 0x17000..=0x187F7 | 0x1B000..=0x1B2FF
        | 0x1F300..=0x1F64F | 0x1F680..=0x1F6FF | 0x1F900..=0x1F9FF
        | 0x20000..=0x2FFFD | 0x30000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// Number of terminal cells a string occupies: tabs advance to the
/// next multiple of 8, control characters are zero-width (they are
/// dropped by [`visible_spans`]).
fn col_width(s: &str) -> usize {
    let mut w = 0usize;
    for c in s.chars() {
        if c == '\t' {
            w = (w / 8 + 1) * 8;
        } else if !c.is_control() {
            w += char_width(c);
        }
    }
    w
}

/// Smallest char-boundary byte offset in `s` whose visual column is
/// at least `col`.  Returns `s.len()` if `col` is past the end.
fn byte_at_col(s: &str, col: usize) -> usize {
    let mut w = 0usize;
    for (b, c) in s.char_indices() {
        if w >= col {
            return b;
        }
        if c == '\t' {
            w = (w / 8 + 1) * 8;
        } else if !c.is_control() {
            w += char_width(c);
        }
    }
    s.len()
}

/// Clip the content spans of one rendered line to the visible
/// horizontal window (measured in terminal cells, not bytes).
///
/// `spans[0]` is the line-number gutter and is always kept; the rest
/// must concatenate to `raw`.  Tabs are expanded to spaces and stray
/// control characters are dropped so the terminal frame never
/// desynchronises.
fn visible_spans(
    raw: &str,
    mut spans: Vec<Span<'static>>,
    left: usize,
    visible_cols: usize,
) -> Vec<Span<'static>> {
    if spans.is_empty() {
        return spans;
    }
    let gutter = spans.remove(0);
    let mut out = vec![gutter];

    let left = raw.floor_char_boundary(left.min(raw.len()));
    let left_col = col_width(&raw[..left]);
    let end_byte = byte_at_col(raw, left_col.saturating_add(visible_cols));
    if end_byte <= left {
        return out;
    }

    let mut byte_off = 0usize;
    let mut current: Option<(Style, String)> = None;

    for span in spans {
        let text = span.content.as_ref();
        let span_start = byte_off;
        let span_end = span_start + text.len();
        byte_off = span_end;

        let s = span_start.max(left);
        let e = span_end.min(end_byte);
        if s >= e {
            continue;
        }
        let slice = &text[s - span_start..e - span_start];
        let mut col = col_width(&raw[..s]);
        for c in slice.chars() {
            if col >= left_col + visible_cols {
                break;
            }
            if c == '\t' {
                let stop = (col / 8 + 1) * 8;
                let spaces = (stop - col).min(left_col + visible_cols - col);
                push_text(&mut out, &mut current, " ".repeat(spaces), span.style);
                col = stop;
            } else if c.is_control() {
                // Drop — control characters would corrupt the frame.
            } else {
                push_text(&mut out, &mut current, c.to_string(), span.style);
                col += char_width(c);
            }
        }
    }
    if let Some((style, text)) = current {
        out.push(Span::styled(text, style));
    }
    out
}

/// Append `text` to the current run in `current`, flushing to `out`
/// when the style changes (keeps adjacent same-styled spans merged).
fn push_text(
    out: &mut Vec<Span<'static>>,
    current: &mut Option<(Style, String)>,
    text: String,
    style: Style,
) {
    if text.is_empty() {
        return;
    }
    match current.as_mut() {
        Some((cs, ct)) if *cs == style => ct.push_str(&text),
        Some((cs, ct)) => {
            out.push(Span::styled(std::mem::take(ct), *cs));
            *cs = style;
            ct.push_str(&text);
        }
        None => *current = Some((style, text)),
    }
}

/// Cheap content fingerprint used to decide whether the highlighter
/// cache is stale.
fn hash_lines(lines: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    lines.hash(&mut h);
    h.finish()
}

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
        KeyCode::Backspace => vec![0x08],
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
        KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],
        KeyCode::F(1) => vec![0x1b, b'O', b'P'],
        KeyCode::F(2) => vec![0x1b, b'O', b'Q'],
        KeyCode::F(3) => vec![0x1b, b'O', b'R'],
        KeyCode::F(4) => vec![0x1b, b'O', b'S'],
        KeyCode::F(5) => vec![0x1b, b'[', b'1', b'5', b'~'],
        KeyCode::F(6) => vec![0x1b, b'[', b'1', b'7', b'~'],
        KeyCode::F(7) => vec![0x1b, b'[', b'1', b'8', b'~'],
        KeyCode::F(8) => vec![0x1b, b'[', b'1', b'9', b'~'],
        KeyCode::F(9) => vec![0x1b, b'[', b'2', b'0', b'~'],
        KeyCode::F(10) => vec![0x1b, b'[', b'2', b'1', b'~'],
        KeyCode::F(11) => vec![0x1b, b'[', b'2', b'3', b'~'],
        KeyCode::F(12) => vec![0x1b, b'[', b'2', b'4', b'~'],
        KeyCode::BackTab => vec![0x1b, b'[', b'Z'],
        _ => vec![],
    }
}

// ── System stats collection (runs in background thread) ──────────

#[cfg(target_os = "macos")]
fn collect_sys_stats() -> (f64, u64, u64, Option<u8>, Option<String>) {
    use std::process::Command;
    let mut cpu: f64 = 0.0;
    let mut mem_used: u64 = 0;
    let mut mem_total: u64 = 0;
    let mut batt_pct: Option<u8> = None;
    let mut batt_status: Option<String> = None;

    // CPU
    if let Ok(out) = Command::new("top")
        .args(["-l", "1", "-n", "0", "-s", "0"])
        .output()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            if let Some(idx) = line.find("CPU usage") {
                if let Some(pct) = line[idx..].split_whitespace()
                    .find(|w| w.ends_with('%'))
                    .and_then(|w| w.trim_end_matches('%').parse::<f64>().ok())
                {
                    cpu = pct;
                }
                break;
            }
        }
    }
    // Memory
    if let Ok(out) = Command::new("vm_stat").output() {
        let s = String::from_utf8_lossy(&out.stdout);
        let mut page_size: u64 = 16384;
        let mut used: u64 = 0;
        for line in s.lines() {
            if line.contains("page size of") {
                page_size = line.split_whitespace().last()
                    .and_then(|n| n.parse().ok()).unwrap_or(16384);
            }
            if line.contains("Pages active") || line.contains("Pages wired") {
                if let Some(n) = line.split(':').last()
                    .and_then(|n| n.trim().trim_end_matches('.').parse::<u64>().ok())
                {
                    used += n;
                }
            }
        }
        mem_used = used * page_size;
        if let Ok(out) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
            mem_total = String::from_utf8_lossy(&out.stdout)
                .trim().parse().unwrap_or(0);
        }
    }
    // Battery
    if let Ok(out) = Command::new("pmset").args(["-g", "batt"]).output() {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            if let Some(pct) = line.split('%').next()
                .and_then(|p| p.split_whitespace().last())
                .and_then(|n| n.parse::<u8>().ok())
            {
                batt_pct = Some(pct);
                let status = if line.contains("discharging") { "discharging" }
                    else if line.contains("charging") { "charging" }
                    else if line.contains("AC") && pct == 100 { "full" }
                    else if line.contains("AC") { "on AC" }
                    else { "?" };
                batt_status = Some(status.to_string());
                break;
            }
        }
    }
    (cpu, mem_used, mem_total, batt_pct, batt_status)
}

#[cfg(target_os = "linux")]
fn collect_sys_stats() -> (f64, u64, u64, Option<u8>, Option<String>) {
    use std::fs;
    let mut mem_used: u64 = 0;
    let mut mem_total: u64 = 0;
    let mut batt_pct: Option<u8> = None;
    let mut batt_status: Option<String> = None;

    // CPU from /proc/stat.  A single sample is meaningless (the
    // counters are cumulative since boot) — take two samples ~120 ms
    // apart and compute the percentage from the deltas.
    let read_cpu = || -> Option<(u64, u64)> {
        let s = fs::read_to_string("/proc/stat").ok()?;
        let line = s.lines().next()?;
        let parts: Vec<u64> = line.split_whitespace().skip(1)
            .filter_map(|v| v.parse().ok()).collect();
        if parts.len() < 4 {
            return None;
        }
        let total: u64 = parts.iter().sum();
        Some((total, parts[3]))
    };
    let cpu: f64 = match (read_cpu(), {
        std::thread::sleep(std::time::Duration::from_millis(120));
        read_cpu()
    }) {
        (Some((t0, i0)), Some((t1, i1))) if t1 > t0 => {
            100.0 * (1.0 - (i1.saturating_sub(i0)) as f64 / (t1 - t0) as f64)
        }
        _ => 0.0,
    };

    // Memory from /proc/meminfo
    if let Ok(s) = fs::read_to_string("/proc/meminfo") {
        for line in s.lines() {
            if line.starts_with("MemTotal:") {
                mem_total = line.split_whitespace().nth(1)
                    .and_then(|v| v.parse().ok()).unwrap_or(0) * 1024;
            }
            if line.starts_with("MemAvailable:") {
                let avail = line.split_whitespace().nth(1)
                    .and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) * 1024;
                mem_used = mem_total.saturating_sub(avail);
            }
        }
    }

    // Battery from /sys/class/power_supply
    let batt_dirs = ["BAT0", "BAT1", "BATT"];
    for name in &batt_dirs {
        let base = format!("/sys/class/power_supply/{name}");
        if let Ok(pct) = fs::read_to_string(format!("{base}/capacity")) {
            batt_pct = pct.trim().parse().ok();
        }
        if let Ok(status) = fs::read_to_string(format!("{base}/status")) {
            batt_status = Some(status.trim().to_lowercase());
        }
        if batt_pct.is_some() { break; }
    }

    (cpu, mem_used, mem_total, batt_pct, batt_status)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn collect_sys_stats() -> (f64, u64, u64, Option<u8>, Option<String>) {
    (0.0, 0, 0, None, None)
}

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

fn run_go(path: &str) -> String {
    let output = ProcCmd::new("go").args(["run", path]).output();
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
        Err(e) => format!("Error running go: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn col_width_counts_cells_not_bytes() {
        assert_eq!(col_width("abc"), 3);
        assert_eq!(col_width("héllo"), 5); // 2-byte char, 1 cell
        assert_eq!(col_width("日本語"), 6); // 3 wide chars
        assert_eq!(col_width("🐍"), 2); // emoji is 2 cells
        assert_eq!(col_width("a\tb"), 9); // tab advances to col 8
        assert_eq!(col_width("e\u{301}"), 1); // combining mark is 0
    }

    #[test]
    fn byte_at_col_finds_char_boundaries() {
        let s = "héllo";
        assert_eq!(byte_at_col(s, 1), 1); // after 'h'
        assert_eq!(byte_at_col(s, 2), 3); // after 'é' (1 byte + 2 bytes)
        assert_eq!(byte_at_col(s, 99), s.len());
    }

    #[test]
    fn visible_spans_clips_and_expands_tabs() {
        let raw = "ab\tcd";
        let gutter = Span::styled("1│".to_string(), Style::new());

        // Window: columns [0, 4) → "ab  " (tab clipped at the edge).
        let spans = vec![
            gutter.clone(),
            Span::styled(raw.to_string(), Style::new()),
        ];
        let out = visible_spans(raw, spans, 0, 4);
        let text: String = out.iter().skip(1).map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "ab  ");

        // Window: columns [8, 12) → "cd" (tab is left of the window).
        let spans = vec![gutter, Span::styled(raw.to_string(), Style::new())];
        let out = visible_spans(raw, spans, byte_at_col(raw, 8), 4);
        let text: String = out.iter().skip(1).map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "cd");
    }

    #[test]
    fn visible_spans_drops_control_chars() {
        let raw = "a\x08b";
        let gutter = Span::styled("1│".to_string(), Style::new());
        let spans = vec![gutter, Span::styled(raw.to_string(), Style::new())];
        let out = visible_spans(raw, spans, 0, 8);
        let text: String = out.iter().skip(1).map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "ab");
    }

    #[test]
    fn undo_redo_roundtrip_restores_cursor() {
        let cfg = crate::config::Config::default();
        let mut ed = Editor::new(None, &cfg).unwrap();

        // One insert session: type "hello".
        ed.mode = Mode::Insert;
        ed.insert_saved = false;
        for ch in "hello".chars() {
            let _ = ed.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        ed.mode = Mode::Normal;
        assert_eq!(ed.lines[0], "hello");
        assert_eq!(ed.cx, 5);

        // u — undo the whole session at once.
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        assert!(ed.lines[0].is_empty());
        assert_eq!(ed.cx, 0);

        // Ctrl+R — redo it back (cursor restored too).
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert_eq!(ed.lines[0], "hello");
        assert_eq!(ed.cx, 5);

        // Undo again — redo stack must still round-trip.
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        assert!(ed.lines[0].is_empty());
    }

    #[test]
    fn ctrl_chars_do_not_insert_in_insert_mode() {
        let cfg = crate::config::Config::default();
        let mut ed = Editor::new(None, &cfg).unwrap();

        ed.mode = Mode::Insert;
        ed.insert_saved = false;
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert!(ed.lines[0].is_empty());

        // Ctrl+C exits insert mode back to normal.
        let keep = ed.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(keep);
        assert_eq!(ed.mode, Mode::Normal);
    }
}
