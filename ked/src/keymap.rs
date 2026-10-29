//! Configurable key bindings.
//!
//! Every editing action has a built-in default (the keys ked has always
//! used).  A `[keys]` table in `~/.config/ked/config.toml` overrides any
//! of them, keyed by action name:
//!
//! ```toml
//! [keys]
//! save    = "ctrl+s"
//! finder  = "f2"
//! left    = "a"        # shared by normal and visual mode
//! quit    = "ctrl+q"
//! ```
//!
//! Binding an action **replaces** its built-in key(s), so the old key
//! becomes unbound (it won't accidentally trigger the old action).
//! Actions shared between normal and visual mode (`left`, `word_next`,
//! the insert/open actions, …) change in both.
//!
//! Keys are written as optional modifiers plus one key:
//! `ctrl+alt+shift+<key>`, where `<key>` is a single character (case
//! matters: `G` means shift+g) or one of `tab`, `backtab`, `enter`,
//! `esc`, `space`, `backspace`, `delete`, `insert`, `home`, `end`,
//! `pageup`, `pagedown`, `left`, `right`, `up`, `down`, `f1`…`f12`.
//!
//! Run `:keys` inside ked to see the effective bindings.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Config;

// ── key specs ────────────────────────────────────────────────────

/// One physical key + modifier combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpec {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeySpec {
    /// Does this spec match an incoming key event?
    ///
    /// Shift is compared only for alphabetic characters (where `G` vs
    /// `g` is meaningful) and named keys.  For symbol characters the
    /// character itself encodes the shift (`$` vs `4`), and terminals
    /// disagree about also setting the SHIFT flag — so it is ignored.
    pub fn matches(&self, e: &KeyEvent) -> bool {
        let code_eq = match (self.code, e.code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => {
                a.to_ascii_lowercase() == b.to_ascii_lowercase()
            }
            (a, b) => a == b,
        };
        if !code_eq {
            return false;
        }
        let base = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER;
        if (self.mods & base) != (e.modifiers & base) {
            return false;
        }
        let shift_matters = match self.code {
            KeyCode::Char(c) => c.is_alphabetic(),
            _ => true,
        };
        if !shift_matters {
            return true;
        }
        let self_shift = self.mods.contains(KeyModifiers::SHIFT)
            || matches!(self.code, KeyCode::Char(c) if c.is_uppercase());
        let e_shift = e.modifiers.contains(KeyModifiers::SHIFT)
            || matches!(e.code, KeyCode::Char(c) if c.is_uppercase());
        self_shift == e_shift
    }

    /// Build the key event the built-in handler understands.
    pub fn to_event(&self) -> KeyEvent {
        KeyEvent::new(self.code, self.mods)
    }

    /// Human-readable form, e.g. `ctrl+shift+v`.
    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            s.push_str("ctrl+");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            s.push_str("alt+");
        }
        if self.mods.contains(KeyModifiers::SUPER) {
            s.push_str("super+");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            s.push_str("shift+");
        }
        s.push_str(&match self.code {
            KeyCode::Char(' ') => "space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Tab => "tab".to_string(),
            KeyCode::BackTab => "backtab".to_string(),
            KeyCode::Enter => "enter".to_string(),
            KeyCode::Esc => "esc".to_string(),
            KeyCode::Backspace => "backspace".to_string(),
            KeyCode::Delete => "delete".to_string(),
            KeyCode::Insert => "insert".to_string(),
            KeyCode::Home => "home".to_string(),
            KeyCode::End => "end".to_string(),
            KeyCode::PageUp => "pageup".to_string(),
            KeyCode::PageDown => "pagedown".to_string(),
            KeyCode::Left => "left".to_string(),
            KeyCode::Right => "right".to_string(),
            KeyCode::Up => "up".to_string(),
            KeyCode::Down => "down".to_string(),
            KeyCode::F(n) => format!("f{n}"),
            other => format!("{other:?}"),
        });
        s
    }
}

