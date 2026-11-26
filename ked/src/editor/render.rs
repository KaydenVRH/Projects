//! Drawing the main view: buffer, tab bar, status line, splash screen,
//! markdown preview and file tree.

use super::*;

impl Editor {
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

        // Markdown preview pane (Ctrl+V): splits the remaining editor
        // area, leaving the editor fully editable.
        let is_markdown = highlight::detect_lang(
            self.filename.as_deref(),
            self.lines.first().map(|s| s.as_str()),
        ) == highlight::Lang::Markdown;
        let (editor_area, preview_area) = if self.preview && is_markdown {
            let [e, pv] = Layout::horizontal([
                Constraint::Min(1),
                Constraint::Percentage(45),
            ])
            .areas(editor_area);
            (e, Some(pv))
        } else {
            (editor_area, None)
        };

        // ── 1. main content / splash ────────────────────────────
        let mut theme = self.theme.theme();
        // Search matches get an extra "glow" while an fx mode is on:
        // the same shift as the theme, but amplified.
        let mut search_glow: Option<Color> = None;
        if self.fx_mode > 0 {
            // Quantise time to ~30 steps/sec: a full 60fps shift
            // recolours every token every frame, forcing a full
            // repaint (and cursor strobe) every frame.
            let elapsed = (self.fx_start.elapsed().as_secs_f64() * 30.0).floor() / 30.0;
            match self.fx_mode {
                1 => { // gentle: very slow ±15° hue oscillation
                    let hue = (elapsed * 3.0).sin() * 15.0;
                    theme = soft_shift_theme(&theme, hue, 0.0);
                    search_glow = Some(shift_color(theme.selection_bg, hue * 3.0, 0.12));
                }
                2 => { // breathing: very slow lightness pulse
                    let lightness = ((elapsed * 0.3).sin() * 0.5 + 0.5) * 0.08;
                    theme = soft_shift_theme(&theme, 0.0, lightness);
                    search_glow = Some(shift_color(theme.selection_bg, 0.0, lightness * 2.5));
                }
                3 => { // warm: very slow drift between amber and purple
                    let hue = (elapsed * 2.0).sin() * 25.0 + 270.0;
                    theme = soft_shift_theme(&theme, hue, 0.0);
                    search_glow = Some(shift_color(theme.selection_bg, hue * 2.5, 0.12));
                }
                _ => {}
            }
        } else if self.animations && self.last_search.is_some() {
            // No fx mode: search matches still pulse gently, like the
            // active-tab shimmer.
            let pulse = 0.5 + 0.5 * (self._frame as f64 * 0.12).sin();
            search_glow = Some(blend_colors(
                theme.selection_bg,
                theme.fg,
                0.0,
                (pulse * 0.12) as f32,
            ));
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

        // Inline images (kitty graphics protocol): find the markdown
        // references in view and the cell rectangle each occupies.
        let md_images = if self.inline_images && self.graphics.supported && is_markdown {
            crate::graphics::visible_images(
                &self.lines,
                self.cy,
                self.top,
                visible_lines,
                self.filename.as_deref().and_then(|f| Path::new(f).parent()),
                visible_content_cols as u16,
                &self.graphics,
                self.mode == Mode::Insert,
            )
        } else {
            Vec::new()
        };

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

            // A row covered by an inline image: leave the cells blank;
            // the image itself is placed after the frame is painted.
            if md_images
                .iter()
                .any(|im| buf_row >= im.line && buf_row < im.line + im.rows as usize)
            {
                lines_vec.push(
                    Line::from(vec![
                        Span::styled(
                            format!("{:>width$}│", buf_row + 1, width = gutter - 1),
                            theme.line_number,
                        ),
                        Span::raw(" ".repeat(visible_content_cols)),
                    ])
                    .style(line_style),
                );
                continue;
            }

            // (a) line number gutter
            let line_num = format!("{:>width$}│", buf_row + 1, width = gutter - 1);
            let num_span = Span::styled(line_num, theme.line_number);

            // (b) highlighted content (tree-sitter, cached per edit).
            // Markdown lines render from concealed display segments;
            // on the cursor line, markers under the cursor are
            // revealed.
            let raw = &self.lines[buf_row];
            let md_segs: Option<Vec<highlight::MdSeg>> = self
                .highlight
                .as_ref()
                .and_then(|h| h.md_line(buf_row))
                .map(|s| s.to_vec());
            let md_segs = match &md_segs {
                Some(segs) if buf_row == self.cy => {
                    let abs = self.md_line_start(buf_row) + self.cx;
                    Some(md_cursor_segs(segs, abs))
                }
                other => other.clone(),
            };
            let highlighted = if let Some(segs) = &md_segs {
                md_seg_spans(segs, &theme)
            } else {
                match &self.highlight {
                    Some(h) => h.spans(buf_row, &theme),
                    None => vec![Span::styled(raw.to_string(), style)],
                }
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
                            // Search matches glow with the fx mode
                            // (brighter than the rest of the theme).
                            let match_bg = search_glow.unwrap_or(theme.selection_bg);
                            let match_style = Style::default()
                                .bg(match_bg)
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
                    // On concealed markdown lines the selection edges
                    // are display-space offsets.
                    let (sel_start, sel_end, full_len) = if let Some(segs) = &md_segs {
                        let total: usize = segs.iter().map(|s| s.display.len()).sum();
                        (
                            if buf_row == sy { md_segs_display_byte(segs, sx) } else { 0 },
                            if buf_row == ey { md_segs_display_byte(segs, ex) } else { total },
                            total,
                        )
                    } else {
                        (
                            if buf_row == sy { sx } else { 0 },
                            if buf_row == ey { ex } else { raw.len() },
                            raw.len(),
                        )
                    };
                    if fully || (sel_start <= 0 && sel_end >= full_len) {
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
            // frame never desynchronises.  Markdown lines clip their
            // concealed display spans by cell directly.
            let mut line_spans = if let Some(segs) = &md_segs {
                let left_col = md_segs_col(segs, self.md_line_start(buf_row) + self.left);
                let mut v = vec![spans[0].clone()];
                v.extend(clip_spans_cells(
                    spans[1..].to_vec(),
                    left_col,
                    visible_content_cols,
                ));
                v
            } else {
                visible_spans(raw, spans, self.left, visible_content_cols)
            };
            // Right-gutter marker for externally changed lines.
            decorate_change_marker(
                &mut line_spans,
                buf_row,
                visible_content_cols,
                line_style,
                &theme,
                &self.line_changes,
                self.change_age,
            );
            lines_vec.push(Line::from(line_spans).style(line_style));
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
        // Queue kitty-graphics placements for the images blanked above.
        self.queue_md_images(&md_images, editor_area, gutter);
        } // end else (normal content)

        // ── 2. buffer bar ──────────────────────────────────────────
        self.render_buffer_bar(f, bufbar_area, &theme);

        // ── 3. status bar ────────────────────────────────────────
        self.render_status(f, status_area, width, &theme);

        // ── 4. cursor position ───────────────────────────────────
        let prompt: Option<(&str, &str)> = match self.state {
            State::Command => Some((":", self.cmd_buf.as_str())),
            State::Search => Some(("/", self.search_query.as_str())),
            State::Memo => Some(("Remember: ", self.memo_buf.as_str())),
            _ => None,
        };
        if let Some((prefix, buf)) = prompt {
            let pw = col_width(prefix) as u16;
            let buf_width = status_area.width.saturating_sub(pw).max(1);
            let (_, off) = prompt_view(buf, self.prompt_cursor, buf_width);
            let x = (status_area.x + pw + off)
                .min(status_area.x + status_area.width.saturating_sub(1));
            self.cursor_screen = Some((x, status_area.y));
            f.set_cursor_position(Position::new(x, status_area.y));
        } else if self.state == State::Shell || self.state == State::Hermes {
            // These overlays set (or hide) the cursor themselves; parking the
            // buffer cursor here as well would drag the caret back out of the
            // popup — which is what made the chat's message line look broken.
            self.cursor_screen = None;
        } else if is_splash {
            self.cursor_screen = Some((0, 0));
            f.set_cursor_position(Position::new(0, 0));
        } else if let Some(gutter) = self.gutter_width().checked_add(1) {
            let cy_screen = (self.cy.saturating_sub(self.top)) as u16;
            // Cursor column in cells: convert byte offsets through
            // visual widths (concealed markdown chars contribute
            // nothing) so the cursor always sits on its character.
            let line = &self.lines[self.cy];
            let cx = line.floor_char_boundary(self.cx.min(line.len()));
            let left = line.floor_char_boundary(self.left.min(line.len()));
            let cx_screen = self
                .display_col(cx)
                .saturating_sub(self.display_col(left)) as u16
                + gutter as u16;
            if cy_screen < editor_area.height {
                let cursor_x = editor_area.x + cx_screen;
                let cursor_y = editor_area.y + cy_screen;
                self.cursor_screen = Some((cursor_x, cursor_y));
                f.set_cursor_position(Position::new(cursor_x, cursor_y));
            }
        }

        // ── 4. overlays ──────────────────────────────────────────
        // The preview pane draws under popups but over the editor.
        if let Some(pv) = preview_area {
            self.render_preview(f, pv, &theme);
        }
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
            State::Hermes => self.render_hermes(f, area),
            State::FileTree => {
                if let Some(tree_area) = tree_area {
                    self.render_filetree(f, tree_area, &theme);
                }
            }
            State::SysInfo => self.render_sysinfo(f, area, &theme),
            State::Help => self.render_help(f, area, &theme),
            State::Memories => self.render_memories(f, area, &theme),
            State::Memo => {}
            State::Planner => self.render_planner(f, area, &theme),
            _ => {}
        }

        // Cursor shape per mode (applied in main.rs after draw)
        let prompt_active =
            matches!(self.state, State::Command | State::Search | State::Memo);
        self.cursor_style = if prompt_active {
            SetCursorStyle::BlinkingBar
        } else {
            match self.mode {
                Mode::Insert => SetCursorStyle::BlinkingBar,
                Mode::Normal if self.state == State::Visual => {
                    SetCursorStyle::SteadyUnderScore
                }
                _ => SetCursorStyle::SteadyBlock,
            }
        };
    }

