//! Shared overlay chrome: modal sizing, borders, selection style and colour blending.

use super::*;

/// Standard modal size shared by every popup overlay (finder, music,
/// theme, run output, help) — one uniform look.
pub(super) const MODAL_W: f32 = 0.75;
pub(super) const MODAL_H: f32 = 0.75;

/// Maximum width of the now-playing song in the buffer bar; longer
/// names scroll.
pub(super) const MUSIC_MAX: usize = 28;

/// Centered rectangle taking `w_pct` × `h_pct` of `area` (clamped).
pub(super) fn modal_rect(area: Rect, w_pct: f32, h_pct: f32) -> Rect {
    let w = ((area.width as f32 * w_pct) as u16).min(area.width);
    let h = ((area.height as f32 * h_pct) as u16).min(area.height);
    Rect::new((area.width - w) / 2, (area.height - h) / 2, w, h)
}

/// Spinner frames for the chat title while a turn is running.
pub(super) const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];

/// The one uniform overlay frame: bordered, with a scrolling title.
pub(super) fn overlay_block<'a>(title: &str, frame: u64, animated: bool, theme: &Theme) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .title(scrolled_title(title, frame, animated))
        .title_alignment(Alignment::Center)
        .title_style(Style::new().fg(theme.fg).bg(theme.status_bg))
        .border_style(Style::new().fg(theme.border))
}

/// The accent "selected row" style used by every list in the editor —
/// dark text on the theme's accent, matching the status-bar mode
/// block.
pub(super) fn selected_style(theme: &Theme) -> Style {
    let accent = theme.keyword.fg.unwrap_or(theme.status_fg);
    Style::new()
        .fg(crate::theme::contrast_fg(accent))
        .bg(accent)
        .add_modifier(Modifier::BOLD)
}


/// Scrolling title bar text: pads with `═` and scrolls left based on
/// the frame counter.  When `animated` is false, returns a static
/// centered title.
pub(super) fn scrolled_title(label: &str, frame: u64, animated: bool) -> String {
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
pub(super) fn blend_colors(a: Color, b: Color, min_factor: f32, max_factor: f32) -> Color {
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

/// Shift a colour: rotate its hue by `hue_offset` degrees and blend
/// its lightness toward white (positive) or black (negative).
pub(super) fn shift_color(c: Color, hue_offset: f64, lightness: f64) -> Color {
    use crate::theme::hsl_to_rgb;

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
}

/// Softly shift a theme's colours.  `hue_offset` rotates hues in degrees
/// (small values = subtle).  `lightness` blends toward white (positive) or
/// black (negative) — used for breathing effects.
///
/// Only the *important* tokens are animated — keywords, functions,
/// types, strings, numbers, constants, config keys — so the plain
/// text, comments, operators, and all UI chrome stay calm.
pub(super) fn soft_shift_theme(t: &Theme, hue_offset: f64, lightness: f64) -> Theme {
    let shift_style = |s: &Style| -> Style {
        Style {
            fg: s.fg.map(|c| shift_color(c, hue_offset, lightness)),
            bg: s.bg.map(|c| shift_color(c, hue_offset, lightness)),
            underline_color: s.underline_color.map(|c| shift_color(c, hue_offset, lightness)),
            add_modifier: s.add_modifier,
            sub_modifier: s.sub_modifier,
        }
    };

    Theme {
        // Static: plain text, comments, operators/punctuation, and
        // every piece of UI chrome.
        fg: t.fg,
        bg: t.bg,
        selection_bg: t.selection_bg,
        line_number: t.line_number,
        tilde: t.tilde,
        status_bg: t.status_bg,
        status_fg: t.status_fg,
        border: t.border,
        comment: t.comment,
        operator: t.operator,
        punctuation: t.punctuation,

        // Animated: the tokens worth drawing attention to.
        keyword: shift_style(&t.keyword),
        builtin: shift_style(&t.builtin),
        rstype: shift_style(&t.rstype),
        function: shift_style(&t.function),
        lifetime: shift_style(&t.lifetime),
        string: shift_style(&t.string),
        fstring_prefix: shift_style(&t.fstring_prefix),
        number: shift_style(&t.number),
        constant: shift_style(&t.constant),
        decorator: shift_style(&t.decorator),
        property: shift_style(&t.property),
    }
}
pub(super) fn color_to_rgb(c: Color) -> Option<(u8, u8, u8)> {
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

/// Right-gutter indicator for lines changed by an external reload:
/// a green `▎` for added lines, amber for changed ones, fading out
/// over a few seconds.
pub(super) fn decorate_change_marker(
    spans: &mut Vec<Span<'static>>,
    row: usize,
    width: usize,
    line_style: Style,
    theme: &Theme,
    line_changes: &Option<Vec<LineChange>>,
    change_age: Instant,
) {
    const CHANGE_TTL: f32 = 10.0;

    let kind = match line_changes {
        Some(c) => c.get(row).copied().unwrap_or(LineChange::None),
        None => LineChange::None,
    };
    if kind == LineChange::None || width == 0 {
        return;
    }
    let fade = (1.0 - change_age.elapsed().as_secs_f32() / CHANGE_TTL).clamp(0.0, 1.0);
    if fade <= 0.0 {
        return;
    }
    let base = match kind {
        LineChange::Added => Color::Rgb(0x7e, 0xd3, 0x86),   // green
        LineChange::Changed => Color::Rgb(0xff, 0xc6, 0x6d),  // amber
        LineChange::None => return,
    };
    // Blend toward the background as the marker ages (skip on
    // transparent backgrounds — no colour to blend into).
    let color = match theme.bg {
        Color::Rgb(..) => blend_colors(theme.bg, base, 0.0, fade),
        _ => base,
    };
    let used: usize = spans.iter().map(|s| col_width(&s.content)).sum();
    let pad = width.saturating_sub(used + 1);
    let bg = line_style.bg.unwrap_or(Color::Reset);
    spans.push(Span::styled(" ".repeat(pad), line_style));
    spans.push(Span::styled("▎", Style::new().fg(color).bg(bg)));
}
