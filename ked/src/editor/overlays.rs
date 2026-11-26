//! Key handling for the status-line prompts and popup overlays:
//! search, `:` commands, finder, run output, music, themes, file tree.

use super::*;

impl Editor {
    // ═══════════════════════════════════════════════════════════════
    //  Search-state key handling (/)
    // ═══════════════════════════════════════════════════════════════

    pub(super) fn handle_search_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc: cancel search.
            KeyCode::Esc => {
                self.state = State::Normal;
                self.search_query.clear();
                self.prompt_cursor = 0;
            }
            // Enter: execute search.
            KeyCode::Enter => {
                self.last_search = Some(self.search_query.clone());
                self.search_query.clear();
                self.prompt_cursor = 0;
                self.state = State::Normal;
                self.search_next();
                self.clamp_scroll();
            }
            // Everything else edits the query (cursor-aware).
            _ => {
                edit_prompt(&mut self.search_query, &mut self.prompt_cursor, &key);
            }
        }
        true
    }

    /// Jump to the next occurrence of the last search query.
    pub(super) fn search_next(&mut self) {
        let query = match &self.last_search {
            Some(q) => q.clone(),
            None => return,
        };
        if query.is_empty() {
            return;
        }
        if let Some((y, x)) = self.find_next(self.cy, self.cx + 1, &query) {
            self.cy = y;
            self.cx = x;
        } else {
            self.flash = Some("search: no more matches".to_string());
        }
    }

    /// Jump to the previous occurrence of the last search query.
    pub(super) fn search_prev(&mut self) {
        let query = match &self.last_search {
            Some(q) => q.clone(),
            None => return,
        };
        if query.is_empty() {
            return;
        }
        // Search backward from just before the cursor.
        let from_x = self.cx.saturating_sub(1);
        if let Some((y, x)) = self.find_prev(self.cy, from_x, &query) {
            self.cy = y;
            self.cx = x;
        } else {
            self.flash = Some("search: no more matches".to_string());
        }
    }

    /// Find the first occurrence of `query` at or after (from_y, from_x),
    /// wrapping around to the top of the buffer if necessary.
    pub(super) fn find_next(&self, from_y: usize, from_x: usize, query: &str) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        // Forward from current position to end.
        for y in from_y..self.lines.len() {
            let line = &self.lines[y];
            let start = if y == from_y { from_x.min(line.len()) } else { 0 };
            if let Some(x) = line[start..].find(query) {
                return Some((y, start + x));
            }
        }
        // Wrap: search from beginning to (from_y, from_x).
        for y in 0..=from_y {
            let line = &self.lines[y];
            let limit = if y == from_y { from_x.min(line.len()) } else { line.len() };
            if limit == 0 { continue; }
            if let Some(x) = line[..limit].find(query) {
                return Some((y, x));
            }
        }
        None
    }

    /// Find the last occurrence of `query` before (from_y, from_x),
    /// wrapping around to the bottom of the buffer if necessary.
    pub(super) fn find_prev(&self, from_y: usize, from_x: usize, query: &str) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }
        // Backward from current position to beginning.
        for y in (0..=from_y).rev() {
            let line = &self.lines[y];
            let limit = if y == from_y { from_x.min(line.len()) } else { line.len() };
            if limit == 0 { continue; }
            if let Some(x) = line[..limit].rfind(query) {
                return Some((y, x));
            }
        }
        // Wrap: search from end backward to (from_y, from_x).
        for y in (from_y + 1..self.lines.len()).rev() {
            let line = &self.lines[y];
            if line.is_empty() { continue; }
            if let Some(x) = line.rfind(query) {
                return Some((y, x));
            }
        }
        None
    }

    // ═══════════════════════════════════════════════════════════════
    //  Command-state key handling (:command)
    // ═══════════════════════════════════════════════════════════════

    pub(super) fn handle_cmd_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc or Enter: process or cancel command.
            KeyCode::Esc => {
                self.state = State::Normal;
                self.cmd_buf.clear();
                self.prompt_cursor = 0;
            }
            KeyCode::Enter => {
                let cmd = self.cmd_buf.clone();
                self.cmd_buf.clear();
                self.prompt_cursor = 0;
                self.state = State::Normal;
                return self.exec_cmd(&cmd);
            }

            // Everything else edits the line (cursor-aware).
            _ => {
                edit_prompt(&mut self.cmd_buf, &mut self.prompt_cursor, &key);
            }
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Finder-state key handling (Ctrl+P overlay)
    // ═══════════════════════════════════════════════════════════════

    pub(super) fn handle_finder_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc: close finder.
            KeyCode::Esc => {
                self.state = State::Normal;
            }

            // Enter: open the selected file.
            KeyCode::Enter => {
                if let Some((path, _)) = self.finder.results.get(self.finder_selection) {
                    let full_path = std::env::current_dir()
                        .unwrap_or_default()
                        .join(path);
                    if let Ok(content) = fs::read_to_string(&full_path) {
                        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                        if content.ends_with('\n') { lines.push(String::new()); }
                        if lines.is_empty() { lines.push(String::new()); }
                        let fname = full_path.to_string_lossy().to_string();
                        let mtime = fs::metadata(&fname).ok().and_then(|m| m.modified().ok());
                        self.sync_to_buffer();
                        self.buffers.push(Buffer::new(lines, Some(fname.to_string()), mtime));
                        self.current = self.buffers.len() - 1;
                        self.sync_from_buffer(self.current);
                    }
                }
                self.state = State::Normal;
            }

            // Navigation: arrows / Ctrl+N / Ctrl+P in the result list.
            // NOTE: every plain character (including h/j/k/l) goes to
            // the query — nothing here shadows typed text.
            KeyCode::Up => {
                self.finder_selection = self.finder_selection.saturating_sub(1);
            }
            KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                self.finder_selection = self.finder_selection.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = self.finder.results.len().saturating_sub(1);
                self.finder_selection = min(self.finder_selection + 1, max);
            }
            KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
                let max = self.finder.results.len().saturating_sub(1);
                self.finder_selection = min(self.finder_selection + 1, max);
            }
            // Page up/down in finder results.
            KeyCode::PageUp => {
                self.finder_selection = self.finder_selection.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = self.finder.results.len().saturating_sub(1);
                self.finder_selection = min(self.finder_selection + 10, max);
            }

            // Backspace: remove last query character.
            KeyCode::Backspace => {
                self.finder_query.pop();
                self.finder.search(&self.finder_query);
                self.finder_selection = 0;
            }

            // Regular char: append to query and re-search.
            KeyCode::Char(ch) => {
                self.finder_query.push(ch);
                self.finder.search(&self.finder_query);
                self.finder_selection = 0;
            }

            _ => {}
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Run-state key handling (Ctrl+E overlay)
    // ═══════════════════════════════════════════════════════════════

    pub(super) fn handle_run_state(&mut self, key: KeyEvent) -> bool {
        // Scroll long output; Esc/Enter/q (or any other key) dismisses.
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.run_scroll = self.run_scroll.saturating_add(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.run_scroll = self.run_scroll.saturating_sub(1);
            }
            KeyCode::PageUp => {
                self.run_scroll = self.run_scroll.saturating_add(10);
            }
            KeyCode::PageDown => {
                self.run_scroll = self.run_scroll.saturating_sub(10);
            }
            _ => {
                // Any other key dismisses the overlay (Ctrl+C/D
                // included — they shouldn't quit the editor here).
                self.state = State::Normal;
                self.run_scroll = 0;
            }
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  Music-state key handling (Ctrl+M overlay)
    // ═══════════════════════════════════════════════════════════════

    /// Handle keys while the music-player picker is open.
    pub(super) fn handle_music_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Esc closes the picker (playback continues).
            KeyCode::Esc => {
                self.state = State::Normal;
            }
            // Enter: start playing the selected file.
            KeyCode::Enter => {
                self.music_player.play(self.music_player.selected);
            }
            // 's' or Backspace: stop playback.
            KeyCode::Char('s') | KeyCode::Backspace => {
                self.music_player.stop();
            }
            // 'l': toggle looping.
            KeyCode::Char('l') => {
                self.music_player.looping = !self.music_player.looping;
            }
            // Navigation.
            KeyCode::Up | KeyCode::Char('k') => {
                self.music_player.selected =
                    self.music_player.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.music_player.files.len().saturating_sub(1);
                self.music_player.selected =
                    (self.music_player.selected + 1).min(max);
            }
            KeyCode::PageUp => {
                self.music_player.selected =
                    self.music_player.selected.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = self.music_player.files.len().saturating_sub(1);
                self.music_player.selected =
                    (self.music_player.selected + 10).min(max);
            }

            // Ctrl+C: quit.
            KeyCode::Char('c')
                if key.modifiers == KeyModifiers::CONTROL =>
            {
                return false;
            }

            _ => {}
        }
        true
    }

    // ── theme-selector state ────────────────────────────────────

    /// Themes matching the current filter, best match first.
    ///
    /// An empty query keeps the canonical `ThemeKind::all()` order (dark
    /// palettes first, then the light ones) so the unfiltered list looks
    /// exactly as it did before search existed.  Scoring is the file
    /// finder's subsequence matcher, so `rp` finds `rose-pine` and `oned`
    /// finds `onedark` — theme names are already lowercase kebab, which is
    /// what `fuzzy_score` expects.
    pub(super) fn theme_matches(&self) -> Vec<ThemeKind> {
        if self.theme_query.is_empty() {
            return ThemeKind::all().to_vec();
        }
        let q: Vec<char> = self.theme_query.chars().flat_map(|c| c.to_lowercase()).collect();
        let mut scored: Vec<(ThemeKind, i32)> = ThemeKind::all()
            .iter()
            .filter_map(|t| crate::finder::fuzzy_score(&q, t.name()).map(|score| (*t, score)))
            .collect();
        scored.sort_by(|a, b| {
            a.1.cmp(&b.1).then_with(|| a.0.name().cmp(b.0.name()))
        });
        scored.into_iter().map(|(t, _)| t).collect()
    }

    pub(super) fn handle_theme_state(&mut self, key: KeyEvent) -> bool {
        let matches = self.theme_matches();
        match key.code {
            // Esc: clear the filter first, close once there is nothing to clear.
            KeyCode::Esc => {
                if self.theme_query.is_empty() {
                    self.state = State::Normal;
                } else {
                    self.theme_query.clear();
                    self.theme_selected = 0;
                    self.theme_scroll = 0;
                }
            }
            // Enter: apply the highlighted theme.
            KeyCode::Enter => {
                if let Some(t) = matches.get(self.theme_selected) {
                    self.theme = *t;
                    self.state = State::Normal;
                }
            }
            // Navigation: arrows / Ctrl+N / Ctrl+P, like the file finder.
            // Plain characters go to the filter, so j/k type rather than move.
            KeyCode::Up => {
                self.theme_selected = self.theme_selected.saturating_sub(1);
            }
            KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                self.theme_selected = self.theme_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = matches.len().saturating_sub(1);
                self.theme_selected = min(self.theme_selected + 1, max);
            }
            KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
                let max = matches.len().saturating_sub(1);
                self.theme_selected = min(self.theme_selected + 1, max);
            }
            KeyCode::PageUp => {
                self.theme_selected = self.theme_selected.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = matches.len().saturating_sub(1);
                self.theme_selected = min(self.theme_selected + 10, max);
            }
            // Backspace: drop the last filter character.
            KeyCode::Backspace => {
                self.theme_query.pop();
                self.theme_selected = 0;
                self.theme_scroll = 0;
            }
            // Ctrl+C: quit.
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                return false;
            }
            // Anything else printable narrows the filter.
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.theme_query.push(c);
                self.theme_selected = 0;
                self.theme_scroll = 0;
            }
            _ => {}
        }
        true
    }

    // ── file-tree state (Ctrl+F) ───────────────────────────────

    pub(super) fn handle_filetree_state(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
                self.state = State::Normal;
            }
            KeyCode::Enter => {
                let entry = self.filetree.selected_entry().cloned();
                if let Some(e) = entry {
                    if e.is_dir {
                        self.filetree.toggle_expand();
                    } else {
                        // Open file in new buffer
                        if let Ok(content) = fs::read_to_string(&e.path) {
                            let mut lines: Vec<String> =
                                content.lines().map(|l| l.to_string()).collect();
                            if content.ends_with('\n') { lines.push(String::new()); }
                            if lines.is_empty() { lines.push(String::new()); }
                            let mtime =
                                fs::metadata(&e.path).ok().and_then(|m| m.modified().ok());
                            self.sync_to_buffer();
                            self.buffers.push(Buffer::new(
                                lines,
                                Some(e.path.clone()),
                                mtime,
                            ));
                            self.current = self.buffers.len() - 1;
                            self.sync_from_buffer(self.current);
                        } else {
                            self.flash = Some(format!("can't read {}", e.path));
                        }
                        self.state = State::Normal;
                    }
                }
            }
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                self.filetree.selected = self.filetree.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.filetree.entries.len().saturating_sub(1);
                self.filetree.selected = (self.filetree.selected + 1).min(max);
            }
            KeyCode::PageUp => {
                self.filetree.selected = self.filetree.selected.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max = self.filetree.entries.len().saturating_sub(1);
                self.filetree.selected = (self.filetree.selected + 10).min(max);
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let entry = self.filetree.selected_entry().cloned();
                if let Some(e) = entry {
                    if e.is_dir && e.expanded {
                        self.filetree.toggle_expand();
                    }
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let entry = self.filetree.selected_entry().cloned();
                if let Some(e) = entry {
                    if e.is_dir && !e.expanded {
                        self.filetree.toggle_expand();
                    }
                }
            }
            // Ctrl+C/D: quit
            // Ctrl+C: quit.
            KeyCode::Char('c')
                if key.modifiers == KeyModifiers::CONTROL =>
            {
                return false;
            }
            _ => {}
        }
        true
    }

    pub(super) fn handle_sysinfo_state(&mut self, _key: KeyEvent) -> bool {
        self.state = State::Normal;
        true
    }

    pub(super) fn handle_help_state(&mut self, _key: KeyEvent) -> bool {
        self.state = State::Normal;
        true
    }
}
