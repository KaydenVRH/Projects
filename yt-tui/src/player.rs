//! A playing video: the audio process, the decoder, and the clock that
//! keeps them together.
//!
//! mpv's playback position is the master clock. A frame is shown once the
//! clock reaches its timestamp; frames that fall behind are dropped rather
//! than shown late, so a slow moment costs smoothness, never sync. Live
//! streams can't seek, so they run on a wall clock instead.

use crate::audio::Audio;
use crate::gfx::{Graphics, MAX_CELLS, VIDEO_ID};
use crate::video::{Decoder, Frame, Next};
use crate::youtube::{Stream, Video};
use ratatui::layout::Rect;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Widest frame ffmpeg will produce; kitty scales it up to fill the
    /// placement on the GPU, which is free.
    pub max_width: u32,
    pub max_fps: u32,
}

pub struct Player {
    pub video: Video,
    pub stream: Stream,
    audio: Option<Audio>,
    pub audio_error: Option<String>,
    decoder: Option<Decoder>,
    pending: Option<Frame>,
    /// Where the picture sits, in cells, and the frame size feeding it.
    pub placement: Option<Placement>,
    wanted: Option<(Placement, Instant)>,
    wall: WallClock,
    paused: bool,
    video_done: bool,
    shown: VecDeque<Instant>,
    pub frames_shown: u64,
    pub frames_dropped: u64,
    limits: Limits,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub area: Rect,
    pub width: u32,
    pub height: u32,
}

