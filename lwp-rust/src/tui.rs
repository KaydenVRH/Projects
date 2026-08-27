use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};

const WALLPAPER_DIR: &str = "Documents/livewalls";

fn wallpaper_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(WALLPAPER_DIR)
}

fn scan_wallpapers() -> Vec<PathBuf> {
    let dir = wallpaper_dir();
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension() {
                    match ext.to_str().unwrap_or("") {
                        "mp4" | "mov" | "m4v" | "webm" => files.push(p),
                        _ => {}
                    }
                }
            }
        }
    }
    files.sort_by_key(|p| p.file_name().unwrap_or_default().to_os_string());
    files
}

fn current_status_lines() -> Vec<String> {
    let pid = fs::read_to_string("/tmp/lwp/pid").ok();
    let video = fs::read_to_string("/tmp/lwp/current_video.txt").ok();
    let icons_hidden = PathBuf::from("/tmp/lwp/icons_hidden").exists();
    let autostart = autostart_plist().exists();
    let mut lines = Vec::new();
    if let Some(ref p) = pid {
        if let Ok(pid) = p.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                lines.push(format!("  Status:     Running  (pid: {pid})"));
                if let Some(ref v) = video {
                    if let Some(line) = v.lines().next() {
                        lines.push(format!("  Video:      {line}"));
                    }
                }
            } else {
                lines.push("  Status:     Stopped".to_string());
            }
        }
    } else {
        lines.push("  Status:     Stopped".to_string());
    }
    lines.push(format!(
        "  Desktop:    {}",
        if icons_hidden { "Icons hidden" } else { "Icons visible" }
    ));
    lines.push(format!(
        "  Autostart:  {}",
        if autostart { "Enabled" } else { "Disabled" }
    ));
    lines
}

fn autostart_plist() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.lwp.wallpaper.plist")
}

pub fn run() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut selected: usize = 0;
    let mut scroll: usize = 0;
    let mut visible_rows: usize = 10;
    let mut wallpapers = scan_wallpapers();
    let mut message: Option<String> = Some(format!(
        "{} wallpapers loaded from {}",
        wallpapers.len(),
        wallpaper_dir().display()
    ));
    let mut message_ticks: u32 = 0;

    loop {
        terminal.draw(|f| {
            ui(
                f,
                &selected,
                &mut scroll,
                &mut visible_rows,
                &wallpapers,
                message.as_deref(),
                message_ticks > 0,
            )
        })?;

        if let Ok(true) = event::poll(Duration::from_millis(100)) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                message_ticks = 0;
                message = None;

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('j') | KeyCode::Down => {
                        if !wallpapers.is_empty() {
                            selected = (selected + 1).min(wallpapers.len() - 1);
                            if selected >= scroll.saturating_add(visible_rows) {
                                scroll = selected.saturating_sub(visible_rows) + 1;
                            }
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                        if selected < scroll {
                            scroll = selected;
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(path) = wallpapers.get(selected) {
                            message = Some(crate::cmd_set_inner(&path.to_string_lossy()));
                            message_ticks = 40;
                        }
                    }
                    KeyCode::Char('s') => {
                        message = Some(crate::cmd_stop_inner());
                        message_ticks = 30;
                    }
                    KeyCode::Char('a') => {
                        if autostart_plist().exists() {
                            message = Some(crate::cmd_autostart_off_inner());
                        } else {
                            message = Some(crate::cmd_autostart_on_inner());
                        }
                        message_ticks = 30;
                    }
                    KeyCode::Char('r') => {
                        wallpapers = scan_wallpapers();
                        selected = 0;
                        scroll = 0;
                        message = Some(format!("{} wallpapers loaded", wallpapers.len()));
                        message_ticks = 40;
                    }
                    _ => {}
                }
            }
        }

        if message_ticks > 0 {
            message_ticks -= 1;
            if message_ticks == 0 {
                message = None;
            }
        }
    }
    Ok(())
}

fn ui(
    f: &mut Frame,
    selected: &usize,
    scroll: &mut usize,
    visible_rows: &mut usize,
    wallpapers: &[PathBuf],
    message: Option<&str>,
    message_active: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(f.area());

    let status_lines: Vec<Line> = current_status_lines()
        .into_iter()
        .map(|s| Line::from(Span::raw(s)))
        .collect();
    let status = Paragraph::new(Text::from(status_lines))
        .block(Block::default().title(" Status ").borders(Borders::ALL));
    f.render_widget(status, chunks[0]);

    let msg_text = if message_active {
        Line::from(Span::styled(
            format!("  {}", message.unwrap_or("")),
            Style::default().fg(Color::Green),
        ))
    } else {
        Line::from(Span::styled(
            "  Press ? for help",
            Style::default().fg(Color::Gray),
        ))
    };
    f.render_widget(Paragraph::new(Text::from(msg_text)), chunks[1]);

    let list_inner = chunks[2].height.saturating_sub(2) as usize;
    *visible_rows = list_inner.max(1);

    if !wallpapers.is_empty() {
        if *scroll + list_inner > wallpapers.len() {
            *scroll = wallpapers.len().saturating_sub(list_inner);
        }
        if *selected < *scroll {
            *scroll = *selected;
        }
        if *selected >= scroll.saturating_add(list_inner) {
            *scroll = *selected - list_inner + 1;
        }
    }

    let end = (*scroll + list_inner).min(wallpapers.len());
    let visible = &wallpapers[*scroll..end];

    let items: Vec<ListItem> = if wallpapers.is_empty() {
        vec![ListItem::new(Span::styled(
            format!("  No videos found in {}", wallpaper_dir().display()),
            Style::default().fg(Color::Yellow),
        ))]
    } else {
        visible
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let real = *scroll + i;
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if real == *selected {
                    ListItem::new(Span::styled(
                        format!("  {name}"),
                        Style::default().fg(Color::Black).bg(Color::Cyan),
                    ))
                } else {
                    ListItem::new(Span::styled(format!("  {name}"), Style::default()))
                }
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(format!(" Wallpapers ({}) ", wallpapers.len()))
            .borders(Borders::ALL),
    );
    f.render_widget(list, chunks[2]);

    let help_text = Line::from(Span::raw(
        "  \u{2191}\u{2193}/jk: Navigate  Enter: Set  s: Stop  a: Toggle Autostart  r: Refresh  q: Quit",
    ));
    let help = Paragraph::new(Text::from(help_text))
        .block(Block::default().title(" Shortcuts ").borders(Borders::ALL));
    f.render_widget(help, chunks[3]);
}
