//! Drawing the popup panels: music, themes, system dashboard, help, run output.

use super::*;

impl Editor {
    pub(super) fn render_music(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {

        let popup = modal_rect(area, MODAL_W, MODAL_H);

        f.render_widget(Clear, popup);

        let block = overlay_block("Music Player", self._frame, self.animations, theme);
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
                    selected_style(theme)
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

    pub(super) fn render_theme(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {

        let popup = modal_rect(area, MODAL_W, MODAL_H);
        f.render_widget(Clear, popup);

        let block = overlay_block("Theme Selector", self._frame, self.animations, theme);
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        // Query bar (1 line) + result list (rest), same shape as the finder.
        if inner.height < 2 {
            return;
        }
        let [query_area, list_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(inner);

        let matches = self.theme_matches();
        // A narrower filter can leave the selection past the end.
        if self.theme_selected >= matches.len() {
            self.theme_selected = matches.len().saturating_sub(1);
        }

        let query_text = if self.theme_query.is_empty() {
            " type to filter themes…".to_string()
        } else {
            format!(" {}", self.theme_query)
        };
        let hint = format!(" {} themes · ↑↓ · Enter · Esc ", matches.len());
        let [q_area, hint_area] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(hint.chars().count() as u16),
        ])
        .areas(query_area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                query_text,
                Style::new().fg(theme.status_fg).bg(theme.status_bg),
            )))
            .style(Style::new().bg(theme.status_bg)),
            q_area,
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                hint,
                Style::new()
                    .fg(theme.comment.fg.unwrap_or(theme.fg))
                    .bg(theme.status_bg),
            )))
            .style(Style::new().bg(theme.status_bg)),
            hint_area,
        );

        let visible_h = list_area.height as usize;

        // Auto-scroll to keep the selected row visible.
        if self.theme_selected < self.theme_scroll {
            self.theme_scroll = self.theme_selected;
        }
        if self.theme_selected >= self.theme_scroll + visible_h {
            self.theme_scroll = self.theme_selected - visible_h + 1;
        }

        let list_items: Vec<ListItem> = matches
            .iter()
            .enumerate()
            .skip(self.theme_scroll)
            .take(visible_h)
            .map(|(i, t)| {
                let name = t.name();
                let is_current = *t == self.theme;
                let style = if i == self.theme_selected {
                    selected_style(theme)
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

        if list_items.is_empty() {
            let none = vec![ListItem::new(Line::from(Span::styled(
                format!(" (no theme matches \"{}\")", self.theme_query),
                Style::new().fg(theme.fg).bg(theme.bg),
            )))];
            f.render_widget(
                List::new(none).style(Style::new().bg(theme.bg)),
                list_area,
            );
        } else {
            f.render_widget(
                List::new(list_items).style(Style::new().bg(theme.bg)),
                list_area,
            );
        }
    }

    // ── system dashboard (:sys) ──────────────────────────────────

    pub(super) fn render_sysinfo(&self, f: &mut Frame, area: Rect, theme: &Theme) {
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
        // Full-screen page with the same frame as every other overlay.
        let block = overlay_block("System Dashboard", self._frame, self.animations, theme);
        let inner = block.inner(area);
        f.render_widget(Clear, area);
        f.render_widget(block, area);

        // info (1) + battery? (1) + gap (1) + log-header (1) + logs (rest)
        let has_batt = batt_line.is_some();
        let mut constraints = vec![
            Constraint::Length(1),          // info
        ];
        if has_batt {
            constraints.push(Constraint::Length(1)); // battery
        }
        constraints.push(Constraint::Length(1));     // gap
        constraints.push(Constraint::Length(1));     // log header
        constraints.push(Constraint::Min(4));       // logs
        let areas = Layout::vertical(constraints).split(inner);
        let mut idx = 0;
        let info_area      = areas[idx]; idx += 1;
        let batt_area      = if has_batt { let r = areas[idx]; idx += 1; Some(r) } else { None };
        let log_head_area  = areas[idx + 1]; // skip gap
        let log_area       = areas[idx + 2];

        // ── Info row ──────────────────────────────────────────────
        let info_text = format!(
            "  CPU: {:.1}%     Memory: {:.1}/{:.1} GB ({:.0}%)     {}     {} ─ {} ─ kernel {}  ",
            cpu_pct,
            mem_used as f64 / (1<<30) as f64,
            mem_total as f64 / (1<<30) as f64,
            mem_pct,
            disk_line,
            host,
            user,
            kernel,
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

    pub(super) fn render_help(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let popup = modal_rect(area, MODAL_W, MODAL_H);
        f.render_widget(Clear, popup);

        let block = overlay_block("Keybinds", self._frame, self.animations, theme);
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        let k = |a: &str| self.keymap.key(a);
        let help_text = vec![
            Line::from(Span::styled("Movement & Editing", theme.keyword)),
            Line::from(format!(
                "  {}/{}/{}/{} move   {}/{} word   {}/{} ends   {}/{} first/last",
                k("left"), k("down"), k("up"), k("right"),
                k("word_next"), k("word_prev"),
                k("line_start"), k("line_end"),
                k("goto_first"), k("goto_last"),
            )),
            Line::from(format!(
                "  {}/{}/{}/{} insert   {}/{} open line   {} delete char   {} delete line",
                k("insert_before"), k("insert_after"),
                k("insert_line_start"), k("insert_line_end"),
                k("open_below"), k("open_above"),
                k("delete_char"), k("delete_line"),
            )),
            Line::from(format!(
                "  {} yank   {}/{} paste   {} undo   {} redo",
                k("yank_line"), k("paste_after"), k("paste_before"),
                k("undo"), k("redo"),
            )),
            Line::from(format!(
                "  {} visual   {}/{} half page   {} exit to normal",
                k("visual"), k("half_page_down"), k("half_page_up"), k("quit"),
            )),
            Line::from(""),
            Line::from(Span::styled("Buffers & Files", theme.keyword)),
            Line::from(format!(
                "  {}/{} switch buffer   :bn/:bp switch buffer",
                k("buffer_next"), k("buffer_prev"),
            )),
            Line::from(format!(
                "  {} find   {} tree   {} save   :e <file> open",
                k("finder"), k("filetree"), k("save"),
            )),
            Line::from(""),
            Line::from(Span::styled("Tools", theme.keyword)),
            Line::from(format!(
                "  {} run   {} music   {} theme   {} shell",
                k("run"), k("music"), k("theme"), k("shell"),
            )),
            Line::from(format!(
                "  {} md preview   {} paste image   :sys dashboard",
                k("preview"), k("paste_image"),
            )),
            Line::from(""),
            Line::from(Span::styled("Commands", theme.keyword)),
            Line::from(format!(
                "  {} search   {}/{} repeat   :w/:wq save   :q/:q! quit",
                k("search"), k("search_next"), k("search_prev"),
            )),
            Line::from("  :keys shows the full effective keymap   :theme <name> switch theme"),
            Line::from(""),
            Line::from(Span::styled("Hermes", theme.keyword)),
            Line::from("  ctrl+shift+h or :hermes chat   :hermes new fresh session"),
            Line::from("  in chat: enter send · shift+enter newline · ctrl+o attach file"),
            Line::from("  ctrl+y copy answer · ctrl+i insert answer · ctrl+u/d scroll · esc close"),
            Line::from(""),
            Line::from(Span::styled("Memories", theme.keyword)),
            Line::from(format!(
                "  {} remember   {} dashboard   :mem <text>   :today journal",
                k("quick_note"), k("memories"),
            )),
            Line::from(Span::styled("Planner", theme.keyword)),
            Line::from(format!(
                "  {} calendar   ←→ day · ↑↓ week · PgUp/PgDn month · t today",
                k("planner"),
            )),
        ];

        f.render_widget(
            Paragraph::new(Text::from(help_text))
                .style(Style::new().fg(theme.fg).bg(theme.bg)),
            inner,
        );
    }

    // ── memories dashboard ───────────────────────────────────────

    pub(super) fn render_run(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let popup = modal_rect(area, MODAL_W, MODAL_H);

        f.render_widget(Clear, popup);

        let block = overlay_block("Run Output", self._frame, self.animations, theme);
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        // Output text — show a scrollable window, anchored at the
        // bottom unless the user has scrolled up (`run_scroll`).
        let inner_h = inner.height as usize;
        let lines: Vec<&str> = self.run_output.split('\n').collect();
        let total = lines.len();
        let max_scroll = total.saturating_sub(inner_h);
        let scroll = max_scroll.saturating_sub(self.run_scroll.min(max_scroll));
        let end = total.min(scroll + inner_h);
        let visible = &lines[scroll..end];
        let text: String = visible.join("\n");
        f.render_widget(
            Paragraph::new(Text::from(text))
                .style(Style::new().fg(theme.fg).bg(theme.bg)),
            inner,
        );
    }
}

/// The now-playing song as a string that fits `width` cells
/// (horizontally scrolling when it doesn't).  Empty when nothing is
/// playing.
pub(super) fn now_playing_text(playing: bool, current_song: Option<&str>, frame: u64, width: usize) -> String {
    let song = match current_song {
        Some(s) if playing => s,
        _ => return String::new(),
    };
    let name = song.rsplit('/').next().unwrap_or(song);
    let label = format!("♫ {name}");
    if width < 6 {
        return String::new();
    }
    if label.chars().count() <= width {
        return label;
    }
    // Scroll at ~3 chars/sec.
    let scrolled: Vec<char> = format!("{label}  {label}").chars().collect();
    let offset = (frame as usize / 20) % (label.chars().count() + 2);
    scrolled[offset..].iter().take(width).collect()
}
