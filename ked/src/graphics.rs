//! Inline image support for ked, using the **kitty graphics
//! protocol** (supported by kitty, ghostty, wezterm, foot, iTerm2 …).
//!
//! Markdown files can render `![alt](path)` references as real images
//! right in the buffer: the image is transmitted to the terminal once
//! and placed at a cell rectangle, with the covered source lines
//! blanked out.  When the cursor sits on the reference, the image is
//! hidden so the markdown stays editable (mirroring the existing
//! markdown conceal behaviour).
//!
//! This module also reads images from the system clipboard (via
//! `osascript` on macOS and `wl-paste`/`xclip` on Linux) so the user
//! can paste a screenshot straight into the buffer as `![](file.png)`.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

/// Cap on how wide an inline image may render, in cells.
const MAX_COLS: u16 = 60;
/// Cap on how tall a single image may render, in cells.
const MAX_ROWS: u16 = 40;
/// Escape terminator for kitty/APC-style sequences.
const ST: &[u8] = b"\x1b\\";

// ── capability detection ─────────────────────────────────────────

/// Terminal capability + cell-size info for inline images.
#[derive(Debug, Clone, Copy)]
pub struct KittyGraphics {
    /// Does the terminal implement the kitty graphics protocol?
    pub supported: bool,
    /// Width of one character cell in pixels (0 if unknown).
    pub cell_w: u16,
    /// Height of one character cell in pixels (0 if unknown).
    pub cell_h: u16,
}

impl KittyGraphics {
    /// A graphics-unsupported placeholder.
    pub fn unsupported() -> Self {
        KittyGraphics { supported: false, cell_w: 0, cell_h: 0 }
    }

    /// How many display rows an image of `img_w`×`img_h` pixels needs
    /// when rendered `cols` cells wide.
    pub fn rows_for(&self, img_w: u32, img_h: u32, cols: u16) -> u16 {
        if img_w == 0 || img_h == 0 || self.cell_w == 0 || self.cell_h == 0 {
            return 1;
        }
        let px_w = cols as u32 * self.cell_w as u32;
        let px_h = (px_w as u64 * img_h as u64) / img_w as u64;
        ((px_h as u32).div_ceil(self.cell_h as u32)).clamp(1, MAX_ROWS as u32) as u16
    }
}

/// Probe the terminal for kitty graphics + cell size, and apply env
/// fallbacks when the queries don't answer.  Called once at startup.
pub fn detect() -> KittyGraphics {
    let mut g = KittyGraphics::unsupported();
    if !io::stdin().is_terminal() {
        return g;
    }
    let fd = libc::STDIN_FILENO;
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return g;
    }
    unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };

    let _ = write_queries();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
    let mut resp: Vec<u8> = Vec::new();
    let mut buf = [0u8; 512];
    while std::time::Instant::now() < deadline {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n > 0 {
            resp.extend_from_slice(&buf[..n as usize]);
            if resp.len() > 4096 {
                break;
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
    }
    unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };

    let (ok, cell) = parse_responses(&resp);
    g.supported = ok || env_supports_kitty();
    if let Some((w, h)) = cell {
        g.cell_w = w;
        g.cell_h = h;
    }
    // Fall back to a ~2:1 cell aspect when we know the protocol works
    // but the size query went unanswered.
    if g.supported && (g.cell_w == 0 || g.cell_h == 0) {
        g.cell_w = 8;
        g.cell_h = 16;
    }
    g
}

fn write_queries() -> io::Result<()> {
    let mut out = io::stdout();
    // Graphics protocol capability query → `\x1b_Gi=1;OK\x1b\\`.
    out.write_all(b"\x1b_Gi=1,a=q,s=1,v=1;\x1b\\")?;
    // xterm cell-size query → `\x1b[6;<h>;<w>t`.
    out.write_all(b"\x1b[16t")?;
    out.flush()
}

