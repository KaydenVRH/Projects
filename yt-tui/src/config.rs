//! `~/.config/yt-tui/config`: plain `key = value` lines, `#` comments.
//!
//! ```text
//! cookies_from_browser = firefox
//! fps = 30
//! quality = 1080
//! max_width = 1920
//! results = 30
//! volume = 80
//! theme = auto      # neon, stark, arcane, or auto: follow kshell
//! ```
//!
//! Command-line options override the file; `YT_TUI_COOKIES_FROM_BROWSER`
//! overrides `cookies_from_browser`.

use std::path::PathBuf;

#[derive(Debug, Default, PartialEq)]
pub struct Config {
    pub cookies_from_browser: Option<String>,
    pub fps: Option<u32>,
    pub quality: Option<u32>,
    pub max_width: Option<u32>,
    pub results: Option<u32>,
    pub volume: Option<f64>,
    /// `neon`, `stark`, `arcane`; anything else (or nothing) follows kshell.
    pub theme: Option<String>,
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("yt-tui").join("config"))
}

pub fn load() -> Config {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

pub fn parse(text: &str) -> Config {
    let mut c = Config::default();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "cookies_from_browser" if !value.is_empty() => c.cookies_from_browser = Some(value.to_string()),
            "fps" => c.fps = value.parse().ok(),
            "quality" => c.quality = value.parse().ok(),
            "max_width" => c.max_width = value.parse().ok(),
            "results" => c.results = value.parse().ok(),
            "volume" => c.volume = value.parse().ok(),
            "theme" if !value.is_empty() => c.theme = Some(value.to_string()),
            _ => {}
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_keys_and_ignores_the_rest() {
        let c = parse(
            "# my settings\ncookies_from_browser = \"firefox\"  # login\nfps=60\nvolume = 55.5\nbogus = 1\nresults = lots\n",
        );
        assert_eq!(c.cookies_from_browser.as_deref(), Some("firefox"));
        assert_eq!(c.fps, Some(60));
        assert_eq!(c.volume, Some(55.5));
        assert_eq!(c.results, None);
        assert_eq!(parse("theme = stark").theme.as_deref(), Some("stark"));
    }
}