/// Parse a key spec such as `ctrl+shift+v`, `tab`, `G` or `f5`.
pub fn parse_key(raw: &str) -> Result<KeySpec, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty key".to_string());
    }
    // Split on '+' but keep a literal trailing '+' usable (`ctrl++`).
    let mut parts: Vec<&str> = raw.split('+').map(str::trim).collect();
    let key_part = if raw.ends_with('+') && parts.len() >= 2 {
        let last = parts.pop().unwrap_or("");
        let _ = last;
        "+"
    } else {
        parts.pop().ok_or("empty key")?
    };

    let mut mods = KeyModifiers::NONE;
    for p in &parts {
        if p.is_empty() {
            continue;
        }
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
            "shift" => mods |= KeyModifiers::SHIFT,
            "alt" | "opt" | "option" => mods |= KeyModifiers::ALT,
            "super" | "cmd" | "command" | "meta" | "win" => mods |= KeyModifiers::SUPER,
            other => return Err(format!("unknown modifier `{other}`")),
        }
    }

    let lp = key_part.to_ascii_lowercase();
    let mut code = match lp.as_str() {
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "enter" | "return" | "cr" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "space" | " " => KeyCode::Char(' '),
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10"
        | "f11" | "f12" => KeyCode::F(lp[1..].parse().unwrap_or(1)),
        _ => {
            let mut chars = key_part.chars();
            let c = chars.next().ok_or("missing key")?;
            if chars.next().is_some() {
                return Err(format!(
                    "`{key_part}` is not a single key (multi-key sequences aren't supported)"
                ));
            }
            KeyCode::Char(c)
        }
    };

    // Shift+Tab is reported by terminals as a distinct BackTab code.
    if matches!(code, KeyCode::Tab) && mods.contains(KeyModifiers::SHIFT) {
        code = KeyCode::BackTab;
        mods.remove(KeyModifiers::SHIFT);
    }
    Ok(KeySpec { code, mods })
}

// ── actions ──────────────────────────────────────────────────────

/// An editing action that can be bound to a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // global
    Finder, Run, Save, Quit, Preview, Help, Command, PasteImage,
    Music, Theme, Shell, FileTree,
    Memories, QuickNote, Planner,
    // motion (normal + visual)
    Left, Down, Up, Right, WordNext, WordPrev, LineStart, LineEnd,
    HalfPageUp, HalfPageDown,
    // motion (normal only)
    PageUp, PageDown,
    // normal mode
    GotoFirst, GotoLast, Visual,
    InsertBefore, InsertAfter, InsertLineStart, InsertLineEnd,
    OpenBelow, OpenAbove, DeleteChar, DeleteLine, YankLine,
    PasteAfter, PasteBefore, Undo, Redo,
    CommandMode, Search, SearchNext, SearchPrev, BufferNext, BufferPrev,
    // insert mode
    Escape, PasteText,
    // visual mode
    VisualExit, VisualYank, VisualDelete,
}

/// Which handler layer a binding lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Global,
    Normal,
    Visual,
    Insert,
}

struct ActionSpec {
    action: Action,
    name: &'static str,
    layers: &'static [Layer],
    defaults: &'static [&'static str],
}

use Action::*;
use Layer::Global as LGlobal;
use Layer::Normal as LNormal;
use Layer::Visual as LVisual;
use Layer::Insert as LInsert;

