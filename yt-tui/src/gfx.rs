//! Kitty graphics protocol output.
//!
//! Images are shown with *Unicode placeholders*: the image is transmitted
//! with a virtual placement (`U=1`), and the cells it covers are ordinary
//! text — U+10EEEE plus two combining diacritics giving the row and column,
//! coloured with the image id. tmux treats those cells like any other text,
//! so the picture stays inside the pane, scrolls with it, and never fights
//! ratatui over the cursor.
//!
//! Every command carries `q=2`, so the terminal never answers: replies would
//! otherwise land in stdin and show up as stray text or phantom key presses.
//!
//! Pixels travel through POSIX shared memory (`t=s`) when the terminal is on
//! this machine — the tty only carries a ~100 byte escape per frame — and
//! fall back to chunked base64 (`t=d`) over SSH.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use std::collections::VecDeque;
use std::ffi::CString;
use std::io::{self, Write};
use std::process::Command;

/// Image ids. Kept below 2^24 so the whole id fits in the placeholder's
/// foreground colour and no third diacritic is needed.
pub const VIDEO_ID: u32 = 0x59_54_01;
pub const THUMB_ID: u32 = 0x59_54_02;

/// The largest grid a placement can address: one diacritic per row/column.
pub const MAX_CELLS: u16 = DIACRITICS.len() as u16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transfer {
    /// POSIX shared memory: the terminal reads the pixels directly.
    Shm,
    /// Base64 in the escape sequence, split into 4096 byte chunks.
    Direct,
}

pub struct Graphics {
    tmux: bool,
    pub transfer: Transfer,
    seq: u64,
    /// Shared-memory objects handed to the terminal. Kitty unlinks each one
    /// after reading it; we unlink old ones too so a terminal that never
    /// reads them can't leak memory.
    handed_out: VecDeque<CString>,
}

impl Graphics {
    /// Graphics are used when the terminal (the outer one, under tmux)
    /// speaks the kitty protocol. `YT_TUI_GRAPHICS=on|off` overrides the
    /// check, `YT_TUI_TRANSFER=shm|direct` the transfer mode.
    pub fn detect() -> Option<Graphics> {
        let tmux = std::env::var_os("TMUX").is_some();
        match std::env::var("YT_TUI_GRAPHICS").as_deref() {
            Ok("off") | Ok("0") => return None,
            Ok("on") | Ok("1") => {}
            _ if !terminal_supports_kitty(tmux) => return None,
            _ => {}
        }
        let remote = std::env::var_os("SSH_CONNECTION").is_some()
            || std::env::var_os("SSH_TTY").is_some();
        let transfer = match std::env::var("YT_TUI_TRANSFER").as_deref() {
            Ok("direct") => Transfer::Direct,
            Ok("shm") => Transfer::Shm,
            _ if remote => Transfer::Direct,
            _ => Transfer::Shm,
        };
        Some(Graphics { tmux, transfer, seq: 0, handed_out: VecDeque::new() })
    }

    /// Transmit an RGB image and give it a virtual placement of
    /// `cols`×`rows` cells. Re-sending the same id replaces the picture
    /// in place, so the placeholder cells never need redrawing.
    pub fn show(
        &mut self,
        out: &mut impl Write,
        id: u32,
        rgb: &[u8],
        width: u32,
        height: u32,
        cols: u16,
        rows: u16,
    ) -> io::Result<()> {
        debug_assert_eq!(rgb.len(), (width * height * 3) as usize);
        let ctrl = format!(
            "a=T,U=1,f=24,s={width},v={height},c={cols},r={rows},i={id},q=2"
        );
        if self.transfer == Transfer::Shm {
            match self.write_shm(rgb) {
                Ok(name) => {
                    let payload = STANDARD_NO_PAD.encode(name.as_bytes());
                    let ctrl = format!("{ctrl},t=s,S={}", rgb.len());
                    self.emit(out, &ctrl, payload.as_bytes())?;
                    return out.flush();
                }
                // Shared memory unavailable (sandbox, exhausted): stay
                // correct and fall back for good.
                Err(_) => self.transfer = Transfer::Direct,
            }
        }
        self.emit_chunked(out, &ctrl, rgb)?;
        out.flush()
    }

    /// Free an image and everything placed from it.
    pub fn delete(&mut self, out: &mut impl Write, id: u32) -> io::Result<()> {
        self.emit(out, &format!("a=d,d=I,i={id},q=2"), b"")?;
        out.flush()
    }

