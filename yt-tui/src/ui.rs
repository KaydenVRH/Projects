//! Drawing. Every function here is pure layout + widgets; the rectangles
//! that input and graphics need are recorded in `app.layout`.

use crate::app::{App, Focus};
use crate::gfx::{Placeholder, THUMB_ID, VIDEO_ID};
use crate::player::Player;
use crate::youtube::{format_duration, format_views, Video};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

const ACCENT: Color = Color::Rgb(255, 72, 72);
const DIM: Color = Color::Rgb(128, 128, 140);
const FAINT: Color = Color::Rgb(70, 70, 82);
const SELECTED_BG: Color = Color::Rgb(40, 40, 50);

pub fn render(f: &mut Frame, app: &mut App) {
    app.layout = Default::default();
    let showing_player = app.player.is_some() || app.loading.is_some();
    if showing_player {
        render_player(f, app);
    } else {
        render_browse(f, app);
    }
    if app.help {
        render_help(f, showing_player);
    }
}

// ── browse ──────────────────────────────────────────────────────────

fn render_browse(f: &mut Frame, app: &mut App) {
    let [header, search, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(f.area().inner(Margin::new(1, 0)));

    render_header(f, app, header);
    render_search(f, app, search);

    let wide = body.width >= 90;
    let [list_area, preview_area] = if wide {
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(body)
    } else {
        [body, Rect::default()]
    };
    render_results(f, app, list_area);
    if wide {
        render_preview(f, app, preview_area);
    }
    render_footer(f, app, footer);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let mut left = vec![Span::styled(" ▶ ", Style::new().fg(Color::White).bg(ACCENT).bold()), " yt-tui".bold()];
    if let Some((query, since, _)) = &app.searching {
        left.push(Span::styled(
            format!("   {} searching “{}” {:.1}s", app.spinner(), query, since.elapsed().as_secs_f32()),
            Style::new().fg(DIM),
        ));
    }
    f.render_widget(Line::from(left), area);
    let mut right = Vec::new();
    if app.gfx.is_none() {
        right.push(Span::styled("audio only — terminal has no kitty graphics", Style::new().fg(DIM)));
    }
    if app.autoplay {
        right.push(Span::styled("  autoplay", Style::new().fg(ACCENT)));
    }
    f.render_widget(Line::from(right).alignment(Alignment::Right), area);
}

fn render_search(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Search;
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { ACCENT } else { FAINT }))
        .title(Span::styled(" Search ", Style::new().fg(if focused { Color::White } else { DIM })));
    let line = if app.query.is_empty() && !focused {
        Line::from(Span::styled("  press / to search", Style::new().fg(DIM)))
    } else if app.query.is_empty() {
        Line::from(vec![
            Span::raw("  "),
            Span::styled("▏", Style::new().fg(ACCENT)),
            Span::styled("Search YouTube, or paste a link", Style::new().fg(DIM)),
        ])
    } else {
        let mut spans = vec![Span::raw("  "), Span::styled(app.query.as_str(), Style::new().bold())];
        if focused {
            spans.push(Span::styled("▏", Style::new().fg(ACCENT)));
        }
        Line::from(spans)
    };
    // Keep the end of a long query (where you're typing) in view.
    let inner_width = area.width.saturating_sub(2) as usize;
    let scroll = line.width().saturating_sub(inner_width) as u16;
    f.render_widget(Paragraph::new(line).block(block).scroll((0, scroll)), area);
}

fn render_results(f: &mut Frame, app: &mut App, area: Rect) {
    app.layout.results = area;
    let focused = app.focus == Focus::Results;
    let title = match (app.results.len(), app.list.selected()) {
        (0, _) => " Results ".to_string(),
        (n, Some(i)) => format!(" Results {}/{} ", i + 1, n),
        (n, None) => format!(" Results {n} "),
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { ACCENT } else { FAINT }))
        .title(Span::styled(title, Style::new().fg(if focused { Color::White } else { DIM })));

    if app.results.is_empty() {
        let hint = if app.searching.is_some() {
            format!("{} searching…", app.spinner())
        } else {
            "Type a search and press Enter.".to_string()
        };
        let text = Text::from(vec![Line::raw(""), Line::styled(hint, Style::new().fg(DIM))]).centered();
        f.render_widget(Paragraph::new(text).block(block), area);
        return;
    }

    let width = area.width.saturating_sub(4) as usize;
    let selected = app.list.selected();
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let on = Some(i) == selected;
            let bar = Span::styled(if on { "▌ " } else { "  " }, Style::new().fg(ACCENT));
            let title = Span::styled(
                truncate(&v.title, width),
                if on { Style::new().bold().fg(Color::White) } else { Style::new() },
            );
            let mut meta = vec![bar.clone()];
            meta.extend(meta_spans(v));
            ListItem::new(vec![Line::from(vec![bar, title]), Line::from(meta), Line::raw("")])
        })
        .collect();
    let list = List::new(items).block(block).highlight_style(Style::new().bg(SELECTED_BG));
    f.render_stateful_widget(list, area, &mut app.list);
}

