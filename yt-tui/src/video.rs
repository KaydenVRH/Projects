//! Video decoding: ffmpeg turns the stream into raw RGB frames of exactly
//! the size they will be shown at, on a background thread.

use crate::youtube::Stream;
use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};

pub struct Frame {
    /// Presentation time in seconds from the start of the video.
    pub pts: f64,
    pub data: Vec<u8>,
}

pub enum Next {
    Frame(Frame),
    /// Nothing decoded yet; try again shortly.
    Waiting,
    /// ffmpeg finished (end of video) or died.
    Ended,
}

pub struct Decoder {
    child: Child,
    frames: Receiver<Frame>,
    recycle: Sender<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    error: Arc<Mutex<String>>,
}

/// Frames decoded ahead of the clock. Enough to ride out a network hiccup
/// at 30 fps without holding hundreds of megabytes.
const QUEUE: usize = 24;

/// Bytes per HTTP range request, for ffmpeg and mpv alike.
pub const CHUNK: &str = "10485760";

impl Decoder {
    pub fn spawn(stream: &Stream, url: &str, start: f64, width: u32, height: u32, fps: f64) -> Result<Self> {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error"]);
        if cfg!(target_os = "macos") {
            // Falls back to software by itself when a codec isn't supported.
            cmd.args(["-hwaccel", "videotoolbox"]);
        }
        if !stream.live && start > 0.0 {
            cmd.args(["-ss", &format!("{start:.3}")]);
        }
        cmd.args(["-user_agent", &stream.user_agent]);
        cmd.args(["-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_delay_max", "4"]);
        if !stream.live {
            // YouTube throttles one long download to about playback speed,
            // which is what made long videos stutter; ranged requests of
            // 10 MB (what yt-dlp itself uses) keep it at full speed.
            cmd.args(["-request_size", CHUNK, "-multiple_requests", "1"]);
        }
        cmd.args(["-i", url, "-an", "-sn", "-dn"]);
        // fps first so scaling only touches frames that will be shown.
        cmd.args(["-vf", &format!("fps={fps},scale={width}:{height}:flags=bilinear")]);
        cmd.args(["-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"]);
        crate::log!("decoder: {width}x{height} @ {fps} fps from {start:.1}s");
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("couldn't run ffmpeg — is it installed?")?;

        let error = Arc::new(Mutex::new(String::new()));
        let stderr = child.stderr.take().unwrap();
        let err = error.clone();
        std::thread::spawn(move || {
            // ffmpeg ends on a generic line ("Invalid data found…"); an HTTP
            // failure before it is the actual cause, so that one sticks.
            let mut found_cause = false;
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if line.trim().is_empty() || found_cause {
                    continue;
                }
                found_cause = line.contains("HTTP error") || line.contains("Server returned");
                *err.lock().unwrap() = line;
            }
        });

        let (tx, frames) = mpsc::sync_channel(QUEUE);
        let (recycle, spare) = mpsc::channel();
        let mut stdout = child.stdout.take().unwrap();
        let frame_len = (width * height * 3) as usize;
        let base = if stream.live { 0.0 } else { start };
        std::thread::spawn(move || read_frames(&mut stdout, tx, spare, frame_len, base, fps));

        Ok(Decoder { child, frames, recycle, width, height, fps, error })
    }

    pub fn next(&self) -> Next {
        match self.frames.try_recv() {
            Ok(f) => Next::Frame(f),
            Err(TryRecvError::Empty) => Next::Waiting,
            Err(TryRecvError::Disconnected) => Next::Ended,
        }
    }

    /// Hand a shown frame's buffer back to be filled again.
    pub fn recycle(&self, frame: Frame) {
        let _ = self.recycle.send(frame.data);
    }

    pub fn error(&self) -> Option<String> {
        let e = self.error.lock().unwrap();
        (!e.is_empty()).then(|| e.clone())
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_frames(
    stdout: &mut impl Read,
    tx: SyncSender<Frame>,
    spare: Receiver<Vec<u8>>,
    frame_len: usize,
    base: f64,
    fps: f64,
) {
    for n in 0u64.. {
        let mut data = spare.try_recv().unwrap_or_else(|_| vec![0; frame_len]);
        data.resize(frame_len, 0);
        if stdout.read_exact(&mut data).is_err() {
            return;
        }
        let frame = Frame { pts: base + n as f64 / fps, data };
        if tx.send(frame).is_err() {
            return; // the player moved on
        }
    }
}
