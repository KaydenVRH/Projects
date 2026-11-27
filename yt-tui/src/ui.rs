//! Drawing. Every function here is pure layout + widgets; the rectangles
//! that input and graphics need are recorded in `app.layout`.

use crate::app::{App, Focus, Source};
use crate::theme as th;
use crate::gfx::{Placeholder, THUMB_ID, VIDEO_ID};
use crate::player::Player;
use crate::youtube::{format_duration, format_views, Video};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

/// The look is one of the three trademark themes (see `theme.rs`).

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
    let [header, search, tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(f.area().inner(Margin::new(1, 0)));

    render_header(f, app, header);
    render_search(f, app, search);
    app.layout.search = search;
    render_tabs(f, app, tabs);

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
    let mut left = th::logo();
    if let Some((source, since, _)) = &app.fetching {
        let what = match source {
            Source::Search(q) => format!("searching “{q}”"),
            feed => format!("loading {}", feed.title()),
        };
        left.push(Span::styled(
            format!("   {} {what} {:.1}s", app.spinner(), since.elapsed().as_secs_f32()),
            Style::new().fg(th::dim()),
        ));
    }
    f.render_widget(Line::from(left), area);
    let mut right = Vec::new();
    if app.gfx.is_none() {
        right.push(Span::styled("audio only — terminal has no kitty graphics", Style::new().fg(th::dim())));
    }
    if app.autoplay {
        right.push(Span::raw("  "));
        right.extend(th::tag(" AUTOPLAY ", th::highlight()));
    }
    f.render_widget(Line::from(right).alignment(Alignment::Right), area);
}

fn render_search(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Search;
    let block = th::panel("Search", focused);
    let line = if app.query.is_empty() && !focused {
        Line::from(Span::styled("  press / to search", Style::new().fg(th::dim())))
    } else if app.query.is_empty() {
        Line::from(vec![
            Span::raw("  "),
            Span::styled("▏", Style::new().fg(th::highlight())),
            Span::styled("Search YouTube, or paste a link", Style::new().fg(th::dim())),
        ])
    } else {
        let mut spans = vec![Span::raw("  "), Span::styled(app.query.as_str(), Style::new().fg(th::text()).bold())];
        if focused {
            spans.push(Span::styled("▏", Style::new().fg(th::highlight())));
        }
        Line::from(spans)
    };
    // Keep the end of a long query (where you're typing) in view.
    let inner_width = area.width.saturating_sub(2) as usize;
    let scroll = line.width().saturating_sub(inner_width) as u16;
    f.render_widget(Paragraph::new(line).block(block).scroll((0, scroll)), area);
}

/// Your feeds and the last search, numbered for the 1–5 keys.
fn render_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    let mut x = area.x + 1;
    for (i, tab) in app.tabs().into_iter().enumerate() {
        let on = tab == app.source;
        let loading = app.fetching.as_ref().is_some_and(|(s, _, _)| *s == tab);
        let mut title = tab.title();
        if title.chars().count() > 24 {
            title = format!("{}…", title.chars().take(23).collect::<String>());
        }
        let mark = if loading { app.spinner().to_string() } else { (i + 1).to_string() };
        // The tab you're on is a tag in the accent; the others are its number
        // in the highlight and the name, quiet.
        let line = if on {
            Line::from(th::tag(format!(" {mark} {} ", title.to_uppercase()), th::accent()))
        } else {
            Line::from(vec![
                Span::styled(format!(" {mark} "), Style::new().fg(th::highlight())),
                Span::styled(format!("{title} "), Style::new().fg(th::dim())),
            ])
        };
        let width = (line.width() as u16).min(area.right().saturating_sub(x));
        if width == 0 {
            break;
        }
        let r = Rect { x, y: area.y, width, height: 1 };
        f.render_widget(line, r);
        app.layout.tabs.push((r, tab));
        x += width + 1;
    }
}

