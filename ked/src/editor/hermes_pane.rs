//! The Hermes chat pane: opening, key handling, and drawing the transcript.

use super::*;

impl Editor {
    /// Open the chat pane, pointed at the file's directory so hermes picks up
    /// the project's AGENTS.md and can read the open file by name.
    pub(super) fn open_hermes(&mut self) {
        if let Some(dir) = self
            .filename
            .as_deref()
            .and_then(|f| Path::new(f).parent())
            .filter(|d| !d.as_os_str().is_empty())
        {
            self.hermes.cwd = dir.to_path_buf();
        }
        // Opened from visual mode: quote what is selected, so "select the
        // function, ask about it" is one keystroke.
        if self.state == State::Visual && self.hermes.input.trim().is_empty() {
            let sel = self.selection_text();
            if !sel.trim().is_empty() {
                self.hermes.input = format!("```\n{}\n```\n\n", sel.trim_end());
            }
        }
        // First open of this ked run: pick up yesterday's thread if it is recent.
        if self.hermes.turns == 0
            && self.hermes.messages.is_empty()
            && self.hermes.session.is_none()
        {
            self.hermes.resume_previous();
        }
        self.state = State::Hermes;
    }

    pub(super) fn toggle_hermes(&mut self) {
        if self.state == State::Hermes {
            self.state = State::Normal;
        } else {
            self.open_hermes();
        }
    }

    /// The chat pane is always in "typing" mode — a chat should not need a mode
    /// switch to type — so plain characters go into the message and the
    /// transcript scrolls on Ctrl+U / Ctrl+D / PgUp / PgDn.
    pub(super) fn handle_hermes_state(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        // The toggle closes the pane from inside as well — otherwise the key
        // that opened it looks dead once the pane is up.
        if matches!(key.code, KeyCode::Char('h') | KeyCode::Char('H'))
            && ctrl
            && (shift || matches!(key.code, KeyCode::Char('H')))
        {
            self.state = State::Normal;
            return true;
        }

        match key.code {
            KeyCode::Esc => {
                if self.hermes.running {
                    self.hermes.cancel();
                } else {
                    self.state = State::Normal;
                }
            }
            KeyCode::Enter if !shift => {
                let msg = self.hermes.input.trim().to_string();
                if !msg.is_empty() && !self.hermes.running {
                    self.hermes.input.clear();
                    let _ = self.hermes.send(msg);
                }
            }
            // Shift+Enter (or Ctrl+J) for a newline: pasting a few lines of code
            // into a question should not send it.
            KeyCode::Enter if shift => self.hermes.input.push('\n'),
            KeyCode::Char('j') if ctrl => self.hermes.input.push('\n'),
            KeyCode::Backspace => {
                self.hermes.input.pop();
            }
            KeyCode::Char('c') if ctrl => {
                if self.hermes.running {
                    self.hermes.cancel();
                } else {
                    self.state = State::Normal;
                }
            }
            KeyCode::Char('n') if ctrl => self.hermes.new_session(),
            KeyCode::Char('u') if ctrl => {
                self.hermes.scroll = self.hermes.scroll.saturating_add(10);
            }
            KeyCode::Char('d') if ctrl => {
                self.hermes.scroll = self.hermes.scroll.saturating_sub(10);
            }
            KeyCode::PageUp => self.hermes.scroll = self.hermes.scroll.saturating_add(15),
            KeyCode::PageDown => self.hermes.scroll = self.hermes.scroll.saturating_sub(15),
            // Attach the file being edited to the message.
            KeyCode::Char('o') if ctrl => {
                if let Some(path) = self.filename.clone() {
                    if !self.hermes.input.is_empty() && !self.hermes.input.ends_with(' ') {
                        self.hermes.input.push(' ');
                    }
                    self.hermes.input.push_str(&path);
                    self.hermes.input.push(' ');
                }
            }
            // Copy the last answer to the system clipboard.
            KeyCode::Char('y') if ctrl => self.copy_last_answer(),
            // Insert the last answer into the buffer at the cursor.
            KeyCode::Char('i') if ctrl => self.insert_last_answer(),
            KeyCode::Char(ch) if !ctrl => self.hermes.input.push(ch),
            _ => {}
        }
        true
    }

    pub(super) fn copy_last_answer(&mut self) {
        let Some(text) = self.hermes.last_answer().map(|s| s.to_string()) else {
            self.hermes.push_note("nothing to copy yet");
            return;
        };
        let chars = text.chars().count();
        // Same clipboard path as yy/dd: the internal buffer plus OSC 52, so it
        // lands in the system clipboard without spawning anything.
        self.set_clipboard(text);
        self.hermes
            .push_note(format!("copied {chars} chars to the clipboard"));
    }

    pub(super) fn insert_last_answer(&mut self) {
        let Some(text) = self.hermes.last_answer().map(|s| s.to_string()) else {
            self.hermes.push_note("nothing to insert yet");
            return;
        };
        let lines = text.lines().count();
        // `paste_from_terminal` only accepts the buffer states, and in insert
        // mode it puts the text at the cursor (not after it, like `p`) with
        // undo and multi-line handled — the same path bracketed paste uses.
        self.state = State::Normal;
        self.mode = Mode::Insert;
        self.paste_from_terminal(&text);
        self.state = State::Hermes;
        self.hermes
            .push_note(format!("inserted {lines} line(s) into the buffer"));
    }