const ACTIONS: &[ActionSpec] = &[
    // ── global ──
    ActionSpec { action: Finder,     name: "finder",      layers: &[LGlobal], defaults: &["ctrl+p"] },
    ActionSpec { action: Run,        name: "run",         layers: &[LGlobal], defaults: &["ctrl+e"] },
    ActionSpec { action: Save,       name: "save",        layers: &[LGlobal], defaults: &["ctrl+s"] },
    ActionSpec { action: Quit,       name: "quit",        layers: &[LGlobal], defaults: &["ctrl+c"] },
    ActionSpec { action: Preview,    name: "preview",     layers: &[LGlobal], defaults: &["ctrl+v"] },
    ActionSpec { action: Help,       name: "help",        layers: &[LGlobal], defaults: &["ctrl+k"] },
    ActionSpec { action: Command,    name: "command",     layers: &[LGlobal], defaults: &["ctrl+l"] },
    ActionSpec { action: PasteImage, name: "paste_image", layers: &[LGlobal], defaults: &["ctrl+shift+v"] },
    ActionSpec { action: Music,      name: "music",       layers: &[LGlobal], defaults: &["ctrl+m"] },
    ActionSpec { action: Theme,      name: "theme",       layers: &[LGlobal], defaults: &["ctrl+t"] },
    ActionSpec { action: Shell,      name: "shell",       layers: &[LGlobal], defaults: &["ctrl+j"] },
    ActionSpec { action: FileTree,   name: "filetree",    layers: &[LGlobal], defaults: &["ctrl+f"] },
    ActionSpec { action: Memories,   name: "memories",    layers: &[LGlobal], defaults: &["ctrl+shift+m"] },
    ActionSpec { action: QuickNote,  name: "quick_note",  layers: &[LGlobal], defaults: &["ctrl+shift+n"] },
    ActionSpec { action: Planner,    name: "planner",     layers: &[LGlobal], defaults: &["ctrl+shift+a"] },

    // ── motion (shared normal + visual) ──
    ActionSpec { action: Left,         name: "left",           layers: &[LNormal, LVisual], defaults: &["h"] },
    ActionSpec { action: Down,         name: "down",           layers: &[LNormal, LVisual], defaults: &["j"] },
    ActionSpec { action: Up,           name: "up",             layers: &[LNormal, LVisual], defaults: &["k"] },
    ActionSpec { action: Right,        name: "right",          layers: &[LNormal, LVisual], defaults: &["l"] },
    ActionSpec { action: WordNext,     name: "word_next",      layers: &[LNormal, LVisual], defaults: &["w"] },
    ActionSpec { action: WordPrev,     name: "word_prev",      layers: &[LNormal, LVisual], defaults: &["b"] },
    ActionSpec { action: LineStart,    name: "line_start",     layers: &[LNormal, LVisual], defaults: &["0"] },
    ActionSpec { action: LineEnd,      name: "line_end",       layers: &[LNormal, LVisual], defaults: &["$"] },
    ActionSpec { action: HalfPageUp,   name: "half_page_up",   layers: &[LNormal, LVisual], defaults: &["ctrl+u"] },
    ActionSpec { action: HalfPageDown, name: "half_page_down", layers: &[LNormal, LVisual], defaults: &["ctrl+d"] },

    // ── motion (normal only) ──
    ActionSpec { action: PageUp,   name: "page_up",   layers: &[LNormal], defaults: &["pageup"] },
    ActionSpec { action: PageDown, name: "page_down", layers: &[LNormal], defaults: &["pagedown"] },

    // ── normal mode ──
    ActionSpec { action: GotoFirst,         name: "goto_first",          layers: &[LNormal], defaults: &["g"] },
    ActionSpec { action: GotoLast,          name: "goto_last",           layers: &[LNormal], defaults: &["G"] },
    ActionSpec { action: Visual,            name: "visual",              layers: &[LNormal], defaults: &["v"] },
    ActionSpec { action: InsertBefore,      name: "insert_before",       layers: &[LNormal, LVisual], defaults: &["i"] },
    ActionSpec { action: InsertAfter,       name: "insert_after",        layers: &[LNormal, LVisual], defaults: &["a"] },
    ActionSpec { action: InsertLineStart,   name: "insert_line_start",   layers: &[LNormal, LVisual], defaults: &["I"] },
    ActionSpec { action: InsertLineEnd,     name: "insert_line_end",     layers: &[LNormal, LVisual], defaults: &["A"] },
    ActionSpec { action: OpenBelow,         name: "open_below",          layers: &[LNormal], defaults: &["o"] },
    ActionSpec { action: OpenAbove,         name: "open_above",          layers: &[LNormal], defaults: &["O"] },
    ActionSpec { action: DeleteChar,        name: "delete_char",         layers: &[LNormal], defaults: &["x"] },
    ActionSpec { action: DeleteLine,        name: "delete_line",         layers: &[LNormal], defaults: &["d"] },
    ActionSpec { action: YankLine,          name: "yank_line",           layers: &[LNormal], defaults: &["y"] },
    ActionSpec { action: PasteAfter,        name: "paste_after",         layers: &[LNormal], defaults: &["p"] },
    ActionSpec { action: PasteBefore,       name: "paste_before",        layers: &[LNormal], defaults: &["P"] },
    ActionSpec { action: Undo,              name: "undo",                layers: &[LNormal], defaults: &["u"] },
    ActionSpec { action: Redo,              name: "redo",                layers: &[LNormal], defaults: &["ctrl+r"] },
    ActionSpec { action: CommandMode,       name: "command_mode",        layers: &[LNormal], defaults: &[":"] },
    ActionSpec { action: Search,            name: "search",              layers: &[LNormal], defaults: &["/"] },
    ActionSpec { action: SearchNext,        name: "search_next",         layers: &[LNormal], defaults: &["n"] },
    ActionSpec { action: SearchPrev,        name: "search_prev",         layers: &[LNormal], defaults: &["N"] },
    ActionSpec { action: BufferNext,        name: "buffer_next",         layers: &[LNormal], defaults: &["tab"] },
    ActionSpec { action: BufferPrev,        name: "buffer_prev",         layers: &[LNormal], defaults: &["shift+tab"] },

    // ── insert mode ──
    ActionSpec { action: Escape,    name: "escape",     layers: &[LInsert], defaults: &["esc"] },
    ActionSpec { action: PasteText, name: "paste_text", layers: &[LInsert], defaults: &["ctrl+a"] },

    // ── visual mode ──
    ActionSpec { action: VisualExit,   name: "visual_exit",   layers: &[LVisual], defaults: &["esc", "v"] },
    ActionSpec { action: VisualYank,   name: "visual_yank",   layers: &[LVisual], defaults: &["y", "c"] },
    ActionSpec { action: VisualDelete, name: "visual_delete", layers: &[LVisual], defaults: &["d"] },
];