fn render_results(f: &mut Frame, app: &mut App, area: Rect) {
    app.layout.results = area;
    let focused = app.focus == Focus::Results;
    let name = app.source.title();
    let title = match (app.results.len(), app.list.selected()) {
        (0, _) => format!(" {name} "),
        (n, Some(i)) => format!(" {name} {}/{} ", i + 1, n),
        (n, None) => format!(" {name} {n} "),
    };
    let block = th::panel(title, focused);

    if app.results.is_empty() {
        let dim = Style::new().fg(th::dim());
        let fetching = app.fetching.as_ref().is_some_and(|(s, _, _)| *s == app.source);
        let lines = if fetching {
            vec![Line::styled(format!("{} loading {}…", app.spinner(), name), dim)]
        } else if !matches!(app.source, Source::Search(_)) && !app.logged_in {
            vec![
                Line::styled("Your feeds need your YouTube login.", Style::new().fg(th::text()).bold()),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("Add ", dim),
                    Span::styled("cookies_from_browser = firefox", Style::new().fg(th::highlight())),
                    Span::styled(" (or chrome, safari…)", dim),
                ]),
                Line::from(vec![
                    Span::styled("to ", dim),
                    Span::styled("~/.config/yt-tui/config", Style::new().fg(th::highlight())),
                    Span::styled(" and restart. Search works without it.", dim),
                ]),
            ]
        } else if matches!(app.source, Source::Search(_)) {
            vec![Line::styled("Nothing found.", dim)]
        } else {
            vec![Line::styled("Nothing here — press r to reload.", dim)]
        };
        // The theme's picture over the message, when there's room for it.
        let mut text = vec![Line::raw("")];
        if area.height >= 16 && !fetching {
            text.extend(th::scene(area.width.saturating_sub(4)));
            text.push(Line::raw(""));
        }
        text.extend(lines);
        // No trimming: it would strip the picture's leading spaces.
        let message = Paragraph::new(Text::from(text)).alignment(Alignment::Center).wrap(Wrap { trim: false });
        f.render_widget(message.block(block), area);
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
            // The armed row: an edge in the accent with a pointer in the
            // highlight, its title in the accent→highlight gradient.
            let bar = if on {
                let (edge, pointer) = th::cursor();
                vec![Span::styled(edge, Style::new().fg(th::accent())), Span::styled(pointer, Style::new().fg(th::highlight()))]
            } else {
                vec![Span::raw("  ")]
            };
            let mut first = bar.clone();
            if on {
                first.extend(th::gradient(&truncate(&v.title, width), th::accent(), th::highlight(), Style::new().bold()));
            } else {
                first.push(Span::styled(truncate(&v.title, width), Style::new().fg(th::text())));
            }
            let mut meta = if on { vec![Span::styled("▌ ", Style::new().fg(th::accent()))] } else { vec![Span::raw("  ")] };
            meta.extend(meta_spans(v));
            ListItem::new(vec![Line::from(first), Line::from(meta), Line::raw("")])
        })
        .collect();
    let list = List::new(items).block(block).highlight_style(Style::new().bg(th::selected()));
    f.render_stateful_widget(list, area, &mut app.list);
}

fn meta_spans(v: &Video) -> Vec<Span<'static>> {
    let dim = Style::new().fg(th::dim());
    let mut spans = Vec::new();
    if v.is_live() {
        spans.extend(th::tag(" LIVE ", th::accent()));
        spans.push(Span::raw("  "));
    } else if let Some(d) = v.duration {
        spans.push(Span::styled(format!("{}  ", format_duration(d)), Style::new().fg(th::mid())));
    }
    spans.push(Span::styled(v.channel().to_string(), dim));
    if let Some(views) = v.view_count {
        spans.push(Span::styled(format!(" · {}", format_views(views)), dim));
    }
    spans
}