    /// Release every shared-memory object we may still own.
    pub fn cleanup(&mut self) {
        while let Some(name) = self.handed_out.pop_front() {
            unsafe { libc::shm_unlink(name.as_ptr()) };
        }
    }

    fn write_shm(&mut self, data: &[u8]) -> io::Result<CString> {
        self.seq += 1;
        // macOS limits shm names to 31 bytes.
        let name = CString::new(format!("/yttui{}-{}", std::process::id(), self.seq))
            .expect("no NUL in name");
        unsafe {
            let fd = libc::shm_open(
                name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o600 as libc::c_uint,
            );
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let fail = |fd| {
                let e = io::Error::last_os_error();
                libc::close(fd);
                libc::shm_unlink(name.as_ptr());
                Err(e)
            };
            if libc::ftruncate(fd, data.len() as libc::off_t) != 0 {
                return fail(fd);
            }
            let map = libc::mmap(
                std::ptr::null_mut(),
                data.len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            if map == libc::MAP_FAILED {
                return fail(fd);
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut u8, data.len());
            libc::munmap(map, data.len());
            libc::close(fd);
        }
        self.handed_out.push_back(name.clone());
        // A couple of seconds of frames is far longer than kitty takes to
        // read one; anything older is either read or never will be.
        while self.handed_out.len() > 120 {
            let old = self.handed_out.pop_front().unwrap();
            unsafe { libc::shm_unlink(old.as_ptr()) };
        }
        Ok(name)
    }

    fn emit_chunked(&self, out: &mut impl Write, ctrl: &str, data: &[u8]) -> io::Result<()> {
        let b64 = STANDARD.encode(data);
        let chunks: Vec<&[u8]> = b64.as_bytes().chunks(4096).collect();
        let last = chunks.len().saturating_sub(1);
        for (i, chunk) in chunks.iter().enumerate() {
            let more = if i == last { 0 } else { 1 };
            if i == 0 {
                self.emit(out, &format!("{ctrl},t=d,m={more}"), chunk)?;
            } else {
                self.emit(out, &format!("m={more},q=2"), chunk)?;
            }
        }
        Ok(())
    }

    fn emit(&self, out: &mut impl Write, ctrl: &str, payload: &[u8]) -> io::Result<()> {
        out.write_all(&encode_command(self.tmux, ctrl, payload))
    }
}

impl Drop for Graphics {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// `ESC _ G <ctrl> ; <payload> ESC \`, wrapped in a tmux passthrough (with
/// every ESC doubled) when running inside tmux.
pub fn encode_command(tmux: bool, ctrl: &str, payload: &[u8]) -> Vec<u8> {
    let mut seq = Vec::with_capacity(ctrl.len() + payload.len() + 8);
    seq.extend_from_slice(b"\x1b_G");
    seq.extend_from_slice(ctrl.as_bytes());
    seq.push(b';');
    seq.extend_from_slice(payload);
    seq.extend_from_slice(b"\x1b\\");
    if !tmux {
        return seq;
    }
    let mut wrapped = Vec::with_capacity(seq.len() + 16);
    wrapped.extend_from_slice(b"\x1bPtmux;");
    for &b in &seq {
        if b == 0x1b {
            wrapped.push(0x1b);
        }
        wrapped.push(b);
    }
    wrapped.extend_from_slice(b"\x1b\\");
    wrapped
}

fn terminal_supports_kitty(tmux: bool) -> bool {
    let kitty_like = |s: &str| s.contains("kitty") || s.contains("ghostty");
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    if tmux {
        // Inside tmux, $TERM describes tmux; ask it about the real client.
        return Command::new("tmux")
            .args(["display-message", "-p", "#{client_termname}"])
            .output()
            .map(|o| kitty_like(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or(false);
    }
    let term = std::env::var("TERM").unwrap_or_default();
    let prog = std::env::var("TERM_PROGRAM").unwrap_or_default().to_lowercase();
    kitty_like(&term) || kitty_like(&prog)
}

/// Draws the placeholder cells for a virtual placement of `id`.
pub struct Placeholder {
    pub id: u32,
}

impl Widget for Placeholder {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let fg = Color::Rgb((self.id >> 16) as u8, (self.id >> 8) as u8, self.id as u8);
        let mut sym = String::with_capacity(12);
        for row in 0..area.height.min(MAX_CELLS) {
            for col in 0..area.width.min(MAX_CELLS) {
                sym.clear();
                sym.push(PLACEHOLDER);
                sym.push(DIACRITICS[row as usize]);
                sym.push(DIACRITICS[col as usize]);
                buf[(area.x + col, area.y + row)].set_symbol(&sym).set_fg(fg);
            }
        }
    }
}

const PLACEHOLDER: char = '\u{10EEEE}';

/// Row/column numbers, in order: kitty's rowcolumn-diacritics table.
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}',
    '\u{0346}', '\u{034A}', '\u{034B}', '\u{034C}', '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}',
    '\u{035B}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}',
    '\u{036A}', '\u{036B}', '\u{036C}', '\u{036D}', '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}',
    '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}', '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}',
    '\u{0598}', '\u{0599}', '\u{059C}', '\u{059D}', '\u{059E}', '\u{059F}', '\u{05A0}', '\u{05A1}',
    '\u{05A8}', '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}', '\u{0610}', '\u{0611}',
    '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}', '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}',
    '\u{0659}', '\u{065A}', '\u{065B}', '\u{065D}', '\u{065E}', '\u{06D6}', '\u{06D7}', '\u{06D8}',
    '\u{06D9}', '\u{06DA}', '\u{06DB}', '\u{06DC}', '\u{06DF}', '\u{06E0}', '\u{06E1}', '\u{06E2}',
    '\u{06E4}', '\u{06E7}', '\u{06E8}', '\u{06EB}', '\u{06EC}', '\u{0730}', '\u{0732}', '\u{0733}',
    '\u{0735}', '\u{0736}', '\u{073A}', '\u{073D}', '\u{073F}', '\u{0740}', '\u{0741}', '\u{0743}',
    '\u{0745}', '\u{0747}', '\u{0749}', '\u{074A}', '\u{07EB}', '\u{07EC}', '\u{07ED}', '\u{07EE}',
    '\u{07EF}', '\u{07F0}', '\u{07F1}', '\u{07F3}', '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}',
    '\u{081B}', '\u{081C}', '\u{081D}', '\u{081E}', '\u{081F}', '\u{0820}', '\u{0821}', '\u{0822}',
    '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082A}', '\u{082B}', '\u{082C}',
    '\u{082D}', '\u{0951}', '\u{0953}', '\u{0954}', '\u{0F82}', '\u{0F83}', '\u{0F86}', '\u{0F87}',
    '\u{135D}', '\u{135E}', '\u{135F}', '\u{17DD}', '\u{193A}', '\u{1A17}', '\u{1A75}', '\u{1A76}',
    '\u{1A77}', '\u{1A78}', '\u{1A79}', '\u{1A7A}', '\u{1A7B}', '\u{1A7C}', '\u{1B6B}', '\u{1B6D}',
    '\u{1B6E}', '\u{1B6F}', '\u{1B70}', '\u{1B71}', '\u{1B72}', '\u{1B73}', '\u{1CD0}', '\u{1CD1}',
    '\u{1CD2}', '\u{1CDA}', '\u{1CDB}', '\u{1CE0}', '\u{1DC0}', '\u{1DC1}', '\u{1DC3}', '\u{1DC4}',
    '\u{1DC5}', '\u{1DC6}', '\u{1DC7}', '\u{1DC8}', '\u{1DC9}', '\u{1DCB}', '\u{1DCC}', '\u{1DD1}',
    '\u{1DD2}', '\u{1DD3}', '\u{1DD4}', '\u{1DD5}', '\u{1DD6}', '\u{1DD7}', '\u{1DD8}', '\u{1DD9}',
    '\u{1DDA}', '\u{1DDB}', '\u{1DDC}', '\u{1DDD}', '\u{1DDE}', '\u{1DDF}', '\u{1DE0}', '\u{1DE1}',
    '\u{1DE2}', '\u{1DE3}', '\u{1DE4}', '\u{1DE5}', '\u{1DE6}', '\u{1DFE}', '\u{20D0}', '\u{20D1}',
    '\u{20D4}', '\u{20D5}', '\u{20D6}', '\u{20D7}', '\u{20DB}', '\u{20DC}', '\u{20E1}', '\u{20E7}',
    '\u{20E9}', '\u{20F0}', '\u{2CEF}', '\u{2CF0}', '\u{2CF1}', '\u{2DE0}', '\u{2DE1}', '\u{2DE2}',
    '\u{2DE3}', '\u{2DE4}', '\u{2DE5}', '\u{2DE6}', '\u{2DE7}', '\u{2DE8}', '\u{2DE9}', '\u{2DEA}',
    '\u{2DEB}', '\u{2DEC}', '\u{2DED}', '\u{2DEE}', '\u{2DEF}', '\u{2DF0}', '\u{2DF1}', '\u{2DF2}',
    '\u{2DF3}', '\u{2DF4}', '\u{2DF5}', '\u{2DF6}', '\u{2DF7}', '\u{2DF8}', '\u{2DF9}', '\u{2DFA}',
    '\u{2DFB}', '\u{2DFC}', '\u{2DFD}', '\u{2DFE}', '\u{2DFF}', '\u{A66F}', '\u{A67C}', '\u{A67D}',
    '\u{A6F0}', '\u{A6F1}', '\u{A8E0}', '\u{A8E1}', '\u{A8E2}', '\u{A8E3}', '\u{A8E4}', '\u{A8E5}',
    '\u{A8E6}', '\u{A8E7}', '\u{A8E8}', '\u{A8E9}', '\u{A8EA}', '\u{A8EB}', '\u{A8EC}', '\u{A8ED}',
    '\u{A8EE}', '\u{A8EF}', '\u{A8F0}', '\u{A8F1}', '\u{AAB0}', '\u{AAB2}', '\u{AAB3}', '\u{AAB7}',
    '\u{AAB8}', '\u{AABE}', '\u{AABF}', '\u{AAC1}', '\u{FE20}', '\u{FE21}', '\u{FE22}', '\u{FE23}',
    '\u{FE24}', '\u{FE25}', '\u{FE26}', '\u{10A0F}', '\u{10A38}', '\u{1D185}', '\u{1D186}',
    '\u{1D187}', '\u{1D188}', '\u{1D189}', '\u{1D1AA}', '\u{1D1AB}', '\u{1D1AC}', '\u{1D1AD}',
    '\u{1D242}', '\u{1D243}', '\u{1D244}',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_wrapping_doubles_escapes() {
        let plain = encode_command(false, "a=d,i=1,q=2", b"");
        assert_eq!(plain, b"\x1b_Ga=d,i=1,q=2;\x1b\\");
        let wrapped = encode_command(true, "a=d,i=1,q=2", b"");
        assert_eq!(wrapped, b"\x1bPtmux;\x1b\x1b_Ga=d,i=1,q=2;\x1b\x1b\\\x1b\\");
    }

