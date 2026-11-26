//! Audio through mpv, which is also the clock the video follows.
//!
//! mpv runs headless with a JSON IPC socket. We observe the properties we
//! show (position, pause, volume…) and send it commands for the controls.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone)]
pub struct State {
    /// Last reported position, and when it was reported.
    pub pos: Option<(f64, Instant)>,
    pub paused: bool,
    pub buffering: bool,
    pub duration: Option<f64>,
    pub volume: f64,
    pub muted: bool,
    pub ended: bool,
}

pub struct Audio {
    child: Child,
    socket: UnixStream,
    path: PathBuf,
    state: Arc<Mutex<State>>,
}

impl Audio {
    pub fn spawn(url: &str, user_agent: &str, start: f64, volume: f64) -> Result<Self> {
        let path = std::env::temp_dir().join(format!("yt-tui-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut cmd = Command::new("mpv");
        cmd.args(["--no-video", "--no-terminal", "--idle=no", "--no-config"])
            .arg(format!("--input-ipc-server={}", path.display()))
            .arg(format!("--user-agent={user_agent}"))
            .arg(format!("--volume={volume}"))
            .arg("--cache=yes")
            .arg("--demuxer-max-bytes=64MiB");
        if start > 0.0 {
            cmd.arg(format!("--start={start:.3}"));
        }
        let mut child = cmd
            .arg("--")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("couldn't run mpv — is it installed?")?;

        let deadline = Instant::now() + Duration::from_secs(5);
        let socket = loop {
            if let Ok(s) = UnixStream::connect(&path) {
                break s;
            }
            if let Ok(Some(status)) = child.try_wait() {
                bail!("mpv exited ({status}) before it started playing");
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                bail!("mpv didn't open its control socket");
            }
            std::thread::sleep(Duration::from_millis(20));
        };

        let state = Arc::new(Mutex::new(State { volume, ..State::default() }));
        let reader = BufReader::new(socket.try_clone()?);
        let st = state.clone();
        std::thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(msg) = serde_json::from_str::<Value>(&line) {
                    apply(&mut st.lock().unwrap(), &msg);
                }
            }
            st.lock().unwrap().ended = true;
        });

        let audio = Audio { child, socket, path, state };
        for (i, prop) in ["time-pos", "pause", "duration", "volume", "mute", "paused-for-cache"]
            .iter()
            .enumerate()
        {
            audio.send(json!(["observe_property", i + 1, prop]));
        }
        Ok(audio)
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    /// Current playback position, advanced smoothly between mpv's reports.
    pub fn clock(&self) -> Option<f64> {
        let st = self.state.lock().unwrap();
        let (pos, at) = st.pos?;
        if st.paused || st.buffering {
            Some(pos)
        } else {
            // mpv reports often; never extrapolate across a long stall.
            Some(pos + at.elapsed().as_secs_f64().min(0.25))
        }
    }

    pub fn set_pause(&self, paused: bool) {
        self.send(json!(["set_property", "pause", paused]));
    }

    pub fn seek(&self, to: f64) {
        // The old position is stale until mpv reports from the new one.
        self.state.lock().unwrap().pos = None;
        self.send(json!(["seek", to, "absolute+exact"]));
    }

    pub fn add_volume(&self, delta: f64) {
        self.send(json!(["add", "volume", delta]));
    }

    pub fn toggle_mute(&self) {
        self.send(json!(["cycle", "mute"]));
    }

    fn send(&self, command: Value) {
        let mut line = json!({ "command": command }).to_string();
        line.push('\n');
        let _ = (&self.socket).write_all(line.as_bytes());
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.path);
    }
}

fn apply(st: &mut State, msg: &Value) {
    match msg["event"].as_str() {
        Some("property-change") => {}
        Some("end-file") => {
            st.ended = true;
            return;
        }
        _ => return,
    }
    let data = &msg["data"];
    match msg["name"].as_str() {
        Some("time-pos") => st.pos = data.as_f64().map(|p| (p, Instant::now())),
        Some("pause") => st.paused = data.as_bool().unwrap_or(false),
        Some("paused-for-cache") => st.buffering = data.as_bool().unwrap_or(false),
        Some("duration") => st.duration = data.as_f64(),
        Some("volume") => st.volume = data.as_f64().unwrap_or(st.volume),
        Some("mute") => st.muted = data.as_bool().unwrap_or(false),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_changes_update_state() {
        let mut st = State::default();
        apply(&mut st, &json!({"event":"property-change","id":1,"name":"time-pos","data":12.5}));
        apply(&mut st, &json!({"event":"property-change","id":2,"name":"pause","data":true}));
        apply(&mut st, &json!({"event":"property-change","id":4,"name":"volume","data":80.0}));
        assert_eq!(st.pos.map(|(p, _)| p), Some(12.5));
        assert!(st.paused);
        assert_eq!(st.volume, 80.0);
        apply(&mut st, &json!({"event":"end-file","reason":"eof"}));
        assert!(st.ended);
    }

    #[test]
    fn command_replies_are_ignored() {
        let mut st = State::default();
        apply(&mut st, &json!({"request_id":0,"error":"success","data":null}));
        assert!(st.pos.is_none() && !st.ended);
    }
}
