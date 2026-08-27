use std::io;
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::editor::char_width;

pub struct ShellProcess {
    fd: i32,
    slave_fd: i32,
    pid: i32,
    rx: mpsc::Receiver<Vec<u8>>,
    alive: bool,
    output: Vec<u8>,

    // Incremental-processing state so we don't re-process the
    // entire buffer on every frame.
    processed_up_to: usize,           // bytes of `output` already consumed
    cache_lines: Vec<Vec<StyledChar>>,// row-indexed editable lines
    cache_cur: Vec<StyledChar>,       // working buffer for current row
    cache_cursor: usize,              // cursor position within cache_cur
    cache_style: Style,               // current SGR style
    cache_default: Style,             // default_style from last call
    cache_out: Vec<Line<'static>>,    // flattened result for rendering

    /// Cursor position in the logical output (row, col).
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl ShellProcess {
    pub fn spawn() -> io::Result<(Self, std::thread::JoinHandle<()>)> {
        unsafe {
            let mut master: i32 = 0;
            let mut slave: i32 = 0;
            let mut slave_name: [libc::c_char; 256] = [0; 256];

            // Use system-default terminal settings — don't inherit
            // the raw-mode termios from crossterm's STDIN.
            let ret = libc::openpty(&mut master, &mut slave,
                slave_name.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut());
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }

            let pid = libc::fork();
            if pid == -1 {
                let err = io::Error::last_os_error();
                libc::close(master);
                libc::close(slave);
                return Err(err);
            }

            if pid == 0 {
                libc::close(master);
                libc::setsid();

                libc::ioctl(slave, libc::TIOCSCTTY as libc::c_ulong, std::ptr::null_mut::<libc::c_int>());

                let mut t: libc::termios = std::mem::zeroed();
                libc::tcgetattr(slave, &mut t);
                // Standard c_cc control characters so line editing works.
                t.c_cc[libc::VERASE] = 0x08; // ^H (Backspace)
                t.c_cc[libc::VWERASE] = 0x17; // Ctrl+W
                t.c_cc[libc::VKILL] = 0x15; // Ctrl+U
                t.c_cc[libc::VEOF] = 0x04; // Ctrl+D
                t.c_cc[libc::VINTR] = 0x03; // Ctrl+C
                t.c_cc[libc::VQUIT] = 0x1c; // Ctrl+\
                t.c_cc[libc::VSUSP] = 0x1a; // Ctrl+Z
                t.c_cc[libc::VSTART] = 0x11; // Ctrl+Q
                t.c_cc[libc::VSTOP] = 0x13; // Ctrl+S
                t.c_cc[libc::VMIN] = 1;
                t.c_cc[libc::VTIME] = 0;
                // Input processing: keep CR→NL so Enter works, disable flow control.
                t.c_iflag &= !(libc::IXON | libc::IXOFF | libc::ISTRIP);
                t.c_iflag |= libc::ICRNL;
                // Enable signal generation so Ctrl+C works.
                t.c_lflag |= libc::ISIG;
                libc::tcsetattr(slave, libc::TCSANOW, &t);

                libc::dup2(slave, 0);
                libc::dup2(slave, 1);
                libc::dup2(slave, 2);
                if slave > 2 { libc::close(slave); }

                // Set TERM so TUI programs can render properly.
                let term = std::ffi::CString::new("xterm-256color").unwrap();
                libc::setenv(
                    std::ffi::CString::new("TERM").unwrap().as_ptr(),
                    term.as_ptr(),
                    1,
                );

                let shell = std::env::var("SHELL")
                    .unwrap_or_else(|_| "bash".to_string());
                let c_shell = std::ffi::CString::new(shell).unwrap();
                let args: [*const libc::c_char; 2] = [
                    c_shell.as_ptr(),
                std::ptr::null_mut(),
                ];
                libc::execvp(c_shell.as_ptr(), args.as_ptr());
                libc::_exit(1);
            }

            let slave_fd = slave;

            let (tx, rx) = mpsc::channel();
            let alive = std::sync::Arc::new(AtomicBool::new(true));
            let alive_clone = alive.clone();

            let reader = std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    // Poll instead of a blocking read so the thread
                    // can notice a closed master fd when the shell is
                    // dropped (a blocked read would never wake up and
                    // joining the thread would hang the editor).
                    let mut pfd = libc::pollfd {
                        fd: master,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let rc = libc::poll(&mut pfd, 1, 100);
                    if rc < 0 {
                        break;
                    }
                    if rc == 0 {
                        continue; // timeout — poll again
                    }
                    let n = libc::read(master, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                    if n <= 0 {
                        let _ = tx.send(Vec::new());
                        break;
                    }
                    if tx.send(buf[..n as usize].to_vec()).is_err() {
                        break;
                    }
                }
                alive_clone.store(false, Ordering::SeqCst);
            });

            Ok((ShellProcess {
                fd: master,
                slave_fd,
                pid,
                rx,
                alive: true,
                output: Vec::new(),
                processed_up_to: 0,
                cache_lines: Vec::new(),
                cache_cur: Vec::new(),
                cache_cursor: 0,
                cache_style: Style::new(),
                cache_default: Style::new(),
                cache_out: Vec::new(),
                cursor_row: 0,
                cursor_col: 0,
            }, reader))
        }
    }

    pub fn force_raw_mode(&self) {
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(self.slave_fd, &mut t) != 0 {
                return;
            }
            if t.c_lflag & libc::ECHO == 0 {
                return;
            }
            t.c_lflag &= !libc::ECHO;
            libc::tcsetattr(self.slave_fd, libc::TCSANOW, &t);
            let mut check: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(self.slave_fd, &mut check) == 0
                && check.c_lflag & libc::ECHO != 0
            {
                libc::tcsetattr(self.slave_fd, libc::TCSADRAIN, &t);
            }
        }
    }

    pub fn write(&self, data: &[u8]) {
        unsafe {
            libc::write(self.fd, data.as_ptr() as *const libc::c_void, data.len());
        }
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        let ws = libc::winsize {
            ws_row: rows as u16,
            ws_col: cols as u16,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(self.fd, libc::TIOCSWINSZ, &ws);
        }
    }

    pub fn tick(&mut self) {
        while let Ok(data) = self.rx.try_recv() {
            if data.is_empty() {
                self.alive = false;
                return;
            }
            self.output.extend_from_slice(&data);
        }
        // Cap the raw buffer so we don't accumulate indefinitely.
        // Keep at most 128 KB — discard the oldest bytes past that.
        const MAX_BYTES: usize = 128 * 1024;
        if self.output.len() > MAX_BYTES {
            let keep = MAX_BYTES / 2;
            let split = self.output.len() - keep;
            // Find a safe cut point: scan forward from `split` for \n.
            let cut = self.output[split..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|pos| split + pos + 1)
                .unwrap_or(split);
            self.output.drain(..cut);
            // Invalidate the full cache since we dropped raw bytes.
            self.processed_up_to = 0;
            self.cache_lines.clear();
            self.cache_cur.clear();
            self.cache_cursor = 0;
            self.cache_style = Style::new();
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Return the shell output as styled ratatui lines, using the
    /// given `default_style` for uncoloured text (e.g. from the
    /// current editor theme).
    ///
    /// ANSI SGR colour codes (`ESC[<params>m`) are parsed and
    /// converted to ratatui styles.  All other escape sequences are
    /// stripped as usual.
    ///
    /// Processing is incremental — only newly arrived bytes are
    /// parsed each call, so this is cheap to call every frame.
    pub fn output_styled(&mut self, default_style: Style) -> &[Line<'static>] {
        let total = self.output.len();

        // Cache hit: no new data and same default style.
        if self.processed_up_to > 0
            && self.processed_up_to == total
            && self.cache_default == default_style
        {
            return &self.cache_out;
        }

        // Default style changed (e.g. theme switch) → reset everything
        // so SGR codes are re-interpreted with the new defaults.
        if self.cache_default != default_style {
            self.processed_up_to = 0;
            self.cache_lines.clear();
            self.cache_cur.clear();
            self.cache_cursor = 0;
            self.cache_style = default_style;
            self.cache_default = default_style;
        }

        // First call or after a cache invalidation.
        if self.processed_up_to == 0 {
            self.cache_lines.clear();
            self.cache_cur.clear();
            self.cache_cursor = 0;
            self.cache_style = default_style;
            self.cache_default = default_style;
        }

        // Process any new bytes.
        if self.processed_up_to < total {
            let new = self.output[self.processed_up_to..].to_vec();
            self.processed_up_to = total;
            self.process_bytes(&new, default_style);
        }

        // Flatten cache_lines into cache_out for rendering.
        self.cache_out.clear();
        // Soft-commit: copy cache_cur to cache_lines so it appears
        // in the output, but don't lose the working buffer.
        while self.cache_lines.len() <= self.cursor_row {
            self.cache_lines.push(Vec::new());
        }
        if !self.cache_cur.is_empty() || self.cache_lines[self.cursor_row].is_empty() {
            self.cache_lines[self.cursor_row] = self.cache_cur.clone();
        }
        for line in &self.cache_lines {
            self.cache_out.push(collapse_styled(line.clone()));
        }
        &self.cache_out
    }

    /// Flush the current working line into cache_lines at cursor_row.
    fn commit_line(&mut self) {
        while self.cache_lines.len() <= self.cursor_row {
            self.cache_lines.push(Vec::new());
        }
        self.cache_lines[self.cursor_row] = std::mem::take(&mut self.cache_cur);
    }

    /// Load cache_lines[row] into the working buffer (or start fresh).
    fn load_line(&mut self, row: usize) {
        self.cursor_row = row;
        while self.cache_lines.len() <= row {
            self.cache_lines.push(Vec::new());
        }
        self.cache_cur = std::mem::take(&mut self.cache_lines[row]);
        self.cache_cursor = char_index_for_col(&self.cache_cur, self.cursor_col);
    }

    /// Track the logical cursor position for render_shell.
    fn cursor_up(&mut self) {
        self.commit_line();
        let row = self.cursor_row.saturating_sub(1);
        self.load_line(row);
    }
    fn cursor_down(&mut self) {
        self.commit_line();
        let row = self.cursor_row + 1;
        self.load_line(row);
    }
    fn cursor_forward(&mut self) {
        if self.cache_cursor < self.cache_cur.len() {
            let w = char_width(self.cache_cur[self.cache_cursor].ch);
            self.cache_cursor += 1;
            self.cursor_col += w;
        }
    }
    fn cursor_back(&mut self) {
        if self.cache_cursor > 0 {
            self.cache_cursor -= 1;
            let w = char_width(self.cache_cur[self.cache_cursor].ch);
            self.cursor_col = self.cursor_col.saturating_sub(w);
        }
    }
    fn cursor_goto(&mut self, row: usize, col: usize) {
        self.commit_line();
        self.cursor_col = col;
        self.load_line(row);
    }

    /// Parse `bytes` as UTF‑8 and feed the characters into the line
    /// buffer, handling \\r, \\b, \\t and ANSI escapes.
    fn process_bytes(&mut self, bytes: &[u8], default_style: Style) {
        let s = String::from_utf8_lossy(bytes);
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\x1b' => match chars.next() {
                    Some('[') => {
                        let mut params = String::new();
                        while let Some(&c) = chars.peek() {
                            if c as u32 >= 0x40 && c as u32 <= 0x7e {
                                let term = chars.next().unwrap();
                                if term == 'm' {
                                    self.cache_style =
                                        parse_sgr(&params, default_style);
                                } else if term == 'J'
                                    && (params == "2" || params == "3")
                                {
                                    self.cache_lines.clear();
                                    self.cache_cur.clear();
                                    self.cache_cursor = 0;
                                    self.cursor_row = 0;
                                    self.cursor_col = 0;
                                } else if term == 'A' {
                                    let n: usize = params.parse().unwrap_or(1);
                                    for _ in 0..n { self.cursor_up(); }
                                } else if term == 'B' {
                                    let n: usize = params.parse().unwrap_or(1);
                                    for _ in 0..n { self.cursor_down(); }
                                } else if term == 'C' {
                                    let n: usize = params.parse().unwrap_or(1);
                                    for _ in 0..n { self.cursor_forward(); }
                                } else if term == 'D' {
                                    let n: usize = params.parse().unwrap_or(1);
                                    for _ in 0..n { self.cursor_back(); }
                                } else if term == 'H' || term == 'f' {
                                    let parts: Vec<&str> = params.split(';').collect();
                                    let row: usize = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1usize).saturating_sub(1);
                                    let col: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1usize).saturating_sub(1);
                                    self.cursor_goto(row, col);
                                } else if term == 'K' {
                                    if params == "0" || params.is_empty() {
                                        self.cache_cur.truncate(self.cache_cursor);
                                    } else if params == "2" {
                                        self.cache_cur.clear();
                                        self.cache_cursor = 0;
                                    }
                                }
                                break;
                            }
                            params.push(chars.next().unwrap());
                        }
                    }
                    Some('O') => {
                        // SS3 — application-mode cursor keys (ESC O A/B/C/D).
                        if let Some(&c) = chars.peek() {
                            if c as u32 >= 0x40 && c as u32 <= 0x7e {
                                let term = chars.next().unwrap();
                                match term {
                                    'A' => self.cursor_up(),
                                    'B' => self.cursor_down(),
                                    'C' => self.cursor_forward(),
                                    'D' => self.cursor_back(),
                                    'H' => self.cursor_goto(0, 0),
                                    _ => {}
                                }
                            }
                        }
                    }
                    Some(']') => {
                        for c2 in &mut chars {
                            if c2 == '\x07' { break; }
                            if c2 == '\x1b' {
                                if let Some('\\') = chars.next() {}
                                break;
                            }
                        }
                    }
                    Some(c) if (c as u32) >= 0x20 && (c as u32) <= 0x2f => {
                        // Two-char sequences: ESC ( B (charset), ESC # 8, etc.
                        chars.next();
                    }
                    Some(c) if (c as u32) >= 0x30 && (c as u32) <= 0x7e => {
                        // Single-char sequences: ESC 7/8 (save/restore cursor),
                        // ESC D/E/M (vertical cursor movement).
                        match c {
                            '7' => {} // save cursor — not yet supported
                            '8' => {} // restore cursor — not yet supported
                            'D' => self.cursor_down(),
                            'M' => self.cursor_up(),
                            'E' => {
                                self.commit_line();
                                self.cursor_col = 0;
                                let next = self.cursor_row + 1;
                                self.load_line(next);
                                self.cache_cursor = 0;
                            }
                            _ => {}
                        }
                    }
                    Some(_) => {}
                    None => break,
                },
                '\r' => {
                    self.cache_cursor = 0;
                    self.cursor_col = 0;
                }
                '\n' => {
                    self.commit_line();
                    self.cursor_col = 0;
                    let next = self.cursor_row + 1;
                    self.load_line(next);
                    self.cache_cursor = 0;
                }
                '\x08' => {
                    if self.cache_cursor > 0 {
                        let w = char_width(self.cache_cur[self.cache_cursor - 1].ch);
                        self.cache_cur.remove(self.cache_cursor - 1);
                        self.cache_cursor -= 1;
                        self.cursor_col = self.cursor_col.saturating_sub(w);
                    }
                }
                '\t' => {
                    let spaces = 8 - (self.cursor_col % 8);
                    for _ in 0..spaces {
                        insert_styled(
                            &mut self.cache_cur,
                            self.cache_cursor,
                            StyledChar {
                                ch: ' ',
                                style: self.cache_style,
                            },
                        );
                        self.cache_cursor += 1;
                    }
                    self.cursor_col += spaces;
                }
                _ => {
                    if ch.is_control() {
                        continue;
                    }
                    insert_styled(
                        &mut self.cache_cur,
                        self.cache_cursor,
                        StyledChar {
                            ch,
                            style: self.cache_style,
                        },
                    );
                    self.cache_cursor += 1;
                    self.cursor_col += char_width(ch);
                }
            }
        }
    }
}

