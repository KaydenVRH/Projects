use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use vt100::{Color as VtColor, Parser};

/// A real PTY-backed terminal.
///
/// The screen is parsed by [`vt100`], so full-screen programs (htop,
/// vim, less, fzf, …) work: cursor addressing, erase/scroll regions,
/// insert/delete line, the alternate screen, colours and attributes are
/// all handled by the parser instead of the old line-oriented
/// approximation.
pub struct ShellProcess {
    fd: i32,
    slave_fd: i32,
    pid: i32,
    rx: mpsc::Receiver<Vec<u8>>,
    alive: bool,
    parser: Parser,
    rows: u16,
    cols: u16,
}

impl ShellProcess {
    pub fn spawn() -> io::Result<(Self, std::thread::JoinHandle<()>)> {
        unsafe {
            let mut master: i32 = 0;
            let mut slave: i32 = 0;
            let mut slave_name: [libc::c_char; 256] = [0; 256];

            // Use system-default terminal settings — don't inherit
            // the raw-mode termios from crossterm's STDIN.
            let ret = libc::openpty(
                &mut master,
                &mut slave,
                slave_name.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
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

                // TIOCSCTTY is c_ulong on macOS, u64 on glibc Linux and
                // c_int on some targets — let inference pick the ioctl
                // request type.
                libc::ioctl(
                    slave,
                    libc::TIOCSCTTY as _,
                    std::ptr::null_mut::<libc::c_int>(),
                );

                let mut t: libc::termios = std::mem::zeroed();
                libc::tcgetattr(slave, &mut t);
                // Standard c_cc control characters so line editing works.
                t.c_cc[libc::VERASE] = 0x7f; // DEL (xterm backspace)
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
                if slave > 2 {
                    libc::close(slave);
                }

                // TERM/COLORTERM let TUI programs pick the right
                // capabilities (including 24-bit colour).
                for (k, v) in [
                    ("TERM", "xterm-256color"),
                    ("COLORTERM", "truecolor"),
                ] {
                    let key = std::ffi::CString::new(k).unwrap();
                    let val = std::ffi::CString::new(v).unwrap();
                    libc::setenv(key.as_ptr(), val.as_ptr(), 1);
                }

                let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
                let c_shell = std::ffi::CString::new(shell).unwrap();
                let args: [*const libc::c_char; 2] = [c_shell.as_ptr(), std::ptr::null_mut()];
                libc::execvp(c_shell.as_ptr(), args.as_ptr());
                libc::_exit(1);
            }

            let slave_fd = slave;

            let (tx, rx) = mpsc::channel();
            let alive = std::sync::Arc::new(AtomicBool::new(true));
            let alive_clone = alive.clone();

            let reader = std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
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
                        continue;
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

            Ok((
                ShellProcess {
                    fd: master,
                    slave_fd,
                    pid,
                    rx,
                    alive: true,
                    parser: Parser::new(24, 80, 2000),
                    rows: 24,
                    cols: 80,
                },
                reader,
            ))
        }
    }

    /// Make sure the slave side doesn't echo characters twice (the
    /// shell / readline echoes them itself).
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

    /// Resize both the emulated screen and the kernel PTY.  Changing
    /// `TIOCSWINSZ` also sends `SIGWINCH` to the child.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if rows == self.rows && cols == self.cols {
            return;
        }
        self.rows = rows;
        self.cols = cols;
        self.parser.set_size(rows, cols);
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
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
                break;
            }
            self.parser.process(&data);
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Cursor position + visibility from the application.
    pub fn cursor(&self) -> Option<(u16, u16)> {
        let screen = self.parser.screen();
        if screen.hide_cursor() {
            None
        } else {
            Some(screen.cursor_position())
        }
    }

    /// Does the application want application-cursor-key mode?  Affects
    /// how arrow/Home/End keys must be encoded.
    pub fn application_cursor(&self) -> bool {
        self.parser.screen().application_cursor()
    }

    /// Has the application enabled bracketed paste?
    pub fn bracketed_paste(&self) -> bool {
        self.parser.screen().bracketed_paste()
    }

    /// Render the visible screen into styled ratatui lines.
    pub fn styled_lines(&self, default_fg: Color, default_bg: Color) -> Vec<Line<'static>> {
        screen_lines(self.parser.screen(), default_fg, default_bg)
    }
}

