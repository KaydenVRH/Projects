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
//!
//! Layout:
//!   This file holds the types, the `Editor` struct and its core (buffers,
//!   key dispatch, undo, scrolling, clipboard). Each submodule adds its own
//!   `impl Editor` block for one concern; see the `mod` list below.

use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command as ProcCmd,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::cursor::SetCursorStyle;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::highlight;
use crate::shell::ShellProcess;
use crate::hermes::{Chat, Role};
use crate::config::Config;
use crate::theme::{Theme, ThemeKind};
use crate::finder::Finder;
use crate::keymap::{Keymap, Layer, Outcome};
use crate::memory::{self, MemoryStore};
use crate::music::MusicPlayer;
use crate::filetree::{FileTree, file_icon as ft_icon};

mod modes;
mod overlays;
mod pty;
mod hermes_pane;
mod commands;
mod images;
mod planner;
mod render;
mod panels;
mod chrome;
mod cells;
mod prompt;
mod markdown;
mod sysstats;
#[cfg(test)]
mod tests;

use panels::*;
use chrome::*;
use cells::*;
use prompt::*;
use markdown::*;
use sysstats::*;

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
    Hermes,  // Hermes chat pane (Ctrl+Shift+H, :hermes)
    Visual,  // visual mode: selecting text with movement keys
    Search,  // typing a / search query on the status line
    FileTree,// file tree panel (Ctrl+F)
    SysInfo, // system dashboard (:sys)
    Help,    // keybind manual (Ctrl+K)
    Memories,// reminders / notes dashboard
    Memo,    // typing a quick "remember" line
    Planner, // month calendar + day agenda
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

/// How a line changed in the last external reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChange {
    None,
    Added,
    Changed,
}

/// One rendered block of the markdown preview, with the source-line
/// range it came from (for scroll-following the cursor).
pub struct MdChunk {
    src_start: usize,
    lines: Vec<Line<'static>>,
}