/// Parse a startup response buffer into (kitty-ok, cell-size).
fn parse_responses(resp: &[u8]) -> (bool, Option<(u16, u16)>) {
    let text = String::from_utf8_lossy(resp);
    let ok = text.contains("\x1b_G") && text.contains(";OK");
    let mut cell = None;
    if let Some(i) = text.find("\x1b[6;") {
        let rest = &text[i + 4..];
        if let Some(j) = rest.find('t') {
            let nums: Vec<&str> = rest[..j].split(';').collect();
            if nums.len() >= 2 {
                if let (Ok(h), Ok(w)) = (nums[0].parse(), nums[1].parse()) {
                    if h > 0 && w > 0 {
                        cell = Some((w, h));
                    }
                }
            }
        }
    }
    (ok, cell)
}

fn env_supports_kitty() -> bool {
    let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    if program == "ghostty" || program == "WezTerm" || program == "kitty" {
        return true;
    }
    let term = std::env::var("TERM").unwrap_or_default();
    if term.contains("kitty") {
        return true;
    }
    std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some()
        || std::env::var_os("WEZTERM_PANE").is_some()
}

// ── protocol escape builders ──────────────────────────────────────

/// Transmit raw image bytes to the terminal under `key`, chunked into
/// ≤4096-char base64 payloads so the terminal's input buffer is happy.
pub fn transmit_cmd(key: u32, data: &[u8], out: &mut Vec<u8>) {
    let fmt = if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        "f=100,"
    } else {
        "" // omit `f` for JPEG/GIF/WebP → terminal auto-detects
    };
    let b64 = base64(data);
    let bytes = b64.as_bytes();
    const CHUNK: usize = 4096;
    let mut i = 0;
    while i < bytes.len() {
        let end = (i + CHUNK).min(bytes.len());
        let more = if end < bytes.len() { 1 } else { 0 };
        if i == 0 {
            out.extend_from_slice(format!("\x1b_Ga=t,i={key},q=2,{fmt}m={more};").as_bytes());
        } else {
            out.extend_from_slice(format!("\x1b_Gq=2,m={more};").as_bytes());
        }
        out.extend_from_slice(&bytes[i..end]);
        out.extend_from_slice(ST);
        i = end;
    }
}