/// Convert a parsed terminal screen into ratatui lines.
pub fn screen_lines(
    screen: &vt100::Screen,
    default_fg: Color,
    default_bg: Color,
) -> Vec<Line<'static>> {
    let (rows, cols) = screen.size();
    let mut lines = Vec::with_capacity(rows as usize);
    for row in 0..rows {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut text = String::new();
        let mut style = Style::new();
        let mut col = 0;
        while col < cols {
            let Some(cell) = screen.cell(row, col) else {
                col += 1;
                continue;
            };
            if cell.is_wide_continuation() {
                col += 1;
                continue;
            }
            let mut contents = cell.contents();
            if contents.is_empty() {
                contents.push(' ');
            }
            let cell_style = cell_style(cell, default_fg, default_bg);
            if cell_style != style {
                if !text.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut text), style));
                }
                style = cell_style;
            }
            text.push_str(&contents);
            col += 1;
        }
        if !text.is_empty() {
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// Convert a vt100 cell's colours/attributes to a ratatui style.
fn cell_style(cell: &vt100::Cell, default_fg: Color, default_bg: Color) -> Style {
    let bold = cell.bold();
    let mut fg = vt_color(cell.fgcolor(), default_fg, bold);
    let mut bg = vt_color(cell.bgcolor(), default_bg, false);
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    let mut style = Style::new().fg(fg).bg(bg);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

fn vt_color(c: VtColor, default: Color, bold: bool) -> Color {
    match c {
        VtColor::Default => default,
        VtColor::Idx(n) => {
            // Bold + a normal ANSI colour is the classic "bright" colour.
            let n = if bold && n < 8 { n + 8 } else { n };
            color_256(n)
        }
        VtColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
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
// Used by the markdown preview's `glow` output.  The PTY shell itself
// is parsed by vt100 (above).

/// One-shot conversion of ANSI-coloured text into styled ratatui
/// lines (used for the `glow` markdown preview).  Colour/attribute
/// sequences are applied; other escape sequences are skipped.
pub(crate) fn ansi_to_lines(text: &str, default_style: Style) -> Vec<Line<'static>> {
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    let mut style = default_style;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    while let Some(&c2) = chars.peek() {
                        if ('\u{40}'..='\u{7e}').contains(&c2) {
                            let term = chars.next().unwrap();
                            if term == 'm' {
                                style = parse_sgr(&params, default_style);
                            }
                            break;
                        }
                        params.push(chars.next().unwrap());
                    }
                }
                Some(']') => {
                    // OSC — skip to BEL or ST.
                    for c2 in &mut chars {
                        if c2 == '\x07' {
                            break;
                        }
                        if c2 == '\x1b' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' => lines.push(Vec::new()),
            '\r' => {}
            c if c.is_control() => {}
            c => {
                let line = lines.last_mut().unwrap();
                match line.last_mut() {
                    // Merge adjacent same-styled chars into one span.
                    Some(last) if last.style == style => last.content.to_mut().push(c),
                    _ => line.push(Span::styled(c.to_string(), style)),
                }
            }
        }
    }
    lines.into_iter().map(Line::from).collect()
}

/// Parse an ANSI SGR parameter string (the part between `ESC[` and
/// `m`) and return the resulting style.
pub(crate) fn parse_sgr(params: &str, default_style: Style) -> Style {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    fn text(parser: &Parser) -> Vec<String> {
        screen_lines(parser.screen(), Color::White, Color::Black)
            .iter()
            .map(|l| line_text(l).trim_end().to_string())
            .collect()
    }

    #[test]
    fn cursor_addressing_overwrites_cells() {
        let mut p = Parser::new(3, 12, 0);
        p.process(b"hello");
        p.process(b"\x1b[1;1Habc");
        assert_eq!(text(&p)[0], "abclo");
        p.process(b"\x1b[2J\x1b[H");
        assert!(text(&p).iter().all(|l| l.trim().is_empty()));
    }

    #[test]
    fn alternate_screen_switches() {
        let mut p = Parser::new(3, 10, 0);
        p.process(b"main");
        p.process(b"\x1b[?1049h");
        p.process(b"alt");
        assert_eq!(text(&p)[0], "alt");
        p.process(b"\x1b[?1049l");
        assert_eq!(text(&p)[0], "main");
    }

    #[test]
    fn colours_and_attributes_are_kept() {
        let mut p = Parser::new(2, 10, 0);
        p.process(b"\x1b[1;31mred\x1b[0m");
        let lines = screen_lines(p.screen(), Color::White, Color::Black);
        let first = &lines[0].spans[0];
        assert_eq!(first.content.as_ref(), "red");
        assert_eq!(first.style.fg, Some(ANSI4[9])); // bold brings out bright red
        assert!(first.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn wide_chars_are_not_split() {
        let mut p = Parser::new(2, 10, 0);
        p.process("日x".as_bytes());
        assert!(text(&p)[0].starts_with("日x"));
    }

    #[test]
    fn cursor_visibility_is_reported() {
        let mut p = Parser::new(2, 10, 0);
        assert!(!p.screen().hide_cursor());
        p.process(b"\x1b[?25l");
        assert!(p.screen().hide_cursor());
        p.process(b"\x1b[?25h");
        assert!(!p.screen().hide_cursor());
    }
}
