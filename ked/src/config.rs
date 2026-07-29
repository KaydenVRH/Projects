use std::path::PathBuf;

use serde::Deserialize;

/// User configuration loaded from `~/.config/ked/config.toml`.
#[derive(Deserialize, Default)]
pub struct Config {
    /// Theme name (default, monokai, solarized, nord, gruvbox, bi,
    /// catppuccin, tokyonight, amber, dracula, onedark, everforest,
    /// rosepine, oxocarbon, system, opencode, ayu, kanagawa).
    #[serde(default)]
    pub theme: String,

    /// Directory to scan for music files (Ctrl+T).
    #[serde(default)]
    pub music_dir: String,

    /// Width of the file tree panel in columns (default 30).
    #[serde(default = "default_filetree_width")]
    pub filetree_width: u16,

    /// Put the status bar at the top instead of the bottom.
    #[serde(default)]
    pub status_bar_top: bool,

    /// Make the content background transparent (terminal default).
    #[serde(default)]
    pub transparent: bool,

    /// Enable/disable visual animations (tab shimmer, rainbow dashes,
    /// splash logo cycling, spinner).  Defaults to true.
    #[serde(default = "default_animations")]
    pub animations: bool,

    /// Show system stats (CPU/MEM/BATT/time) in the buffer bar.
    /// Defaults to true.
    #[serde(default = "default_true")]
    pub bar_stats: bool,

    /// Set the colour fx mode at startup.
    /// 0 = off, 1 = gentle, 2 = breathing, 3 = warm.
    #[serde(default)]
    pub fx_mode: u8,

    /// UI element opacity (0.0 = transparent, 1.0 = solid).
    /// Affects status bar, buffer bar, and popup backgrounds.
    /// Defaults to 1.0 (fully opaque).
    #[serde(default = "default_opacity")]
    pub opacity: f32,

    /// Number of characters to scroll horizontally with Option+Left/Right.
    #[serde(default = "default_alt_scroll")]
    pub alt_scroll: usize,
}

const fn default_filetree_width() -> u16 {
    30
}

const fn default_animations() -> bool {
    true
}

const fn default_true() -> bool {
    true
}

const fn default_opacity() -> f32 {
    1.0
}

const fn default_alt_scroll() -> usize {
    5
}

impl Config {
    /// Load config from `~/.config/ked/config.toml`.
    /// Returns a default config if the file doesn't exist or is unreadable.
    pub fn load() -> Self {
        let path = config_path();
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Config::default(),
        };
        match toml::from_str(&contents) {
            Ok(cfg) => cfg,
            Err(e) => {
                // Silently fall back to default on parse error.
                eprintln!("ked: ignoring bad config: {e}");
                Config::default()
            }
        }
    }
}

/// `~/.config/ked/config.toml`
fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".into());
    let mut p = PathBuf::from(home);
    p.push(".config");
    p.push("ked");
    p.push("config.toml");
    p
}
