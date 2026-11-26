//! Memories, quick notes, the planner calendar and reminders.

use super::*;

impl Editor {
    /// Open the memories dashboard with a fresh scan from disk.
    pub(super) fn open_memories(&mut self) {
        self.memory.reload();
        self.refresh_mem_rows();
        self.mem_selected = 0;
        self.mem_scroll = 0;
        self.state = State::Memories;
        if let Some(e) = &self.memory.error {
            self.flash = Some(e.clone());
        }
    }

    pub(super) fn start_quick_note(&mut self, origin: State) {
        self.memo_buf.clear();
        self.prompt_cursor = 0;
        self.memo_origin = origin;
        self.state = State::Memo;
    }

    /// Rebuild the dashboard's flat row list from the store's sections.
    pub(super) fn refresh_mem_rows(&mut self) {
        let now = memory::now();
        let sec = self.memory.sections(now);
        let mut rows = Vec::new();
        for (label, idxs) in [
            ("Overdue", &sec.overdue),
            ("Today", &sec.today),
            ("Upcoming", &sec.upcoming),
            ("Recent", &sec.recent),
        ] {
            if idxs.is_empty() {
                continue;
            }
            rows.push(MemRow::Header(label));
            for i in idxs {
                rows.push(MemRow::Entry(*i));
            }
        }
        self.mem_rows = rows;
        let n = self.mem_entry_count();
        self.mem_selected = self.mem_selected.min(n.saturating_sub(1));
    }

    pub(super) fn mem_entry_count(&self) -> usize {
        self.mem_rows
            .iter()
            .filter(|r| matches!(r, MemRow::Entry(_)))
            .count()
    }

    pub(super) fn selected_mem_index(&self) -> Option<usize> {
        let mut n = 0;
        for r in &self.mem_rows {
            if let MemRow::Entry(i) = r {
                if n == self.mem_selected {
                    return Some(*i);
                }
                n += 1;
            }
        }
        None
    }