// ── lookup tables ────────────────────────────────────────────────

/// Result of feeding a key through a layer.
pub enum Outcome {
    /// No binding involved — dispatch the key unchanged.
    Pass,
    /// The key is bound to an action; dispatch this built-in key.
    Replace(KeyEvent),
    /// The key was a built-in default that the user reassigned —
    /// swallow it so the old action doesn't fire.
    Swallow,
}

#[derive(Default)]
struct LayerMap {
    /// Active bindings (config override, or the built-in defaults).
    triggers: Vec<(KeySpec, Action)>,
    /// Built-in defaults, used to suppress keys that were remapped away.
    defaults: Vec<(KeySpec, Action)>,
    /// The first built-in key for each action — what we translate to.
    primary: Vec<(Action, KeySpec)>,
}

impl LayerMap {
    fn primary_for(&self, action: Action) -> Option<KeySpec> {
        self.primary
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, k)| *k)
    }

    fn lookup(&self, key: &KeyEvent) -> Outcome {
        if let Some((_, action)) = self.triggers.iter().find(|(s, _)| s.matches(key)) {
            return match self.primary_for(*action) {
                Some(primary) if !primary.matches(key) => Outcome::Replace(primary.to_event()),
                _ => Outcome::Pass,
            };
        }
        if self.defaults.iter().any(|(s, _)| s.matches(key)) {
            return Outcome::Swallow;
        }
        Outcome::Pass
    }

    fn has(&self, key: &KeyEvent) -> bool {
        self.triggers.iter().any(|(s, _)| s.matches(key))
    }
}

/// The effective keymap for every layer.
pub struct Keymap {
    global: LayerMap,
    normal: LayerMap,
    visual: LayerMap,
    insert: LayerMap,
    warnings: Vec<String>,
}

impl Keymap {
    /// Build the keymap from the built-in defaults plus `[keys]` overrides.
    pub fn from_config(cfg: &Config) -> Self {
        let mut warnings = Vec::new();

        let mut overrides: HashMap<String, KeySpec> = HashMap::new();
        for (name, spec) in &cfg.keys {
            let norm = normalise(name);
            if !ACTIONS.iter().any(|a| normalise(a.name) == norm) {
                warnings.push(format!("unknown keybind action `{name}`"));
                continue;
            }
            match parse_key(spec) {
                Ok(k) => {
                    overrides.insert(norm, k);
                }
                Err(e) => warnings.push(format!("keybind `{name}`: {e}")),
            }
        }

        let mut global = LayerMap::default();
        let mut normal = LayerMap::default();
        let mut visual = LayerMap::default();
        let mut insert = LayerMap::default();

        for spec in ACTIONS {
            let defaults: Vec<KeySpec> = spec
                .defaults
                .iter()
                .filter_map(|s| parse_key(s).ok())
                .collect();
            let Some(primary) = defaults.first().copied() else {
                continue;
            };
            let triggers: Vec<KeySpec> = match overrides.get(&normalise(spec.name)) {
                Some(k) => vec![*k],
                None => defaults.clone(),
            };
            for layer in spec.layers {
                let lm = match layer {
                    LGlobal => &mut global,
                    LNormal => &mut normal,
                    LVisual => &mut visual,
                    LInsert => &mut insert,
                };
                for d in &defaults {
                    lm.defaults.push((*d, spec.action));
                }
                for t in &triggers {
                    lm.triggers.push((*t, spec.action));
                }
                lm.primary.push((spec.action, primary));
            }
        }

        // Report keys claimed by two different actions in one layer.
        for (layer_name, lm) in [
            ("global", &global),
            ("normal", &normal),
            ("visual", &visual),
            ("insert", &insert),
        ] {
            for i in 0..lm.triggers.len() {
                for j in (i + 1)..lm.triggers.len() {
                    let (ka, aa) = lm.triggers[i];
                    let (kb, ab) = lm.triggers[j];
                    if aa != ab && ka.matches(&kb.to_event()) {
                        let msg = format!(
                            "{layer_name}: `{}` is bound to both `{}` and `{}`",
                            ka.display(),
                            action_name(aa),
                            action_name(ab)
                        );
                        if !warnings.contains(&msg) {
                            warnings.push(msg);
                        }
                    }
                }
            }
        }

        Self { global, normal, visual, insert, warnings }
    }

