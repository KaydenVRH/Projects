//! Application state and input handling.
//!
//! Slow work (searching, resolving streams, thumbnails) runs on threads and
//! reports back over channels, so the interface never freezes.

use crate::gfx::{Graphics, THUMB_ID, VIDEO_ID};
use crate::player::{fit, Limits, Player};
use crate::theme::{self, Theme};
use crate::thumbs::{self, Thumbs};
use crate::youtube::{self, Stream, Video};
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// Where the list of videos comes from: one of your feeds, or a search.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Home,
    Subscriptions,
    WatchLater,
    History,
    Search(String),
}

impl Source {
    pub const FEEDS: [Source; 4] = [Source::Home, Source::Subscriptions, Source::WatchLater, Source::History];

    pub fn title(&self) -> String {
        match self {
            Source::Home => "Home".into(),
            Source::Subscriptions => "Subscriptions".into(),
            Source::WatchLater => "Watch later".into(),
            Source::History => "History".into(),
            Source::Search(q) if youtube::looks_like_url(q) => "Link".into(),
            Source::Search(q) => format!("“{q}”"),
        }
    }

    fn feed_url(&self) -> Option<&'static str> {
        match self {
            Source::Home => Some(":ytrec"),
            Source::Subscriptions => Some(":ytsubs"),
            Source::WatchLater => Some(":ytwatchlater"),
            Source::History => Some(":ythistory"),
            Source::Search(_) => None,
        }
    }
}

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
    /// A look to keep; `None` follows kshell's theme.
    pub theme: Option<Theme>,
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
    pub tabs: Vec<(Rect, Source)>,
    pub thumb: Option<Rect>,
    pub video: Option<Rect>,
    pub progress: Option<Rect>,
    pub search: Rect,
    /// The key hints on screen, each a button for its key.
    pub buttons: Vec<(Rect, &'static str)>,
}

pub struct App {
    pub query: String,
    pub focus: Focus,
    /// The list on screen, and where it came from.
    pub source: Source,
    pub results: Vec<Video>,
    pub list: ListState,
    /// Lists of the other tabs, kept so switching back is instant.
    lists: HashMap<Source, (Vec<Video>, Option<usize>)>,
    pub last_search: Option<String>,
    pub fetching: Option<(Source, Instant, Receiver<Result<Vec<Video>>>)>,
    /// A browser login is configured, so the feeds can load.
    pub logged_in: bool,
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
    /// Keeps the look in step with kshell while on `auto`.
    look: theme::Follower,
    look_checked: Instant,
    /// Where and when the last left click landed, for double clicks.
    last_click: Option<(Position, Instant)>,
    /// Dragging along the progress bar.
    scrubbing: bool,
}