/// Move the physical cursor to `(x, y)` and place the image there,
/// occupying `cols`×`rows` cells.  `C=1` keeps the cursor put so the
/// placement never nudges the terminal cursor.
pub fn place_cmd(key: u32, x: u16, y: u16, cols: u16, rows: u16, out: &mut Vec<u8>) {
    out.extend_from_slice(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
    out.extend_from_slice(
        format!(
            "\x1b_Ga=p,i={key},c={cols},r={rows},z=1,C=1,q=2;\x1b\\"
        )
        .as_bytes(),
    );
}

/// Delete the image (and all its placements).
pub fn delete_cmd(key: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(format!("\x1b_Ga=d,d=i,i={key},q=2;\x1b\\").as_bytes());
}

/// Delete every image and placement.
pub fn delete_all_cmd(out: &mut Vec<u8>) {
    out.extend_from_slice(b"\x1b_Ga=d,d=a,q=2;\x1b\\");
}

fn base64(data: &[u8]) -> String {
    const B64: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | (*chunk.get(2).unwrap_or(&0) as u32);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

// ── markdown image references ────────────────────────────────────

/// Extract the image path from a markdown image line: `![alt](path)`.
/// Returns `None` for non-image lines, remote URLs, and empty paths.
pub fn parse_md_image(line: &str) -> Option<&str> {
    let start = line.find("![")?;
    let open = line[start + 2..].find(']')? + start + 2;
    let rest = line.get(open + 1..)?;
    let after = rest.strip_prefix('(')?;
    let close = after.find(')')?;
    let mut target = after[..close].trim();
    // `<path with spaces>` syntax.
    if let Some(rest) = target.strip_prefix('<') {
        target = rest.split('>').next().unwrap_or(rest);
    }
    // Optional title:  path "title" / path 'title'.
    for marker in [" \"", " '"] {
        if let Some(i) = target.find(marker) {
            target = &target[..i];
            break;
        }
    }
    let target = target.trim();
    if target.is_empty() || target.starts_with("http") || target.starts_with("data:") {
        None
    } else {
        Some(target)
    }
}

/// One inline image to render in the current viewport.
pub struct PlacedImage {
    pub key: u32,
    pub path: PathBuf,
    /// Source line the reference lives on.
    pub line: usize,
    pub cols: u16,
    pub rows: u16,
}

/// Find the markdown images that should render between `top` and
/// `top + visible`, resolving paths against `base_dir` (the markdown
/// file's folder).  When `editing` is true (insert mode) an image
/// whose span contains the cursor is skipped so the markdown source
/// stays editable; in normal mode the image shows even under the
/// cursor (the cursor renders on top of it).
pub fn visible_images(
    lines: &[String],
    cursor_line: usize,
    top: usize,
    visible: usize,
    base_dir: Option<&Path>,
    content_cols: u16,
    gfx: &KittyGraphics,
    editing: bool,
) -> Vec<PlacedImage> {
    let mut out = Vec::new();
    let mut line = top;
    let end = lines.len().min(top + visible);
    while line < end {
        if let Some(target) = parse_md_image(&lines[line]) {
            if let Some(path) = resolve_image(target, base_dir) {
                if let Some((w, h)) = image_dimensions_file(&path) {
                    let cols = content_cols.min(MAX_COLS).max(4);
                    let rows = gfx.rows_for(w, h, cols);
                    let span_end = line + rows as usize;
                    let hidden_by_cursor =
                        editing && cursor_line >= line && cursor_line < span_end;
                    if !hidden_by_cursor {
                        out.push(PlacedImage {
                            key: image_key(&path),
                            path,
                            line,
                            cols,
                            rows,
                        });
                        line = span_end;
                        continue;
                    }
                }
            }
        }
        line += 1;
    }
    out
}

fn resolve_image(target: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let p = PathBuf::from(target);
    let p = if p.is_absolute() {
        p
    } else {
        match base_dir {
            Some(dir) if !dir.as_os_str().is_empty() => dir.join(&p),
            _ => p,
        }
    };
    p.is_file().then_some(p)
}

/// Session-stable id for an image path, salted with its mtime so
/// editing the file produces a fresh id (and a fresh transmission).
pub fn image_key(path: &Path) -> u32 {
    let mtime = fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut h = DefaultHasher::new();
    path.to_string_lossy().hash(&mut h);
    mtime.hash(&mut h);
    // Keep ids in the top half of the space (non-zero, far from the
    // small ids the terminal itself may use).
    (h.finish() as u32) | 0x8000_0000
}

// ── image dimension sniffing ─────────────────────────────────────

/// Read enough of the file to sniff its pixel dimensions.
pub fn image_dimensions_file(path: &Path) -> Option<(u32, u32)> {
    let mut f = fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        let n = f.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() >= 8192 {
            break;
        }
    }
    image_dimensions(&buf)
}

/// Sniff pixel dimensions from PNG/JPEG/GIF/WebP headers.
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // PNG
    if bytes.len() >= 24 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return (w > 0 && h > 0).then_some((w, h));
    }
    // GIF
    if bytes.len() >= 10 && &bytes[0..4] == b"GIF8" {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return (w > 0 && h > 0).then_some((w, h));
    }
    // JPEG: walk segments until a SOF marker.
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2;
        while i + 9 <= bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return (w > 0 && h > 0).then_some((w, h));
            }
            if i + 3 >= bytes.len() {
                break;
            }
            let seg = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            i += 2 + seg;
        }
        return None;
    }
    // WebP (extended VP8X).
    if bytes.len() >= 30
        && &bytes[0..4] == b"RIFF"
        && &bytes[8..12] == b"WEBP"
        && &bytes[12..16] == b"VP8X"
    {
        let w = 1 + (bytes[24] as u32)
            | ((bytes[25] as u32) << 8)
            | ((bytes[26] as u32) << 16);
        let h = 1 + (bytes[27] as u32)
            | ((bytes[28] as u32) << 8)
            | ((bytes[29] as u32) << 16);
        return Some((w, h));
    }
    None
}

// ── system clipboard images ──────────────────────────────────────