fn meta_spans(v: &Video) -> Vec<Span<'static>> {
    let dim = Style::new().fg(DIM);
    let mut spans = Vec::new();
    if v.is_live() {
        spans.push(Span::styled("● LIVE  ", Style::new().fg(ACCENT).bold()));
    } else if let Some(d) = v.duration {
        spans.push(Span::styled(format!("{}  ", format_duration(d)), Style::new().fg(Color::White)));
    }
    spans.push(Span::styled(v.channel().to_string(), dim));
    if let Some(views) = v.view_count {
        spans.push(Span::styled(format!(" · {}", format_views(views)), dim));
    }
    spans
}

fn render_preview(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(FAINT));
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    let Some(video) = app.selected().cloned() else { return };

    let art_height = (inner.height * 55 / 100).max(3);
    let [art, _, text] =
        Layout::vertical([Constraint::Length(art_height), Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    render_art(f, app, &video, art);

    let mut lines = vec![Line::styled(video.title.clone(), Style::new().bold()), Line::raw("")];
    lines.push(Line::from(meta_spans(&video)));
    if let Some(desc) = video.description.as_deref().filter(|d| !d.trim().is_empty()) {
        lines.push(Line::raw(""));
        lines.extend(desc.lines().take(20).map(|l| Line::styled(l.to_string(), Style::new().fg(DIM))));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);
}

/// The thumbnail, or a quiet placeholder while it loads.
fn render_art(f: &mut Frame, app: &mut App, video: &Video, area: Rect) {
    if app.gfx.is_none() {
        return;
    }
    let boxed = app.thumb_box(area);
    app.layout.thumb = Some(boxed);
    if app.thumb_ready(&video.id) {
        f.render_widget(Placeholder { id: THUMB_ID }, boxed);
    } else {
        let label = if app.thumbs.loading(&video.id) { app.spinner() } else { "" };
        let frame = Block::new().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::new().fg(FAINT));
        let inner = frame.inner(boxed);
        f.render_widget(frame, boxed);
        f.render_widget(
            Paragraph::new(label).style(Style::new().fg(DIM)).alignment(Alignment::Center),
            Rect { y: inner.y + inner.height / 2, height: 1.min(inner.height), ..inner },
        );
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    if let Some(s) = app.status_text() {
        let style = if s.error { Style::new().fg(ACCENT) } else { Style::new().fg(DIM) };
        f.render_widget(Line::styled(format!(" {}", s.text), style), area);
        return;
    }
    if !app.missing.is_empty() {
        let text = format!(" missing: {} — install with brew", app.missing.join(", "));
        f.render_widget(Line::styled(text, Style::new().fg(ACCENT)), area);
        return;
    }
    let keys: &[(&str, &str)] = match app.focus {
        Focus::Search => &[("enter", "search"), ("tab", "results"), ("ctrl-u", "clear"), ("esc", "back")],
        Focus::Results => &[
            ("enter", "play"),
            ("a", "audio only"),
            ("/", "search"),
            ("A", "autoplay"),
            ("?", "help"),
            ("q", "quit"),
        ],
    };
    f.render_widget(hints(keys), area);
}

fn hints(keys: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (k, what) in keys {
        spans.push(Span::styled(k.to_string(), Style::new().fg(Color::White).bold()));
        spans.push(Span::styled(format!(" {what}   "), Style::new().fg(DIM)));
    }
    Line::from(spans)
}

// ── player ──────────────────────────────────────────────────────────

fn render_player(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let osd = app.osd_until.is_some_and(|t| Instant::now() < t);
    let paused = app.player.as_ref().is_some_and(|p| p.paused());
    let chrome = app.chrome || paused || app.player.is_none();
    let (title_area, stage, bar_area, controls_area) = if chrome {
        let [t, s, b, c] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        (Some(t), s, Some(b), Some(c))
    } else if osd {
        // Fullscreen, but the bar shows briefly after a key press.
        let [s, b] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);
        (None, s, Some(b), None)
    } else {
        (None, area, None, None)
    };

    let Some(player) = &app.player else {
        render_loading(f, app, stage, title_area);
        return;
    };

    if let Some(t) = title_area {
        let line = Line::from(vec![
            Span::styled(" ▶ ", Style::new().fg(Color::White).bg(ACCENT).bold()),
            Span::styled(format!(" {}", player.stream.title), Style::new().bold()),
            Span::styled(format!("  {}", player.video.channel()), Style::new().fg(DIM)),
        ]);
        f.render_widget(line, t);
    }

    if player.has_video() {
        app.layout.video = Some(stage);
        if let Some(p) = player.placement.filter(|_| player.frames_shown > 0) {
            f.render_widget(Placeholder { id: VIDEO_ID }, p.area);
        }
    } else {
        render_audio_stage(f, app, stage);
    }

    let player = app.player.as_ref().unwrap();
    if player.buffering() {
        let label = format!(" {} buffering ", app.spinner());
        let w = label.chars().count() as u16;
        let r = Rect { x: stage.x + stage.width.saturating_sub(w) / 2, y: stage.y + stage.height / 2, width: w.min(stage.width), height: 1 };
        f.render_widget(Clear, r);
        f.render_widget(Line::styled(label, Style::new().fg(Color::White).bg(SELECTED_BG)), r);
    }
    if let Some(b) = bar_area {
        app.layout.progress = Some(Rect { x: b.x + 1, width: b.width.saturating_sub(2), ..b });
        render_progress(f, player, b);
    }
    if let Some(c) = controls_area {
        render_controls(f, app, c);
    }
}