/// The cached preview: either our own chunked render (with source
/// mapping) or glow's ANSI output (flattened lines).
pub enum PreviewRendered {
    Chunks(Vec<MdChunk>),
    Lines(Vec<Line<'static>>),
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

/// One row of the memories dashboard: a section header or an entry.
#[derive(Debug, Clone, Copy)]
pub enum MemRow {
    Header(&'static str),
    Entry(usize),
}

/// One cell of the planner month grid.
#[derive(Debug, Clone, Copy)]
pub struct DayCell {
    pub date: NaiveDate,
    pub in_month: bool,
    pub reminders: usize,
    pub notes: usize,
    pub is_today: bool,
}

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
    /// Byte cursor inside the active prompt (command / search /
    /// remember).  Shared because only one prompt is open at a time.
    pub prompt_cursor: usize,

    // ── search ──
    pub search_query: String,
    pub last_search: Option<String>,

    // ── external-change markers (from auto-reload) ──
    pub line_changes: Option<Vec<LineChange>>,
    /// When the last external reload happened (markers fade out).
    pub change_age: Instant,

    // ── markdown preview pane (Ctrl+V) ──
    pub preview: bool,
    /// Cached preview render, keyed by content hash (and pane width
    /// for glow output).
    pub preview_cache: Option<(u64, PreviewRendered)>,
    /// Is the `glow` CLI available?  Checked once, lazily.
    pub glow_available: Option<bool>,

    // ── syntax highlighter (tree-sitter, re-parsed on edits) ──
    pub highlight: Option<highlight::Highlighter>,

    // ── fuzzy finder ──
    pub finder: Finder,
    /// User-configurable key bindings.
    pub keymap: Keymap,

    // ── memories / reminders / journal ──
    pub memory: crate::memory::MemoryStore,
    pub mem_cfg: crate::config::MemoryConfig,
    pub mem_rows: Vec<MemRow>,
    pub mem_selected: usize,
    pub mem_scroll: usize,
    pub memo_buf: String,
    memo_origin: State,
    mem_last_check: Instant,
    /// Planner: displayed month (first day), selected day, grid cells,
    /// the selected day's entries, and which pane has focus.
    pub planner_month: NaiveDate,
    pub planner_day: NaiveDate,
    pub planner_days: Vec<DayCell>,
    pub planner_entries: Vec<usize>,
    pub planner_sel: usize,
    pub planner_list_focus: bool,
    pub finder_query: String,
    pub finder_selection: usize,
    pub finder_scroll: usize,

    // ── file tree (Ctrl+F) ──
    pub filetree: FileTree,

    // ── markdown inline images (kitty graphics protocol) ──
    pub graphics: crate::graphics::KittyGraphics,
    /// Render `![alt](path)` references as real images in the buffer.
    pub inline_images: bool,
    /// Currently placed (key → rect on screen) images.
    placed_images: HashMap<u32, (u16, u16, u16, u16)>,
    /// Images whose bytes were already transmitted this session.
    transmitted_images: HashSet<u32>,
    /// Kitty-graphics escape bytes queued by render(), flushed after
    /// the frame is drawn.
    graphics_out: Vec<u8>,
    /// Screen position of the editor cursor from the last frame.
    cursor_screen: Option<(u16, u16)>,

    // ── run output ──
    pub run_output: String,
    /// Lines scrolled up from the bottom of the run overlay.
    pub run_scroll: usize,

    // ── music player (Ctrl+M) ──
    pub music_player: MusicPlayer,

    // ── Hermes chat (Ctrl+Shift+H, `:hermes`) ──
    pub hermes: Chat,

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
    /// Fuzzy filter typed into the theme selector.
    pub theme_query: String,

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
            match fs::read_to_string(path) {
                Ok(content) => {
                    // Split into lines.  If the file ends with `\n`, push an
                    // extra empty line so the cursor can sit on the "virtual"
                    // line below the last content line (vim behaviour).
                    let mut lines: Vec<String> =
                        content.lines().map(|l| l.to_string()).collect();
                    if content.ends_with('\n') {
                        lines.push(String::new());
                    }
                    if lines.is_empty() {
                        lines.push(String::new());
                    }
                    let mtime =
                        fs::metadata(path).ok().and_then(|m| m.modified().ok());
                    (lines, Some(path.to_string()), mtime)
                }
                // Like vim: a path that doesn't exist yet starts as an
                // empty buffer and is created on `:w`.
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    (vec![String::new()], Some(path.to_string()), None)
                }
                Err(e) => return Err(e).with_context(|| format!("can't read {path}")),
            }
        } else {
            (vec![String::new()], None, None)
        };