/// Read a PNG from the system clipboard, if the clipboard holds an
/// image.  macOS uses `osascript`; Linux tries `wl-paste` then `xclip`.
pub fn clipboard_image_png() -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        let tmp = std::env::temp_dir().join(format!("ked_clip_{}.png", std::process::id()));
        let path = tmp.display().to_string();
        let script = format!(
            "set f to POSIX file \"{p}\"\n\
             set d to the clipboard as «class PNGf»\n\
             set o to open for access f with write permission\n\
             write d to o\n\
             close access o",
            p = path
        );
        let ok = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let bytes = if ok { fs::read(&tmp).ok() } else { None };
        let _ = fs::remove_file(&tmp);
        return bytes.filter(|b| !b.is_empty());
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            if let Some(b) = capture("wl-paste", &["--type", "image/png", "--no-newline"]) {
                return Some(b);
            }
        }
        return capture("xclip", &["-selection", "clipboard", "-t", "image/png", "-o"]);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn capture(cmd: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        Some(out.stdout)
    } else {
        None
    }
}

// ── tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn parse_md_image_variants() {
        assert_eq!(parse_md_image("![alt](pic.png)"), Some("pic.png"));
        assert_eq!(parse_md_image("a ![x](b/c.png) tail"), Some("b/c.png"));
        assert_eq!(parse_md_image("![](pic.png)"), Some("pic.png"));
        assert_eq!(parse_md_image("![alt](pic.png \"title\")"), Some("pic.png"));
        assert_eq!(parse_md_image("![alt](<my pic.png>)"), Some("my pic.png"));
        assert_eq!(parse_md_image("![alt](https://x/y.png)"), None);
        assert_eq!(parse_md_image("![alt](data:image/png;base64,xx)"), None);
        assert_eq!(parse_md_image(concat!("# heading ", "![x]")), None);
        assert_eq!(parse_md_image("![alt]()"), None);
        assert_eq!(parse_md_image("[link](page)"), None);
    }

    #[test]
    fn png_dimensions() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&[0; 8]); // IHDR len/type
        png.extend_from_slice(&100u32.to_be_bytes());
        png.extend_from_slice(&50u32.to_be_bytes());
        assert_eq!(image_dimensions(&png), Some((100, 50)));
    }

    #[test]
    fn gif_and_webp_dimensions() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&320u16.to_le_bytes());
        gif.extend_from_slice(&240u16.to_le_bytes());
        assert_eq!(image_dimensions(&gif), Some((320, 240)));

        let mut webp = b"RIFF\x00\x00\x00\x00WEBPVP8X".to_vec();
        webp.extend_from_slice(&[0u8; 8]); // flags + reserved + a bit
        // canvas width-1 = 639, height-1 = 479 (24-bit LE each)
        webp.extend_from_slice(&[0x7F, 0x02, 0x00, 0xDF, 0x01, 0x00]);
        assert_eq!(image_dimensions(&webp), Some((640, 480)));
    }

    #[test]
    fn jpeg_dimensions() {
        // SOI + APP0 segment + SOF0 with 480x320.
        let mut jpg = vec![0xFF, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04]); // APP0, len 4
        jpg.extend_from_slice(&[0x00, 0x00]);
        jpg.extend_from_slice(&[0xFF, 0xC0]); // SOF0
        jpg.extend_from_slice(&[0x00, 0x11, 0x08]); // len 17, precision 8
        jpg.extend_from_slice(&480u16.to_be_bytes());
        jpg.extend_from_slice(&320u16.to_be_bytes());
        assert_eq!(image_dimensions(&jpg), Some((320, 480)));
    }

    #[test]
    fn rows_scale_with_cell_height() {
        let g = KittyGraphics { supported: true, cell_w: 10, cell_h: 20 };
        // Square image, 20 cols wide → 20*10=200px wide → 200px tall
        // → 200/20 = 10 rows.
        assert_eq!(g.rows_for(200, 200, 20), 10);
        // 2:1 landscape → half the rows.
        assert_eq!(g.rows_for(400, 200, 20), 5);
        // Portrait 1:2 → double the rows.
        assert_eq!(g.rows_for(200, 400, 20), 20);
    }

    #[test]
    fn parse_responses_finds_capability_and_cell() {
        let resp = b"\x1b_Gi=1;OK\x1b\\\x1b[6;24;10t";
        let (ok, cell) = parse_responses(resp);
        assert!(ok);
        assert_eq!(cell, Some((10, 24)));
    }
}