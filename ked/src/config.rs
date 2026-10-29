use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

/// User configuration loaded from `~/.config/ked/config.toml`.
#[derive(Deserialize, Default)]
pub struct Config {
    /// Theme name (default, mono, monokai, solarized, nord, gruvbox,
    /// bi, blue-lagoon, catppuccin, tokyonight, amber, dracula, onedark,
    /// everforest, rosepine, oxocarbon, ayu, kanagawa, palenight,
    /// darkplus, moonlight, osaka-jade, last-horizon, latte, gruvbox-light, one-light,
    /// github-light, tokyonight-day, rosepine-dawn, everforest-light).
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

    /// Show the info bar on the right of the buffer bar: language,
    /// buffer position, line count, now-playing music, and clock.
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

    /// Render `![alt](path)` images inline in markdown buffers (kitty
    /// graphics protocol — kitty, ghostty, wezterm, foot, …).
    /// Defaults to true.
    #[serde(default = "default_inline_images")]
    pub inline_images: bool,

    /// Custom key bindings: action name → key spec (e.g. `save = "ctrl+s"`).
    /// Every action defaults to ked's built-in key; setting one replaces
    /// the built-in binding(s).  Run `:keys` for the full action list.
    #[serde(default)]
    pub keys: HashMap<String, String>,

    /// Remembered things (notes, reminders, journal).
    #[serde(default)]
    pub memories: MemoryConfig,
}

/// `[memories]` — reminders / notes / journal.
#[derive(Deserialize, Clone, Debug)]
pub struct MemoryConfig {
    /// Directory for the markdown entries (default: XDG data dir).
    #[serde(default)]
    pub dir: String,
    /// Pop a macOS notification (and optionally a sound) when a
    /// reminder comes due.
    #[serde(default = "default_true")]
    pub notify: bool,
    /// Play a sound alongside the notification.
    #[serde(default = "default_true")]
    pub sound: bool,
    /// Quiet window (e.g. `"22:00-08:00"`) — no alerts inside it.
    #[serde(default)]
    pub quiet_hours: String,
    /// First day of the week in the planner: `"monday"` (default) or
    /// `"sunday"`.
    #[serde(default = "default_week_start")]
    pub week_start: String,
}

fn default_week_start() -> String {
    "monday".to_string()
}

impl Default for MemoryConfig {
    fn default() -> Self {
        MemoryConfig {
            dir: String::new(),
            notify: true,
            sound: true,
            quiet_hours: String::new(),
            week_start: default_week_start(),
        }
    }
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

const fn default_inline_images() -> bool {
    true
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

/// A fully commented example config generated from the current
/// defaults.  Printed by `ked --print-config` and mirrored in
/// `config.example.toml`.
pub fn example() -> String {
    let mut s = String::new();
    s.push_str(
        "# ked — example configuration\n\
         #\n\
         # Copy to ~/.config/ked/config.toml.  Every key is optional:\n\
         # anything omitted keeps the default shown here.  The active\n\
         # values below are the defaults, so this file changes nothing\n\
         # until you edit something.\n\
         #\n\
         # Re-generate with:  ked --print-config > ~/.config/ked/config.toml\n\
         # Live keymap:       ked --print-keys   (or `:keys` in ked)\n\n",
    );
    s.push_str(
        "# ── appearance ──────────────────────────────────────────\n\
         # theme: default, mono, monokai, solarized, nord, gruvbox, bi,\n\
         # blue-lagoon, catppuccin, tokyonight, amber, dracula, onedark,\n\
         # everforest, rosepine, oxocarbon, ayu, kanagawa, palenight,\n\
         # darkplus, moonlight, osaka-jade, last-horizon, latte, gruvbox-light, one-light,\n\
         # github-light, tokyonight-day, rosepine-dawn, everforest-light\n\
         theme          = \"oxocarbon\"\n\
         transparent    = false      # let the terminal background show through\n\
         animations     = true       # tab shimmer, rainbow dashes, splash\n\
         bar_stats      = true       # language/buffers/clock context bar\n\
         status_bar_top = false      # status bar above the buffer bar\n\
         fx_mode        = 0          # 0 off, 1 gentle, 2 breathing, 3 warm\n\
         opacity        = 1.0        # UI chrome opacity, 0.0–1.0\n\
         alt_scroll     = 5          # cells per Alt/Cmd+arrow jump\n\n",
    );
    s.push_str(
        "# ── buffers & tools ─────────────────────────────────────\n\
         filetree_width = 30         # file-tree panel width in columns\n\
         music_dir      = \"~/Music\"  # where Ctrl+M scans for .mp3 files\n\
         inline_images  = true       # render ![alt](path) in .md buffers\n\n",
    );
    s.push_str(
        "# ── memories / reminders / journal ──────────────────────\n\
         [memories]\n\
         # dir = \"~/notes/memories\"  # default: $XDG_DATA_HOME/ked/memories\n\
         notify      = true          # macOS notification when a reminder is due\n\
         sound       = true          # play a sound alongside it\n\
         week_start  = \"monday\"      # planner: \"monday\" or \"sunday\"\n\
         # quiet_hours = \"22:00-08:00\"  # no alerts in this window (empty = never)\n\n",
    );
    s.push_str(&crate::keymap::template());
    s.push_str(
        "\n# ── tips ─────────────────────────────────────────────────\n\
         # • `:theme <name>` switches without editing this file.\n\
         # • `:mem call mom @tomorrow 9am #family !2` captures a reminder.\n\
         # • `ked --install-agent` makes reminders fire while ked is closed.\n",
    );
    s
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_is_valid_and_has_no_active_overrides() {
        let text = example();
        let cfg: Config = toml::from_str(&text).expect("example config must parse");
        // All [keys] lines are commented out, so the live map is unchanged.
        assert!(cfg.keys.is_empty());
        assert_eq!(cfg.theme, "oxocarbon");
        assert!(cfg.memories.notify);
        // Every documented option is present.
        for key in [
            "theme",
            "music_dir",
            "filetree_width",
            "status_bar_top",
            "transparent",
            "animations",
            "bar_stats",
            "fx_mode",
            "opacity",
            "alt_scroll",
            "inline_images",
            "memories",
            "keys",
            "week_start",
        ] {
            assert!(text.contains(key), "example missing `{key}`");
        }
    }
}