impl Player {
    pub fn start(video: Video, stream: Stream, volume: f64, limits: Limits) -> Player {
        let (audio, audio_error) = match Audio::spawn(&stream.audio_url, &stream.user_agent, 0.0, volume) {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Player {
            video,
            stream,
            audio,
            audio_error,
            decoder: None,
            pending: None,
            placement: None,
            wanted: None,
            wall: WallClock::default(),
            paused: false,
            video_done: false,
            shown: VecDeque::new(),
            frames_shown: 0,
            frames_dropped: 0,
            limits,
        }
    }

    pub fn has_video(&self) -> bool {
        self.stream.video_url.is_some()
    }

    /// Fit the picture into `area` (cells of `cell` pixels). A new size
    /// restarts the decoder at the current position, but only once the
    /// size has held still for a moment — dragging a window edge fires
    /// dozens of resizes.
    pub fn set_area(&mut self, area: Rect, cell: (f64, f64)) {
        if !self.has_video() || area.width == 0 || area.height == 0 {
            return;
        }
        let p = placement_for(area, cell, &self.stream, self.limits.max_width);
        if self.placement.is_none() {
            self.placement = Some(p);
            self.restart_decoder(0.0);
        } else if self.placement == Some(p) {
            self.wanted = None;
        } else if self.wanted.map(|(w, _)| w) != Some(p) {
            self.wanted = Some((p, Instant::now()));
        }
    }

    fn restart_decoder(&mut self, at: f64) {
        let (Some(p), Some(url)) = (self.placement, self.stream.video_url.clone()) else {
            return;
        };
        self.pending = None;
        self.video_done = false;
        self.decoder = None; // kill the old ffmpeg before starting another
        self.decoder = Decoder::spawn(&self.stream, &url, at, p.width, p.height, self.fps()).ok();
        if self.decoder.is_none() {
            self.video_done = true;
        }
    }

    fn fps(&self) -> f64 {
        self.stream.fps.min(self.limits.max_fps as f64).max(1.0)
    }

    fn audio_clock(&self) -> Option<&Audio> {
        self.audio.as_ref().filter(|a| !self.stream.live && !a.state().ended)
    }

    /// The position the picture should be showing, if playback has begun.
    pub fn clock(&self) -> Option<f64> {
        match self.audio_clock() {
            Some(a) => a.clock(),
            None => self.wall.now(),
        }
    }

    pub fn position(&self) -> f64 {
        self.clock().unwrap_or(0.0)
    }

    pub fn duration(&self) -> Option<f64> {
        self.stream
            .duration
            .or_else(|| self.audio.as_ref().and_then(|a| a.state().duration))
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn buffering(&self) -> bool {
        let audio_waiting = self.audio.as_ref().is_some_and(|a| {
            let st = a.state();
            st.buffering || (st.pos.is_none() && !st.ended)
        });
        let video_waiting = self.has_video() && self.frames_shown == 0 && !self.video_done;
        !self.paused && (audio_waiting || video_waiting)
    }

    pub fn volume(&self) -> Option<(f64, bool)> {
        self.audio.as_ref().map(|a| {
            let st = a.state();
            (st.volume, st.muted)
        })
    }

    /// Frames actually shown over the last second.
    pub fn fps_now(&self) -> usize {
        let second = Duration::from_secs(1);
        self.shown.iter().filter(|t| t.elapsed() <= second).count()
    }

    pub fn decode_size(&self) -> Option<(u32, u32, f64)> {
        self.decoder.as_ref().map(|d| (d.width, d.height, d.fps))
    }

    pub fn video_error(&self) -> Option<String> {
        self.decoder.as_ref().and_then(|d| d.error())
    }

    pub fn ended(&self) -> bool {
        let audio_done = self.audio.as_ref().map_or(true, |a| a.state().ended);
        let video_done = !self.has_video() || (self.video_done && self.pending.is_none());
        audio_done && video_done
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if let Some(a) = &self.audio {
            a.set_pause(self.paused);
        }
        if self.paused {
            self.wall.pause();
        } else {
            self.wall.resume();
        }
    }

    pub fn seek_by(&mut self, delta: f64) {
        self.seek_to(self.position() + delta);
    }

    pub fn seek_to(&mut self, to: f64) {
        if self.stream.live {
            return;
        }
        let end = self.duration().map_or(f64::MAX, |d| (d - 1.0).max(0.0));
        let to = to.clamp(0.0, end);
        if let Some(a) = &self.audio {
            a.seek(to);
        }
        // Without audio the wall clock restarts with the first new frame.
        self.wall = WallClock { paused: self.paused, ..WallClock::default() };
        self.restart_decoder(to);
    }

    pub fn add_volume(&self, delta: f64) {
        if let Some(a) = &self.audio {
            a.add_volume(delta);
        }
    }

    pub fn toggle_mute(&self) {
        if let Some(a) = &self.audio {
            a.toggle_mute();
        }
    }

    /// How long the main loop may sleep before the next frame is due.
    pub fn next_deadline(&self) -> Duration {
        match (&self.pending, self.clock()) {
            (Some(f), Some(now)) if !self.paused => {
                Duration::from_secs_f64((f.pts - now).clamp(0.001, 0.02))
            }
            _ => Duration::from_millis(10),
        }
    }

    /// Show whatever frame is due. Returns whether the picture changed.
    pub fn tick(&mut self, gfx: &mut Graphics, out: &mut impl Write) -> io::Result<bool> {
        if let Some((p, since)) = self.wanted {
            if since.elapsed() > Duration::from_millis(200) {
                self.wanted = None;
                self.placement = Some(p);
                let at = self.position();
                self.restart_decoder(at);
            }
        }
        // The audio ran out (or died) first: carry on from where it was.
        if self.audio.is_some() && self.audio_clock().is_none() && !self.stream.live && !self.wall.started() {
            let at = self.audio.as_ref().and_then(|a| a.state().pos).map_or(0.0, |(p, _)| p);
            self.wall.start_at(at, self.paused);
        }
        let Some(decoder) = &self.decoder else { return Ok(false) };
        let Some(place) = self.placement else { return Ok(false) };
        let clock = self.clock();
        let half_frame = 0.5 / decoder.fps;

        let mut due: Option<Frame> = None;
        loop {
            let frame = match self.pending.take() {
                Some(f) => f,
                None => match decoder.next() {
                    Next::Frame(f) => f,
                    Next::Waiting => break,
                    Next::Ended => {
                        self.video_done = true;
                        break;
                    }
                },
            };
            let show = match clock {
                Some(now) => frame.pts <= now + half_frame,
                // Not started yet: the first frame doubles as a poster.
                None => self.frames_shown == 0 && due.is_none(),
            };
            if !show {
                self.pending = Some(frame);
                break;
            }
            if let Some(late) = due.replace(frame) {
                self.frames_dropped += 1;
                decoder.recycle(late);
            }
            if clock.is_none() {
                break;
            }
        }

        let Some(frame) = due else { return Ok(false) };
        if self.audio_clock().is_none() && !self.wall.started() {
            self.wall.start_at(frame.pts, self.paused);
        }
        gfx.show(out, VIDEO_ID, &frame.data, place.width, place.height, place.area.width, place.area.height)?;
        decoder.recycle(frame);
        self.frames_shown += 1;
        let now = Instant::now();
        self.shown.push_back(now);
        while self.shown.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1)) {
            self.shown.pop_front();
        }
        Ok(true)
    }
}