    /// Translate a key through the global layer.
    pub fn global(&self, key: &KeyEvent) -> Outcome {
        self.global.lookup(key)
    }

    /// Translate a key through the normal-mode layer.
    pub fn normal(&self, key: &KeyEvent) -> Outcome {
        self.normal.lookup(key)
    }

    /// Translate a key through the visual-mode layer.
    pub fn visual(&self, key: &KeyEvent) -> Outcome {
        self.visual.lookup(key)
    }

    /// Translate a key through the insert-mode layer.
    pub fn insert(&self, key: &KeyEvent) -> Outcome {
        self.insert.lookup(key)
    }

    /// Is this key explicitly bound in normal/visual/insert mode?  Used
    /// to let user bindings win over the hard-wired ⌥/⌘+arrow jumps.
    pub fn bound_in_mode(&self, layer: Layer, key: &KeyEvent) -> bool {
        match layer {
            LGlobal => self.global.has(key),
            LNormal => self.normal.has(key),
            LVisual => self.visual.has(key),
            LInsert => self.insert.has(key),
        }
    }

    /// Configuration problems found while building the map.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The first configured key for an action, for UI hints.
    pub fn key(&self, name: &str) -> String {
        let norm = normalise(name);
        let Some(spec) = ACTIONS.iter().find(|a| normalise(a.name) == norm) else {
            return format!("?{name}");
        };
        let lm = match spec.layers.first().copied().unwrap_or(Layer::Global) {
            LGlobal => &self.global,
            LNormal => &self.normal,
            LVisual => &self.visual,
            LInsert => &self.insert,
        };
        lm.triggers
            .iter()
            .find(|(_, a)| *a == spec.action)
            .map(|(k, _)| k.display())
            .unwrap_or_else(|| "—".to_string())
    }

    /// Human-readable dump for `:keys`.
    pub fn describe(&self) -> String {
        let mut s = String::new();
        if !self.warnings.is_empty() {
            s.push_str("keybind warnings:\n");
            for w in &self.warnings {
                s.push_str(&format!("  ! {w}\n"));
            }
            s.push('\n');
        }
        for (label, lm) in [
            ("global", &self.global),
            ("normal", &self.normal),
            ("visual", &self.visual),
            ("insert", &self.insert),
        ] {
            s.push_str(&format!("[{label}]\n"));
            for spec in ACTIONS {
                if !spec.layers.contains(&match label {
                    "global" => LGlobal,
                    "normal" => LNormal,
                    "visual" => LVisual,
                    _ => LInsert,
                }) {
                    continue;
                }
                let keys: Vec<String> = lm
                    .triggers
                    .iter()
                    .filter(|(_, a)| *a == spec.action)
                    .map(|(k, _)| k.display())
                    .collect();
                if !keys.is_empty() {
                    s.push_str(&format!("  {:<20} {}\n", spec.name, keys.join(", ")));
                }
            }
            s.push('\n');
        }
        s
    }
}

fn normalise(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('-', "_")
}

fn group_of(layers: &[Layer]) -> usize {
    if layers.contains(&LGlobal) {
        0
    } else if layers.contains(&LNormal) && layers.contains(&LVisual) {
        1
    } else if layers.contains(&LNormal) {
        2
    } else if layers.contains(&LVisual) {
        3
    } else {
        4
    }
}

