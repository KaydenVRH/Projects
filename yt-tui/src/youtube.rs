//! Everything that talks to yt-dlp: searching, and turning a video into
//! stream URLs that ffmpeg and mpv can open directly.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;

/// One search result, or the video picked from a pasted URL.
#[derive(Debug, Clone, Deserialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub uploader: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub view_count: Option<u64>,
    #[serde(default)]
    pub live_status: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl Video {
    pub fn channel(&self) -> &str {
        self.channel.as_deref().or(self.uploader.as_deref()).unwrap_or("unknown channel")
    }

    pub fn is_live(&self) -> bool {
        self.live_status.as_deref() == Some("is_live")
    }

    pub fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.id)
    }
}

/// What the player needs to start: direct stream URLs plus the headers
/// YouTube expects on them.
#[derive(Debug, Clone)]
pub struct Stream {
    pub video_url: Option<String>,
    pub audio_url: String,
    pub user_agent: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: Option<f64>,
    pub live: bool,
    pub title: String,
}

/// yt-dlp arguments every call shares. A `player_client` set in the user's
/// yt-dlp config (web_embedded, say) can leave only storyboards available;
/// the default client set is the one yt-dlp keeps working.
/// `YT_TUI_PLAYER_CLIENT` picks another.
fn ytdlp() -> Command {
    let client = std::env::var("YT_TUI_PLAYER_CLIENT").unwrap_or_else(|_| "default".into());
    let mut cmd = Command::new("yt-dlp");
    cmd.args(["--no-warnings", "--no-playlist", "--extractor-args"])
        .arg(format!("youtube:player_client={client}"));
    // "Sign in to confirm you're not a bot": borrow a browser's YouTube
    // login, e.g. YT_TUI_COOKIES_FROM_BROWSER=firefox.
    if let Ok(browser) = std::env::var("YT_TUI_COOKIES_FROM_BROWSER") {
        if !browser.is_empty() {
            cmd.args(["--cookies-from-browser", &browser]);
        }
    }
    // Anything else yt-dlp should get, e.g. "--remote-components ejs:github".
    if let Ok(extra) = std::env::var("YT_TUI_YTDLP_ARGS") {
        cmd.args(extra.split_whitespace());
    }
    cmd
}

/// Whether yt-dlp is given a browser's YouTube login (needed for feeds).
pub fn logged_in() -> bool {
    std::env::var("YT_TUI_COOKIES_FROM_BROWSER").is_ok_and(|b| !b.trim().is_empty())
}

pub fn search(query: &str, limit: usize) -> Result<Vec<Video>> {
    list(&format!("ytsearch{limit}:{query}"), None)
}

/// One of your feeds — yt-dlp's `:ytrec` (Home), `:ytsubs`,
/// `:ytwatchlater`, `:ythistory`. These need the browser login.
pub fn feed(url: &str, limit: usize) -> Result<Vec<Video>> {
    list(url, Some(limit))
}

fn list(target: &str, limit: Option<usize>) -> Result<Vec<Video>> {
    let mut cmd = ytdlp();
    cmd.args(["--flat-playlist", "--dump-json"]);
    if let Some(n) = limit {
        cmd.args(["--playlist-end", &n.to_string()]);
    }
    let output = cmd.arg(target).output().context("couldn't run yt-dlp — is it installed?")?;
    if !output.status.success() {
        bail!("{}", last_error_line(&output.stderr));
    }
    Ok(output
        .stdout
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_slice::<Video>(l).ok())
        // Channels and playlists can show up in search; only videos play.
        .filter(|v| v.id.len() == 11)
        .collect())
}

/// Look up a single video by URL (for a pasted link).
pub fn lookup(url: &str) -> Result<Video> {
    let output = ytdlp()
        .args(["--flat-playlist", "--dump-single-json"])
        .arg(url)
        .output()
        .context("couldn't run yt-dlp — is it installed?")?;
    if !output.status.success() {
        bail!("{}", last_error_line(&output.stderr));
    }
    serde_json::from_slice(&output.stdout).context("yt-dlp returned something unexpected")
}

