//! The three trademark looks, shared with kshell:
//!
//! - **Neon** — a cyberpunk city on tape: hot magenta and electric cyan with
//!   sunset yellow, tags that lean forward (◢ … ◤), panels with cut corners and
//!   `▸▸` readout titles, a synthwave sun on the empty screens.
//! - **Stark** — a HUD: ice-blue light and Iron Man red, heavy corner brackets,
//!   `[ READOUT ]` titles and `[KEY]` tags, an arc reactor on the empty screens.
//! - **Arcane** — magic: violet into rose with a spectral spark, rounded panels
//!   with `✦` titles, glowing runestone tags, a magic circle among the stars.
//!
//! By default the look follows whichever of the three kshell is wearing (its
//! `[theme] name`), live; `theme = …` in the config, `--theme` or `t` pick one.
//! True colour throughout; the background is left to the terminal.

use ratatui::{
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders},
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Neon,
    Stark,
    Arcane,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Neon, Theme::Stark, Theme::Arcane];

    pub fn name(self) -> &'static str {
        match self {
            Theme::Neon => "Neon",
            Theme::Stark => "Stark",
            Theme::Arcane => "Arcane",
        }
    }

    /// A theme by name, as kshell or a person would write it.
    pub fn parse(name: &str) -> Option<Theme> {
        match name.trim().to_lowercase().as_str() {
            "neon" | "cyberpunk" | "synthwave" => Some(Theme::Neon),
            "stark" | "hud" | "jarvis" => Some(Theme::Stark),
            "arcane" | "magic" => Some(Theme::Arcane),
            _ => None,
        }
    }

    pub fn next(self) -> Theme {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    fn index(self) -> u8 {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0) as u8
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Theme {
    Theme::ALL[CURRENT.load(Ordering::Relaxed) as usize % Theme::ALL.len()]
}

pub fn set(theme: Theme) {
    CURRENT.store(theme.index(), Ordering::Relaxed);
}

// MARK: - following kshell

/// Keeps the look in step with kshell's theme while it's on `auto`: checks
/// kshell's config when it changes (a stat a second, nothing more).
pub struct Follower {
    path: Option<PathBuf>,
    seen: Option<SystemTime>,
    pub auto: bool,
}

impl Follower {
    pub fn new(auto: bool) -> Self {
        let path = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/kshell/config.toml"));
        let mut follower = Follower { path, seen: None, auto };
        follower.poll();
        follower
    }

    /// Re-read kshell's theme if its config has changed. True when the look did.
    pub fn poll(&mut self) -> bool {
        if !self.auto {
            return false;
        }
        let Some(path) = &self.path else { return false };
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if modified.is_none() || modified == self.seen {
            return false;
        }
        self.seen = modified;
        let Some(theme) = std::fs::read_to_string(path).ok().and_then(|text| kshell_theme(&text)) else {
            return false;
        };
        let changed = theme != current();
        set(theme);
        changed
    }
}

/// The trademark theme named in a kshell config's `[theme]` section, if any.
pub fn kshell_theme(config: &str) -> Option<Theme> {
    let mut in_theme = false;
    for line in config.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_theme = line == "[theme]";
            continue;
        }
        if !in_theme {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "name" {
                return Theme::parse(value.trim().trim_matches('"'));
            }
        }
    }
    None
}

// MARK: - colours

struct Palette {
    accent: Color,
    highlight: Color,
    mid: Color,
    text: Color,
    dim: Color,
    faint: Color,
    selected: Color,
    ink: Color,
}

