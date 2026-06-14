use std::path::PathBuf;

use serde::Deserialize;

/// User configuration loaded from `~/.config/ked/config.toml`.
#[derive(Deserialize, Default)]
pub struct Config {
    /// Theme name (default, monokai, solarized, nord, gruvbox, bi,
    /// catppuccin, tokyonight, amber).
    #[serde(default)]
    pub theme: String,

    /// Directory to scan for music files (Ctrl+T).
    #[serde(default)]
    pub music_dir: String,

    /// Width of the file tree panel in columns (default 30).
    #[serde(default = "default_filetree_width")]
    pub filetree_width: u16,
}

const fn default_filetree_width() -> u16 {
    30
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