fn render_preview(f: &mut Frame, app: &mut App, area: Rect) {
    let block = th::panel("Preview", false);
    let inner = block.inner(area).inner(Margin::new(1, 0));
    f.render_widget(block, area);
    let Some(video) = app.selected().cloned() else { return };

    let art_height = (inner.height * 55 / 100).max(3);
    let [art, _, text] =
        Layout::vertical([Constraint::Length(art_height), Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    render_art(f, app, &video, art);

    let mut lines = vec![Line::from(th::gradient(&video.title, th::accent(), th::highlight(), Style::new().bold())), Line::raw("")];
    lines.push(Line::from(meta_spans(&video)));
    if let Some(desc) = video.description.as_deref().filter(|d| !d.trim().is_empty()) {
        lines.push(Line::raw(""));
        lines.extend(desc.lines().take(20).map(|l| Line::styled(l.to_string(), Style::new().fg(th::dim()))));
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
        let frame = th::panel("", false);
        let inner = frame.inner(boxed);
        f.render_widget(frame, boxed);
        f.render_widget(
            Paragraph::new(label).style(Style::new().fg(th::dim())).alignment(Alignment::Center),
            Rect { y: inner.y + inner.height / 2, height: 1.min(inner.height), ..inner },
        );
    }
}

fn render_footer(f: &mut Frame, app: &mut App, area: Rect) {
    if let Some(s) = app.status_text() {
        let style = if s.error { Style::new().fg(th::accent()) } else { Style::new().fg(th::dim()) };
        f.render_widget(Line::styled(format!(" {}", s.text), style), area);
        return;
    }
    if !app.missing.is_empty() {
        let text = format!(" missing: {} — install with brew", app.missing.join(", "));
        f.render_widget(Line::styled(text, Style::new().fg(th::accent())), area);
        return;
    }
    let keys: &[(&'static str, &'static str)] = match app.focus {
        Focus::Search => &[("enter", "search"), ("tab", "results"), ("ctrl-u", "clear"), ("esc", "back")],
        Focus::Results => &[
            ("enter", "play"),
            ("a", "audio only"),
            ("/", "search"),
            ("1-5", "tabs"),
            ("r", "reload"),
            ("?", "help"),
            ("q", "quit"),
        ],
    };
    let line = hints(app, keys, area, false);
    f.render_widget(line, area);
}

/// Key hints along `area` (from its left, or flush right), each recorded as a
/// button for its key so the mouse can press it.
fn hints(app: &mut App, keys: &[(&'static str, &'static str)], area: Rect, right: bool) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    let mut widths = Vec::new();
    for (k, what) in keys {
        let hint = th::hint(k, what);
        widths.push((*k, hint.iter().map(|s| s.width() as u16).sum::<u16>()));
        spans.extend(hint);
    }
    let line = Line::from(spans);
    let total = line.width() as u16;
    let mut x = if right { area.x + area.width.saturating_sub(total) } else { area.x } + 1;
    for (key, width) in widths {
        // the hint without the gap after it
        let r = Rect { x, y: area.y, width: width.saturating_sub(3), height: 1 }.intersection(area);
        if r.width > 0 {
            app.layout.buttons.push((r, key));
        }
        x += width;
    }
    line
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
        let mut spans = th::tag(" ▶ ", th::accent());
        spans.push(Span::raw(" "));
        spans.extend(th::gradient(&player.stream.title, th::accent(), th::highlight(), Style::new().bold()));
        spans.push(Span::styled(format!("  {} {}", th::mark(), player.video.channel().to_uppercase()), Style::new().fg(th::dim())));
        let line = Line::from(spans);
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
        f.render_widget(Line::styled(label, Style::new().fg(th::highlight()).bg(th::selected()).bold()), r);
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
        let mut spans = th::tag(" ▶ ", th::accent());
        spans.push(Span::raw(" "));
        spans.extend(th::gradient(&loading.video.title, th::accent(), th::highlight(), Style::new().bold()));
        f.render_widget(Line::from(spans), t);
    }
    let what = if loading.audio_only { "audio" } else { "video" };
    // The theme's picture while it loads, when there's room.
    let mut lines = Vec::new();
    if stage.height >= 16 {
        lines.extend(th::scene(stage.width));
        lines.push(Line::raw(""));
    }
    lines.extend([
        Line::styled(format!("{} FINDING THE {} STREAM…", app.spinner(), what.to_uppercase()), Style::new().fg(th::highlight()).bold()),
        Line::styled(format!("{:.1}s", loading.since.elapsed().as_secs_f32()), Style::new().fg(th::mid())),
        Line::raw(""),
        Line::styled("esc to cancel", Style::new().fg(th::dim())),
    ]);
    let height = (lines.len() as u16).min(stage.height);
    let text = Text::from(lines).centered();
    let r = Rect { y: stage.y + stage.height.saturating_sub(height) / 2, height, ..stage };
    f.render_widget(Paragraph::new(text).alignment(Alignment::Center), r);
}

/// Audio-only: the thumbnail as cover art, with the title under it.
fn render_audio_stage(f: &mut Frame, app: &mut App, stage: Rect) {
    let Some(player) = &app.player else { return };
    let video = player.video.clone();
    let [art, _, text] =
        Layout::vertical([Constraint::Percentage(70), Constraint::Length(1), Constraint::Min(2)]).areas(stage.inner(Margin::new(2, 1)));
    render_art(f, app, &video, art);
    let lines = vec![
        Line::from(th::gradient(&video.title, th::accent(), th::highlight(), Style::new().bold())),
        Line::styled(format!("{} {}", th::mark(), video.channel().to_uppercase()), Style::new().fg(th::dim())),
    ];
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center).wrap(Wrap { trim: true }), text);
}