fn palette() -> Palette {
    match current() {
        Theme::Neon => Palette {
            accent: Color::Rgb(255, 46, 151),
            highlight: Color::Rgb(0, 240, 255),
            mid: Color::Rgb(255, 210, 63),
            text: Color::Rgb(245, 230, 255),
            dim: Color::Rgb(122, 92, 153),
            faint: Color::Rgb(66, 40, 96),
            selected: Color::Rgb(52, 16, 74),
            ink: Color::Rgb(11, 4, 24),
        },
        Theme::Stark => Palette {
            accent: Color::Rgb(55, 216, 255),
            highlight: Color::Rgb(230, 246, 255),
            // Iron Man red-gold for the warm notes
            mid: Color::Rgb(255, 190, 70),
            text: Color::Rgb(207, 233, 247),
            dim: Color::Rgb(84, 122, 141),
            faint: Color::Rgb(30, 58, 72),
            selected: Color::Rgb(8, 42, 58),
            ink: Color::Rgb(2, 12, 18),
        },
        Theme::Arcane => Palette {
            accent: Color::Rgb(181, 108, 255),
            highlight: Color::Rgb(255, 122, 217),
            mid: Color::Rgb(125, 240, 255),
            text: Color::Rgb(236, 223, 255),
            dim: Color::Rgb(125, 106, 158),
            faint: Color::Rgb(62, 44, 92),
            selected: Color::Rgb(44, 22, 70),
            ink: Color::Rgb(14, 6, 26),
        },
    }
}

/// The main colour: magenta, ice blue, violet.
pub fn accent() -> Color {
    palette().accent
}
/// The second colour: cyan, white light, rose.
pub fn highlight() -> Color {
    palette().highlight
}
/// The warm note: sunset yellow, Iron Man gold, spectral cyan.
pub fn mid() -> Color {
    palette().mid
}
pub fn text() -> Color {
    palette().text
}
pub fn dim() -> Color {
    palette().dim
}
pub fn faint() -> Color {
    palette().faint
}
pub fn selected() -> Color {
    palette().selected
}
/// What sits on a filled tag: near-black.
pub fn ink() -> Color {
    palette().ink
}

// MARK: - glyphs

/// What leads a secondary line (a channel, a readout).
pub fn mark() -> &'static str {
    match current() {
        Theme::Neon => "▸▸",
        Theme::Stark => "//",
        Theme::Arcane => "✦",
    }
}

/// The armed row's markers: a bar at the edge and a pointer.
pub fn cursor() -> (&'static str, &'static str) {
    match current() {
        Theme::Neon => ("▌", "▸"),
        Theme::Stark => ("┃", "▶"),
        Theme::Arcane => ("│", "✦"),
    }
}

/// The progress bar: filled, head, empty.
pub fn track() -> (&'static str, &'static str, &'static str) {
    match current() {
        Theme::Neon => ("━", "◆", "─"),
        Theme::Stark => ("━", "▮", "╌"),
        Theme::Arcane => ("━", "✦", "┄"),
    }
}

// MARK: - pieces

/// A panel's corners cut off, the way a city sign would be.
const CUT: border::Set = border::Set {
    top_left: "╱",
    top_right: "╲",
    bottom_left: "╲",
    bottom_right: "╱",
    vertical_left: "│",
    vertical_right: "│",
    horizontal_top: "─",
    horizontal_bottom: "─",
};

/// Heavy corners on fine lines: a HUD's targeting brackets.
const BRACKETS: border::Set = border::Set {
    top_left: "┏",
    top_right: "┓",
    bottom_left: "┗",
    bottom_right: "┛",
    vertical_left: "╎",
    vertical_right: "╎",
    horizontal_top: "─",
    horizontal_bottom: "─",
};

/// A panel in the current look: lit when it has the keyboard, faint otherwise.
pub fn panel(title: impl Into<String>, focused: bool) -> Block<'static> {
    let title = title.into();
    let p = palette();
    let edge = Style::new().fg(if focused { p.accent } else { p.faint });
    let (set, spans) = match current() {
        Theme::Neon => (
            CUT,
            vec![
                Span::raw(" "),
                Span::styled("▸▸ ", Style::new().fg(if focused { p.accent } else { p.dim })),
                Span::styled(title.trim().to_uppercase(), Style::new().fg(if focused { p.highlight } else { p.dim }).bold()),
                Span::raw(" "),
            ],
        ),
        Theme::Stark => (
            BRACKETS,
            vec![
                Span::styled(" [ ", Style::new().fg(if focused { p.accent } else { p.dim })),
                Span::styled(title.trim().to_uppercase(), Style::new().fg(if focused { p.highlight } else { p.dim }).bold()),
                Span::styled(" ] ", Style::new().fg(if focused { p.accent } else { p.dim })),
            ],
        ),
        Theme::Arcane => (
            border::ROUNDED,
            vec![
                Span::styled(" ✦ ", Style::new().fg(if focused { p.highlight } else { p.dim })),
                Span::styled(title.trim().to_string(), Style::new().fg(if focused { p.text } else { p.dim }).italic().bold()),
                Span::styled(" ✦ ", Style::new().fg(if focused { p.accent } else { p.dim })),
            ],
        ),
    };
    let mut block = Block::new().borders(Borders::ALL).border_set(set).border_style(edge);
    if !title.trim().is_empty() {
        block = block.title(Line::from(spans));
    }
    block
}

