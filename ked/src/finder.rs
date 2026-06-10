//! Fuzzy file finder for ked (invoked with Ctrl+P).
//!
//! Walks the directory tree (up to a configurable depth, skipping
//! `.git`) and collects relative file paths.  A search query is
//! matched character-by-character against each path; results are
//! sorted by a simple score that favours matches at path-component
//! boundaries (after `/`, `_`, `-`) and consecutive runs.
//!
//! The [`Finder`] struct lives inside the editor and is reused
//! across invocations so the file list is cached.

use std::path::Path;
use std::fs;

/// Holds the file list and the current search results.
pub struct Finder {
    /// All files discovered during the last directory walk.
    pub files: Vec<String>,
    /// Filtered & scored results for the current query.
    pub results: Vec<(String, i32)>,
}

impl Finder {
    /// Create a new empty finder.
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            results: Vec::new(),
        }
    }

    /// Walk the current directory tree and cache the file list.
    ///
    /// Skips:
    ///   - The `.git` directory
    ///   - Hidden files (starting with `.`)
    ///   - Directories deeper than 6 levels
    pub fn collect_files(&mut self) {
        self.files.clear();
        let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        walk(&cwd, &cwd, &mut self.files, 0);
        self.files.sort();
        // Re-apply any active query.
        let query = std::mem::take(&mut self.results);
        drop(query);
    }

    /// Run the current query against the cached file list.
    ///
    /// Populates `self.results` with scored (path, score) pairs
    /// sorted by score (lower = better match).
    pub fn search(&mut self, query: &str) {
        self.results.clear();
        if query.is_empty() {
            // Show the first ~20 files as a simple list.
            for f in self.files.iter().take(20) {
                self.results.push((f.clone(), 0));
            }
            return;
        }
        for f in &self.files {
            if let Some(score) = fuzzy_score(query, f) {
                self.results.push((f.clone(), score));
            }
        }
        self.results.sort_by(|a, b| a.1.cmp(&b.1));
        // Keep top 50 results.
        self.results.truncate(50);
    }
}

/// Recursively walk a directory, collecting relative file paths.
fn walk(root: &Path, dir: &Path, files: &mut Vec<String>, depth: usize) {
    if depth > 6 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        // Skip hidden files / dirs and .git
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, files, depth + 1);
        } else if path.is_file() {
            // Store relative path from root.
            if let Ok(rel) = path.strip_prefix(root) {
                files.push(rel.to_string_lossy().to_string());
            }
        }
    }
}

/// Score a query against a text (lower = better match).
///
/// Returns `None` if the query characters don't all appear in order.
/// Otherwise returns a score where:
///   - Matching after `/`, `_`, `-` gives a -10 bonus
///   - Consecutive character matches give a -5 bonus
fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let t: Vec<char> = text.chars().flat_map(|c| c.to_lowercase()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let mut qi = 0;
    let mut score = 0i32;
    let mut prev_match = false;
    for (ti, &tc) in t.iter().enumerate() {
        if qi < q.len() && tc == q[qi] {
            // Bonus for matching at start of a path component.
            if ti == 0 || t.get(ti.wrapping_sub(1)).map_or(false, |&c| c == '/' || c == '_' || c == '-' || c == '.') {
                score -= 10;
            }
            // Bonus for consecutive matches.
            if prev_match {
                score -= 5;
            }
            prev_match = true;
            qi += 1;
        } else {
            prev_match = false;
        }
    }
    if qi == q.len() { Some(score) } else { None }
}
