use std::fs;
use std::path::PathBuf;

use dialoguer::{theme::ColorfulTheme, FuzzySelect, Select};
use libc;

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

fn current_status() -> String {
    let pid = fs::read_to_string("/tmp/lwp/pid").ok();
    let video = fs::read_to_string("/tmp/lwp/current_video.txt").ok();
    let icons_hidden = PathBuf::from("/tmp/lwp/icons_hidden").exists();
    let autostart = autostart_plist().exists();
    let mut lines = Vec::new();
    if let Some(p) = pid {
        if let Ok(pid) = p.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                lines.push(format!("  running (pid: {pid})"));
                if let Some(v) = video {
                    if let Some(line) = v.lines().next() {
                        lines.push(format!("  video: {line}"));
                    }
                }
            } else {
                lines.push("  not running".to_string());
            }
        }
    } else {
        lines.push("  not running".to_string());
    }
    if icons_hidden {
        lines.push("  desktop icons: hidden".to_string());
    }
    lines.push(format!(
        "  autostart: {}",
        if autostart { "enabled" } else { "disabled" }
    ));
    lines.join("\n")
}

fn autostart_plist() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.lwp.wallpaper.plist")
}

pub fn run() {
    let theme = ColorfulTheme::default();
    loop {
        let choices = &[
            "Select Wallpaper",
            "Status",
            "Stop",
            "Toggle Autostart",
            "Exit",
        ];
        let selection = Select::with_theme(&theme)
            .with_prompt("Live Wallpaper")
            .default(0)
            .items(choices)
            .interact()
            .unwrap();

        match selection {
            0 => {
                let videos = scan_wallpapers();
                if videos.is_empty() {
                    println!("no wallpapers found in {}", wallpaper_dir().display());
                    continue;
                }
                let names: Vec<String> = videos
                    .iter()
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                let pick = FuzzySelect::with_theme(&theme)
                    .with_prompt("Choose a wallpaper (type to filter)")
                    .default(0)
                    .items(&names)
                    .interact()
                    .unwrap();
                let path = videos[pick].to_string_lossy().to_string();
                cmd_set_from_tui(&path);
            }
            1 => {
                println!("{}", current_status());
            }
            2 => {
                cmd_stop_from_tui();
            }
            3 => {
                if autostart_plist().exists() {
                    cmd_autostart_off_from_tui();
                } else {
                    cmd_autostart_on_from_tui();
                }
            }
            4 => break,
            _ => break,
        }

        if selection != 1 {
            println!();
        }
    }
}

fn cmd_set_from_tui(path: &str) {
    let abs = std::path::PathBuf::from(path);
    if !abs.is_file() {
        eprintln!("error: file not found: {path}");
        return;
    }
    let vp = abs.to_string_lossy().to_string();
    crate::cmd_set_inner(&vp);
}

fn cmd_stop_from_tui() {
    crate::cmd_stop_inner();
}

fn cmd_autostart_on_from_tui() {
    crate::cmd_autostart_on_inner();
}

fn cmd_autostart_off_from_tui() {
    crate::cmd_autostart_off_inner();
}
