use std::io;
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub struct ShellProcess {
    fd: i32,
    slave_fd: i32,
    pid: i32,
    rx: mpsc::Receiver<Vec<u8>>,
    alive: bool,
    output: Vec<u8>,

    // Incremental-processing state so we don't re-process the
    // entire buffer on every frame.
    processed_up_to: usize,       // bytes of `output` already consumed
    cache_lines: Vec<Line<'static>>, // styled output lines
    cache_cur: Vec<StyledChar>,   // current (incomplete) line
    cache_cursor: usize,          // cursor position within cache_cur
    cache_style: Style,           // current SGR style
    cache_default: Style,         // default_style from last call
    cache_out: Vec<Line<'static>>, // flattened result (cache_lines + cur line)
}

impl ShellProcess {
    pub fn spawn() -> io::Result<(Self, std::thread::JoinHandle<()>)> {
        unsafe {
            let mut master: i32 = 0;
            let mut slave: i32 = 0;
            let mut slave_name: [libc::c_char; 256] = [0; 256];

            let mut t: libc::termios = std::mem::zeroed();
            libc::tcgetattr(libc::STDIN_FILENO, &mut t);
            t.c_iflag &= !(libc::IGNBRK | libc::BRKINT | libc::PARMRK
                | libc::ISTRIP | libc::INLCR | libc::IGNCR
                | libc::ICRNL | libc::IXON);
            t.c_oflag &= !libc::OPOST;
            t.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON
                | libc::IEXTEN);
            t.c_lflag |= libc::ISIG;
            t.c_cflag &= !(libc::CSIZE | libc::PARENB);
            t.c_cflag |= libc::CS8;
            t.c_cc[libc::VMIN] = 1;
            t.c_cc[libc::VTIME] = 0;

            let ret = libc::openpty(&mut master, &mut slave,
                slave_name.as_mut_ptr(),
                &mut t,
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
                t.c_iflag &= !(libc::IGNBRK | libc::BRKINT | libc::PARMRK
                    | libc::ISTRIP | libc::INLCR | libc::IGNCR
                    | libc::ICRNL | libc::IXON);
                t.c_oflag &= !libc::OPOST;
                t.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON
                    | libc::IEXTEN);
                t.c_lflag |= libc::ISIG;
                t.c_cflag &= !(libc::CSIZE | libc::PARENB);
                t.c_cflag |= libc::CS8;
                t.c_cc[libc::VMIN] = 1;
                t.c_cc[libc::VTIME] = 0;
                libc::tcsetattr(slave, libc::TCSANOW, &t);

                libc::dup2(slave, 0);
                libc::dup2(slave, 1);
                libc::dup2(slave, 2);
                if slave > 2 { libc::close(slave); }

                let shell = std::env::var("SHELL")
                    .unwrap_or_else(|_| "bash".to_string());
                let c_shell = std::ffi::CString::new(shell).unwrap();
                let args: [*const libc::c_char; 2] = [
                    c_shell.as_ptr(),
                    std::ptr::null(),
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

        // Flatten completed lines + the incomplete current line.
        self.cache_out.clear();
        self.cache_out.extend(self.cache_lines.iter().cloned());
        if !self.cache_cur.is_empty() {
            self.cache_out.push(collapse_styled(self.cache_cur.clone()));
        }
        &self.cache_out
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
                                }
                                break;
                            }
                            params.push(chars.next().unwrap());
                        }
                    }
                    Some(']') => {
                        for c2 in &mut chars {
                            if c2 == '\x07' {
                                break;
                            }
                            if c2 == '\x1b' {
                                if let Some('\\') = chars.next() {}
                                break;
                            }
                        }
                    }
                    Some(c) if c as u32 >= 0x40 && c as u32 <= 0x7e => {}
                    Some(_) => {}
                    None => break,
                },
                '\r' => {
                    self.cache_cursor = 0;
                }
                '\n' => {
                    let line = std::mem::take(&mut self.cache_cur);
                    self.cache_lines
                        .push(collapse_styled(line));
                    self.cache_cursor = 0;
                }
                '\x08' => {
                    if self.cache_cursor > 0 {
                        self.cache_cur.remove(self.cache_cursor - 1);
                        self.cache_cursor -= 1;
                    }
                }
                '\t' => {
                    let spaces = 8 - (self.cache_cursor % 8);
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
                }
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
