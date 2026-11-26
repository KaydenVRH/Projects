//! yt-tui — search and watch YouTube in the terminal.

mod app;
mod audio;
mod config;
mod gfx;
mod log;
mod player;
mod thumbs;
mod ui;
mod video;
mod youtube;

use anyhow::{bail, Result};
use app::{App, Settings};
use crossterm::{
    cursor,
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use gfx::{Graphics, THUMB_ID, VIDEO_ID};
use player::Limits;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

const USAGE: &str = "\
yt-tui — search and watch YouTube in the terminal

usage: yt-tui [options] [search terms | youtube link]

options:
  --fps N          highest frame rate to play (default 30)
  --quality H      highest stream height, e.g. 720 or 1080 (default 1080)
  --max-width W    widest frame to decode before kitty scales it (default 1920)
  --results N      search results to fetch (default 30)
  --volume N       starting volume, 0-100 (default 80)
  -h, --help       show this

config file: ~/.config/yt-tui/config, e.g.
  cookies_from_browser = firefox    your login: Home, Subscriptions, …
  fps = 30 / quality = 1080 / max_width = 1920 / results = 30 / volume = 80

environment:
  YT_TUI_GRAPHICS=on|off      force kitty graphics on or off
  YT_TUI_TRANSFER=shm|direct  how pixels reach the terminal
  YT_TUI_PLAYER_CLIENT=name   yt-dlp player client (default: yt-dlp's own)
  YT_TUI_COOKIES_FROM_BROWSER=firefox
                              use a browser's YouTube login (for bot checks)
  YT_TUI_YTDLP_ARGS=args      extra arguments for every yt-dlp call
";

fn main() -> Result<()> {
    let (settings, initial) = parse_args()?;
    let gfx = Graphics::detect();
    log!("start: graphics {:?}, cell {:?}", gfx.as_ref().map(|g| g.transfer), app::cell_size());

    let mut terminal = setup()?;
    // Put the terminal back even if something panics.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        default_hook(info);
    }));

    let mut app = App::new(gfx, settings);
    if let Some(q) = initial {
        app.query = q.clone();
        app.search(q);
    } else if app.logged_in {
        app.focus = app::Focus::Results;
        app.open(app::Source::Home);
    }
    let result = run(&mut terminal, &mut app);

    // Free the pictures and the processes before handing the screen back.
    let out = terminal.backend_mut();
    let _ = app.stop(out);
    if let Some(gfx) = &mut app.gfx {
        let _ = gfx.delete(out, VIDEO_ID);
        let _ = gfx.delete(out, THUMB_ID);
        gfx.cleanup();
    }
    restore()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    let mut dirty = true;
    while !app.quit {
        dirty |= app.update(terminal.backend_mut())?;

        // Redraw on change, and steadily while anything moves (spinners,
        // the progress bar) — but never per video frame: frames replace
        // the picture's pixels, and the cells around it don't change.
        let animating = app.player.is_some() || app.busy() || app.status.is_some();
        let interval = if animating { Duration::from_millis(100) } else { Duration::from_secs(1) };
        if dirty || last_draw.elapsed() >= interval {
            terminal.draw(|f| ui::render(f, app))?;
            app.after_draw(terminal.backend_mut())?;
            last_draw = Instant::now();
            dirty = false;
        }

        let wait = match &app.player {
            Some(p) => p.next_deadline(),
            None if app.busy() => Duration::from_millis(50),
            None => Duration::from_millis(250),
        };
        if event::poll(wait)? {
            // Drain everything queued so held keys don't lag behind.
            loop {
                let ev = event::read()?;
                app.handle(ev, terminal.backend_mut())?;
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
            dirty = true;
        }
    }
    Ok(())
}

fn setup() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, cursor::Hide)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore() -> Result<()> {
    terminal::disable_raw_mode()?;
    execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen, cursor::Show)?;
    Ok(())
}

fn parse_args() -> Result<(Settings, Option<String>)> {
    let file = config::load();
    // The login for yt-dlp; the environment variable wins over the file.
    if std::env::var_os("YT_TUI_COOKIES_FROM_BROWSER").is_none() {
        if let Some(browser) = &file.cookies_from_browser {
            std::env::set_var("YT_TUI_COOKIES_FROM_BROWSER", browser);
        }
    }
    let mut settings = Settings {
        max_height: file.quality.unwrap_or(1080),
        limits: Limits {
            max_width: file.max_width.unwrap_or(1920),
            max_fps: file.fps.unwrap_or(30),
        },
        results: file.results.unwrap_or(30) as usize,
        volume: file.volume.unwrap_or(80.0).clamp(0.0, 100.0),
    };
    let mut words = Vec::new();
    let mut args = std::env::args().skip(1);
    let number =|name: &str, v: Option<String>| -> Result<u32> {
        match v.as_deref().map(str::parse::<u32>) {
            Some(Ok(n)) if n > 0 => Ok(n),
            _ => bail!("{name} needs a positive number"),
        }
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--fps" => settings.limits.max_fps = number("--fps", args.next())?,
            "--quality" => settings.max_height = number("--quality", args.next())?,
            "--max-width" => settings.limits.max_width = number("--max-width", args.next())?,
            "--results" => settings.results = number("--results", args.next())? as usize,
            "--volume" => match args.next().as_deref().map(str::parse::<f64>) {
                Some(Ok(v)) if (0.0..=100.0).contains(&v) => settings.volume = v,
                _ => bail!("--volume needs a number from 0 to 100"),
            },
            s if s.starts_with("--") => bail!("unknown option {s} (see --help)"),
            _ => words.push(arg),
        }
    }
    let initial = (!words.is_empty()).then(|| words.join(" "));
    Ok((settings, initial))
}