fn render_loading(f: &mut Frame, app: &mut App, stage: Rect, title: Option<Rect>) {
    let Some(loading) = &app.loading else { return };
    if let Some(t) = title {
        f.render_widget(
            Line::from(vec![
                Span::styled(" ▶ ", Style::new().fg(Color::White).bg(ACCENT).bold()),
                Span::styled(format!(" {}", loading.video.title), Style::new().bold()),
            ]),
            t,
        );
    }
    let what = if loading.audio_only { "audio" } else { "video" };
    let text = Text::from(vec![
        Line::styled(format!("{} finding the {what} stream…", app.spinner()), Style::new().fg(Color::White)),
        Line::styled(format!("{:.1}s", loading.since.elapsed().as_secs_f32()), Style::new().fg(DIM)),
        Line::raw(""),
        Line::styled("esc to cancel", Style::new().fg(DIM)),
    ])
    .centered();
    let r = Rect { y: stage.y + stage.height.saturating_sub(4) / 2, height: 4.min(stage.height), ..stage };
    f.render_widget(Paragraph::new(text), r);
}

/// Audio-only: the thumbnail as cover art, with the title under it.
fn render_audio_stage(f: &mut Frame, app: &mut App, stage: Rect) {
    let Some(player) = &app.player else { return };
    let video = player.video.clone();
    let [art, _, text] =
        Layout::vertical([Constraint::Percentage(70), Constraint::Length(1), Constraint::Min(2)]).areas(stage.inner(Margin::new(2, 1)));
    render_art(f, app, &video, art);
    let lines = vec![
        Line::styled(video.title.clone(), Style::new().bold()),
        Line::styled(video.channel().to_string(), Style::new().fg(DIM)),
    ];
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center).wrap(Wrap { trim: true }), text);
}

fn render_progress(f: &mut Frame, player: &Player, area: Rect) {
    let area = Rect { x: area.x + 1, width: area.width.saturating_sub(2), ..area };
    if player.stream.live {
        let line = Line::from(vec![
            Span::styled("● LIVE ", Style::new().fg(ACCENT).bold()),
            Span::styled("━".repeat(area.width.saturating_sub(7) as usize), Style::new().fg(ACCENT)),
        ]);
        f.render_widget(line, area);
        return;
    }
    let Some(duration) = player.duration() else { return };
    let w = area.width as usize;
    let frac = (player.position() / duration).clamp(0.0, 1.0);
    let filled = ((w.saturating_sub(1)) as f64 * frac).round() as usize;
    let line = Line::from(vec![
        Span::styled("━".repeat(filled), Style::new().fg(ACCENT)),
        Span::styled("●", Style::new().fg(Color::White)),
        Span::styled("─".repeat(w.saturating_sub(filled + 1)), Style::new().fg(FAINT)),
    ]);
    f.render_widget(line, area);
}