/// A tag: `◢ TEXT ◤` leaning forward, `[TEXT]` on a HUD, or a glowing
/// runestone with round ends.
pub fn tag(text: impl Into<String>, colour: Color) -> Vec<Span<'static>> {
    let text = text.into();
    match current() {
        Theme::Neon => vec![
            Span::styled("◢", Style::new().fg(colour)),
            Span::styled(text, Style::new().fg(ink()).bg(colour).bold()),
            Span::styled("◤", Style::new().fg(colour)),
        ],
        Theme::Stark => vec![
            Span::styled("[", Style::new().fg(colour)),
            Span::styled(text.trim().to_uppercase(), Style::new().fg(colour).bold()),
            Span::styled("]", Style::new().fg(colour)),
        ],
        Theme::Arcane => vec![
            Span::styled("\u{e0b6}", Style::new().fg(colour)),
            Span::styled(text, Style::new().fg(ink()).bg(colour).bold()),
            Span::styled("\u{e0b4}", Style::new().fg(colour)),
        ],
    }
}

/// Text running from one colour to another, character by character.
pub fn gradient(text: &str, from: Color, to: Color, style: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(2) - 1;
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| Span::styled(c.to_string(), style.fg(mix(from, to, i as f64 / n as f64))))
        .collect()
}

/// A colour part-way between two RGB colours.
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let l = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t.clamp(0.0, 1.0)).round() as u8;
            Color::Rgb(l(r1, r2), l(g1, g2), l(b1, b2))
        }
        _ => a,
    }
}

/// The yt-tui logo in the current look.
pub fn logo() -> Vec<Span<'static>> {
    let p = palette();
    match current() {
        Theme::Neon => {
            let mut spans = tag(" ▶ ", p.accent);
            spans.push(Span::raw(" "));
            spans.extend(gradient("YT//TUI", p.accent, p.highlight, Style::new().bold().italic()));
            spans
        }
        Theme::Stark => {
            let mut spans = vec![Span::styled("◉ ", Style::new().fg(p.accent).bold())];
            spans.extend(gradient("Y T · T U I", p.highlight, p.accent, Style::new().bold()));
            spans.push(Span::styled("  // MK-II", Style::new().fg(p.dim)));
            spans
        }
        Theme::Arcane => {
            let mut spans = vec![Span::styled("✦ ", Style::new().fg(p.highlight).bold())];
            spans.extend(gradient("yt·tui", p.accent, p.highlight, Style::new().bold().italic()));
            spans.push(Span::styled(" ✧", Style::new().fg(p.mid)));
            spans
        }
    }
}

/// A key hint: the key on a small tag, the label after it.
pub fn hint(key: &str, what: &str) -> Vec<Span<'static>> {
    let mut spans = tag(key.to_string(), accent());
    let label = if current() == Theme::Arcane { what.to_string() } else { what.to_uppercase() };
    spans.push(Span::styled(format!(" {label}   "), Style::new().fg(dim())));
    spans
}