    pub(super) fn handle_memories_state(&mut self, key: KeyEvent) -> bool {
        let n = self.mem_entry_count();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.state = State::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if n > 0 {
                    self.mem_selected = (self.mem_selected + 1).min(n - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.mem_selected = self.mem_selected.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => self.mem_selected = 0,
            KeyCode::Char('G') | KeyCode::End => {
                if n > 0 {
                    self.mem_selected = n - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(i) = self.selected_mem_index() {
                    let path = self.memory.entries[i].path.clone();
                    self.open_path(&path);
                    self.state = State::Normal;
                }
            }
            KeyCode::Char('n') => self.start_quick_note(State::Memories),
            KeyCode::Char('d') => {
                if let Some(i) = self.selected_mem_index() {
                    let now = memory::now();
                    match self.memory.mark_done(i, now) {
                        Ok(()) => {
                            self.flash = Some("Done".to_string());
                            self.refresh_mem_rows();
                        }
                        Err(e) => self.flash = Some(e),
                    }
                }
            }
            KeyCode::Char('s') => {
                if let Some(i) = self.selected_mem_index() {
                    let now = memory::now();
                    match self.memory.snooze(i, now, chrono::Duration::hours(1)) {
                        Ok(()) => {
                            self.flash = Some("Snoozed 1h".to_string());
                            self.refresh_mem_rows();
                        }
                        Err(e) => self.flash = Some(e),
                    }
                }
            }
            KeyCode::Char('r') => {
                self.memory.reload();
                self.refresh_mem_rows();
            }
            _ => {}
        }
        true
    }

    pub(super) fn handle_memo_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.memo_buf.clear();
                self.prompt_cursor = 0;
                self.state = self.memo_origin;
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.memo_buf);
                let now = memory::now();
                let current = self.filename.clone();
                match self.memory.add(&text, now, current.as_deref()) {
                    Ok(path) => {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("saved")
                            .to_string();
                        self.flash = Some(format!("Remembered → {name}"));
                        if self.memo_origin == State::Memories {
                            self.memory.reload();
                            self.refresh_mem_rows();
                        }
                        if self.memo_origin == State::Planner {
                            self.memory.reload();
                            self.planner_refresh();
                        }
                    }
                    Err(e) => self.flash = Some(format!("memory: {e}")),
                }
                self.prompt_cursor = 0;
                self.state = self.memo_origin;
            }
            _ => {
                edit_prompt(&mut self.memo_buf, &mut self.prompt_cursor, &key);
            }
        }
        true
    }

    // ── planner / calendar ──────────────────────────────────────

    /// Rebuild the month grid and the selected day's agenda.
    pub(super) fn planner_refresh(&mut self) {
        let today = memory::now().date();
        let y = self.planner_month.year();
        let m = self.planner_month.month();
        let start = self.planner_week_start();
        let dates = memory::month_grid(y, m, start);
        self.planner_days = dates
            .into_iter()
            .map(|date| {
                let (reminders, notes) = self.memory.counts_on(date);
                DayCell {
                    date,
                    in_month: date.year() == y && date.month() == m,
                    reminders,
                    notes,
                    is_today: date == today,
                }
            })
            .collect();
        self.planner_entries = self.memory.entries_on(self.planner_day);
        let n = self.planner_entries.len();
        self.planner_sel = self.planner_sel.min(n.saturating_sub(1));
    }

    pub(super) fn planner_week_start(&self) -> u32 {
        if self.mem_cfg.week_start.eq_ignore_ascii_case("sunday") {
            6
        } else {
            0
        }
    }

    pub(super) fn open_planner(&mut self) {
        self.memory.reload();
        let today = memory::now().date();
        self.planner_day = today;
        self.planner_month = month_first(today);
        self.planner_sel = 0;
        self.planner_list_focus = false;
        self.planner_refresh();
        self.state = State::Planner;
    }

    pub(super) fn planner_set_day(&mut self, date: NaiveDate) {
        self.planner_day = date;
        self.planner_month = month_first(date);
        self.planner_sel = 0;
        self.planner_refresh();
    }

    pub(super) fn planner_shift_day(&mut self, days: i64) {
        self.planner_set_day(self.planner_day + chrono::Duration::days(days));
    }

    pub(super) fn planner_shift_month(&mut self, months: i32) {
        let mut y = self.planner_month.year();
        let mut m = self.planner_month.month() as i32 + months;
        while m < 1 {
            m += 12;
            y -= 1;
        }
        while m > 12 {
            m -= 12;
            y += 1;
        }
        let m = m as u32;
        let dim = memory::days_in_month(y, m);
        let day = self.planner_day.day().min(dim);
        self.planner_month =
            NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(self.planner_month);
        self.planner_day = NaiveDate::from_ymd_opt(y, m, day).unwrap_or(self.planner_day);
        self.planner_sel = 0;
        self.planner_refresh();
    }

    pub(super) fn handle_planner_state(&mut self, key: KeyEvent) -> bool {
        let n = self.planner_entries.len();
        match key.code {
            KeyCode::Esc => {
                if self.planner_list_focus {
                    self.planner_list_focus = false;
                } else {
                    self.state = State::Normal;
                }
            }
            KeyCode::Char('q') => self.state = State::Normal,
            KeyCode::Tab | KeyCode::BackTab => {
                self.planner_list_focus = !self.planner_list_focus;
            }
            KeyCode::Char('t') => {
                let today = memory::now().date();
                self.planner_set_day(today);
            }
            KeyCode::PageUp => self.planner_shift_month(-1),
            KeyCode::PageDown => self.planner_shift_month(1),
            KeyCode::Char('h') | KeyCode::Left => self.planner_shift_day(-1),
            KeyCode::Char('l') | KeyCode::Right => self.planner_shift_day(1),
            KeyCode::Char('j') | KeyCode::Down => {
                if self.planner_list_focus {
                    if n > 0 {
                        self.planner_sel = (self.planner_sel + 1).min(n - 1);
                    }
                } else {
                    self.planner_shift_day(7);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.planner_list_focus {
                    self.planner_sel = self.planner_sel.saturating_sub(1);
                } else {
                    self.planner_shift_day(-7);
                }
            }
            KeyCode::Char('n') => {
                self.start_quick_note_for(self.planner_day, State::Planner);
            }
            KeyCode::Enter => {
                if self.planner_list_focus {
                    if let Some(i) = self.planner_entries.get(self.planner_sel) {
                        let path = self.memory.entries[*i].path.clone();
                        self.open_path(&path);
                        self.state = State::Normal;
                    }
                } else if n > 0 {
                    self.planner_list_focus = true;
                }
            }
            KeyCode::Char('d') if self.planner_list_focus => {
                if let Some(i) = self.planner_entries.get(self.planner_sel).copied() {
                    let now = memory::now();
                    match self.memory.mark_done(i, now) {
                        Ok(()) => {
                            self.flash = Some("Done".to_string());
                            self.planner_refresh();
                        }
                        Err(e) => self.flash = Some(e),
                    }
                }
            }
            KeyCode::Char('s') if self.planner_list_focus => {
                if let Some(i) = self.planner_entries.get(self.planner_sel).copied() {
                    let now = memory::now();
                    match self.memory.snooze(i, now, chrono::Duration::hours(1)) {
                        Ok(()) => {
                            self.flash = Some("Snoozed 1h".to_string());
                            self.planner_refresh();
                        }
                        Err(e) => self.flash = Some(e),
                    }
                }
            }
            KeyCode::Char('r') => {
                self.memory.reload();
                self.planner_refresh();
            }
            _ => {}
        }
        true
    }

    /// Start a quick note with the date pre-filled (`@YYYY-MM-DD `).
    pub(super) fn start_quick_note_for(&mut self, date: NaiveDate, origin: State) {
        self.memo_buf = format!("@{} ", date.format("%Y-%m-%d"));
        self.prompt_cursor = self.memo_buf.len();
        self.memo_origin = origin;
        self.state = State::Memo;
    }

    /// Fire any reminders that have come due, once each.
    pub(super) fn check_reminders(&mut self) {
        let now = memory::now();
        if memory::in_quiet_hours(&self.mem_cfg, now) || !self.mem_cfg.notify {
            return;
        }
        let alerts = self.memory.take_due(now);
        for a in &alerts {
            let mut body = a.body.clone();
            if a.priority > 0 {
                body = format!("{}{}", "!".repeat(a.priority as usize), body);
            }
            memory::notify(&body, &self.mem_cfg);
        }
        if let Some(first) = alerts.first() {
            self.flash = Some(format!(
                "⏰ {} · {}",
                first.due.format("%H:%M"),
                first.body
            ));
        }
    }

    pub(super) fn render_memories(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let popup = modal_rect(area, 0.84, 0.82);
        f.render_widget(Clear, popup);
        let block = overlay_block("Memories", self._frame, self.animations, theme);
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        if inner.height < 3 || inner.width < 10 {
            return;
        }
        let [body_area, foot_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(inner);

        // Keep the selected entry visible.
        let mut selected_row = 0usize;
        let mut entry_n = 0usize;
        for (ri, r) in self.mem_rows.iter().enumerate() {
            if let MemRow::Entry(_) = r {
                if entry_n == self.mem_selected {
                    selected_row = ri;
                    break;
                }
                entry_n += 1;
            }
        }
        let h = body_area.height as usize;
        if selected_row < self.mem_scroll {
            self.mem_scroll = selected_row;
        }
        if h > 0 && selected_row >= self.mem_scroll + h {
            self.mem_scroll = selected_row + 1 - h;
        }

        let now = memory::now();
        let dim = Style::new().fg(theme.comment.fg.unwrap_or(theme.fg));
        let mut lines: Vec<Line> = Vec::new();
        if self.mem_rows.is_empty() {
            lines.push(Line::from(Span::styled(
                "  Nothing remembered yet.",
                Style::new().fg(theme.fg),
            )));
            lines.push(Line::from(Span::styled(
                "  Press n to add something, or use :mem <text> / :today.",
                dim,
            )));
        } else {
            let mut n = 0usize;
            for r in &self.mem_rows {
                match r {
                    MemRow::Header(label) => lines.push(Line::from(Span::styled(
                        format!(" {label}"),
                        Style::new()
                            .fg(theme.keyword.fg.unwrap_or(theme.fg))
                            .add_modifier(Modifier::BOLD),
                    ))),
                    MemRow::Entry(i) => {
                        let m = &self.memory.entries[*i];
                        let selected = n == self.mem_selected;
                        n += 1;
                        if selected {
                            lines.push(Line::from(Span::styled(
                                format!("▸ {}", mem_plain_line(m, now)),
                                selected_style(theme),
                            )));
                        } else {
                            lines.push(Line::from(mem_colored_line(m, now, theme)));
                        }
                    }
                }
            }
        }

        f.render_widget(
            Paragraph::new(Text::from(lines))
                .scroll((self.mem_scroll as u16, 0))
                .style(Style::new().fg(theme.fg).bg(theme.bg)),
            body_area,
        );

        let footer = if let Some(e) = &self.memory.error {
            format!(" {e}")
        } else {
            " n new · Enter open · d done · s snooze 1h · r refresh · Esc close"
                .to_string()
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(footer, dim))),
            foot_area,
        );
    }

    // ── planner (month calendar + day agenda) ────────────────────

    pub(super) fn render_planner(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let popup = modal_rect(area, 0.92, 0.85);
        f.render_widget(Clear, popup);
        let block = overlay_block("Planner", self._frame, self.animations, theme);
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        if inner.height < 6 || inner.width < 30 {
            return;
        }

        let [body, foot] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let (cal_area, list_area) = if body.width >= 64 {
            let [c, l] = Layout::horizontal([
                Constraint::Length(30),
                Constraint::Min(1),
            ])
            .areas(body);
            (c, l)
        } else {
            let cal_h = ((2 + self.planner_days.len() / 7) as u16)
                .min(body.height.saturating_sub(1));
            let [c, l] = Layout::vertical([
                Constraint::Length(cal_h),
                Constraint::Min(1),
            ])
            .areas(body);
            (c, l)
        };

        let accent = Style::new()
            .fg(theme.keyword.fg.unwrap_or(theme.fg))
            .add_modifier(Modifier::BOLD);
        let dim = Style::new().fg(theme.comment.fg.unwrap_or(theme.fg));
        let bold = Style::new().fg(theme.fg).add_modifier(Modifier::BOLD);

        // ── month grid ──
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            format!(
                " {} {}",
                self.planner_month.format("%B"),
                self.planner_month.year()
            ),
            if self.planner_list_focus { bold } else { accent },
        )));
        let labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
        let start = self.planner_week_start() as usize;
        let mut header = String::from(" ");
        for i in 0..7 {
            header.push_str(&format!("{:<4}", labels[(i + start) % 7]));
        }
        lines.push(Line::from(Span::styled(header, dim)));

        for week in self.planner_days.chunks(7) {
            let mut spans = vec![Span::raw(" ")];
            for cell in week {
                let marker = if cell.reminders > 0 {
                    "•"
                } else if cell.notes > 0 {
                    "·"
                } else {
                    " "
                };
                let text = format!("{:>2}{}", cell.date.day(), marker);
                let style = if cell.date == self.planner_day {
                    selected_style(theme)
                } else {
                    let mut s = if cell.in_month {
                        Style::new().fg(theme.fg)
                    } else {
                        dim
                    };
                    if cell.is_today {
                        s = s.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                    }
                    if cell.reminders > 0 && cell.in_month {
                        s = s.fg(theme.keyword.fg.unwrap_or(theme.fg));
                    }
                    s
                };
                spans.push(Span::styled(text, style));
                spans.push(Span::raw(" "));
            }
            lines.push(Line::from(spans));
        }
        f.render_widget(
            Paragraph::new(Text::from(lines)).style(Style::new().bg(theme.bg)),
            cal_area,
        );

        // ── day agenda ──
        let date = self.planner_day;
        let date_header = format!(
            "{} {} {}",
            date.format("%A"),
            date.day(),
            date.format("%B %Y")
        );
        let mut alines: Vec<Line> = vec![
            Line::from(Span::styled(
                date_header,
                if self.planner_list_focus { accent } else { bold },
            )),
            Line::from(Span::styled(
                "─".repeat(list_area.width.saturating_sub(1) as usize),
                dim,
            )),
        ];
        if self.planner_entries.is_empty() {
            alines.push(Line::from(Span::styled(" nothing on this day", dim)));
            alines.push(Line::from(Span::styled(" press n to add something", dim)));
        } else {
            let now = memory::now();
            for (n, &i) in self.planner_entries.iter().enumerate() {
                let m = &self.memory.entries[i];
                if self.planner_list_focus && n == self.planner_sel {
                    alines.push(Line::from(Span::styled(
                        format!("▸ {}", mem_plain_line(m, now)),
                        selected_style(theme),
                    )));
                } else {
                    alines.push(mem_colored_line(m, now, theme));
                }
            }
        }
        f.render_widget(
            Paragraph::new(Text::from(alines)).style(Style::new().bg(theme.bg)),
            list_area,
        );

        let hint = if self.planner_list_focus {
            " ↑↓ select · Enter open · d done · s snooze · Tab calendar · Esc back"
        } else {
            " ←→ day · ↑↓ week · PgUp/PgDn month · t today · Enter agenda · n new · Esc close"
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, dim))),
            foot,
        );
    }

    // ── run output overlay ───────────────────────────────────────
}