        let theme = ThemeKind::from_str(&cfg.theme).unwrap_or(ThemeKind::Oxocarbon);
        let keymap = Keymap::from_config(cfg);
        let startup_flash = match keymap.warnings() {
            [] => None,
            [one] => Some(one.clone()),
            many => Some(format!(
                "{} keybind warnings — run :keys for details",
                many.len()
            )),
        };
        let mem_cfg = cfg.memories.clone();
        let memory = MemoryStore::load(memory::dir_for(&mem_cfg));
        let planner_day = memory::now().date();
        let planner_month = NaiveDate::from_ymd_opt(
            planner_day.year(),
            planner_day.month(),
            1,
        )
        .unwrap_or(planner_day);

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
            prompt_cursor: 0,
            search_query: String::new(),
            last_search: None,
            line_changes: None,
            change_age: Instant::now(),
            preview: false,
            preview_cache: None,
            glow_available: None,
            highlight: None,
            finder: Finder::new(),
            keymap,
            memory,
            mem_cfg,
            mem_rows: Vec::new(),
            mem_selected: 0,
            mem_scroll: 0,
            memo_buf: String::new(),
            memo_origin: State::Normal,
            mem_last_check: Instant::now(),
            planner_month,
            planner_day,
            planner_days: Vec::new(),
            planner_entries: Vec::new(),
            planner_sel: 0,
            planner_list_focus: false,
            finder_query: String::new(),
            finder_selection: 0,
            finder_scroll: 0,
            filetree: FileTree::new(),
            graphics: crate::graphics::KittyGraphics::unsupported(),
            inline_images: cfg.inline_images,
            placed_images: HashMap::new(),
            transmitted_images: HashSet::new(),
            graphics_out: Vec::new(),
            cursor_screen: None,
            run_output: String::new(),
            run_scroll: 0,
            music_player: MusicPlayer::new(),
            hermes: Chat::new(
                std::env::current_dir().unwrap_or_default(),
                cfg.hermes.command.clone(),
                cfg.hermes.model.clone(),
            ),
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
            theme_query: String::new(),
            flash: startup_flash,
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
        // Change markers belong to the buffer we just left.
        self.line_changes = None;
    }

    // ═══════════════════════════════════════════════════════════════
    //  Key dispatch
    // ═══════════════════════════════════════════════════════════════

    /// Handle a key press.  Returns `true` to keep running, `false` to quit.
    pub fn handle_key(&mut self, mut key: KeyEvent) -> bool {
        // If the shell overlay is active, forward every key to the
        // shell (including Ctrl+J, Ctrl+C, etc.).  The only way out
        // is to type `exit` in the shell.
        if self.state == State::Shell {
            return self.handle_shell_state(key);
        }

        // The Hermes pane owns every key while it is open: characters go into
        // the message line, Esc leaves, Ctrl+C stops a running turn.
        if self.state == State::Hermes {
            return self.handle_hermes_state(key);
        }

        // Clear any flash message on the next keypress.
        self.flash = None;

        // Apply user keybinding overrides.  Typed text (plain characters
        // in insert mode or an overlay) is left alone so a global binding
        // on a letter can't hijack typing.
        if !self.typing_context(&key) {
            match self.translate(Layer::Global, key) {
                Some(k) => key = k,
                None => return true,
            }
        }

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

        // Ctrl+T = theme selector (toggles from any state).
        if key.code == KeyCode::Char('t') && key.modifiers == KeyModifiers::CONTROL {
            if self.state == State::Theme {
                self.state = State::Normal;
            } else {
                self.state = State::Theme;
                self.theme_selected = 0;
                self.theme_scroll = 0;
                self.theme_query.clear();
            }
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
            self.open_shell();
            return true;
        }

        // Ctrl+Shift+H = Hermes chat (toggles). Terminals differ on how they
        // report the shift — some fold it into an uppercase character — so
        // accept either form; `:hermes` is always available.
        if matches!(key.code, KeyCode::Char('h') | KeyCode::Char('H'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.modifiers.contains(KeyModifiers::SHIFT)
                || matches!(key.code, KeyCode::Char('H')))
        {
            self.toggle_hermes();
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
            State::Memories => return self.handle_memories_state(key),
            State::Memo => return self.handle_memo_state(key),
            State::Planner => return self.handle_planner_state(key),
            State::Normal => {}
            State::Shell => {}
            State::Hermes => {}
            State::Visual => {}
        }

        // Global keybindings that work in both normal and insert modes
        // (and also visual mode, via the fall-through above):
        // Ctrl+P = finder, Ctrl+E = run, Ctrl+S = save, Ctrl+C = quit.
        // Ctrl+Shift+V = paste the clipboard image as markdown.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
        {
            match key.code {
                KeyCode::Char('v') | KeyCode::Char('V') => {
                    let _ = self.paste_clipboard_image();
                    return true;
                }
                KeyCode::Char('m') | KeyCode::Char('M') => {
                    self.open_memories();
                    return true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.start_quick_note(State::Normal);
                    return true;
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    self.open_planner();
                    return true;
                }
                _ => {}
            }
        }
        if key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('p') => {
                    self.state = State::Finder;
                    self.finder_query.clear();
                    self.finder_selection = 0;
                    self.finder_scroll = 0;
                    // Walk the tree in the background so the UI stays
                    // responsive in big directories.
                    self.finder.start_scan();
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
                    self.run_scroll = 0;
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
                KeyCode::Char('v') => {
                    // Toggle the markdown preview pane.
                    if highlight::detect_lang(
                        self.filename.as_deref(),
                        self.lines.first().map(|s| s.as_str()),
                    ) == highlight::Lang::Markdown
                    {
                        self.preview = !self.preview;
                        self.preview_cache = None;
                    } else {
                        self.flash = Some("preview: not a markdown file".to_string());
                    }
                    return true;
                }
                KeyCode::Char('k') => {
                    self.state = State::Help;
                    return true;
                }
                KeyCode::Char('l') => {
                    self.state = State::Command;
                    self.cmd_buf.clear();
                    self.prompt_cursor = 0;
                    return true;
                }
                _ => {}
            }
        }

        // Cmd/Option+arrows / Cmd/Option+hjkl: jump cursor (works in any mode).
        // A user binding in the current mode takes precedence.
        let mode_layer = if self.state == State::Visual {
            Layer::Visual
        } else {
            match self.mode {
                Mode::Normal => Layer::Normal,
                Mode::Insert => Layer::Insert,
            }
        };
        let mods = key.modifiers;
        if (mods.intersects(KeyModifiers::SUPER) || mods.intersects(KeyModifiers::ALT))
            && !self.keymap.bound_in_mode(mode_layer, &key)
        {
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

        // Dispatch by visual state or editing mode, applying the
        // configured bindings for that layer first.
        let Some(key) = self.translate(mode_layer, key) else {
            return true;
        };
        if self.state == State::Visual {
            return self.handle_visual_mode(key);
        }
        match self.mode {
            Mode::Normal => self.handle_normal_mode(key),
            Mode::Insert => self.handle_insert_mode(key),
        }
    }

    /// Translate a key through a keymap layer.
    /// `None` means the key was a reassigned default and must be swallowed.
    fn translate(&self, layer: Layer, key: KeyEvent) -> Option<KeyEvent> {
        let outcome = match layer {
            Layer::Global => self.keymap.global(&key),
            Layer::Normal => self.keymap.normal(&key),
            Layer::Visual => self.keymap.visual(&key),
            Layer::Insert => self.keymap.insert(&key),
        };
        match outcome {
            Outcome::Pass => Some(key),
            Outcome::Replace(k) => Some(k),
            Outcome::Swallow => None,
        }
    }

    /// Is this key being typed as text (so global letter bindings must
    /// not intercept it)?
    fn typing_context(&self, key: &KeyEvent) -> bool {
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
        if !plain {
            return false;
        }
        let texty = matches!(
            key.code,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace
                | KeyCode::Delete | KeyCode::Tab
        );
        if !texty {
            return false;
        }
        match self.state {
            State::Command
            | State::Search
            | State::Finder
            | State::Run
            | State::Theme
            | State::FileTree
            | State::Music
            | State::Memo => true,
            _ => self.mode == Mode::Insert,
        }
    }

    /// Poll the music-player thread for events (auto-next-track).
    /// Called every frame: polls music events and auto-reloads the
    /// file if it changed on disk (only when there are no unsaved edits).
    pub fn tick(&mut self) {
        self.music_player.poll();
        // Fold any Hermes chat events that arrived since the last frame.
        self.hermes.poll();
        // When the background file scan finishes, re-run the current
        // query so results appear without another keypress.
        if self.finder.poll() {
            let query = self.finder_query.clone();
            self.finder.search(&query);
            // Results may have shrunk: keep the selection in range.
            let max = self.finder.results.len().saturating_sub(1);
            self.finder_selection = min(self.finder_selection, max);
        }
        // Reminders: check once a second (never blocks the UI).
        if self.mem_last_check.elapsed() >= Duration::from_secs(1) {
            self.mem_last_check = Instant::now();
            self.check_reminders();
        }
        self.auto_reload();
        // Non-blocking system stats: poll completed task or spawn a
        // new one (only while the dashboard is open).
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
        } else if self.state == State::SysInfo && self.sys_updated.elapsed().as_secs() >= 3 {
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
        // Mark which lines changed so the right gutter can show it.
        let old_lines = std::mem::take(&mut self.lines);
        let changes = diff_lines(&old_lines, &new_lines);
        self.lines = new_lines;
        self.cy = saved_cy;
        self.cx = saved_cx;
        self.top = self.top.min(self.lines.len().saturating_sub(1));
        self.last_mtime = Some(current_mtime);
        if changes.len() == self.lines.len() {
            self.line_changes = Some(changes);
            self.change_age = Instant::now();
        }
        self.flash = Some("file changed on disk — reloaded".to_string());
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
    //  Scroll helpers
    // ═══════════════════════════════════════════════════════════════

    /// Absolute byte offset of a line's start within the joined
    /// highlighter source (`lines.join("\n")`).
    fn md_line_start(&self, line: usize) -> usize {
        self.lines[..line.min(self.lines.len())]
            .iter()
            .map(|l| l.len() + 1)
            .sum()
    }

    /// Visual column (terminal cells) of a line-relative byte offset
    /// on the current line, accounting for concealed markdown
    /// characters (which are revealed under the cursor).
    fn display_col(&self, byte: usize) -> usize {
        if let Some(h) = &self.highlight {
            if let Some(segs) = h.md_line(self.cy) {
                if !segs.is_empty() {
                    let abs = self.md_line_start(self.cy) + byte;
                    let effective = md_cursor_segs(segs, abs);
                    return md_segs_col(&effective, abs);
                }
                // Blank line: no segments — fall through to raw width.
            }
        }
        let line = &self.lines[self.cy];
        let b = line.floor_char_boundary(byte.min(line.len()));
        col_width(&line[..b])
    }

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
        let left_col = self.display_col(left);
        let cx_col = self.display_col(cx);
        if cx_col < left_col {
            self.left = cx;
        } else if cx_col >= left_col + visible_cols {
            let target = cx_col.saturating_sub(visible_cols) + 1;
            self.left = if let Some(segs) =
                self.highlight.as_ref().and_then(|h| h.md_line(self.cy))
            {
                if segs.is_empty() {
                    byte_at_col(line, target)
                } else {
                    let abs = self.md_line_start(self.cy) + self.cx;
                    let effective = md_cursor_segs(segs, abs);
                    md_segs_byte_at_col(&effective, target)
                }
            } else {
                byte_at_col(line, target)
            };
        }
    }

    /// Number of chars needed to print the highest line number.
    fn gutter_width(&self) -> usize {
        let total = self.lines.len();
        if total == 0 { 1 } else { total.to_string().len().max(2) }
    }

    /// Advance `cx` one full UTF-8 character to the right (or to end of line).
    /// On markdown lines, concealed marker runs are skipped as a unit
    /// so the cursor never sticks inside hidden text.
    fn right_cx(&self) -> usize {
        let line = &self.lines[self.cy];
        if self.cx >= line.len() { return self.cx; }
        let pos = if line.is_char_boundary(self.cx) { self.cx } else { line.floor_char_boundary(self.cx) };
        let ch = line[pos..].chars().next().unwrap();
        let next = (pos + ch.len_utf8()).min(line.len());
        self.skip_concealed_right(next)
    }

    /// Retreat `cx` one full UTF-8 character to the left (or to column 0).
    fn left_cx(&self) -> usize {
        if self.cx == 0 { return 0; }
        let line = &self.lines[self.cy];
        let pos = if line.is_char_boundary(self.cx) { self.cx } else { line.floor_char_boundary(self.cx) };
        if let Some(ch) = line[..pos].chars().next_back() {
            let next = pos - ch.len_utf8();
            self.skip_concealed_left(next)
        } else {
            0
        }
    }

    /// If `next` lands strictly inside a concealed (width-changing)
    /// markdown segment, jump past it instead.
    fn skip_concealed_right(&self, next: usize) -> usize {
        if let Some(segs) = self.highlight.as_ref().and_then(|h| h.md_line(self.cy)) {
            for s in segs {
                if s.display != s.raw && s.start < next && next < s.end {
                    return s.end;
                }
            }
        }
        next
    }

    fn skip_concealed_left(&self, next: usize) -> usize {
        if let Some(segs) = self.highlight.as_ref().and_then(|h| h.md_line(self.cy)) {
            for s in segs {
                if s.display != s.raw && s.start < next && next < s.end {
                    return s.start;
                }
            }
        }
        next
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
        // The user is editing: external-change markers no longer
        // describe the buffer.
        self.line_changes = None;
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

    // ── clipboard ──────────────────────────────────────────────

    /// Store text in the internal clipboard and push it to the system
    /// clipboard via OSC 52 (kitty, wezterm, foot, iTerm2, …).
    fn set_clipboard(&mut self, text: String) {
        self.clipboard_text = text;
        osc52_copy(&self.clipboard_text);
    }

    /// Paste `text` after the cursor (`p`): multi-line text becomes
    /// whole lines below the current one; single-line text inserts at
    /// the cursor.
    fn paste_after(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.save_undo();
        if text.contains('\n') {
            let pasted_lines: Vec<&str> = text.split('\n').collect();
            for (i, line) in pasted_lines.iter().enumerate() {
                self.lines.insert(self.cy + 1 + i, line.to_string());
            }
            self.cy += pasted_lines.len();
            self.cx = 0;
        } else {
            let line = &mut self.lines[self.cy];
            line.insert_str(self.cx, text);
            self.cx += text.len();
        }
        self.modified = true;
        self.clamp_scroll();
    }

    /// Paste `text` before the cursor (`P`).
    fn paste_before(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.save_undo();
        if text.contains('\n') {
            let pasted_lines: Vec<&str> = text.split('\n').collect();
            for (i, line) in pasted_lines.iter().enumerate() {
                self.lines.insert(self.cy + i, line.to_string());
            }
            self.cx = 0;
        } else {
            let line = &mut self.lines[self.cy];
            line.insert_str(self.cx, text);
            self.cx += text.len();
        }
        self.modified = true;
        self.clamp_scroll();
    }

    /// Handle a bracketed paste from the terminal: the text is
    /// inserted literally — never interpreted as keypresses.  In
    /// insert mode it is typed at the cursor; in normal / visual mode
    /// it pastes after the cursor like `p`.  Ignored while an overlay
    /// is open.
    pub fn paste_from_terminal(&mut self, raw: &str) {
        // In the shell overlay, pastes belong to the shell.
        if self.state == State::Shell {
            if let Some(s) = self.shell.as_ref() {
                if s.bracketed_paste() {
                    s.write(b"\x1b[200~");
                    s.write(raw.as_bytes());
                    s.write(b"\x1b[201~");
                } else {
                    s.write(raw.as_bytes());
                }
            }
            return;
        }
        if self.state != State::Normal && self.state != State::Visual {
            return;
        }
        // Normalise Windows line endings.
        let text = raw.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return;
        }
        if self.mode == Mode::Insert {
            self.save_undo_if_needed();
            let mut parts = text.split('\n');
            let first = parts.next().unwrap_or("");
            {
                let line = &mut self.lines[self.cy];
                let pos = line.floor_char_boundary(self.cx);
                line.insert_str(pos, first);
                self.cx = pos + first.len();
            }
            for part in parts {
                let rest = self.lines[self.cy].split_off(self.cx);
                self.lines.insert(self.cy + 1, part.to_string() + &rest);
                self.cy += 1;
                self.cx = part.len();
            }
            self.modified = true;
            self.clamp_scroll();
        } else {
            if self.state == State::Visual {
                self.state = State::Normal;
                self.selection_anchor = None;
            }
            self.paste_after(&text);
        }
    }

    // ── markdown inline images (kitty graphics protocol) ───────

    /// Open a file from disk in a new buffer (used by the memories UI).
    fn open_path(&mut self, path: &Path) {
        match fs::read_to_string(path) {
            Ok(content) => {
                let mut lines: Vec<String> =
                    content.lines().map(|l| l.to_string()).collect();
                if content.ends_with('\n') {
                    lines.push(String::new());
                }
                if lines.is_empty() {
                    lines.push(String::new());
                }
                let fname = path.to_string_lossy().to_string();
                let mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
                self.sync_to_buffer();
                self.buffers
                    .push(Buffer::new(lines, Some(fname), mtime));
                self.current = self.buffers.len() - 1;
                self.sync_from_buffer(self.current);
            }
            Err(e) => {
                self.flash = Some(format!("can't open {}: {e}", path.display()));
            }
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
}

/// Tell the terminal to copy `text` to the system clipboard via
/// OSC 52 (works in kitty, wezterm, foot, iTerm2, …; other terminals
/// ignore it).  We only ever write — never query — so there is no
/// way for a terminal to feed us data through it.
fn osc52_copy(text: &str) {
    use std::io::Write;
    let b64 = base64_encode(text.as_bytes());
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{}\x1b\\", b64);
    let _ = out.flush();
}

/// Minimal base64 encoder (for OSC 52 payloads).
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