/// Pick streams for `video`. Resolution matters most — a 1080p60 stream
/// beats a 480p30 one, since ffmpeg drops the extra frames anyway — then
/// frame rate, then h264, which every Mac decodes in hardware.
pub fn resolve(video: &Video, max_height: u32, max_fps: u32, audio_only: bool) -> Result<Stream> {
    let (format, sort) = if audio_only {
        ("ba/b".to_string(), "proto:https,acodec:aac".to_string())
    } else {
        (
            format!("bv*[height<={max_height}]+ba/b[height<={max_height}]/bv*+ba/b"),
            // proto: direct https before HLS. Logged in, YouTube also offers
            // HLS (format 301 and friends), whose segments fail often enough
            // that ffmpeg gives up with "Invalid data found when processing
            // input". Live streams only come as HLS, so they still get it.
            format!("res:{max_height},proto:https,fps:{max_fps},vcodec:h264,acodec:aac"),
        )
    };
    let output = ytdlp()
        .args(["-J", "-f", &format, "-S", &sort])
        .arg(video.url())
        .output()
        .context("couldn't run yt-dlp — is it installed?")?;
    if !output.status.success() {
        bail!("{}", last_error_line(&output.stderr));
    }
    let info: Info = serde_json::from_slice(&output.stdout)
        .context("yt-dlp returned something unexpected")?;
    stream_from_info(info, audio_only)
}

#[derive(Deserialize)]
struct Info {
    title: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    is_live: Option<bool>,
    #[serde(default)]
    requested_formats: Option<Vec<Format>>,
    #[serde(flatten)]
    format: Format,
}

#[derive(Deserialize, Default)]
struct Format {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    vcodec: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    fps: Option<f64>,
    #[serde(default)]
    http_headers: Option<std::collections::HashMap<String, String>>,
}

impl Format {
    fn has_video(&self) -> bool {
        self.vcodec.as_deref().is_some_and(|v| v != "none")
    }
}

fn stream_from_info(info: Info, audio_only: bool) -> Result<Stream> {
    let formats = info.requested_formats.unwrap_or_default();
    let (video, audio) = if formats.is_empty() {
        // One muxed format: the same URL feeds both players.
        let url = info.format.url.clone().context("yt-dlp found no stream")?;
        let video = (!audio_only && info.format.has_video()).then(|| url.clone());
        (video.map(|u| (u, &info.format)), url)
    } else {
        let v = formats.iter().find(|f| f.has_video());
        let a = formats.iter().find(|f| !f.has_video()).or(v).context("no audio stream")?;
        let v = v.filter(|_| !audio_only);
        (
            v.and_then(|f| f.url.clone().map(|u| (u, f))),
            a.url.clone().context("audio stream has no URL")?,
        )
    };
    let meta = video.as_ref().map(|(_, f)| *f).unwrap_or(&info.format);
    let user_agent = meta
        .http_headers
        .as_ref()
        .or(info.format.http_headers.as_ref())
        .and_then(|h| h.get("User-Agent").cloned())
        .unwrap_or_else(|| "Mozilla/5.0".into());
    Ok(Stream {
        video_url: video.as_ref().map(|(u, _)| u.clone()),
        audio_url: audio,
        user_agent,
        width: meta.width.unwrap_or(1280),
        height: meta.height.unwrap_or(720),
        fps: meta.fps.filter(|f| *f > 0.0).unwrap_or(30.0),
        duration: info.duration.filter(|d| *d > 0.0),
        live: info.is_live.unwrap_or(false),
        title: info.title,
    })
}

/// Recognise a pasted YouTube link (watch, youtu.be, shorts, live).
pub fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("http://") || s.starts_with("https://") || s.starts_with("www.") || s.starts_with("youtu"))
        && (s.contains("youtube.com/") || s.contains("youtu.be/"))
}

/// Thumbnail URLs, best first. hq720 is 16:9 without letterboxing but not
/// every video has one.
pub fn thumbnail_urls(id: &str) -> [String; 2] {
    [
        format!("https://i.ytimg.com/vi/{id}/hq720.jpg"),
        format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
    ]
}

