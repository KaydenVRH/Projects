//! Application state and input handling.
//!
//! Slow work (searching, resolving streams, thumbnails) runs on threads and
//! reports back over channels, so the interface never freezes.

use crate::gfx::{Graphics, THUMB_ID, VIDEO_ID};
use crate::player::{fit, Limits, Player};
use crate::thumbs::{self, Thumbs};
use crate::youtube::{self, Stream, Video};
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::widgets::ListState;
use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Search,
    Results,
}

pub struct Settings {
    pub max_height: u32,
    pub limits: Limits,
    pub results: usize,
    pub volume: f64,
}

pub struct Loading {
    pub video: Video,
    pub audio_only: bool,
    pub since: Instant,
    rx: Receiver<Result<Stream>>,
}

pub struct Status {
    pub text: String,
    pub error: bool,
    at: Instant,
}

/// Where things were drawn last frame; filled in by `ui::render`.
#[derive(Default)]
pub struct Layout {
    pub results: Rect,
    pub thumb: Option<Rect>,
    pub video: Option<Rect>,
    pub progress: Option<Rect>,
}

pub struct App {
    pub query: String,
    pub focus: Focus,
    pub results: Vec<Video>,
    pub list: ListState,
    pub searching: Option<(String, Instant, Receiver<Result<Vec<Video>>>)>,
    pub loading: Option<Loading>,
    pub player: Option<Player>,
    pub status: Option<Status>,
    pub thumbs: Thumbs,
    pub gfx: Option<Graphics>,
    /// Pixel size of one terminal cell.
    pub cell: (f64, f64),
    pub settings: Settings,
    pub volume: f64,
    /// Title bar and controls around the video; `f` hides them.
    pub chrome: bool,
    pub osd_until: Option<Instant>,
    pub help: bool,
    pub autoplay: bool,
    pub missing: Vec<&'static str>,
    pub quit: bool,
    pub layout: Layout,
    thumb_shown: Option<(String, Rect)>,
    pub start: Instant,
}

impl App {
    pub fn new(gfx: Option<Graphics>, settings: Settings) -> App {
        let missing = ["yt-dlp", "ffmpeg", "mpv"].into_iter().filter(|t| !on_path(t)).collect();
        let volume = settings.volume;
        App {
            query: String::new(),
            focus: Focus::Search,
            results: Vec::new(),
            list: ListState::default(),
            searching: None,
            loading: None,
            player: None,
            status: None,
            thumbs: Thumbs::new(),
            gfx,
            cell: cell_size(),
            settings,
            volume,
            chrome: true,
            osd_until: None,
            help: false,
            autoplay: false,
            missing,
            quit: false,
            layout: Layout::default(),
            thumb_shown: None,
            start: Instant::now(),
        }
    }

    pub fn selected(&self) -> Option<&Video> {
        self.list.selected().and_then(|i| self.results.get(i))
    }

    pub fn say(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        if error {
            crate::log!("error: {text}");
        }
        self.status = Some(Status { text, error, at: Instant::now() });
    }

    pub fn status_text(&self) -> Option<&Status> {
        self.status.as_ref().filter(|s| s.error || s.at.elapsed() < Duration::from_secs(4))
    }