/// A commented `[keys]` reference block showing every action and its
/// current built-in binding.  Paste it into `config.toml` and
/// uncomment the lines you want to change; the defaults stay in effect
/// for everything commented out.
pub fn template() -> String {
    const GROUPS: [&str; 5] = [
        "global (any mode)",
        "normal + visual",
        "normal mode",
        "visual mode only",
        "insert mode",
    ];
    let mut s = String::new();
    s.push_str("# ── keybindings ───────────────────────────────────────────\n");
    s.push_str("# Every action and its built-in key.  Uncomment a line and\n");
    s.push_str("# change the key to override it; the old key becomes unbound.\n");
    s.push_str("# Actions shared by normal/visual move in both modes.\n");
    s.push_str("# Run `:keys` in ked (or `ked --print-keys`) to see the live map.\n");
    s.push_str("#\n");
    s.push_str("# Examples:\n");
    s.push_str("#   save        = \"f2\"\n");
    s.push_str("#   finder      = \"alt+p\"\n");
    s.push_str("#   quit        = \"ctrl+q\"\n");
    s.push_str("#   left        = \"a\"          # normal + visual\n");
    s.push_str("#   delete_line = \"D\"\n");
    s.push_str("#   search      = \"/\"\n");
    s.push_str("[keys]\n");
    for (g, label) in GROUPS.iter().enumerate() {
        let mut printed_header = false;
        for spec in ACTIONS {
            if group_of(spec.layers) != g {
                continue;
            }
            if !printed_header {
                s.push_str(&format!("\n# ── {label} ──\n"));
                printed_header = true;
            }
            let primary = spec.defaults.first().copied().unwrap_or("");
            let extra = if spec.defaults.len() > 1 {
                format!("   # built-in also: {}", spec.defaults[1..].join(", "))
            } else {
                String::new()
            };
            let line = format!(
                "# {:<20} = {:<14}{}",
                spec.name,
                format!("\"{}\"", primary),
                extra
            );
            s.push_str(line.trim_end());
            s.push('\n');
        }
    }
    s
}

fn action_name(action: Action) -> &'static str {
    ACTIONS
        .iter()
        .find(|a| a.action == action)
        .map(|a| a.name)
        .unwrap_or("?")
}

// ── tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn cfg_with(pairs: &[(&str, &str)]) -> Config {
        let mut cfg = Config::default();
        for (k, v) in pairs {
            cfg.keys.insert(k.to_string(), v.to_string());
        }
        cfg
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn press_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn parses_key_specs() {
        assert_eq!(
            parse_key("ctrl+s").unwrap(),
            KeySpec { code: KeyCode::Char('s'), mods: KeyModifiers::CONTROL }
        );
        assert_eq!(
            parse_key("shift+tab").unwrap(),
            KeySpec { code: KeyCode::BackTab, mods: KeyModifiers::NONE }
        );
        assert_eq!(
            parse_key("ctrl+shift+v").unwrap(),
            KeySpec {
                code: KeyCode::Char('v'),
                mods: KeyModifiers::CONTROL | KeyModifiers::SHIFT
            }
        );
        assert_eq!(parse_key("f5").unwrap().code, KeyCode::F(5));
        assert_eq!(parse_key("space").unwrap().code, KeyCode::Char(' '));
        assert!(parse_key("ctrl+nope").is_err());
        assert!(parse_key("ab").is_err());
        assert!(parse_key("").is_err());
    }

    #[test]
    fn shift_is_inferred_from_uppercase() {
        let g = parse_key("G").unwrap();
        // Shift+G may arrive as Char('G') with or without the SHIFT flag.
        assert!(g.matches(&press(KeyCode::Char('G'))));
        assert!(g.matches(&press_mod(KeyCode::Char('G'), KeyModifiers::SHIFT)));
        // ... but plain g must not match.
        assert!(!g.matches(&press(KeyCode::Char('g'))));
    }

    #[test]
    fn ctrl_shift_v_is_distinct_from_ctrl_v() {
        let paste = parse_key("ctrl+shift+v").unwrap();
        let preview = parse_key("ctrl+v").unwrap();
        let ev_shift = press_mod(KeyCode::Char('V'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let ev_plain = press_mod(KeyCode::Char('v'), KeyModifiers::CONTROL);
        assert!(paste.matches(&ev_shift));
        assert!(!paste.matches(&ev_plain));
        assert!(preview.matches(&ev_plain));
        assert!(!preview.matches(&ev_shift));
    }

    #[test]
    fn symbol_keys_ignore_the_shift_flag() {
        // Terminals differ on whether `$` arrives with SHIFT set.
        let dollar = parse_key("$").unwrap();
        assert!(dollar.matches(&press(KeyCode::Char('$'))));
        assert!(dollar.matches(&press_mod(
            KeyCode::Char('$'),
            KeyModifiers::SHIFT
        )));
        let colon = parse_key(":").unwrap();
        assert!(colon.matches(&press_mod(
            KeyCode::Char(':'),
            KeyModifiers::SHIFT
        )));
    }

    #[test]
    fn defaults_pass_through_unchanged() {
        let km = Keymap::from_config(&Config::default());
        assert!(matches!(
            km.global(&press_mod(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            Outcome::Pass
        ));
        assert!(matches!(km.normal(&press(KeyCode::Char('h'))), Outcome::Pass));
        assert!(matches!(km.normal(&press(KeyCode::Char('G'))), Outcome::Pass));
        assert!(matches!(km.insert(&press(KeyCode::Esc)), Outcome::Pass));
        // Unbound keys still pass (arrows are hard-wired by the handler).
        assert!(matches!(km.normal(&press(KeyCode::Left)), Outcome::Pass));
    }

    #[test]
    fn rebinding_translates_and_unbinds_old_key() {
        let km = Keymap::from_config(&cfg_with(&[("save", "f2")]));
        match km.global(&press(KeyCode::F(2))) {
            Outcome::Replace(ev) => {
                assert_eq!(ev.code, KeyCode::Char('s'));
                assert_eq!(ev.modifiers, KeyModifiers::CONTROL);
            }
            _ => panic!("F2 should translate to the built-in Ctrl+S"),
        }
        assert!(matches!(
            km.global(&press_mod(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            Outcome::Swallow
        ));
    }

    #[test]
    fn motion_rebind_applies_to_normal_and_visual() {
        let km = Keymap::from_config(&cfg_with(&[("left", "a")]));
        match km.normal(&press(KeyCode::Char('a'))) {
            Outcome::Replace(ev) => assert_eq!(ev.code, KeyCode::Char('h')),
            _ => panic!("normal `a` should translate to `h`"),
        }
        match km.visual(&press(KeyCode::Char('a'))) {
            Outcome::Replace(ev) => assert_eq!(ev.code, KeyCode::Char('h')),
            _ => panic!("visual `a` should translate to `h`"),
        }
        assert!(matches!(
            km.normal(&press(KeyCode::Char('h'))),
            Outcome::Swallow
        ));
        // The arrow key is hard-wired and still works.
        assert!(matches!(km.normal(&press(KeyCode::Left)), Outcome::Pass));
    }

    #[test]
    fn bad_config_reports_warnings() {
        let km = Keymap::from_config(&cfg_with(&[
            ("nonsense", "ctrl+x"),
            ("save", "ctrl+notakey"),
        ]));
        assert_eq!(km.warnings().len(), 2);
    }

    #[test]
    fn conflicts_are_reported() {
        let km = Keymap::from_config(&cfg_with(&[("save", "ctrl+p")]));
        assert!(km.warnings().iter().any(|w| w.contains("ctrl+p")));
    }

    #[test]
    fn describe_lists_actions() {
        let km = Keymap::from_config(&Config::default());
        let text = km.describe();
        assert!(text.contains("finder"));
        assert!(text.contains("ctrl+p"));
        assert!(text.contains("delete_line"));
    }

    #[test]
    fn key_helper_returns_the_override() {
        let km = Keymap::from_config(&cfg_with(&[("save", "f2")]));
        assert_eq!(km.key("save"), "f2");
        assert_eq!(km.key("quit"), "ctrl+c");
        assert_eq!(km.key("nope"), "?nope");
    }

    #[test]
    fn template_lists_every_action_and_default() {
        let t = template();
        assert!(t.contains("[keys]"));
        assert!(t.contains("ctrl+p"));
        assert!(t.contains("ctrl+shift+m"));
        for spec in ACTIONS {
            assert!(t.contains(spec.name), "template missing `{}`", spec.name);
        }
    }
}