/// The picture for the empty and loading screens, `width` columns wide.
pub fn scene(width: u16) -> Vec<Line<'static>> {
    let mut lines = match current() {
        Theme::Neon => synthwave(width),
        Theme::Stark => arc_reactor(width),
        Theme::Arcane => magic_circle(width),
    };
    // Every row the full width: centred as lines, the picture keeps its shape.
    let w = width.clamp(18, 44) as usize;
    for line in &mut lines {
        let gap = w.saturating_sub(line.width());
        if gap > 0 {
            line.spans.push(Span::raw(" ".repeat(gap)));
        }
    }
    lines
}

fn centred(text: &str, w: usize, style: Style) -> Line<'static> {
    let len = text.chars().count();
    Line::from(vec![Span::raw(" ".repeat(w.saturating_sub(len) / 2)), Span::styled(text.to_string(), style)])
}

/// A striped sun going down behind a grid floor.
fn synthwave(width: u16) -> Vec<Line<'static>> {
    let p = palette();
    let w = width.clamp(18, 44) as usize;
    let mut lines = Vec::new();
    // The sun, drawn in half blocks: rows of a circle, yellow fading into
    // magenta, with stripes cut out of its lower half.
    let rows = 5usize;
    let radius = (w as f64 * 0.32).min(12.0);
    for row in 0..rows {
        let y = (row as f64 + 0.5) / rows as f64; // 0 top .. 1 bottom (the horizon)
        let half = radius * (1.0 - (1.0 - y) * (1.0 - y)).sqrt().max(0.0);
        let half = if row == 0 { half * 0.75 } else { half };
        let span_w = (half * 2.0).round() as usize;
        let pad = w.saturating_sub(span_w) / 2;
        let colour = mix(p.mid, p.accent, y);
        let glyph = if row >= 2 && row % 2 == 0 { "▀" } else { "█" };
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(pad)),
            Span::styled(glyph.repeat(span_w), Style::new().fg(colour)),
        ]));
    }
    // The horizon, then the floor's rays fanning out.
    lines.push(Line::from(gradient(&"━".repeat(w), p.accent, p.highlight, Style::new())));
    for row in 0..3usize {
        let mut s = String::new();
        let spread = 2 + row * 3;
        for i in 0..w {
            let off = i as isize - (w / 2) as isize;
            let on = off.unsigned_abs() % spread == 0;
            s.push(if !on { ' ' } else if off < 0 { '╱' } else if off > 0 { '╲' } else { '│' });
        }
        lines.push(Line::styled(s, Style::new().fg(mix(p.accent, p.faint, row as f64 / 3.0))));
    }
    lines
}

/// An arc reactor: a glowing core in a ring of coils, under a readout.
fn arc_reactor(width: u16) -> Vec<Line<'static>> {
    let p = palette();
    let w = width.clamp(18, 44) as usize;
    let ring = Style::new().fg(p.accent);
    let coil = Style::new().fg(p.dim);
    let core = Style::new().fg(p.highlight).bold();
    let row = |parts: Vec<(&str, Style)>| -> Line<'static> {
        let len: usize = parts.iter().map(|(s, _)| s.chars().count()).sum();
        let mut spans = vec![Span::raw(" ".repeat(w.saturating_sub(len) / 2))];
        spans.extend(parts.into_iter().map(|(s, st)| Span::styled(s.to_string(), st)));
        Line::from(spans)
    };
    // every row 14 wide, so centring each keeps the drawing square
    vec![
        row(vec![("  ╭─", ring), ("▮▮▮▮▮▮", coil), ("─╮  ", ring)]),
        row(vec![("╭─╯ ", ring), ("▮", coil), ("╭──╮", ring), ("▮", coil), (" ╰─╮", ring)]),
        row(vec![("│   ", ring), ("▮", coil), ("│", ring), ("◉◉", core), ("│", ring), ("▮", coil), ("   │", ring)]),
        row(vec![("╰─╮ ", ring), ("▮", coil), ("╰──╯", ring), ("▮", coil), (" ╭─╯", ring)]),
        row(vec![("  ╰─", ring), ("▮▮▮▮▮▮", coil), ("─╯  ", ring)]),
        Line::raw(""),
        centred("[ REACTOR ONLINE ]", w, Style::new().fg(p.mid)),
    ]
}

