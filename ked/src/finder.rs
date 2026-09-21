//! Fuzzy file finder for ked (invoked with Ctrl+P).
//!
//! The directory tree is walked on a **background thread** so a large
//! tree (say, your home folder) never blocks the UI; the editor polls
//! for the result once per frame.  Common heavy directories
//! (`node_modules`, `Library`, `target`, …) are skipped.
//!
//! A search query is matched character-by-character against each path;
//! results are scored so that matches at path-component boundaries,
//! consecutive runs, and matches in the file name itself rank first.
//!
//! The [`Finder`] struct lives inside the editor and is reused across
//! invocations so the file list is cached.

use std::path::Path;
use std::fs;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

/// Directories that are skipped while scanning — large, generated, or
/// simply never what you're looking for.
const IGNORED_DIRS: &[&str] = &[
    "node_modules", "target", "Library", ".cache", ".local", ".cargo",
    ".rustup", ".npm", ".Trash", "venv", ".venv", "__pycache__",
    "dist", "build", ".next", ".gradle", ".m2", "site-packages",
    "Pods", ".terraform", ".tox", ".idea", ".vscode",
];

/// Stop after this many files so a giant tree can't eat memory.
const MAX_FILES: usize = 50_000;
/// How deep to descend.
const MAX_DEPTH: usize = 8;
/// How many results to keep.
const MAX_RESULTS: usize = 50;

/// Holds the file list and the current search results.
pub struct Finder {
    /// All files discovered during the last directory walk.
    pub files: Vec<String>,
    /// Lowercased copies of `files`, precomputed for scoring.
    lower: Vec<String>,
    /// Filtered & scored results for the current query.
    pub results: Vec<(String, i32)>,
    /// Is a background scan still running?
    pub scanning: bool,
    rx: Option<Receiver<Vec<String>>>,
}

impl Finder {
    /// Create a new empty finder.
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            lower: Vec::new(),
            results: Vec::new(),
            scanning: false,
            rx: None,
        }
    }

    /// Start walking the current directory tree in the background.
    /// Call [`Finder::poll`] each frame to collect the result.
    pub fn start_scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        self.files.clear();
        self.lower.clear();
        self.results.clear();

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let cwd =
                std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
            let mut files = Vec::new();
            walk(&cwd, &cwd, &mut files, 0);
            files.sort();
            let _ = tx.send(files);
        });
    }

    /// Collect the background scan's result when it finishes.
    /// Returns `true` when the file list was just updated (so the
    /// caller can re-run the current query).  Cheap to call every
    /// frame.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        match rx.try_recv() {
            Ok(files) => {
                self.lower = files.iter().map(|f| f.to_lowercase()).collect();
                self.files = files;
                self.scanning = false;
                self.rx = None;
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.scanning = false;
                self.rx = None;
                true
            }
            Err(TryRecvError::Empty) => false,
        }
    }

    /// Run the current query against the cached file list.
    ///
    /// Populates `self.results` with scored (path, score) pairs
    /// sorted by score (lower = better match).
    pub fn search(&mut self, query: &str) {
        self.results.clear();
        if query.is_empty() {
            // Show the first files as a simple list.
            for f in self.files.iter().take(MAX_RESULTS) {
                self.results.push((f.clone(), 0));
            }
            return;
        }
        // Lowercase the query once, not once per file.
        let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
        let mut scored: Vec<(String, i32)> = Vec::new();
        for (f, lower) in self.files.iter().zip(self.lower.iter()) {
            if let Some(score) = fuzzy_score(&q, lower) {
                scored.push((f.clone(), score));
            }
        }
        scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.len().cmp(&b.0.len())));
        scored.truncate(MAX_RESULTS);
        self.results = scored;
    }
}