    pub(super) fn render_hermes(&mut self, f: &mut Frame, area: Rect) {
        let theme = self.theme.theme();
        let popup = modal_rect(area, 0.94, 0.88);
        f.render_widget(Clear, popup);

        let spinner = if self.hermes.running {
            SPINNER[(self._frame as usize / 3) % SPINNER.len()]
        } else {
            "·"
        };
        let elapsed = if self.hermes.running {
            format!(" · {}s", self.hermes.elapsed_secs())
        } else {
            String::new()
        };
        let title = format!(
            " {spinner} Hermes — {} · {} · {} turn{}{elapsed} · {} tok ",
            self.hermes.status,
            self.hermes.session_tag(),
            self.hermes.turns,
            if self.hermes.turns == 1 { "" } else { "s" },
            self.hermes.tokens.1
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(Style::new().fg(theme.fg).bg(theme.status_bg))
            .border_style(Style::new().fg(theme.border));
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        if inner.height < 3 || inner.width < 8 {
            return;
        }

        // The message line grows with the input (up to four rows), so a pasted
        // snippet stays visible while it is being written.
        let avail = inner.width.saturating_sub(2) as usize;
        let input_lines = wrap_text(&self.hermes.input, avail);
        let input_rows = input_lines.len().clamp(1, 4) as u16;
        let body_h = inner.height.saturating_sub(input_rows + 1) as usize;
        let text_w = inner.width.saturating_sub(3) as usize;

        let mut lines: Vec<Line> = Vec::new();
        if self.hermes.messages.is_empty() {
            lines.push(Line::from(Span::styled(
                "Ask Hermes anything about this project.",
                Style::new().fg(theme.fg),
            )));
            lines.push(Line::from(""));
            for hint in [
                "enter send · shift+enter newline · ctrl+o attach file",
                "ctrl+y copy answer · ctrl+i insert answer · ctrl+n new session",
                "ctrl+u / ctrl+d scroll · esc close",
            ] {
                lines.push(Line::from(Span::styled(
                    hint,
                    Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)),
                )));
            }
        }
        for msg in &self.hermes.messages {
            let (prefix, style) = match msg.role {
                Role::User => (
                    "› ",
                    Style::new()
                        .fg(theme.function.fg.unwrap_or(theme.fg))
                        .add_modifier(Modifier::BOLD),
                ),
                Role::Assistant => ("  ", Style::new().fg(theme.fg)),
                Role::Tool => (
                    "⚙ ",
                    Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)),
                ),
                Role::ToolResult => (
                    "  ↳ ",
                    Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)),
                ),
                Role::Error => (
                    "! ",
                    Style::new()
                        .fg(theme.keyword.fg.unwrap_or(theme.fg))
                        .add_modifier(Modifier::BOLD),
                ),
                Role::Note => (
                    "· ",
                    Style::new().fg(theme.comment.fg.unwrap_or(theme.fg)),
                ),
            };
            let pad = " ".repeat(prefix.chars().count());
            let body = text_w.saturating_sub(prefix.chars().count());
            for (i, chunk) in wrap_text(&msg.text, body).into_iter().enumerate() {
                let lead = if i == 0 { prefix } else { pad.as_str() };
                lines.push(Line::from(vec![
                    Span::styled(lead.to_string(), style),
                    Span::styled(chunk, style),
                ]));
            }
            lines.push(Line::from(""));
        }

        // Clamp: scrolling past the top would otherwise show an empty pane.
        let max_scroll = lines.len().saturating_sub(body_h);
        let scroll = (self.hermes.scroll as usize).min(max_scroll);
        let end = lines.len().saturating_sub(scroll);
        let start = end.saturating_sub(body_h);
        let visible: Vec<Line> = lines.into_iter().skip(start).take(end - start).collect();
        f.render_widget(
            Paragraph::new(Text::from(visible)),
            Rect::new(inner.x, inner.y, inner.width, body_h as u16),
        );

        let rule_y = inner.y + body_h as u16;
        f.render_widget(
            Paragraph::new("─".repeat(inner.width as usize)).style(Style::new().fg(theme.border)),
            Rect::new(inner.x, rule_y, inner.width, 1),
        );
        if scroll > 0 {
            f.render_widget(
                Paragraph::new(format!("↓ {scroll} more "))
                    .alignment(Alignment::Right)
                    .style(Style::new().fg(theme.comment.fg.unwrap_or(theme.fg))),
                Rect::new(inner.x, rule_y, inner.width, 1),
            );
        }

        // Message line: the tail of the wrapped input, caret after the last
        // character. A long message scrolls itself rather than pushing the
        // transcript away.
        let truncated = input_lines.len() > input_rows as usize;
        let first = input_lines.len().saturating_sub(input_rows as usize);
        let caret = "❯ ";
        let prompt_style = if self.hermes.running {
            Style::new().fg(theme.comment.fg.unwrap_or(theme.fg))
        } else {
            Style::new().fg(theme.fg)
        };
        for (i, chunk) in input_lines[first..].iter().enumerate() {
            let lead = if i == 0 {
                if truncated {
                    "… "
                } else {
                    caret
                }
            } else {
                "  "
            };
            f.render_widget(
                Paragraph::new(format!("{lead}{chunk}")).style(prompt_style),
                Rect::new(inner.x, rule_y + 1 + i as u16, inner.width, 1),
            );
        }
        let last_row = input_lines.len().saturating_sub(1) - first;
        let last_len = input_lines
            .last()
            .map(|l| l.chars().count())
            .unwrap_or(0) as u16;
        let cx = (inner.x + 2 + last_len).min(inner.x + inner.width.saturating_sub(1));
        let cy = rule_y + 1 + last_row as u16;
        self.cursor_screen = Some((cx, cy));
        f.set_cursor_position(Position::new(cx, cy));
    }
}
