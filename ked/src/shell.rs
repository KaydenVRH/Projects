use std::io;
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct ShellProcess {
    fd: i32,
    /// Original slave fd from openpty — kept open so we can call
    /// tcsetattr directly (more reliable than reopening the device).
    slave_fd: i32,
    pid: i32,
    rx: mpsc::Receiver<Vec<u8>>,
    alive: bool,
    /// Raw bytes from the shell (kept as bytes to preserve UTF-8).
    output: Vec<u8>,
    /// ANSI escape sequence state machine.
    esc_state: EscState,
}

/// Simple ANSI escape sequence state machine.
enum EscState {
    Normal,
    EscSeen,   // just saw 0x1b
    Csi,       // ESC [ … collecting until final byte
    Osc,       // ESC ] … collecting until BEL or ST
}

impl ShellProcess {
    pub fn spawn() -> io::Result<(Self, std::thread::JoinHandle<()>)> {
        unsafe {
            let mut master: i32 = 0;
            let mut slave: i32 = 0;
            let mut slave_name: [libc::c_char; 256] = [0; 256];

            // Seed the PTY slave with a raw-mode termios (ECHO off,
            // ICANON off, ISIG on) so the terminal driver never
            // echoes typed characters before the shell initialises.
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
                // ── child ──
                libc::close(master);
                libc::setsid();

                // Make the PTY slave the controlling terminal, so
                // Ctrl+C etc generate signals.
                libc::ioctl(slave, libc::TIOCSCTTY as libc::c_ulong, std::ptr::null_mut::<libc::c_int>());

                // Hard raw mode (ECHO off + ICANON off).  The terminal
                // driver will NOT echo; only readline / zle will echo
                // (once per char).
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

                // Keep TERM normal so Starship and other prompt
                // tools work correctly.
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

            // ── parent ──
            // Keep the slave fd open so we can call tcsetattr on it
            // directly.  Reopening the device path can fail or have
            // side effects when we aren't the controlling process.
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
                esc_state: EscState::Normal,
            }, reader))
        }
    }

    /// Force the slave to raw mode (ECHO off).  Uses the original
    /// slave fd from `openpty` so `tcsetattr` is applied on a fd
    /// that the kernel definitely recognises as the slave device.
    /// Verifies the change and retries with `TCSADRAIN` if needed.
    pub fn force_raw_mode(&self) {
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(self.slave_fd, &mut t) != 0 {
                return;
            }
            if t.c_lflag & libc::ECHO == 0 {
                return; // already off
            }
            t.c_lflag &= !libc::ECHO;
            libc::tcsetattr(self.slave_fd, libc::TCSANOW, &t);
            // Verify — if ECHO is still on, retry with TCSADRAIN.
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
            for &b in &data {
                self.feed_byte(b);
            }
        }
    }

    fn feed_byte(&mut self, b: u8) {
        match self.esc_state {
            EscState::Normal => {
                match b {
                    0x1b => self.esc_state = EscState::EscSeen,
                    b'\n' => self.output.push(b'\n'),
                    b'\t' => self.output.push(b'\t'),
                    b'\r' => {},
                    0x20..=0x7E => self.output.push(b),
                    _ if b >= 0xa0 => self.output.push(b),
                    _ => {},
                }
            }
            EscState::EscSeen => {
                match b {
                    b'[' => self.esc_state = EscState::Csi,
                    b']' => self.esc_state = EscState::Osc,
                    0x40..=0x7E => self.esc_state = EscState::Normal,
                    0x20..=0x2F => {},
                    _ => self.esc_state = EscState::Normal,
                }
            }
            EscState::Csi => {
                if (0x40..=0x7E).contains(&b) {
                    self.esc_state = EscState::Normal;
                }
            }
            EscState::Osc => {
                if b == 0x07 {
                    self.esc_state = EscState::Normal;
                } else if b == 0x1b {
                    self.esc_state = EscState::EscSeen;
                }
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    pub fn output(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

impl Drop for ShellProcess {
    fn drop(&mut self) {
        unsafe {
            libc::kill(self.pid, libc::SIGTERM);
            libc::close(self.fd);
            libc::close(self.slave_fd);
        }
    }
}