/// Recursively walk a directory, collecting relative file paths.
fn walk(root: &Path, dir: &Path, files: &mut Vec<String>, depth: usize) {
    if depth > MAX_DEPTH || files.len() >= MAX_FILES {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Skip hidden entries and known-heavy directories.
        if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_ref()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, files, depth + 1);
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                files.push(rel.to_string_lossy().to_string());
            }
        }
    }
}

/// Score a query against an already-lowercased text (lower = better).
///
/// Returns `None` if the query characters don't all appear in order.
/// Scoring:
///   - matching after `/`, `_`, `-`, `.` gives a -10 bonus
///   - consecutive character matches give a -5 bonus
///   - a match in the file name (not the directories) gives -10
///   - each leading directory costs a little (+2) so shallower wins
fn fuzzy_score(q: &[char], text: &str) -> Option<i32> {
    if q.is_empty() {
        return Some(0);
    }
    // All offsets here are BYTE offsets (char_indices), never char
    // counts — slicing a str by a char count panics on multi-byte
    // paths (`é`, emoji, macOS NFD combining marks, …).
    let bytes = text.as_bytes();
    let base_start = text.rfind('/').map(|i| i + 1).unwrap_or(0);
    let mut qi = 0;
    let mut score = 0i32;
    let mut prev_match = false;
    let mut first_match: Option<usize> = None;
    for (bi, tc) in text.char_indices() {
        if qi < q.len() && tc == q[qi] {
            if first_match.is_none() {
                first_match = Some(bi);
            }
            // Continuation bytes are >= 0x80, so comparing the byte
            // before a match to ASCII separators is safe.
            if bi == 0
                || matches!(
                    bytes.get(bi.wrapping_sub(1)),
                    Some(b'/') | Some(b'_') | Some(b'-') | Some(b'.')
                )
            {
                score -= 10;
            }
            if prev_match {
                score -= 5;
            }
            prev_match = true;
            qi += 1;
        } else {
            prev_match = false;
        }
    }
    if qi != q.len() {
        return None;
    }
    let first = first_match.unwrap_or(0);
    if first >= base_start {
        score -= 10; // matched the file name itself
    }
    // Mildly prefer files closer to the root.
    score += (text[..first].matches('/').count() as i32) * 2;
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str) -> Vec<char> {
        s.chars().flat_map(|c| c.to_lowercase()).collect()
    }

    /// Regression: `first_match` used to be a char count sliced as a
    /// byte offset, panicking on any multi-byte path.
    #[test]
    fn fuzzy_score_handles_multibyte_paths() {
        for (text, query) in [
            ("éa.txt", "a"),
            ("sub/ünïcode_p.txt", "p"),
            ("İstanbul/i.txt", "i"),
            ("café/notes.md", "n"),
            ("e\u{301}clair/p.rs", "p"),
            ("日本語/ファイル.txt", "イ"),
            ("🐍/snake.py", "s"),
        ] {
            let score = fuzzy_score(&q(query), &text.to_lowercase());
            assert!(score.is_some(), "expected {query:?} to match {text:?}");
        }
    }

    #[test]
    fn fuzzy_score_rejects_out_of_order() {
        assert!(fuzzy_score(&q("zzz"), &"abc".to_string()).is_none());
        assert!(fuzzy_score(&q("ba"), &"abc".to_string()).is_none());
    }

    #[test]
    fn fuzzy_score_prefers_file_name_and_boundaries() {
        let name = fuzzy_score(&q("main"), "src/main.rs").unwrap();
        let deep = fuzzy_score(&q("main"), "a/b/c/mxaxixn.rs").unwrap();
        assert!(name < deep, "file-name match should rank better");
    }

    #[test]
    fn search_finds_unicode_files() {
        let mut f = Finder::new();
        f.files = vec!["éa.txt".into(), "sub/ünïcode_p.txt".into(), "b.rs".into()];
        f.lower = f.files.iter().map(|s| s.to_lowercase()).collect();
        f.search("p");
        assert_eq!(f.results.len(), 1);
        assert_eq!(f.results[0].0, "sub/ünïcode_p.txt");
    }
}