fn last_error_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text
        .lines()
        .rev()
        .find(|l| l.starts_with("ERROR"))
        .or_else(|| text.lines().rev().find(|l| !l.trim().is_empty()))
        .map(|l| l.trim_start_matches("ERROR: ").to_string())
        .unwrap_or_else(|| "yt-dlp failed".into());
    explain(&line).map(str::to_string).unwrap_or(line)
}

/// yt-dlp's wording for the failures people actually hit, translated into
/// what to do about them.
fn explain(error: &str) -> Option<&'static str> {
    if error.contains("confirm you") && error.contains("not a bot") {
        Some("YouTube is rate-limiting this network. Wait a while, or set YT_TUI_COOKIES_FROM_BROWSER=firefox (or chrome, safari…) to use your login.")
    } else if error.contains("Requested format is not available") {
        Some("YouTube only offered preview images — it's rate-limiting, or yt-dlp needs an update (brew upgrade yt-dlp).")
    } else {
        None
    }
}

pub fn format_duration(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn format_views(v: u64) -> String {
    match v {
        0..=999 => format!("{v} views"),
        1_000..=999_999 => format!("{:.1}K views", v as f64 / 1e3),
        1_000_000..=999_999_999 => format!("{:.1}M views", v as f64 / 1e6),
        _ => format!("{:.1}B views", v as f64 / 1e9),
    }
    .replace(".0", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_streams_are_split_into_video_and_audio() {
        let json = r#"{"title":"t","duration":61,"requested_formats":[
            {"url":"V","vcodec":"avc1.64002A","width":1920,"height":1080,"fps":60,
             "http_headers":{"User-Agent":"UA"}},
            {"url":"A","vcodec":"none"}]}"#;
        let s = stream_from_info(serde_json::from_str(json).unwrap(), false).unwrap();
        assert_eq!(s.video_url.as_deref(), Some("V"));
        assert_eq!(s.audio_url, "A");
        assert_eq!((s.width, s.height, s.fps), (1920, 1080, 60.0));
        assert_eq!(s.user_agent, "UA");
        assert!(!s.live);
    }

    #[test]
    fn a_muxed_format_feeds_both_players() {
        let json = r#"{"title":"live","is_live":true,"url":"M","vcodec":"avc1","width":1280,"height":720}"#;
        let s = stream_from_info(serde_json::from_str(json).unwrap(), false).unwrap();
        assert_eq!(s.video_url.as_deref(), Some("M"));
        assert_eq!(s.audio_url, "M");
        assert!(s.live);
        assert_eq!(s.duration, None);
    }

    #[test]
    fn audio_only_drops_the_video_stream() {
        let json = r#"{"title":"t","url":"A","vcodec":"none"}"#;
        let s = stream_from_info(serde_json::from_str(json).unwrap(), true).unwrap();
        assert!(s.video_url.is_none());
        assert_eq!(s.audio_url, "A");
    }

    #[test]
    fn urls_are_recognised() {
        assert!(looks_like_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(looks_like_url("youtu.be/dQw4w9WgXcQ"));
        assert!(!looks_like_url("lofi hip hop"));
    }

    #[test]
    fn common_yt_dlp_failures_are_explained() {
        let bot = b"WARNING: x\nERROR: [youtube] abc: Sign in to confirm you\xe2\x80\x99re not a bot. This helps";
        assert!(last_error_line(bot).contains("rate-limiting"));
        let fmt = b"ERROR: [youtube] abc: Requested format is not available. Use --list-formats";
        assert!(last_error_line(fmt).contains("preview images"));
        let other = b"ERROR: [youtube] abc: Video unavailable";
        assert_eq!(last_error_line(other), "[youtube] abc: Video unavailable");
    }

    #[test]
    fn durations_and_views_read_naturally() {
        assert_eq!(format_duration(59.9), "0:59");
        assert_eq!(format_duration(3725.0), "1:02:05");
        assert_eq!(format_views(950), "950 views");
        assert_eq!(format_views(12_000), "12K views");
        assert_eq!(format_views(1_250_000), "1.2M views");
    }
}