pub(super) fn month_first(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d)
}

/// When a memory is due (or when it was created).
pub(super) fn mem_when(m: &crate::memory::Memory, now: chrono::NaiveDateTime) -> String {
    match m.due {
        Some(d) if d.date() == now.date() => d.format("%H:%M").to_string(),
        Some(d) => d.format("%m-%d %H:%M").to_string(),
        None => m.created.format("%m-%d").to_string(),
    }
}

pub(super) fn mem_marker(m: &crate::memory::Memory, now: chrono::NaiveDateTime) -> &'static str {
    if m.due.map(|d| d <= now).unwrap_or(false) {
        "!"
    } else if m.repeat.is_some() {
        "↻"
    } else {
        " "
    }
}

pub(super) fn mem_tags(m: &crate::memory::Memory) -> String {
    let mut s = String::new();
    for t in &m.tags {
        s.push_str(&format!(" #{t}"));
    }
    if m.priority > 0 {
        s.push_str(&format!(" !{}", m.priority));
    }
    s
}

pub(super) fn mem_plain_line(m: &crate::memory::Memory, now: chrono::NaiveDateTime) -> String {
    format!(
        "{} {:<11} {}{}",
        mem_marker(m, now),
        mem_when(m, now),
        m.summary(64),
        mem_tags(m)
    )
}

pub(super) fn mem_colored_line(
    m: &crate::memory::Memory,
    now: chrono::NaiveDateTime,
    theme: &Theme,
) -> Line<'static> {
    let dim = Style::new().fg(theme.comment.fg.unwrap_or(theme.fg));
    let overdue = m.due.map(|d| d <= now).unwrap_or(false);
    let when_style = if overdue {
        Style::new()
            .fg(theme.rstype.fg.unwrap_or(theme.fg))
            .add_modifier(Modifier::BOLD)
    } else {
        dim
    };
    Line::from(vec![
        Span::styled(format!("{} ", mem_marker(m, now)), when_style),
        Span::styled(format!("{:<11}", mem_when(m, now)), when_style),
        Span::styled(m.summary(64), Style::new().fg(theme.fg)),
        Span::styled(mem_tags(m), dim),
    ])
}