impl App {
    pub fn new(gfx: Option<Graphics>, settings: Settings) -> App {
        let missing = ["yt-dlp", "ffmpeg", "mpv"].into_iter().filter(|t| !on_path(t)).collect();
        let volume = settings.volume;
        if let Some(theme) = settings.theme {
            theme::set(theme);
        }
        let look = theme::Follower::new(settings.theme.is_none());
        App {
            query: String::new(),
            focus: Focus::Search,
            source: Source::Home,
            results: Vec::new(),
            list: ListState::default(),
            lists: HashMap::new(),
            last_search: None,
            fetching: None,
            logged_in: youtube::logged_in(),
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
            look,
            look_checked: Instant::now(),
            last_click: None,
            scrubbing: false,
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
        self.fetching.is_some() || self.loading.is_some()
    }

    /// The tabs above the list: your feeds, then the last search.
    pub fn tabs(&self) -> Vec<Source> {
        let mut tabs = Source::FEEDS.to_vec();
        if let Some(q) = &self.last_search {
            tabs.push(Source::Search(q.clone()));
        }
        tabs
    }

    // ── background work ────────────────────────────────────────────

    pub fn search(&mut self, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.last_search = Some(query.clone());
        let source = Source::Search(query);
        self.lists.remove(&source);
        self.open(source);
    }

    /// Show a tab: from the cache if it was loaded before, else fetch it.
    pub fn open(&mut self, source: Source) {
        if source != self.source {
            let list = std::mem::take(&mut self.results);
            self.lists.insert(self.source.clone(), (list, self.list.selected()));
            self.source = source.clone();
            self.list = ListState::default();
        }
        if let Some((videos, selected)) = self.lists.remove(&source) {
            self.results = videos;
            self.list.select(selected.or(Some(0)));
            return;
        }
        self.results.clear();
        self.fetch(source);
    }

    /// Fetch the current tab again.
    pub fn refresh(&mut self) {
        let source = self.source.clone();
        self.lists.remove(&source);
        self.fetch(source);
    }

    fn fetch(&mut self, source: Source) {
        if source.feed_url().is_some() && !self.logged_in {
            return; // the empty list explains how to log in
        }
        let (tx, rx) = mpsc::channel();
        let limit = self.settings.results;
        let s = source.clone();
        std::thread::spawn(move || {
            let result = match &s {
                Source::Search(q) if youtube::looks_like_url(q) => youtube::lookup(q).map(|v| vec![v]),
                Source::Search(q) => youtube::search(q, limit),
                feed => youtube::feed(feed.feed_url().unwrap(), limit),
            };
            let _ = tx.send(result);
        });
        self.fetching = Some((source, Instant::now(), rx));
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

        // kshell switched theme: follow it (a stat a second).
        if self.look_checked.elapsed() >= Duration::from_secs(1) {
            self.look_checked = Instant::now();
            changed |= self.look.poll();
        }

        if let Some((source, _, rx)) = &self.fetching {
            if let Ok(result) = rx.try_recv() {
                let source = source.clone();
                self.fetching = None;
                changed = true;
                match result {
                    Ok(videos) if videos.is_empty() => match &source {
                        Source::Search(q) => self.say(format!("Nothing found for “{q}”"), true),
                        feed => self.say(format!("{} is empty", feed.title()), false),
                    },
                    Ok(videos) if source != self.source => {
                        // You switched tabs while it loaded; keep it for later.
                        self.lists.insert(source, (videos, Some(0)));
                    }
                    Ok(videos) => {
                        let direct = matches!(&source, Source::Search(q) if youtube::looks_like_url(q));
                        self.results = videos;
                        self.list = ListState::default().with_selected(Some(0));
                        self.focus = Focus::Results;
                        self.status = None;
                        if direct {
                            let v = self.results[0].clone();
                            self.play(v, false);
                        }
                    }
                    Err(e) => self.say(format!("Couldn't load {}: {e}", source.title()), true),
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
            Event::Mouse(m) => self.mouse(m, out),
            Event::Resize(..) => {
                self.cell = cell_size();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The mouse, everywhere:
    ///
    /// - browsing: click a result to select it, click it again (or double
    ///   click) to play, right click to play just the audio; click the search
    ///   box to type, a tab to open it, the thumbnail to play; scroll the list.
    /// - playing: click the video to pause, double click for fullscreen,
    ///   scroll on it for volume; click or drag along the bar to seek, scroll
    ///   on the bar to step 5 s; right click to go back.
    /// - any key hint on screen is a button for its key; a click closes help.
    fn mouse(&mut self, m: MouseEvent, out: &mut impl Write) -> io::Result<()> {
        let pos = Position::new(m.column, m.row);
        let press = |key: KeyCode| KeyEvent::new(key, KeyModifiers::NONE);
        if self.help {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.help = false;
            }
            return Ok(());
        }
        let playing = self.player.is_some();
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(label) = self.layout.buttons.iter().find(|(r, _)| r.contains(pos)).map(|(_, l)| *l) {
                    if let Some(key) = hint_key(label) {
                        return self.key(key, out);
                    }
                }
                let double = self.last_click.is_some_and(|(at, when)| {
                    at.y == pos.y && at.x.abs_diff(pos.x) <= 2 && when.elapsed() < Duration::from_millis(400)
                });
                self.last_click = if double { None } else { Some((pos, Instant::now())) };
                if playing {
                    self.player_click(pos, double);
                    return Ok(());
                }
                return self.browse_click(pos, double, out);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.scrubbing => self.seek_at(pos.x),
            MouseEventKind::Up(MouseButton::Left) => self.scrubbing = false,
            MouseEventKind::Down(MouseButton::Right) => {
                if playing || self.loading.is_some() {
                    return self.key(press(KeyCode::Esc), out);
                }
                // right click a result: play just its audio
                if let Some(row) = self.row_at(pos) {
                    self.focus = Focus::Results;
                    self.list.select(Some(row));
                    return self.key(press(KeyCode::Char('a')), out);
                }
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let up = m.kind == MouseEventKind::ScrollUp;
                if let Some(player) = &mut self.player {
                    if self.layout.progress.is_some_and(|bar| bar.contains(pos)) {
                        player.seek_by(if up { 5.0 } else { -5.0 });
                    } else {
                        player.add_volume(if up { 5.0 } else { -5.0 });
                    }
                } else {
                    self.move_selection(if up { -3 } else { 3 });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn player_click(&mut self, pos: Position, double: bool) {
        if self.layout.progress.is_some_and(|bar| bar.contains(pos)) {
            self.scrubbing = true;
            self.seek_at(pos.x);
            return;
        }
        if self.layout.video.is_some_and(|v| v.contains(pos)) {
            let Some(player) = &mut self.player else { return };
            // the first click of a double paused it: put that back
            player.toggle_pause();
            if double {
                self.chrome = !self.chrome;
            }
        }
    }

    /// Seek to where column `x` falls on the progress bar.
    fn seek_at(&mut self, x: u16) {
        let (Some(player), Some(bar)) = (&mut self.player, self.layout.progress) else { return };
        if let Some(d) = player.duration() {
            let frac = (x.saturating_sub(bar.x)) as f64 / bar.width.max(1) as f64;
            player.seek_to(frac.clamp(0.0, 1.0) * d);
        }
    }

    fn browse_click(&mut self, pos: Position, double: bool, out: &mut impl Write) -> io::Result<()> {
        let press = |key: KeyCode| KeyEvent::new(key, KeyModifiers::NONE);
        if self.layout.search.contains(pos) {
            self.focus = Focus::Search;
            return Ok(());
        }
        if let Some((_, tab)) = self.layout.tabs.iter().find(|(r, _)| r.contains(pos)) {
            let tab = tab.clone();
            self.focus = Focus::Results;
            self.open(tab);
            return Ok(());
        }
        if self.layout.thumb.is_some_and(|t| t.contains(pos)) && self.selected().is_some() {
            self.focus = Focus::Results;
            return self.key(press(KeyCode::Enter), out);
        }
        if let Some(row) = self.row_at(pos) {
            // a second click on the armed row plays it
            let again = self.focus == Focus::Results && self.list.selected() == Some(row);
            self.focus = Focus::Results;
            self.list.select(Some(row));
            if again || double {
                return self.key(press(KeyCode::Enter), out);
            }
        }
        Ok(())
    }

    /// The result under `pos`, if any.
    fn row_at(&self, pos: Position) -> Option<usize> {
        let r = self.layout.results;
        if !r.contains(pos) || pos.y <= r.y {
            return None;
        }
        // Rows are two lines plus a gap, under the border.
        let row = (pos.y - r.y - 1) as usize / 3 + self.list.offset();
        (row < self.results.len()).then_some(row)
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
            KeyCode::Char('t') => {
                // Picking one stops following kshell until the next start.
                let next = theme::current().next();
                theme::set(next);
                self.look.auto = false;
                self.say(format!("Theme: {} (theme = {} in the config keeps it)", next.name(), next.name().to_lowercase()), false);
            }
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char(c @ '1'..='5') => {
                if let Some(tab) = self.tabs().get((c as u8 - b'1') as usize) {
                    self.open(tab.clone());
                }
            }
            KeyCode::Char(']') => self.cycle_tab(1),
            KeyCode::Char('[') => self.cycle_tab(-1),
            _ => {}
        }
    }

    fn cycle_tab(&mut self, by: i64) {
        let tabs = self.tabs();
        let i = tabs.iter().position(|t| *t == self.source).unwrap_or(0) as i64;
        let next = (i + by).rem_euclid(tabs.len() as i64) as usize;
        self.open(tabs[next].clone());
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

/// The key a hint stands for, as drawn: `enter`, `ctrl-u`, `←→`, `1-5`, `q`…
/// Ranges press the step forward (`1-5` is the next tab, `←→` seeks ahead).
pub fn hint_key(label: &str) -> Option<KeyEvent> {
    let plain = |code| Some(KeyEvent::new(code, KeyModifiers::NONE));
    match label {
        "enter" => plain(KeyCode::Enter),
        "tab" => plain(KeyCode::Tab),
        "esc" => plain(KeyCode::Esc),
        "space" => plain(KeyCode::Char(' ')),
        "←→" => plain(KeyCode::Right),
        "↑↓" => plain(KeyCode::Up),
        "1-5" => plain(KeyCode::Char(']')),
        "ctrl-u" => Some(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => plain(KeyCode::Char(c)),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(n: usize) -> Video {
        serde_json::from_value(serde_json::json!({ "id": format!("vid{n:08}"), "title": format!("v{n}") })).unwrap()
    }

    fn app() -> App {
        let settings = Settings { max_height: 1080, limits: Limits { max_width: 1920, max_fps: 30 }, results: 30, volume: 0.0, theme: Some(crate::theme::Theme::Neon) };
        let mut app = App::new(None, settings);
        app.logged_in = false; // no yt-dlp calls in tests
        app
    }

    #[test]
    fn tabs_keep_their_list_and_selection() {
        let mut app = app();
        app.results = (0..10).map(video).collect();
        app.list.select(Some(5));
        app.open(Source::WatchLater);
        assert_eq!(app.source, Source::WatchLater);
        assert!(app.results.is_empty());
        assert!(app.fetching.is_none(), "feeds don't load without a login");
        app.open(Source::Home);
        assert_eq!(app.results.len(), 10);
        assert_eq!(app.list.selected(), Some(5));
    }

    #[test]
    fn a_search_adds_a_fifth_tab_and_tabs_cycle() {
        let mut app = app();
        assert_eq!(app.tabs().len(), 4);
        app.last_search = Some("lofi".into());
        // Cached, so switching to it doesn't run a real search.
        app.lists.insert(Source::Search("lofi".into()), (vec![video(1)], Some(0)));
        assert_eq!(app.tabs().last(), Some(&Source::Search("lofi".into())));
        app.cycle_tab(-1);
        assert_eq!(app.source, Source::Search("lofi".into()));
        app.cycle_tab(1);
        assert_eq!(app.source, Source::Home);
    }

    fn click(app: &mut App, x: u16, y: u16, button: MouseButton) {
        let m = MouseEvent { kind: MouseEventKind::Down(button), column: x, row: y, modifiers: KeyModifiers::NONE };
        app.mouse(m, &mut Vec::new()).unwrap();
    }

    #[test]
    fn hints_name_their_keys() {
        assert_eq!(hint_key("enter").map(|k| k.code), Some(KeyCode::Enter));
        assert_eq!(hint_key("space").map(|k| k.code), Some(KeyCode::Char(' ')));
        assert_eq!(hint_key("1-5").map(|k| k.code), Some(KeyCode::Char(']')));
        assert_eq!(hint_key("←→").map(|k| k.code), Some(KeyCode::Right));
        let clear = hint_key("ctrl-u").unwrap();
        assert_eq!((clear.code, clear.modifiers), (KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(hint_key("/").map(|k| k.code), Some(KeyCode::Char('/')));
        assert_eq!(hint_key("nonsense"), None);
    }

    #[test]
    fn clicking_picks_rows_the_search_box_and_buttons() {
        let mut app = app();
        app.results = (0..10).map(video).collect();
        app.focus = Focus::Search;
        app.layout.results = Rect::new(0, 5, 50, 30);
        app.layout.search = Rect::new(0, 1, 50, 3);
        // rows are three lines each under the border: row 2 starts at 5+1+6
        click(&mut app, 4, 12, MouseButton::Left);
        assert_eq!(app.focus, Focus::Results);
        assert_eq!(app.list.selected(), Some(2));
        click(&mut app, 4, 2, MouseButton::Left);
        assert_eq!(app.focus, Focus::Search);
        // a hint is a button for its key
        app.focus = Focus::Results;
        app.layout.buttons = vec![(Rect::new(60, 40, 6, 1), "q")];
        click(&mut app, 62, 40, MouseButton::Left);
        assert!(app.quit);
    }

    #[test]
    fn a_click_closes_help() {
        let mut app = app();
        app.help = true;
        click(&mut app, 1, 1, MouseButton::Left);
        assert!(!app.help);
    }
}