fn render_controls(f: &mut Frame, app: &App, area: Rect) {
    let Some(player) = &app.player else { return };
    let state = if player.paused() { "❚❚" } else { "▶" };
    let time = match player.duration() {
        Some(d) if !player.stream.live => format!("{} / {}", format_duration(player.position()), format_duration(d)),
        _ => format_duration(player.position()),
    };
    let mut left = vec![
        Span::styled(format!(" {state} "), Style::new().fg(ACCENT).bold()),
        Span::styled(time, Style::new().fg(Color::White)),
    ];
    if let Some((vol, muted)) = player.volume() {
        let v = if muted { "muted".to_string() } else { format!("vol {:.0}%", vol) };
        left.push(Span::styled(format!("   {v}"), Style::new().fg(DIM)));
    }
    if let Some((w, h, fps)) = player.decode_size() {
        left.push(Span::styled(
            format!("   {w}×{h} · {}/{:.0} fps", player.fps_now(), fps),
            Style::new().fg(DIM),
        ));
    }
    f.render_widget(Line::from(left), area);
    if area.width > 100 {
        let keys = hints(&[("space", "pause"), ("←→", "seek"), ("↑↓", "volume"), ("f", "fullscreen"), ("n", "next"), ("q", "back")]);
        f.render_widget(keys.alignment(Alignment::Right), area);
    }
    if let Some(s) = app.status_text().filter(|s| s.error) {
        let w = (s.text.chars().count() as u16 + 2).min(area.width);
        let r = Rect { x: area.x + area.width - w, width: w, ..area };
        f.render_widget(Clear, r);
        f.render_widget(Line::styled(format!(" {}", s.text), Style::new().fg(ACCENT)), r);
    }
}

// ── help ────────────────────────────────────────────────────────────

fn render_help(f: &mut Frame, in_player: bool) {
    let rows: &[(&str, &str)] = if in_player {
        &[
            ("space / k", "pause"),
            ("← → / h l", "seek 5s"),
            ("shift-← → / H L", "seek 30s"),
            ("0 – 9", "jump to 0% – 90%"),
            ("↑ ↓ / + -", "volume"),
            ("m", "mute"),
            ("f", "fullscreen"),
            ("n / p", "next / previous result"),
            ("click the bar", "seek there"),
            ("q / esc", "back to results"),
        ]
    } else {
        &[
            ("/", "search (or paste a link)"),
            ("j k / ↑ ↓", "move"),
            ("d u / pgdn pgup", "page"),
            ("g G", "top / bottom"),
            ("enter", "play"),
            ("a", "play audio only"),
            ("A", "toggle autoplay"),
            ("q", "quit"),
        ]
    };
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!("{k:>16}  "), Style::new().fg(Color::White).bold()),
                Span::styled(what.to_string(), Style::new().fg(DIM)),
            ])
        })
        .collect();
    let h = lines.len() as u16 + 2;
    let w = 52.min(f.area().width);
    let area = f.area();
    let r = Rect { x: area.x + (area.width - w) / 2, y: area.y + area.height.saturating_sub(h) / 2, width: w, height: h.min(area.height) };
    f.render_widget(Clear, r);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(" keys ")
        .title_bottom(Line::styled(" any key to close ", Style::new().fg(DIM)).right_aligned());
    f.render_widget(Paragraph::new(lines).block(block).style(Style::new().add_modifier(Modifier::empty())), r);
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Settings;
    use crate::gfx::Graphics;
    use crate::player::{Limits, Placement, Player};
    use crate::youtube::Stream;
    use ratatui::{backend::TestBackend, Terminal};

    fn video(n: usize) -> Video {
        serde_json::from_value(serde_json::json!({
            "id": format!("vid{n:08}"), "title": format!("A rather long video title number {n} that will not fit"),
            "channel": "Channel", "duration": 3725.0, "view_count": 1234567, "description": "line one\nline two"
        }))
        .unwrap()
    }

    fn app() -> App {
        let settings = Settings { max_height: 1080, limits: Limits { max_width: 1920, max_fps: 30 }, results: 30, volume: 0.0 };
        let mut app = App::new(Some(Graphics::for_tests()), settings);
        app.results = (0..30).map(video).collect();
        app.list.select(Some(3));
        app
    }

    fn draw_everywhere(app: &mut App) {
        for w in 1..=120u16 {
            for h in (1..=50u16).step_by(7) {
                let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
                term.draw(|f| render(f, app)).unwrap();
            }
        }
    }

    #[test]
    fn browsing_draws_at_every_size() {
        let mut app = app();
        draw_everywhere(&mut app);
        app.help = true;
        draw_everywhere(&mut app);
    }

    #[test]
    fn the_player_survives_the_window_shrinking_below_the_picture() {
        let mut app = app();
        let stream = Stream {
            video_url: Some("http://127.0.0.1:9/none".into()),
            audio_url: "http://127.0.0.1:9/none".into(),
            user_agent: "test".into(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            duration: Some(600.0),
            live: false,
            title: "t".into(),
        };
        let mut player = Player::start(video(1), stream, 0.0, app.settings.limits);
        // The picture as it was sized for a big window, before the resize
        // settles and the decoder restarts.
        player.placement = Some(Placement { area: Rect::new(0, 1, 160, 43), width: 1600, height: 900 });
        player.frames_shown = 1;
        app.player = Some(player);
        draw_everywhere(&mut app);
        app.chrome = false;
        draw_everywhere(&mut app);
    }
}