    /// A spinner frame, shared by everything that waits.
    pub fn spinner(&self) -> &'static str {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[(self.start.elapsed().as_millis() / 80) as usize % FRAMES.len()]
    }

    pub fn busy(&self) -> bool {
        self.searching.is_some() || self.loading.is_some()
    }

    // ── background work ────────────────────────────────────────────

    pub fn search(&mut self, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let q = query.clone();
        if youtube::looks_like_url(&query) {
            std::thread::spawn(move || {
                let _ = tx.send(youtube::lookup(&q).map(|v| vec![v]));
            });
        } else {
            let limit = self.settings.results;
            std::thread::spawn(move || {
                let _ = tx.send(youtube::search(&q, limit));
            });
        }
        self.searching = Some((query, Instant::now(), rx));
    }

    pub fn play(&mut self, video: Video, audio_only: bool) {
        let (tx, rx) = mpsc::channel();
        let v = video.clone();
        let height = self.settings.max_height;
        let fps = self.settings.limits.max_fps;
        // Without graphics, video would only be decoded to be thrown away.
        let audio_only = audio_only || self.gfx.is_none();
        std::thread::spawn(move || {
            let _ = tx.send(youtube::resolve(&v, height, fps, audio_only));
        });
        self.loading = Some(Loading { video, audio_only, since: Instant::now(), rx });
    }

    /// Poll the background work and advance playback. Returns whether
    /// anything visible changed.
    pub fn update(&mut self, out: &mut impl Write) -> io::Result<bool> {
        let mut changed = false;

        if let Some((query, _, rx)) = &self.searching {
            if let Ok(result) = rx.try_recv() {
                let query = query.clone();
                self.searching = None;
                changed = true;
                match result {
                    Ok(videos) if videos.is_empty() => self.say(format!("Nothing found for “{query}”"), true),
                    Ok(videos) => {
                        let direct = youtube::looks_like_url(&query);
                        self.results = videos;
                        self.list = ListState::default().with_selected(Some(0));
                        self.focus = Focus::Results;
                        self.status = None;
                        if direct {
                            let v = self.results[0].clone();
                            self.play(v, false);
                        }
                    }
                    Err(e) => self.say(format!("Search failed: {e}"), true),
                }
            }
        }

        if let Some(loading) = &self.loading {
            if let Ok(result) = loading.rx.try_recv() {
                let loading = self.loading.take().unwrap();
                changed = true;
                match result {
                    Ok(stream) => {
                        crate::log!(
                            "stream: {}x{} @ {} fps, live {}, video {}",
                            stream.width, stream.height, stream.fps, stream.live, stream.video_url.is_some()
                        );
                        self.stop(out)?;
                        let player = Player::start(loading.video, stream, self.volume, self.settings.limits);
                        if let Some(e) = &player.audio_error {
                            self.say(format!("No audio: {e}"), true);
                        }
                        self.player = Some(player);
                        self.osd_until = Some(Instant::now() + Duration::from_secs(3));
                    }
                    Err(e) => self.say(format!("Couldn't play “{}”: {e}", loading.video.title), true),
                }
            }
        }

        if let Some(player) = &mut self.player {
            if let Some(gfx) = &mut self.gfx {
                player.tick(gfx, out)?;
            }
            if player.ended() {
                changed = true;
                let error = (player.frames_shown == 0).then(|| player.video_error()).flatten();
                let next = self.list.selected().map(|i| i + 1).filter(|&i| i < self.results.len());
                self.stop(out)?;
                if let Some(e) = error {
                    self.say(format!("Playback failed: {e}"), true);
                } else if let (true, Some(i)) = (self.autoplay, next) {
                    self.list.select(Some(i));
                    let v = self.results[i].clone();
                    self.play(v, false);
                }
            }
        }
        Ok(changed)
    }

    /// Leave the player, remembering the volume for next time.
    pub fn stop(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some(p) = self.player.take() {
            if let Some((v, _)) = p.volume() {
                self.volume = v;
            }
            if let Some(gfx) = &mut self.gfx {
                gfx.delete(out, VIDEO_ID)?;
            }
        }
        Ok(())
    }

    /// After a frame is drawn: size the video to its area and send the
    /// thumbnail when the selection or its box changed.
    pub fn after_draw(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let (Some(player), Some(area)) = (&mut self.player, self.layout.video) {
            player.set_area(area, self.cell);
        }
        let id = match (&self.player, &self.loading) {
            (Some(p), _) => Some(p.video.id.clone()),
            (None, Some(l)) => Some(l.video.id.clone()),
            _ => self.selected().map(|v| v.id.clone()),
        };
        let Some(gfx) = &mut self.gfx else { return Ok(()) };
        let (Some(area), Some(id)) = (self.layout.thumb, id) else {
            if self.thumb_shown.take().is_some() {
                gfx.delete(out, THUMB_ID)?;
            }
            return Ok(());
        };
        if self.thumb_shown.as_ref() == Some(&(id.clone(), area)) {
            return Ok(());
        }
        if let Some(rgb) = self.thumbs.get(&id) {
            gfx.show(out, THUMB_ID, &rgb, thumbs::WIDTH, thumbs::HEIGHT, area.width, area.height)?;
            self.thumb_shown = Some((id, area));
        }
        Ok(())
    }

    /// The thumbnail box for `id` if its picture is on screen.
    pub fn thumb_ready(&self, id: &str) -> bool {
        self.thumb_shown.as_ref().is_some_and(|(shown, _)| shown == id)
    }

    /// The box a 16:9 thumbnail takes inside `area`.
    pub fn thumb_box(&self, area: Rect) -> Rect {
        fit(area, self.cell, thumbs::WIDTH, thumbs::HEIGHT)
    }

    // ── input ───────────────────────────────────────────────────────

    pub fn handle(&mut self, event: Event, out: &mut impl Write) -> io::Result<()> {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key, out),
            Event::Mouse(m) => {
                let pos = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::ScrollDown if self.player.is_none() => self.move_selection(3),
                    MouseEventKind::ScrollUp if self.player.is_none() => self.move_selection(-3),
                    MouseEventKind::Down(MouseButton::Left) => self.click(pos),
                    _ => {}
                }
                Ok(())
            }
            Event::Resize(..) => {
                self.cell = cell_size();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn click(&mut self, pos: Position) {
        if let (Some(player), Some(bar)) = (&mut self.player, self.layout.progress) {
            if bar.contains(pos) {
                if let Some(d) = player.duration() {
                    let frac = (pos.x - bar.x) as f64 / bar.width.max(1) as f64;
                    player.seek_to(frac * d);
                }
            }
            return;
        }
        let r = self.layout.results;
        if r.contains(pos) && pos.y > r.y {
            // Rows are two lines plus a gap, under the border.
            let row = (pos.y - r.y - 1) as usize / 3 + self.list.offset();
            if row < self.results.len() {
                self.focus = Focus::Results;
                self.list.select(Some(row));
            }
        }
    }

    fn key(&mut self, key: KeyEvent, out: &mut impl Write) -> io::Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return Ok(());
        }
        if self.help {
            self.help = false;
            return Ok(());
        }
        if self.player.is_some() || self.loading.is_some() {
            return self.player_key(key, out);
        }
        match self.focus {
            Focus::Search => self.search_key(key),
            Focus::Results => self.results_key(key),
        }
        Ok(())
    }

    fn search_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => self.search(self.query.clone()),
            KeyCode::Esc | KeyCode::Down | KeyCode::Tab if !self.results.is_empty() => {
                self.focus = Focus::Results
            }
            KeyCode::Esc if self.query.is_empty() => self.quit = true,
            KeyCode::Esc => self.query.clear(),
            KeyCode::Char('u') if ctrl => self.query.clear(),
            KeyCode::Char('w') if ctrl => {
                let trimmed = self.query.trim_end().len();
                self.query.truncate(trimmed);
                let cut = self.query.rfind(' ').map_or(0, |i| i + 1);
                self.query.truncate(cut);
            }
            KeyCode::Backspace => {
                self.query.pop();
            }
            KeyCode::Char(c) if !ctrl => self.query.push(c),
            _ => {}
        }
    }

    fn results_key(&mut self, key: KeyEvent) {
        let page = (self.layout.results.height as i64 / 3).max(1);
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('/') | KeyCode::Char('s') | KeyCode::Char('i') | KeyCode::Tab | KeyCode::Esc => {
                self.focus = Focus::Search
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.list.selected() == Some(0) {
                    self.focus = Focus::Search;
                } else {
                    self.move_selection(-1)
                }
            }
            KeyCode::PageDown | KeyCode::Char('d') => self.move_selection(page),
            KeyCode::PageUp | KeyCode::Char('u') => self.move_selection(-page),
            KeyCode::Home | KeyCode::Char('g') => self.list.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => self.list.select(Some(self.results.len().saturating_sub(1))),
            KeyCode::Enter | KeyCode::Char('l') => self.play_selected(false),
            KeyCode::Char('a') => self.play_selected(true),
            KeyCode::Char('A') => {
                self.autoplay = !self.autoplay;
                let state = if self.autoplay { "on" } else { "off" };
                self.say(format!("Autoplay {state}"), false);
            }
            KeyCode::Char('?') => self.help = true,
            _ => {}
        }
    }

    fn play_selected(&mut self, audio_only: bool) {
        if let Some(v) = self.selected().cloned() {
            self.play(v, audio_only);
        }
    }

    fn move_selection(&mut self, by: i64) {
        if self.results.is_empty() {
            return;
        }
        let cur = self.list.selected().unwrap_or(0) as i64;
        let next = (cur + by).clamp(0, self.results.len() as i64 - 1);
        self.list.select(Some(next as usize));
    }

    fn player_key(&mut self, key: KeyEvent, out: &mut impl Write) -> io::Result<()> {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace) {
            self.loading = None;
            return self.stop(out);
        }
        if matches!(key.code, KeyCode::Char('?')) {
            self.help = true;
            return Ok(());
        }
        let Some(player) = &mut self.player else { return Ok(()) };
        self.osd_until = Some(Instant::now() + Duration::from_secs(2));
        match key.code {
            KeyCode::Char(' ') | KeyCode::Char('k') => player.toggle_pause(),
            KeyCode::Left | KeyCode::Char('h') => player.seek_by(if shift { -30.0 } else { -5.0 }),
            KeyCode::Right | KeyCode::Char('l') => player.seek_by(if shift { 30.0 } else { 5.0 }),
            KeyCode::Char('H') => player.seek_by(-30.0),
            KeyCode::Char('L') => player.seek_by(30.0),
            KeyCode::Char(c @ '0'..='9') => {
                if let Some(d) = player.duration() {
                    player.seek_to(d * (c as u8 - b'0') as f64 / 10.0);
                }
            }
            KeyCode::Up | KeyCode::Char('+') | KeyCode::Char('=') => player.add_volume(5.0),
            KeyCode::Down | KeyCode::Char('-') => player.add_volume(-5.0),
            KeyCode::Char('m') => player.toggle_mute(),
            KeyCode::Char('f') => self.chrome = !self.chrome,
            KeyCode::Char('n') => self.skip(1, out)?,
            KeyCode::Char('p') => self.skip(-1, out)?,
            _ => {}
        }
        Ok(())
    }

    fn skip(&mut self, by: i64, out: &mut impl Write) -> io::Result<()> {
        let Some(i) = self.list.selected() else { return Ok(()) };
        let next = i as i64 + by;
        if next < 0 || next >= self.results.len() as i64 {
            return Ok(());
        }
        let audio_only = self.player.as_ref().is_some_and(|p| !p.has_video());
        self.stop(out)?;
        self.list.select(Some(next as usize));
        let v = self.results[next as usize].clone();
        self.play(v, audio_only);
        Ok(())
    }
}

fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()))
        .unwrap_or(false)
}

/// Pixel size of a terminal cell, from the tty. Falls back to a typical
/// 1:2 cell when the terminal doesn't say.
pub fn cell_size() -> (f64, f64) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
    if ok && ws.ws_xpixel > 0 && ws.ws_ypixel > 0 && ws.ws_col > 0 && ws.ws_row > 0 {
        (ws.ws_xpixel as f64 / ws.ws_col as f64, ws.ws_ypixel as f64 / ws.ws_row as f64)
    } else {
        (10.0, 20.0)
    }
}