    // ── status bar ───────────────────────────────────────────────

    /// Render a one‑line buffer (tab) bar at the top of the screen.
    pub(super) fn render_buffer_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Split: tabs on the left, editor context + music + clock on
        // the right (when bar_stats is on).  The right side sizes
        // itself to its content, so it collapses when nothing is
        // playing and grows (up to a cap) to fit the song name.
        let lang = highlight::detect_lang(
            self.filename.as_deref(),
            self.lines.first().map(|s| s.as_str()),
        )
        .name();
        let buf_str = if self.buffers.len() > 1 {
            format!(" {}/{} ", self.current + 1, self.buffers.len())
        } else {
            String::new()
        };
        let context = format!(" {lang} {buf_str}{}L", self.lines.len());
        let music = now_playing_text(
            self.music_player.playing,
            self.music_player.current_song.as_deref(),
            self._frame,
            MUSIC_MAX,
        );
        let clock = chrono::Local::now().format(" %H:%M:%S").to_string();

        // Segments joined by dim `│` separators so the scrolling song
        // name reads as its own thing.
        let plain = Style::new().fg(theme.status_fg).bg(theme.status_bg);
        let dim = Style::new()
            .fg(theme.comment.fg.unwrap_or(theme.status_fg))
            .bg(theme.status_bg);
        let mut bar_spans: Vec<Span> = vec![Span::styled(context.clone(), plain)];
        let mut text_w = context.chars().count();
        if !music.is_empty() {
            bar_spans.push(Span::styled(" │ ", dim));
            bar_spans.push(Span::styled(music.clone(), plain));
            text_w += 3 + music.chars().count();
        }
        bar_spans.push(Span::styled(" │ ", dim));
        bar_spans.push(Span::styled(clock.clone(), plain));
        text_w += 3 + clock.chars().count();
        bar_spans.push(Span::styled(" ", plain));
        text_w += 1;