fn render_progress(f: &mut Frame, player: &Player, area: Rect) {
    let area = Rect { x: area.x + 1, width: area.width.saturating_sub(2), ..area };
    if player.stream.live {
        let mut spans = th::tag(" LIVE ", th::accent());
        spans.push(Span::raw(" "));
        spans.extend(th::gradient(&"━".repeat(area.width.saturating_sub(9) as usize), th::accent(), th::highlight(), Style::new()));
        let line = Line::from(spans);
        f.render_widget(line, area);
        return;
    }
    let Some(duration) = player.duration() else { return };
    let w = area.width as usize;
    let frac = (player.position() / duration).clamp(0.0, 1.0);
    let filled = ((w.saturating_sub(1)) as f64 * frac).round() as usize;
    // Played: an accent→highlight gradient; the head in the warm colour; the rest faint.
    let end = th::mix(th::accent(), th::highlight(), frac);
    let (done, head, todo) = th::track();
    let mut spans = th::gradient(&done.repeat(filled), th::accent(), end, Style::new());
    spans.push(Span::styled(head, Style::new().fg(th::mid()).bold()));
    spans.push(Span::styled(todo.repeat(w.saturating_sub(filled + 1)), Style::new().fg(th::faint())));
    let line = Line::from(spans);
    f.render_widget(line, area);
}

fn render_controls(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(player) = &app.player else { return };
    let state = if player.paused() { " ❚❚ " } else { " ▶ " };
    let time = match player.duration() {
        Some(d) if !player.stream.live => format!("{} / {}", format_duration(player.position()), format_duration(d)),
        _ => format_duration(player.position()),
    };
    let mut left = vec![Span::raw(" ")];
    let tag = th::tag(state, if player.paused() { th::mid() } else { th::accent() });
    // the play / pause tag is a button
    let tag_width = tag.iter().map(|s| s.width() as u16).sum::<u16>();
    let pause_button = Rect { x: area.x + 1, y: area.y, width: tag_width, height: 1 }.intersection(area);
    left.extend(tag);
    left.push(Span::styled(format!(" {time}"), Style::new().fg(th::highlight()).bold()));
    if let Some((vol, muted)) = player.volume() {
        let v = if muted { "muted".to_string() } else { format!("vol {:.0}%", vol) };
        left.push(Span::styled(format!("   {v}"), Style::new().fg(th::dim())));
    }
    if let Some((w, h, fps)) = player.decode_size() {
        left.push(Span::styled(
            format!("   {w}×{h} · {}/{:.0} fps", player.fps_now(), fps),
            Style::new().fg(th::dim()),
        ));
    }
    f.render_widget(Line::from(left), area);
    app.layout.buttons.push((pause_button, "space"));
    if area.width > 100 {
        let keys = hints(app, &[("space", "pause"), ("←→", "seek"), ("↑↓", "volume"), ("f", "fullscreen"), ("n", "next"), ("q", "back")], area, true);
        f.render_widget(keys.alignment(Alignment::Right), area);
    }
    if let Some(s) = app.status_text().filter(|s| s.error) {
        let w = (s.text.chars().count() as u16 + 2).min(area.width);
        let r = Rect { x: area.x + area.width - w, width: w, ..area };
        f.render_widget(Clear, r);
        f.render_widget(Line::styled(format!(" {}", s.text), Style::new().fg(th::accent())), r);
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
            ("1 – 5 / [ ]", "switch tab: feeds, search"),
            ("r", "reload this tab"),
            ("A", "toggle autoplay"),
            ("t", "next theme: neon, stark, arcane"),
            ("q", "quit"),
        ]
    };
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!("{k:>16}  "), Style::new().fg(th::highlight()).bold()),
                Span::styled(what.to_string(), Style::new().fg(th::text())),
            ])
        })
        .collect();
    let h = lines.len() as u16 + 2;
    let w = 52.min(f.area().width);
    let area = f.area();
    let r = Rect { x: area.x + (area.width - w) / 2, y: area.y + area.height.saturating_sub(h) / 2, width: w, height: h.min(area.height) };
    f.render_widget(Clear, r);
    let block = th::panel("Keys", true)
        .title_bottom(Line::styled(" any key to close ", Style::new().fg(th::dim())).right_aligned());
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
        let settings = Settings { max_height: 1080, limits: Limits { max_width: 1920, max_fps: 30 }, results: 30, volume: 0.0, theme: Some(crate::theme::Theme::Neon) };
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

    /// Each key hint is recorded as a button exactly over its tag and label.
    #[test]
    fn hint_buttons_sit_on_their_hints() {
        for theme in crate::theme::Theme::ALL {
            crate::theme::set(theme);
            let mut app = app();
            app.focus = Focus::Results;
            let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
            term.draw(|f| render(f, &mut app)).unwrap();
            let buffer = term.backend().buffer().clone();
            assert!(!app.layout.buttons.is_empty());
            for (r, key) in &app.layout.buttons {
                let text: String = (r.x..r.x + r.width).map(|x| buffer[(x, r.y)].symbol().to_string()).collect();
                assert!(text.to_lowercase().contains(&key.to_lowercase()), "{theme:?}: {key} → {text:?}");
            }
        }
        crate::theme::set(crate::theme::Theme::Neon);
    }
}

