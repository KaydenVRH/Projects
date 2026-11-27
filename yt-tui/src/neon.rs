//! The Neon look: a cyberpunk city on tape, in a terminal. Hot magenta and
//! electric cyan with sunset yellow, text in magenta→cyan gradients, tags that
//! lean forward (◢ … ◤), panels with cut corners and `▸▸` readout titles, and a
//! little synthwave sun for the empty screens. True colour throughout; the
//! background is left to the terminal.

use ratatui::{
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders},
};

pub const MAGENTA: Color = Color::Rgb(255, 46, 151);
pub const CYAN: Color = Color::Rgb(0, 240, 255);
pub const YELLOW: Color = Color::Rgb(255, 210, 63);
pub const TEXT: Color = Color::Rgb(245, 230, 255);
pub const DIM: Color = Color::Rgb(122, 92, 153);
pub const FAINT: Color = Color::Rgb(66, 40, 96);
pub const SELECTED_BG: Color = Color::Rgb(52, 16, 74);
/// What sits on a filled tag: near-black purple.
pub const INK: Color = Color::Rgb(11, 4, 24);

/// A panel's corners cut off, the way a HUD panel would be.
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

/// A cut-corner panel, magenta when it has the keyboard, faint otherwise, with
/// its title as a `▸▸ READOUT`.
pub fn panel(title: impl Into<String>, focused: bool) -> Block<'static> {
    let title = title.into();
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_set(CUT)
        .border_style(Style::new().fg(if focused { MAGENTA } else { FAINT }));
    if !title.trim().is_empty() {
        block = block.title(Line::from(vec![
            Span::raw(" "),
            Span::styled("▸▸ ", Style::new().fg(if focused { MAGENTA } else { DIM })),
            Span::styled(title.trim().to_uppercase(), Style::new().fg(if focused { CYAN } else { DIM }).bold()),
            Span::raw(" "),
        ]));
    }
    block
}

/// A tag leaning forward: `◢ TEXT ◤`, filled with `colour`.
pub fn tag(text: impl Into<String>, colour: Color) -> Vec<Span<'static>> {
    vec![
        Span::styled("◢", Style::new().fg(colour)),
        Span::styled(text.into(), Style::new().fg(INK).bg(colour).bold()),
        Span::styled("◤", Style::new().fg(colour)),
    ]
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

/// The yt-tui logo: a slanted magenta tag with a play mark, then the name in a
/// magenta→cyan gradient.
pub fn logo() -> Vec<Span<'static>> {
    let mut spans = tag(" ▶ ", MAGENTA);
    spans.push(Span::raw(" "));
    spans.extend(gradient("YT//TUI", MAGENTA, CYAN, Style::new().bold().italic()));
    spans
}

/// A key hint: the key on a small slanted tag, the label after it.
pub fn hint(key: &str, what: &str) -> Vec<Span<'static>> {
    let mut spans = tag(key.to_string(), MAGENTA);
    spans.push(Span::styled(format!(" {}   ", what.to_uppercase()), Style::new().fg(DIM)));
    spans
}

/// A small synthwave scene for the empty and loading screens: a striped sun
/// going down behind a grid floor, `width` columns wide.
pub fn synthwave(width: u16) -> Vec<Line<'static>> {
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
        let colour = mix(YELLOW, MAGENTA, y);
        let glyph = if row >= 2 && row % 2 == 0 { "▀" } else { "█" };
        let mut spans = vec![Span::raw(" ".repeat(pad))];
        spans.push(Span::styled(glyph.repeat(span_w), Style::new().fg(colour)));
        lines.push(Line::from(spans));
    }
    // The horizon, then the floor's rays fanning out.
    lines.push(Line::from(gradient(&"━".repeat(w), MAGENTA, CYAN, Style::new())));
    for row in 0..3usize {
        let mut s = String::new();
        let spread = 2 + row * 3;
        for i in 0..w {
            let off = i as isize - (w / 2) as isize;
            let on = off.unsigned_abs() % spread == 0;
            s.push(if !on { ' ' } else if off < 0 { '╱' } else if off > 0 { '╲' } else { '│' });
        }
        lines.push(Line::styled(s, Style::new().fg(mix(MAGENTA, FAINT, row as f64 / 3.0))));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_runs_end_to_end() {
        let spans = gradient("abc", Color::Rgb(0, 0, 0), Color::Rgb(200, 100, 50), Style::new());
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].style.fg, Some(Color::Rgb(0, 0, 0)));
        assert_eq!(spans[2].style.fg, Some(Color::Rgb(200, 100, 50)));
    }

    #[test]
    fn the_synthwave_scene_fits_its_width() {
        for w in [1u16, 18, 30, 80] {
            for line in synthwave(w) {
                assert!(line.width() <= (w.clamp(18, 44)) as usize);
            }
        }
    }
}
