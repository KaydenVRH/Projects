//! File‑tree explorer for ked (invoked with Ctrl+F).
//!
//! Walks the current directory and builds a flat list of [`TreeEntry`]
//! with depth information so the renderer can draw an indented tree.
//! Hidden files are shown (only `.git` is skipped).

use std::fs;
use std::path::Path;

/// A single entry in the flattened file‑tree list.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

/// The file‑tree state: cached entries, cursor position, scroll offset.
pub struct FileTree {
    pub entries: Vec<TreeEntry>,
    pub selected: usize,
    pub scroll: usize,
}

/// Return a Nerd Font icon character for a file or directory.
pub fn file_icon(path: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "\u{f07b}"; //  folder
    }
    match path.rsplit('.').next().unwrap_or("") {
        "py"   => "\u{e606}", //  python
        "rs"   => "\u{e7a8}", //  rust
        "js"   => "\u{e781}", //  javascript
        "ts"   => "\u{e628}", //  typescript
        "jsx"  => "\u{e7ba}", //  react
        "tsx"  => "\u{e7ba}",
        "c"    => "\u{e61e}", //  c
        "h"    => "\u{e61e}",
        "cpp"  => "\u{e61d}", //  c++
        "hpp"  => "\u{e61d}",
        "html" => "\u{e736}", //  html
        "css"  => "\u{e749}", //  css
        "json" => "\u{e60b}", //  json
        "md"   => "\u{e73b}", //  markdown
        "toml" => "\u{e615}", //  toml/config
        "yml"  => "\u{e615}",
        "yaml" => "\u{e615}",
        "sh"   => "\u{e68d}", // terminal
        _      => "\u{f15b}", //  default file
    }
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
        }
    }

    /// Walk the current working directory and rebuild the tree.
    pub fn refresh(&mut self) {
        self.entries.clear();
        let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        let root_name = cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| cwd.to_string_lossy().to_string());
        self.entries.push(TreeEntry {
            name: root_name,
            path: cwd.to_string_lossy().to_string(),
            is_dir: true,
            depth: 0,
            expanded: true,
        });
        let root_path = self.entries[0].path.clone();
        let children = collect_children(&root_path, 1);
        self.entries.splice(1..1, children);
        self.selected = 0;
        self.scroll = 0;
    }

    pub fn selected_entry(&self) -> Option<&TreeEntry> {
        self.entries.get(self.selected)
    }

    /// Toggle expand/collapse for the selected entry.
    /// Returns true if the tree was modified.
    pub fn toggle_expand(&mut self) -> bool {
        let idx = self.selected;
        let entry = match self.entries.get(idx) {
            Some(e) if e.is_dir => e.clone(),
            _ => return false,
        };
        let next = idx + 1;

        if entry.expanded {
            let mut remove_end = next;
            while remove_end < self.entries.len() && self.entries[remove_end].depth > entry.depth {
                remove_end += 1;
            }
            self.entries.splice(next..remove_end, std::iter::empty());
            self.entries[idx].expanded = false;
        } else {
            let children = collect_children(&entry.path, entry.depth + 1);
            self.entries.splice(next..next, children);
            self.entries[idx].expanded = true;
        }
        true
    }
}

/// Collect child entries of a directory, sorted dirs-first then alphabetical.
fn collect_children(dir: &str, depth: usize) -> Vec<TreeEntry> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut items: Vec<TreeEntry> = entries
        .flatten()
        .filter(|e| e.file_name() != ".git")
        .map(|e| {
            let path = e.path();
            TreeEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                is_dir: path.is_dir(),
                depth,
                expanded: false,
            }
        })
        .collect();
    items.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.cmp(&b.name)
        }
    });
    // Cap depth at 5 to avoid huge trees.
    if depth > 5 && !items.is_empty() {
        items.truncate(1);
        items[0].name = "...".to_string();
        items[0].is_dir = true;
    }
    items
}
