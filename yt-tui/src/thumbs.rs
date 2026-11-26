//! Thumbnails, fetched and decoded by ffmpeg on a background thread.
//!
//! Only the selected video's thumbnail is wanted at any moment, so the
//! worker skips requests that went stale while it was busy — holding
//! j through fifty results costs one download, not fifty.

use crate::youtube::thumbnail_urls;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 360;

enum Done {
    Loaded(String, Arc<Vec<u8>>),
    Failed(String),
    Skipped(String),
}

#[derive(Clone)]
enum Entry {
    Pending,
    Ready(Arc<Vec<u8>>),
    Missing,
}

pub struct Thumbs {
    requests: Sender<String>,
    done: Receiver<Done>,
    wanted: Arc<Mutex<String>>,
    cache: HashMap<String, Entry>,
}

impl Thumbs {
    pub fn new() -> Thumbs {
        let (requests, rx) = mpsc::channel::<String>();
        let (tx, done) = mpsc::channel();
        let wanted = Arc::new(Mutex::new(String::new()));
        let w = wanted.clone();
        std::thread::spawn(move || {
            for id in rx {
                if *w.lock().unwrap() != id {
                    let _ = tx.send(Done::Skipped(id));
                    continue;
                }
                let msg = match fetch(&id) {
                    Some(rgb) => Done::Loaded(id, Arc::new(rgb)),
                    None => Done::Failed(id),
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
        });
        Thumbs { requests, done, wanted, cache: HashMap::new() }
    }

    /// The thumbnail for `id` if it has arrived; asks for it otherwise.
    pub fn get(&mut self, id: &str) -> Option<Arc<Vec<u8>>> {
        self.collect();
        *self.wanted.lock().unwrap() = id.to_string();
        match self.cache.get(id) {
            Some(Entry::Ready(rgb)) => Some(rgb.clone()),
            Some(Entry::Pending) | Some(Entry::Missing) => None,
            None => {
                self.cache.insert(id.to_string(), Entry::Pending);
                let _ = self.requests.send(id.to_string());
                None
            }
        }
    }

    /// True while a fetch is in flight (for a spinner).
    pub fn loading(&self, id: &str) -> bool {
        matches!(self.cache.get(id), Some(Entry::Pending))
    }

    fn collect(&mut self) {
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Loaded(id, rgb) => {
                    self.cache.insert(id, Entry::Ready(rgb));
                }
                Done::Failed(id) => {
                    self.cache.insert(id, Entry::Missing);
                }
                // Forget it, so coming back to it asks again.
                Done::Skipped(id) => {
                    self.cache.remove(&id);
                }
            }
        }
        // ~700 KB each; keep memory bounded on long sessions.
        if self.cache.len() > 200 {
            self.cache.retain(|_, e| !matches!(e, Entry::Ready(_)));
        }
    }
}

fn fetch(id: &str) -> Option<Vec<u8>> {
    let len = (WIDTH * HEIGHT * 3) as usize;
    for url in thumbnail_urls(id) {
        let child = Command::new("ffmpeg")
            .args(["-nostdin", "-hide_banner", "-loglevel", "quiet", "-i", &url, "-frames:v", "1"])
            .arg("-vf")
            .arg(format!(
                "scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=increase,crop={WIDTH}:{HEIGHT}"
            ))
            .args(["-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else { return None };
        let mut buf = vec![0u8; len];
        let ok = child.stdout.take().is_some_and(|mut out| out.read_exact(&mut buf).is_ok());
        let _ = child.wait();
        if ok {
            return Some(buf);
        }
    }
    None
}