    #[test]
    fn chunked_transfer_splits_at_4096_and_marks_the_last() {
        let g = Graphics { tmux: false, transfer: Transfer::Direct, seq: 0, handed_out: VecDeque::new() };
        let mut out = Vec::new();
        g.emit_chunked(&mut out, "a=T,i=1,q=2", &vec![7u8; 6000]).unwrap();
        let text = String::from_utf8(out).unwrap();
        let cmds: Vec<&str> = text.split("\x1b\\").filter(|s| !s.is_empty()).collect();
        // 6000 bytes → 8000 base64 chars → two chunks.
        assert_eq!(cmds.len(), 2);
        assert!(cmds[0].starts_with("\x1b_Ga=T,i=1,q=2,t=d,m=1;"));
        assert!(cmds[1].starts_with("\x1b_Gm=0,q=2;"));
        assert!(cmds.iter().all(|c| c.len() <= 4096 + 40));
    }

    #[test]
    fn diacritics_are_unique_combining_marks() {
        let set: std::collections::HashSet<_> = DIACRITICS.iter().collect();
        assert_eq!(set.len(), 297);
        assert_eq!(DIACRITICS[0], '\u{0305}');
        assert_eq!(DIACRITICS[296], '\u{1D244}');
    }

    #[test]
    fn placeholder_cells_are_one_column_wide() {
        // ratatui diffs by display width; a wrong width would shift the
        // whole row of text after the picture.
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        Placeholder { id: VIDEO_ID }.render(area, &mut buf);
        let cell = &buf[(3, 1)];
        let chars: Vec<char> = cell.symbol().chars().collect();
        assert_eq!(chars, vec![PLACEHOLDER, DIACRITICS[1], DIACRITICS[3]]);
        assert_eq!(cell.fg, Color::Rgb(0x59, 0x54, 0x01));
        let prev = Buffer::empty(area);
        let updates = prev.diff(&buf);
        assert_eq!(updates.len(), 8, "every cell is drawn, none skipped as 'wide'");
    }
}