/// Fit a `w`×`h` picture into `area`, keeping its shape on cells that
/// are `cell.0`×`cell.1` pixels, and centre it.
pub fn fit(area: Rect, cell: (f64, f64), w: u32, h: u32) -> Rect {
    let aspect = w.max(1) as f64 / h.max(1) as f64;
    let max_cols = area.width.min(MAX_CELLS) as f64;
    let max_rows = area.height.min(MAX_CELLS) as f64;
    let mut rows = max_rows;
    let mut cols = rows * aspect * cell.1 / cell.0;
    if cols > max_cols {
        cols = max_cols;
        rows = cols * cell.0 / (aspect * cell.1);
    }
    let cols = (cols.round() as u16).clamp(1, max_cols as u16);
    let rows = (rows.round() as u16).clamp(1, max_rows as u16);
    Rect {
        x: area.x + (area.width - cols) / 2,
        y: area.y + (area.height - rows) / 2,
        width: cols,
        height: rows,
    }
}

fn placement_for(area: Rect, cell: (f64, f64), stream: &Stream, max_width: u32) -> Placement {
    let area = fit(area, cell, stream.width, stream.height);
    let px_w = area.width as f64 * cell.0;
    let px_h = area.height as f64 * cell.1;
    // Never decode larger than the source or the cap; kitty upscales.
    let scale = (max_width as f64 / px_w).min(stream.width.max(2) as f64 / px_w).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32).max(1) * 2;
    Placement { area, width: even(px_w * scale), height: even(px_h * scale) }
}

/// Playback position for streams without an audio clock.
#[derive(Debug, Default, Clone, Copy)]
struct WallClock {
    base: f64,
    since: Option<Instant>,
    paused: bool,
    running: bool,
}

impl WallClock {
    fn started(&self) -> bool {
        self.running
    }

    fn start_at(&mut self, t: f64, paused: bool) {
        *self = WallClock { base: t, since: (!paused).then(Instant::now), paused, running: true };
    }

    fn now(&self) -> Option<f64> {
        if !self.running {
            return None;
        }
        Some(self.base + self.since.map_or(0.0, |s| s.elapsed().as_secs_f64()))
    }

    fn pause(&mut self) {
        if let Some(t) = self.now() {
            self.base = t;
        }
        self.since = None;
        self.paused = true;
    }

    fn resume(&mut self) {
        self.paused = false;
        if self.running {
            self.since = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(w: u32, h: u32) -> Stream {
        Stream {
            video_url: Some("v".into()),
            audio_url: "a".into(),
            user_agent: "ua".into(),
            width: w,
            height: h,
            fps: 30.0,
            duration: Some(100.0),
            live: false,
            title: "t".into(),
        }
    }

    #[test]
    fn widescreen_video_fills_the_width_and_is_centred() {
        // 100×50 cells of 10×20 px = 1000×1000 px: a 16:9 picture is
        // width-bound and gets letterboxed top and bottom.
        let r = fit(Rect::new(0, 0, 100, 50), (10.0, 20.0), 1920, 1080);
        assert_eq!(r.width, 100);
        assert_eq!(r.height, 28); // 1000 px / (16/9) = 562 px = 28 rows
        assert_eq!(r.y, 11);
    }

    #[test]
    fn tall_video_is_height_bound() {
        let r = fit(Rect::new(0, 0, 200, 40), (10.0, 20.0), 1080, 1920);
        assert_eq!(r.height, 40);
        assert_eq!(r.width, 45); // 800 px * 9/16 = 450 px
        assert_eq!(r.x, (200 - 45) / 2);
    }

    #[test]
    fn placements_never_exceed_the_placeholder_grid() {
        let r = fit(Rect::new(0, 0, 600, 400), (4.0, 8.0), 1920, 1080);
        assert!(r.width <= MAX_CELLS && r.height <= MAX_CELLS);
    }

    #[test]
    fn decode_size_is_capped_even_and_keeps_shape() {
        let p = placement_for(Rect::new(0, 0, 200, 60), (16.0, 32.0), &stream(1920, 1080), 1280);
        assert!(p.width <= 1280 && p.width % 2 == 0 && p.height % 2 == 0);
        let shape = p.width as f64 / p.height as f64;
        assert!((shape - 16.0 / 9.0).abs() < 0.05, "{shape}");
    }

    #[test]
    fn decode_size_never_upscales_the_source() {
        let p = placement_for(Rect::new(0, 0, 200, 60), (16.0, 32.0), &stream(640, 360), 4000);
        assert_eq!(p.width, 640);
    }

    #[test]
    fn wall_clock_stops_while_paused() {
        let mut w = WallClock::default();
        assert_eq!(w.now(), None);
        w.start_at(5.0, true);
        assert_eq!(w.now(), Some(5.0));
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(w.now(), Some(5.0));
        w.resume();
        std::thread::sleep(Duration::from_millis(20));
        assert!(w.now().unwrap() > 5.01);
    }
}
