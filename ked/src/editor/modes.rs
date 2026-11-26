//! Normal, insert and visual mode key handling.

use super::*;

impl Editor {
    // ═══════════════════════════════════════════════════════════════
    //  Normal-mode key handling
    // ═══════════════════════════════════════════════════════════════

    pub(super) fn handle_normal_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // ── cursor motion ──
            KeyCode::Char('h') | KeyCode::Left => {
                if self.cx > 0 {
                    self.cx = self.left_cx();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cy + 1 < self.lines.len() {
                    self.cy += 1;
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let max_cx = self.lines[self.cy].len();
                if self.cx < max_cx {
                    self.cx = self.right_cx();
                }
            }
            KeyCode::Char('0') | KeyCode::Home => self.cx = 0,
            KeyCode::Char('$') | KeyCode::End => {
                self.cx = self.lines[self.cy].len();
            }

            // ── word motion ──
            KeyCode::Char('w') => {
                let (r, c) = self.next_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
            }
            KeyCode::Char('b') => {
                let (r, c) = self.prev_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
            }

            // ── page motion ──
            KeyCode::PageUp => {
                let page = (self.terminal_height() as usize).saturating_sub(2).max(1);
                self.cy = self.cy.saturating_sub(page / 2);
            }
            KeyCode::PageDown => {
                let page = (self.terminal_height() as usize).saturating_sub(2).max(1);
                self.cy = min(self.cy + page / 2, self.lines.len().saturating_sub(1));
            }
            // vim-style half-page scroll.
            KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = min(self.cy + page, self.lines.len().saturating_sub(1));
                self.cx = self.cx.min(self.lines[self.cy].len());
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = self.cy.saturating_sub(page);
                self.cx = self.cx.min(self.lines[self.cy].len());
            }

            // ── jump to first / last line ──
            KeyCode::Char('g') => {
                if self.cmd_buf == "g" {
                    // gg: reset pending buffer first
                } else {
                    self.cmd_buf.clear();
                }
                self.cy = 0;
                self.cx = 0;
            }
            KeyCode::Char('G') => {
                self.cmd_buf.clear();
                if !self.lines.is_empty() {
                    self.cy = self.lines.len() - 1;
                    self.cx = 0;
                }
            }

            // ── enter visual mode ──
            KeyCode::Char('v') => {
                self.state = State::Visual;
                self.selection_anchor = Some((self.cy, self.cx));
            }

            // ── enter insert mode ──
            KeyCode::Char('i') => {
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('a') => {
                if self.cx < self.lines[self.cy].len() {
                    self.cx = self.right_cx();
                }
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('I') => {
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('A') => {
                self.cx = self.lines[self.cy].len();
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }

            // ── open new lines ──
            KeyCode::Char('o') => {
                self.save_undo();
                self.lines.insert(self.cy + 1, String::new());
                self.cy += 1;
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = true; // snapshot already taken above
                self.modified = true;
            }
            KeyCode::Char('O') => {
                self.save_undo();
                self.lines.insert(self.cy, String::new());
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = true; // snapshot already taken above
                self.modified = true;
            }

            // ── delete char (x) ──
            KeyCode::Char('x') => {
                self.save_undo();
                let line = &mut self.lines[self.cy];
                if self.cx < line.len() {
                    let pos = line.floor_char_boundary(self.cx);
                    let ch = line[pos..].chars().next().unwrap();
                    for _ in 0..ch.len_utf8() {
                        line.remove(pos);
                    }
                    self.modified = true;
                }
            }

            // ── delete line (dd) ──
            KeyCode::Char('d') => {
                self.save_undo();
                if !self.lines.is_empty() {
                    let removed = self.lines.remove(self.cy);
                    self.set_clipboard(removed);
                    self.modified = true;
                    if self.lines.is_empty() {
                        self.lines.push(String::new());
                    }
                    self.cy = min(self.cy, self.lines.len().saturating_sub(1));
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }

            // ── yank line (yy) ──
            KeyCode::Char('y') => {
                if !self.lines.is_empty() {
                    self.set_clipboard(self.lines[self.cy].clone());
                }
            }

            // ── paste (p / P) ──
            KeyCode::Char('p') => {
                let text = self.clipboard_text.clone();
                self.paste_after(&text);
            }
            KeyCode::Char('P') => {
                let text = self.clipboard_text.clone();
                self.paste_before(&text);
            }

            // ── undo / redo ──
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => self.redo(),

            // ── enter command mode ──
            KeyCode::Char(':') => {
                self.state = State::Command;
                self.cmd_buf.clear();
                self.prompt_cursor = 0;
            }

            // ── enter search mode ──
            KeyCode::Char('/') => {
                self.state = State::Search;
                self.search_query.clear();
                self.prompt_cursor = 0;
            }

            // ── repeat last search ──
            KeyCode::Char('n') => {
                if self.last_search.is_some() {
                    self.search_next();
                }
            }
            KeyCode::Char('N') => {
                if self.last_search.is_some() {
                    self.search_prev();
                }
            }

            // ── buffer switching ──
            KeyCode::Tab => {
                if self.buffers.len() > 1 {
                    let next = (self.current + 1) % self.buffers.len();
                    self.switch_buffer(next);
                }
            }
            KeyCode::BackTab => {
                if self.buffers.len() > 1 {
                    let prev = if self.current == 0 {
                        self.buffers.len() - 1
                    } else {
                        self.current - 1
                    };
                    self.switch_buffer(prev);
                }
            }

            _ => {}
        }

        // Re-clamp scroll after any motion.
        self.clamp_scroll();
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Insert-mode key handling
    // ═══════════════════════════════════════════════════════════════

    pub(super) fn handle_insert_mode(&mut self, key: KeyEvent) -> bool {
        // Take one undo snapshot per insert session — right before the
        // first modification — so `u` undoes the whole typed run like
        // vim, instead of one character at a time.
        let modifies = matches!(key.code, KeyCode::Char(_))
            || matches!(key.code, KeyCode::Enter | KeyCode::Backspace | KeyCode::Delete | KeyCode::Tab);
        if modifies {
            self.save_undo_if_needed();
        }

        match key.code {
            // Escape returns to normal mode.
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.insert_saved = false;
                // Move cursor one left if past end of line (vim does this
                // so the block cursor sits on the last character).
                if self.cx > 0 && self.cx > self.lines[self.cy].len() {
                    self.cx = self.lines[self.cy].len();
                }
            }

            // Enter: split the line at the cursor, preserving indent.
            KeyCode::Enter => {
                let indent: String = {
                    let line = &self.lines[self.cy];
                    line.chars().take_while(|c| *c == ' ' || *c == '\t').collect()
                };
                let right = self.lines[self.cy].split_off(self.cx);
                self.lines.insert(self.cy + 1, indent.clone() + &right);
                self.cy += 1;
                self.cx = indent.len();
                self.modified = true;
            }

            // Backspace: delete character before cursor.
            KeyCode::Backspace => {
                if self.cx > 0 {
                    let left = self.left_cx();
                    let char_len = self.cx - left;
                    let line = &mut self.lines[self.cy];
                    for _ in 0..char_len {
                        line.remove(left);
                    }
                    self.cx = left;
                    self.modified = true;
                } else if self.cy > 0 {
                    // Join with previous line.
                    let prev_len = self.lines[self.cy - 1].len();
                    let current = self.lines.remove(self.cy);
                    self.lines[self.cy - 1].push_str(&current);
                    self.cy -= 1;
                    self.cx = prev_len;
                    self.modified = true;
                }
            }

            // Delete: remove character at cursor.
            KeyCode::Delete => {
                let line = &mut self.lines[self.cy];
                if self.cx < line.len() {
                    let pos = line.floor_char_boundary(self.cx);
                    let ch = line[pos..].chars().next().unwrap();
                    for _ in 0..ch.len_utf8() {
                        line.remove(pos);
                    }
                    self.modified = true;
                } else if self.cy + 1 < self.lines.len() {
                    let next = self.lines.remove(self.cy + 1);
                    self.lines[self.cy].push_str(&next);
                    self.modified = true;
                }
            }

            // Tab: insert 4 spaces.
            KeyCode::Tab => {
                let line = &mut self.lines[self.cy];
                line.insert_str(self.cx, "    ");
                self.cx += 4;
                self.modified = true;
            }

            // Arrow keys: move cursor (don't exit insert mode).
            KeyCode::Left => {
                if self.cx > 0 { self.cx = self.left_cx(); }
            }
            KeyCode::Right => {
                if self.cx < self.lines[self.cy].len() { self.cx = self.right_cx(); }
            }
            KeyCode::Up => {
                if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = self.cx.min(self.lines[self.cy].len());
                }
            }
            KeyCode::Down => {
                if self.cy + 1 < self.lines.len() {
                    self.cy += 1;
                    self.cx = self.cx.min(self.lines[self.cy].len());
                }
            }

            // Ctrl+A: paste clipboard text.
            KeyCode::Char('a') if key.modifiers == KeyModifiers::CONTROL => {
                if !self.clipboard_text.is_empty() {
                    let line = &mut self.lines[self.cy];
                    line.insert_str(self.cx, &self.clipboard_text);
                    self.cx += self.clipboard_text.len();
                    self.modified = true;
                }
            }

            // Regular character: insert at cursor (Ctrl-modified
            // chars are reserved for editor actions, so ignore them).
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let line = &mut self.lines[self.cy];
                let pos = line.floor_char_boundary(self.cx);
                line.insert(pos, ch);
                self.cx = pos + ch.len_utf8();
                self.modified = true;
            }

            _ => {}
        }

        self.clamp_scroll();
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Visual-mode key handling
    // ═══════════════════════════════════════════════════════════════

    pub(super) fn handle_visual_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc or v: exit visual mode.
            KeyCode::Esc => {
                self.state = State::Normal;
                self.selection_anchor = None;
            }
            KeyCode::Char('v') => {
                self.state = State::Normal;
                self.selection_anchor = None;
            }

            // Cursor motion extends (or shrinks) the selection.
            KeyCode::Char('h') | KeyCode::Left => {
                if self.cx > 0 {
                    self.cx = self.left_cx();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cy + 1 < self.lines.len() {
                    self.cy += 1;
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = min(self.cx, self.lines[self.cy].len());
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let max_cx = self.lines[self.cy].len();
                if self.cx < max_cx {
                    self.cx = self.right_cx();
                }
            }
            KeyCode::Char('0') | KeyCode::Home => self.cx = 0,
            KeyCode::Char('$') | KeyCode::End => {
                self.cx = self.lines[self.cy].len();
            }
            KeyCode::Char('w') => {
                let (r, c) = self.next_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
            }
            KeyCode::Char('b') => {
                let (r, c) = self.prev_word_pos(self.cy, self.cx);
                self.cy = r;
                self.cx = c;
            }
            // vim-style half-page scroll (extends the selection).
            KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = min(self.cy + page, self.lines.len().saturating_sub(1));
                self.cx = self.cx.min(self.lines[self.cy].len());
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                let page = (self.terminal_height() as usize / 2).max(1);
                self.cy = self.cy.saturating_sub(page);
                self.cx = self.cx.min(self.lines[self.cy].len());
            }

            // Yank selection (y or c both copy).
            KeyCode::Char('y') | KeyCode::Char('c') => {
                let text = self.selection_text();
                self.set_clipboard(text);
                self.state = State::Normal;
                self.selection_anchor = None;
            }

            // Delete selection.
            KeyCode::Char('d') => {
                if self.selection_anchor.is_some() {
                    self.save_undo();
                    let text = self.selection_text();
                self.set_clipboard(text);
                    let (start_y, end_y, start_x, end_x) = self.selection_bounds();
                    if start_y == end_y {
                        let line = &mut self.lines[start_y];
                        let to_remove = end_x.saturating_sub(start_x);
                        for _ in 0..to_remove {
                            line.remove(start_x);
                        }
                    } else {
                        // First line: remove from start_x to end.
                        {
                            let line = &mut self.lines[start_y];
                            line.truncate(start_x);
                        }
                        // Middle lines: remove.
                        for _ in (start_y + 1)..end_y {
                            self.lines.remove(start_y + 1);
                        }
                        // Last line: remove from 0 to end_x.
                        if end_y > start_y {
                            let last = &mut self.lines[start_y + 1];
                            let remaining = last.split_off(end_x);
                            self.lines[start_y].push_str(&remaining);
                            self.lines.remove(start_y + 1);
                        }
                    }
                    self.cy = start_y;
                    self.cx = start_x;
                    self.modified = true;
                }
                self.state = State::Normal;
                self.selection_anchor = None;
            }

            // Enter insert mode (exits visual mode like vim).
            KeyCode::Char('i') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('a') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                if self.cx < self.lines[self.cy].len() {
                    self.cx = self.right_cx();
                }
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('I') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.cx = 0;
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }
            KeyCode::Char('A') => {
                self.state = State::Normal;
                self.selection_anchor = None;
                self.cx = self.lines[self.cy].len();
                self.mode = Mode::Insert;
                self.insert_saved = false;
            }

            _ => {}
        }

        self.clamp_scroll();
        true
    }
}
