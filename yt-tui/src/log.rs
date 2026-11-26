//! Opt-in debug log: `YT_TUI_LOG=/tmp/yt-tui.log yt-tui`. The screen
//! belongs to the interface, so diagnostics go to a file.

use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static LOG: OnceLock<Option<(Mutex<std::fs::File>, Instant)>> = OnceLock::new();

pub fn write(args: std::fmt::Arguments) {
    let log = LOG.get_or_init(|| {
        let path = std::env::var_os("YT_TUI_LOG")?;
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
        Some((Mutex::new(file), Instant::now()))
    });
    if let Some((file, start)) = log {
        let _ = writeln!(file.lock().unwrap(), "{:8.3} {}", start.elapsed().as_secs_f64(), args);
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(format_args!($($arg)*)) };
}