impl Drop for ShellProcess {
    /// Close the PTY, kill the child shell, and reap it so ked never
    /// leaks processes or hangs joining the reader thread.
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
            libc::close(self.slave_fd);
            libc::kill(self.pid, libc::SIGKILL);
            let mut status: libc::c_int = 0;
            for _ in 0..10 {
                if libc::waitpid(self.pid, &mut status, libc::WNOHANG) != 0 {
                    break; // reaped, or ECHILD (already reaped)
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

// ── ANSI SGR (Select Graphic Rendition) support ──────────────────
//
// We parse `ESC[<params>m` sequences and turn them into ratatui
// Styles so coloured shell output (e.g. `ls --color`, `grep --color`,
// git diff) shows up in the pop‑up shell.

/// A single character with its ANSI-derived style.
#[derive(Clone, Copy)]
struct StyledChar {
    ch: char,
    style: Style,
}

/// Map a terminal column (cells) to a char index within `chars`.
fn char_index_for_col(chars: &[StyledChar], col: usize) -> usize {
    let mut w = 0usize;
    for (i, sc) in chars.iter().enumerate() {
        if w >= col {
            return i;
        }
        w += char_width(sc.ch);
    }
    chars.len()
}

/// Insert `sc` at `pos`, overwriting if within bounds.
fn insert_styled(line: &mut Vec<StyledChar>, pos: usize, sc: StyledChar) {
    if pos < line.len() {
        line[pos] = sc;
    } else {
        line.push(sc);
    }
}

/// Collapse a line of [`StyledChar`]s into a ratatui `Line` by
/// merging consecutive spans with identical style.
fn collapse_styled(chars: Vec<StyledChar>) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let s = chars[i].style;
        let mut text = String::new();
        while i < chars.len() && chars[i].style == s {
            text.push(chars[i].ch);
            i += 1;
        }
        spans.push(Span::styled(text, s));
    }
    Line::from(spans)
}

/// Parse an ANSI SGR parameter string (the part between `ESC[` and
/// `m`) and return the resulting style.
fn parse_sgr(params: &str, default_style: Style) -> Style {
    use Color::*;
    if params.is_empty() {
        return default_style;
    }

    let mut style = default_style;
    let parts: Vec<&str> = params.split(';').collect();
    let mut i = 0;
    while i < parts.len() {
        let p: u8 = match parts[i].parse() {
            Ok(n) => n,
            Err(_) => {
                i += 1;
                continue;
            }
        };
        match p {
            0 => style = default_style,
            1 => style = style.add_modifier(Modifier::BOLD),
            3 => style = style.add_modifier(Modifier::ITALIC),
            4 => style = style.add_modifier(Modifier::UNDERLINED),
            7 => style = style.add_modifier(Modifier::REVERSED),
            22 => style = style.remove_modifier(Modifier::BOLD),
            23 => style = style.remove_modifier(Modifier::ITALIC),
            24 => style = style.remove_modifier(Modifier::UNDERLINED),
            27 => style = style.remove_modifier(Modifier::REVERSED),
            30..=37 => style = style.fg(ANSI4[p as usize - 30]),
            38 => {
                i += 1;
                if i < parts.len() {
                    match parts[i] {
                        "5" => {
                            i += 1;
                            if i < parts.len() {
                                if let Ok(n) = parts[i].parse::<u8>() {
                                    style = style.fg(color_256(n));
                                }
                            }
                        }
                        "2" => {
                            let mut rgb = [0u8; 3];
                            let mut ok = true;
                            for v in &mut rgb {
                                i += 1;
                                if i < parts.len() {
                                    if let Ok(n) = parts[i].parse::<u8>() {
                                        *v = n;
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                } else {
                                    ok = false;
                                    break;
                                }
                            }
                            if ok {
                                style = style.fg(Rgb(rgb[0], rgb[1], rgb[2]));
                            }
                        }
                        _ => {}
                    }
                }
            }
            39 => {
                if let Some(fg) = default_style.fg {
                    style = style.fg(fg);
                }
            }
            40..=47 => style = style.bg(ANSI4[p as usize - 40]),
            48 => {
                i += 1;
                if i < parts.len() {
                    match parts[i] {
                        "5" => {
                            i += 1;
                            if i < parts.len() {
                                if let Ok(n) = parts[i].parse::<u8>() {
                                    style = style.bg(color_256(n));
                                }
                            }
                        }
                        "2" => {
                            let mut rgb = [0u8; 3];
                            let mut ok = true;
                            for v in &mut rgb {
                                i += 1;
                                if i < parts.len() {
                                    if let Ok(n) = parts[i].parse::<u8>() {
                                        *v = n;
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                } else {
                                    ok = false;
                                    break;
                                }
                            }
                            if ok {
                                style = style.bg(Rgb(rgb[0], rgb[1], rgb[2]));
                            }
                        }
                        _ => {}
                    }
                }
            }
            49 => {
                if let Some(bg) = default_style.bg {
                    style = style.bg(bg);
                }
            }
            90..=97 => {
                let idx = p as usize - 90;
                style = style.fg(ANSI4[idx + 8]);
            }
            100..=107 => {
                let idx = p as usize - 100;
                style = style.bg(ANSI4[idx + 8]);
            }
            _ => {}
        }
        i += 1;
    }
    style
}

/// Map an ANSI 256‑colour index to a ratatui Color.
fn color_256(n: u8) -> Color {
    if n < 16 {
        ANSI4[n as usize].into()
    } else if n < 232 {
        let n = n - 16;
        let r = (n / 36) as u8;
        let g = ((n / 6) % 6) as u8;
        let b = (n % 6) as u8;
        let r2 = if r > 0 { r * 40 + 55 } else { 0 };
        let g2 = if g > 0 { g * 40 + 55 } else { 0 };
        let b2 = if b > 0 { b * 40 + 55 } else { 0 };
        Color::Rgb(r2, g2, b2)
    } else {
        let v = (n - 232) * 10 + 8;
        Color::Rgb(v, v, v)
    }
}

// Standard 16 ANSI colours (a pleasant Solarised‑inspired palette).
static ANSI4: [Color; 16] = [
    Color::Rgb(0x1d, 0x1f, 0x21), // black
    Color::Rgb(0xcc, 0x66, 0x66), // red
    Color::Rgb(0xb5, 0xbd, 0x68), // green
    Color::Rgb(0xf0, 0xc6, 0x74), // yellow
    Color::Rgb(0x81, 0xa2, 0xbe), // blue
    Color::Rgb(0xb2, 0x94, 0xbb), // magenta
    Color::Rgb(0x8a, 0xbe, 0xb7), // cyan
    Color::Rgb(0xc5, 0xc8, 0xc6), // white
    Color::Rgb(0x66, 0x66, 0x66), // bright black
    Color::Rgb(0xd5, 0x4e, 0x53), // bright red
    Color::Rgb(0xb9, 0xca, 0x4a), // bright green
    Color::Rgb(0xe6, 0xc5, 0x47), // bright yellow
    Color::Rgb(0x7a, 0xa6, 0xda), // bright blue
    Color::Rgb(0xc3, 0x97, 0xd8), // bright magenta
    Color::Rgb(0x70, 0xc0, 0xb1), // bright cyan
    Color::Rgb(0xea, 0xea, 0xea), // bright white
];
