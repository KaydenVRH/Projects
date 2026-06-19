//! Inline music player for ked.
//!
//! Invoked with Ctrl+M.  Scans the working directory for `*.mp3`
//! files and shows them in a picker overlay (same style as the
//! fuzzy file finder).  Selecting a file starts playing it and
//! queues the rest of the directory as a playlist — the next
//! file starts automatically when the current one finishes.
//!
//! Playback uses macOS's built-in `afplay` command so no extra
//! Cargo dependencies are needed.  A background thread manages
//! the `afplay` process and sends events back to the editor via
//! a channel so the UI stays responsive.

use std::{
    fs,
    path::Path,
    process::{Child, Command},
    sync::mpsc,
    thread,
    time::Duration,
};

// ═══════════════════════════════════════════════════════════════════
//  Platform-specific player backend
// ═══════════════════════════════════════════════════════════════════

/// Build the command to play an audio file.
fn player_cmd(path: &str) -> Command {
    #[cfg(target_os = "macos")]
    { let mut c = Command::new("afplay"); c.arg(path); c }

    #[cfg(target_os = "linux")]
    { let mut c = Command::new("mpv"); c.args(["--no-video", "--really-quiet", "--no-terminal", path]); c }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    { let mut c = Command::new("echo"); c.arg("no audio player"); c }
}

/// Kill orphaned players from a previous session.
fn kill_orphans() {
    #[cfg(target_os = "macos")]
    { let _ = Command::new("pkill").arg("-x").arg("afplay").output(); }

    #[cfg(target_os = "linux")]
    { let _ = Command::new("pkill").arg("-x").arg("mpv").output(); }
}

/// Commands sent FROM the editor TO the audio thread.
pub enum PlayerCmd {
    /// Start playing `path` (absolute).
    Play(String),
    /// Stop whatever is currently playing.
    Stop,
}

/// Events sent FROM the audio thread TO the editor.
#[derive(Debug)]
pub enum PlayerEvent {
    /// A track has begun playback.
    Started(String),
    /// The current track ended (either naturally or because we
    /// stopped it).
    Ended,
}

// ═══════════════════════════════════════════════════════════════════
//  MusicPlayer  (editor-side handle)
// ═══════════════════════════════════════════════════════════════════

/// Playlist state and the channel endpoints for talking to the
/// background audio thread.
pub struct MusicPlayer {
    /// All `*.mp3` paths found during the last scan (absolute).
    pub files: Vec<String>,
    /// Index in `files` that the cursor is on in the picker UI.
    pub selected: usize,

    /// Is something currently playing?
    pub playing: bool,
    /// Absolute path of the current track (if any).
    pub current_song: Option<String>,
    /// Index of the current track within `files`.
    pub current_index: usize,
    /// The full ordered playlist (same as `files` while playing).
    pub playlist: Vec<String>,

    /// The directory we last scanned (for potential re-scan).
    pub scan_dir: Option<String>,

    // channels
    pub cmd_tx:  mpsc::Sender<PlayerCmd>,
    pub event_rx: mpsc::Receiver<PlayerEvent>,
}

impl MusicPlayer {
    /// Create a fresh player, kill any orphaned `afplay` from a
    /// previous ked session, and spawn the background audio thread.
    pub fn new() -> Self {
        kill_orphans();

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        // ── background audio thread ──────────────────────────────
        // This thread loops every ~100 ms, checks for incoming
        // commands, and monitors the afplay child process.
        thread::spawn(move || {
            let mut child: Option<Child> = None;

            loop {
                // Drain any pending commands from the editor.
                while let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        PlayerCmd::Play(path) => {
                            // Kill any existing playback.
                            if let Some(mut c) = child.take() {
                                let _ = c.kill();
                                let _ = c.wait();
                            }
                            // Start afplay (shipped with macOS).
                            match player_cmd(&path).spawn() {
                                Ok(c) => {
                                    let _ = event_tx.send(PlayerEvent::Started(path));
                                    child = Some(c);
                                }
                                Err(_e) => {
                                    let _ = event_tx
                                        .send(PlayerEvent::Ended);
                                }
                            }
                        }
                        PlayerCmd::Stop => {
                            if let Some(mut c) = child.take() {
                                let _ = c.kill();
                                let _ = c.wait();
                            }
                            let _ = event_tx.send(PlayerEvent::Ended);
                        }
                    }
                }

                // Check if the current afplay process has exited.
                if let Some(ref mut c) = child {
                    match c.try_wait() {
                        Ok(Some(_)) => {
                            // Process finished naturally or was killed.
                            child = None;
                            let _ = event_tx.send(PlayerEvent::Ended);
                        }
                        _ => {}
                    }
                }

                thread::sleep(Duration::from_millis(100));
            }
        });

        MusicPlayer {
            files: Vec::new(),
            selected: 0,
            playing: false,
            current_song: None,
            current_index: 0,
            playlist: Vec::new(),
            scan_dir: None,
            cmd_tx,
            event_rx,
        }
    }

    /// Scan a directory tree for `*.mp3` files (sorted).
    ///
    /// Walks `dir` recursively (max depth 4, skips hidden dirs) and
    /// collects every `*.mp3` it finds.
    pub fn scan(&mut self, dir: &str) {
        self.files.clear();
        self.scan_dir = Some(dir.to_string());
        scan_dir_recursive(dir, &mut self.files, 0);
        self.files.sort();

        self.selected = 0;
        // If something is already playing, highlight it.
        if let Some(ref current) = self.current_song {
            if let Ok(canon) = Path::new(current).canonicalize() {
                let cs = canon.to_string_lossy().to_string();
                if let Some(pos) = self.files.iter().position(|f| *f == cs) {
                    self.selected = pos;
                }
            }
        }
    }

    /// Start playing the file at `index` in `self.files`, queuing
    /// the rest as a playlist.
    pub fn play(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        self.playlist = self.files.clone();
        self.current_index = index;
        self.playing = true;
        let path = self.files[index].clone();
        let _ = self.cmd_tx.send(PlayerCmd::Play(path));
    }

    /// Stop playback and reset the now-playing state.
    pub fn stop(&mut self) {
        self.playing = false;
        self.current_song = None;
        self.current_index = 0;
        let _ = self.cmd_tx.send(PlayerCmd::Stop);
    }

    /// Move to the next track in the playlist.  Does nothing if
    /// already at the end.
    pub fn next(&mut self) {
        let next = self.current_index + 1;
        if next < self.playlist.len() {
            self.play(next);
        } else {
            // Playlist exhausted.
            self.playing = false;
            self.current_song = None;
        }
    }

    /// Poll for events from the audio thread and update state.
    /// Should be called once per frame (from the event loop).
    pub fn poll(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                PlayerEvent::Started(path) => {
                    self.playing = true;
                    self.current_song = Some(path);
                }
                PlayerEvent::Ended => {
                    // If we were playing and the thread didn't
                    // get a new Play command yet, auto-advance.
                    if self.playing {
                        self.next();
                    }
                }
            }
        }
    }
}

// ── recursive scan helper ──────────────────────────────────────

/// Recursively walk `dir` (max depth 4, skip hidden dirs) and
/// append every `*.mp3` absolute path to `out`.
fn scan_dir_recursive(dir: &str, out: &mut Vec<String>, depth: usize) {
    if depth > 4 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let fname = entry.file_name();
        // Skip hidden entries.
        if fname.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            scan_dir_recursive(&path.to_string_lossy(), out, depth + 1);
        } else if path.is_file() {
            if path.extension().map(|s| s == "mp3").unwrap_or(false) {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
}