/// A magic circle among the stars: rings, runes and a star at the heart.
fn magic_circle(width: u16) -> Vec<Line<'static>> {
    let p = palette();
    let w = width.clamp(18, 44) as usize;
    let violet = Style::new().fg(p.accent);
    let rose = Style::new().fg(p.highlight);
    let star = Style::new().fg(p.mid).bold();
    let faint = Style::new().fg(p.dim);
    let row = |parts: Vec<(&str, Style)>| -> Line<'static> {
        let len: usize = parts.iter().map(|(s, _)| s.chars().count()).sum();
        let mut spans = vec![Span::raw(" ".repeat(w.saturating_sub(len) / 2))];
        spans.extend(parts.into_iter().map(|(s, st)| Span::styled(s.to_string(), st)));
        Line::from(spans)
    };
    // every row 17 wide, so centring each keeps the circle round
    vec![
        row(vec![("✧   ", faint), ("· ᚠ · ᚱ ·", rose), ("   ✧", faint)]),
        row(vec![("  ╭─", violet), ("ᛟ", rose), ("───────", violet), ("ᛉ", rose), ("─╮  ", violet)]),
        row(vec![("  │   ", violet), ("╲ ✦ ╱", rose), ("   │  ", violet)]),
        row(vec![("· ᛞ ", rose), ("──  ", violet), ("✦", star), ("  ──", violet), (" ᚹ ·", rose)]),
        row(vec![("  │   ", violet), ("╱ ✦ ╲", rose), ("   │  ", violet)]),
        row(vec![("  ╰─", violet), ("ᛒ", rose), ("───────", violet), ("ᛗ", rose), ("─╯  ", violet)]),
        row(vec![("✧   ", faint), ("· ᛃ · ᛜ ·", rose), ("   ✧", faint)]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The look is global: tests that change it take turns.
    static LOOK: Mutex<()> = Mutex::new(());

    #[test]
    fn gradient_runs_end_to_end() {
        let spans = gradient("abc", Color::Rgb(0, 0, 0), Color::Rgb(200, 100, 50), Style::new());
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].style.fg, Some(Color::Rgb(0, 0, 0)));
        assert_eq!(spans[2].style.fg, Some(Color::Rgb(200, 100, 50)));
    }

    #[test]
    fn every_scene_fits_its_width() {
        let _turn = LOOK.lock().unwrap_or_else(|e| e.into_inner());
        for theme in Theme::ALL {
            set(theme);
            for w in [1u16, 18, 30, 80] {
                for line in scene(w) {
                    assert_eq!(line.width(), (w.clamp(18, 44)) as usize, "{theme:?} at {w}");
                }
            }
        }
        set(Theme::Neon);
    }

    #[test]
    fn scene_rows_line_up() {
        let _turn = LOOK.lock().unwrap_or_else(|e| e.into_inner());
        for theme in [Theme::Stark, Theme::Arcane] {
            set(theme);
            let lines = scene(44);
            let widths: Vec<usize> = lines.iter().take(5).map(|l| l.width()).collect();
            assert!(widths.windows(2).all(|w| w[0] == w[1]), "{theme:?}: {widths:?}");
        }
        set(Theme::Neon);
    }

    #[test]
    fn reads_kshells_theme() {
        let config = "[bar]\nname = \"bar\"\n\n[theme]\nname = \"Stark\"  # trademark\naccent = \"#fff\"\n";
        assert_eq!(kshell_theme(config), Some(Theme::Stark));
        assert_eq!(kshell_theme("[theme]\nname = \"Rose Pine\"\n"), None);
        assert_eq!(kshell_theme("[bar]\nname = \"Arcane\"\n"), None, "only the [theme] section counts");
    }

    #[test]
    fn names_and_cycling() {
        assert_eq!(Theme::parse("ARCANE"), Some(Theme::Arcane));
        assert_eq!(Theme::parse("hud"), Some(Theme::Stark));
        assert_eq!(Theme::parse("auto"), None);
        assert_eq!(Theme::Neon.next(), Theme::Stark);
        assert_eq!(Theme::Arcane.next(), Theme::Neon);
    }
}