        let info_w = if self.bar_stats {
            (text_w as u16 + 1).min(area.width.saturating_sub(24))
        } else {
            0
        };
        let [tabs_area, info_area] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(info_w),
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

        // ── right side: context + music + clock ──
        if !self.bar_stats {
            return;
        }

        let [sep_area, text_area] = Layout::horizontal([
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(info_area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                if theme.status_bg == Color::Reset { " " } else { "" },
                Style::new().fg(theme.status_bg).bg(theme.bg))))
            .style(Style::new().bg(theme.status_bg)),
            sep_area,
        );
        f.render_widget(
            Paragraph::new(Line::from(bar_spans))
                .alignment(Alignment::Right)
                .style(Style::new().bg(theme.status_bg)),
            text_area,
        );
    }

    /// Render the status bar at the bottom of the screen — lualine
    /// style: a coloured mode block on the left, and everything else
    /// floating on a transparent background.
    pub(super) fn render_status(&self, f: &mut Frame, area: Rect, _width: usize, theme: &Theme) {
        f.render_widget(ratatui::widgets::Clear, area);

        // Flash / command / search take over the whole line (plain
        // floating text, no background fill).
        if let Some(ref msg) = self.flash {
            let bar = Paragraph::new(Line::from(Span::styled(
                format!(" {msg} "),
                Style::new().fg(theme.fg),
            )));
            f.render_widget(bar, area);
            return;
        }
        // Active prompt (command / search / remember): fixed accent
        // prefix + horizontally scrolled editable text.
        let prompt: Option<(&str, &str)> = match self.state {
            State::Command => Some((":", self.cmd_buf.as_str())),
            State::Search => Some(("/", self.search_query.as_str())),
            State::Memo => Some(("Remember: ", self.memo_buf.as_str())),
            _ => None,
        };
        if let Some((prefix, buf)) = prompt {
            let pw = col_width(prefix) as u16;
            let buf_width = area.width.saturating_sub(pw).max(1);
            let (visible, _) = prompt_view(buf, self.prompt_cursor, buf_width);
            let bar = Paragraph::new(Line::from(vec![
                Span::styled(
                    prefix.to_string(),
                    Style::new()
                        .fg(theme.keyword.fg.unwrap_or(theme.fg))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(visible, Style::new().fg(theme.fg)),
            ]));
            f.render_widget(bar, area);
            return;
        }

        // ── mode block (lualine section "a") ──
        // Bold, near-black text on a per-mode accent colour from the
        // active theme (like the lualine config in ~/.config/nvim).
        let (mode_text, accent) = if self.state == State::Visual {
            (" VISUAL ", theme.rstype.fg.unwrap_or(theme.status_fg))
        } else {
            match self.mode {
                Mode::Insert => (" INSERT ", theme.builtin.fg.unwrap_or(theme.status_fg)),
                Mode::Normal => (" NORMAL ", theme.keyword.fg.unwrap_or(theme.status_fg)),
            }
        };
        let block = Span::styled(
            mode_text,
            Style::new()
                .fg(crate::theme::contrast_fg(accent))
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        );

        // ── right: reminders + theme + location (dim) ──
        let dim = Style::new().fg(theme.comment.fg.unwrap_or(theme.fg));
        let location = format!("{}:{}", self.cy + 1, self.cx + 1);
        let now = memory::now();
        let due = self.memory.due_count(now);
        let next = self.memory.next_due(now);
        let mut right_spans = Vec::new();
        if due > 0 || next.is_some() {
            let txt = match (due, next) {
                (d, Some(n)) if d > 0 => {
                    format!("⏰ {d} due · {}", n.format("%H:%M"))
                }
                (d, None) if d > 0 => format!("⏰ {d} due"),
                (_, Some(n)) => format!("⏰ {}", n.format("%H:%M")),
                _ => String::new(),
            };
            if !txt.is_empty() {
                right_spans.push(Span::styled(
                    txt,
                    Style::new().fg(theme.keyword.fg.unwrap_or(theme.fg)),
                ));
                right_spans.push(Span::styled("  ─  ", dim));
            }
        }
        right_spans.push(Span::styled(self.theme.name().to_string(), dim));
        right_spans.push(Span::styled("  ─  ", dim));
        right_spans.push(Span::styled(location, dim));
        right_spans.push(Span::styled(" ", dim));
        let right = Line::from(right_spans);

        // Give the right section only the width it needs, so a long
        // filename gets the rest of the bar instead of being clipped
        // at the halfway point.  Always keep room for the mode block.
        let max_right = area.width.saturating_sub(10);
        let right_w = (right.width() as u16).min(max_right);
        let left_w = area.width.saturating_sub(right_w);

        // ── left: mode block + filename (elided from the left) ──
        let bright = Style::new().fg(theme.fg);
        let fname = self.filename.as_deref().unwrap_or("[No Name]");
        let mod_str = if self.modified { " +" } else { "" };
        let mode_w = col_width(block.content.as_ref());
        let avail = (left_w as usize).saturating_sub(mode_w + 2 + mod_str.len());
        let shown = elide_left(fname, avail);
        let left = Line::from(vec![
            block,
            Span::styled(format!("  {shown}{mod_str}"), bright),
        ]);

        let [left_area, right_area] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(right_w),
        ])
        .areas(area);
        f.render_widget(Paragraph::new(left), left_area);
        f.render_widget(
            Paragraph::new(right).alignment(Alignment::Right),
            right_area,
        );
    }

    // ── finder overlay ───────────────────────────────────────────

     fn render_finder(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {

        // The standard centered modal.
        let popup = modal_rect(area, MODAL_W, MODAL_H);
        f.render_widget(Clear, popup);

        // Uniform overlay frame with scrolling title.
        let block = overlay_block("Find File", self._frame, self.animations, theme);
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

        // Query bar (status-bar style line) + navigation hint.
        let query_text = if self.finder.scanning && self.finder.files.is_empty() {
            " scanning…".to_string()
        } else if self.finder_query.is_empty() {
            " type to search…".to_string()
        } else {
            format!(" {}", self.finder_query)
        };
        let hint = if self.finder.scanning {
            " scanning files… ".to_string()
        } else {
            format!(
                " {} files · ↑↓/Ctrl+N/P · Enter · Esc ",
                self.finder.files.len()
            )
        };
        let [q_area, hint_area] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(hint.chars().count() as u16),
        ])
        .areas(query_area);
        let query_span = Span::styled(
            query_text,
            Style::new().fg(theme.status_fg).bg(theme.status_bg),
        );
        f.render_widget(
            Paragraph::new(Line::from(query_span))
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
                if i == self.finder_selection {
                    return ListItem::new(Line::from(Span::styled(
                        path.clone(),
                        selected_style(theme),
                    )));
                }
                // Directory dim, file name bright.
                let split = path.rfind('/').map(|p| p + 1).unwrap_or(0);
                let (dir, name) = path.split_at(split);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        dir.to_string(),
                        Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)).bg(theme.bg),
                    ),
                    Span::styled(name.to_string(), Style::new().fg(theme.fg).bg(theme.bg)),
                ]))
            })
            .collect();

        if results.is_empty() {
            let msg = if self.finder.scanning && self.finder.files.is_empty() {
                " scanning…"
            } else {
                " (no files found)"
            };
            let no_files = vec![ListItem::new(Line::from(Span::styled(
                msg,
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

    pub(super) fn render_splash(&self, f: &mut Frame, area: Rect, theme: &Theme) {
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
            ("Ctrl+Shift+H", "hermes"),
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
            .map(|(_, d)| key_w + 2 + d.len())
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

    // ── markdown preview pane (Ctrl+V) ──────────────────────────

    /// Render the live markdown preview pane: our own tree-sitter-md
    /// renderer, or `glow` when it's installed.  Follows the cursor.
    pub(super) fn render_preview(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::LEFT)
            .title(" Preview ")
            .title_style(Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)))
            .border_style(Style::new().fg(theme.border));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.width < 3 || inner.height < 1 {
            return;
        }

        let hash = hash_lines(&self.lines) ^ ((inner.width as u64) << 32);
        let stale = match &self.preview_cache {
            Some((h, _)) => *h != hash,
            None => true,
        };
        if stale {
            let rendered = if self.glow_installed() {
                PreviewRendered::Lines(self.render_with_glow())
            } else {
                PreviewRendered::Chunks(render_markdown(&self.lines, theme))
            };
            self.preview_cache = Some((hash, rendered));
        }

        match self.preview_cache.as_ref().map(|(_, r)| r) {
            Some(PreviewRendered::Chunks(chunks)) => {
                // Flatten, attributing each display line to the source
                // line of its chunk, then follow the cursor.
                let mut display: Vec<Line> = Vec::new();
                let mut srcs: Vec<usize> = Vec::new();
                for ch in chunks {
                    for l in &ch.lines {
                        display.push(l.clone());
                        srcs.push(ch.src_start);
                    }
                }
                let target = srcs.iter().rposition(|s| *s <= self.cy).unwrap_or(0);
                let scroll = target.saturating_sub(inner.height as usize / 3);
                f.render_widget(
                    Paragraph::new(Text::from(display))
                        .wrap(Wrap { trim: false })
                        .scroll((scroll as u16, 0))
                        .style(Style::new().bg(theme.bg)),
                    inner,
                );
            }
            Some(PreviewRendered::Lines(lines)) => {
                // glow output has no source mapping — scroll
                // proportionally through the document.
                let frac = if self.lines.len() > 1 {
                    self.cy as f64 / (self.lines.len() - 1) as f64
                } else {
                    0.0
                };
                let max_scroll = lines.len().saturating_sub(inner.height as usize);
                let scroll = (frac * max_scroll as f64) as u16;
                f.render_widget(
                    Paragraph::new(Text::from(lines.clone()))
                        .wrap(Wrap { trim: false })
                        .scroll((scroll, 0))
                        .style(Style::new().bg(theme.bg)),
                    inner,
                );
            }
            None => {}
        }
    }

    /// Is the `glow` CLI installed?  Checked once per session.
    pub(super) fn glow_installed(&mut self) -> bool {
        if let Some(v) = self.glow_available {
            return v;
        }
        let ok = std::process::Command::new("glow")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        self.glow_available = Some(ok);
        ok
    }

    /// Render the buffer through the `glow` CLI.
    pub(super) fn render_with_glow(&self) -> Vec<Line<'static>> {
        use std::io::Write;
        let default_style = Style::new().fg(self.theme.theme().fg);
        let result = std::process::Command::new("glow")
            .args(["-s", "dark", "-w", "80", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .and_then(|mut child| {
                child
                    .stdin
                    .as_mut()
                    .unwrap()
                    .write_all(self.lines.join("\n").as_bytes())?;
                child.wait_with_output()
            });
        match result {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                crate::shell::ansi_to_lines(&text, default_style)
            }
            Err(_) => Vec::new(),
        }
    }

    // ── file tree panel ─────────────────────────────────────────

    pub(super) fn render_filetree(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::RIGHT)
            .title(" Files ")
            .title_style(Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)))
            .border_style(Style::new().fg(theme.border));
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
                if i == selected {
                    let indent = "  ".repeat(entry.depth);
                    let label = format!("{indent}{prefix}{icon} {}", entry.name);
                    return ListItem::new(Line::from(Span::styled(
                        label,
                        selected_style(theme),
                    )));
                }
                let dim = Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)).bg(theme.bg);
                let is_open = self.filename.as_deref() == Some(entry.path.as_str());
                let name_style = if entry.is_dir {
                    Style::new()
                        .fg(theme.keyword.fg.unwrap_or(theme.fg))
                        .bg(theme.bg)
                        .add_modifier(Modifier::BOLD)
                } else if is_open {
                    Style::new().fg(theme.builtin.fg.unwrap_or(theme.fg)).bg(theme.bg)
                } else {
                    Style::new().fg(theme.fg).bg(theme.bg)
                };
                ListItem::new(Line::from(vec![
                    Span::styled("  ".repeat(entry.depth), dim),
                    Span::styled(prefix.to_string(), dim),
                    Span::styled(format!("{icon} "), dim),
                    Span::styled(entry.name.clone(), name_style),
                ]))
            })
            .collect();

        f.render_widget(
            List::new(visible).style(Style::new().bg(theme.bg)),
            inner,
        );
    }
}
