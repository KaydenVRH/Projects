use clap::{Parser, Subcommand};
use libc::SIGTERM;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

mod tui;
mod wallpaper;

// ---- state ----------------------------------------------------------------

const STATE_DIR: &str = "/tmp/lwp";
const PID_PATH: &str = "/tmp/lwp/pid";
const VIDEO_PATH: &str = "/tmp/lwp/current_video.txt";
const ICONS_HIDDEN_PATH: &str = "/tmp/lwp/icons_hidden";
const HOME_CONFIG_DIR: &str = ".lwp";

static SHOULD_QUIT: AtomicBool = AtomicBool::new(false);

fn ensure_state_dir() {
    fs::create_dir_all(STATE_DIR).ok();
}

fn stop_wallpaper() {
    let p = PathBuf::from(PID_PATH);
    if let Ok(pid_str) = fs::read_to_string(&p) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            unsafe { libc::kill(pid, SIGTERM) };
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(100));
                if unsafe { libc::kill(pid, 0) } != 0 {
                    break;
                }
            }
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        fs::remove_file(&p).ok();
    }
}

fn hide_desktop_icons() {
    Command::new("defaults")
        .args(["write", "com.apple.finder", "CreateDesktop", "-bool", "false"])
        .output()
        .ok();
    Command::new("killall").arg("Finder").output().ok();
    fs::write(ICONS_HIDDEN_PATH, "1").ok();
}

fn show_desktop_icons() {
    Command::new("defaults")
        .args(["write", "com.apple.finder", "CreateDesktop", "-bool", "true"])
        .output()
        .ok();
    Command::new("killall").arg("Finder").output().ok();
    fs::remove_file(ICONS_HIDDEN_PATH).ok();
}

fn icons_hidden() -> bool {
    PathBuf::from(ICONS_HIDDEN_PATH).exists()
}

// ---- config ---------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct Config {
    last_video: String,
    hide_icons: bool,
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(HOME_CONFIG_DIR).join("config.json")
}

fn save_config(video: &str, hide: bool) {
    let p = config_path();
    fs::create_dir_all(p.parent().unwrap()).ok();
    if let Ok(json) = serde_json::to_string(&Config {
        last_video: video.to_string(),
        hide_icons: hide,
    }) {
        fs::write(&p, &json).ok();
    }
}

fn load_config() -> Option<Config> {
    let p = config_path();
    fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok())
}

// ---- commands -------------------------------------------------------------

pub fn cmd_set_inner(path: &str) {
    let abs = PathBuf::from(path);
    if !abs.is_file() {
        eprintln!("error: file not found: {}", abs.display());
        return;
    }
    ensure_state_dir();
    stop_wallpaper();
    let vp = abs.to_string_lossy().to_string();
    fs::write(VIDEO_PATH, &vp).ok();
    save_config(&vp, icons_hidden());

    let exe = std::env::current_exe().unwrap();
    let child = Command::new(&exe)
        .arg("run")
        .arg("--path")
        .arg(&vp)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to spawn wallpaper process: {e}");
            std::process::exit(1);
        });
    let pid = child.id();
    fs::write(PID_PATH, pid.to_string()).ok();
    println!("wallpaper started (pid: {pid})");
}

fn cmd_set(path: &str, hide_icons: bool) {
    let abs = PathBuf::from(path);
    let abs = if abs.is_relative() {
        std::env::current_dir().unwrap_or_default().join(&abs)
    } else {
        abs
    };
    if !abs.is_file() {
        eprintln!("error: file not found: {}", abs.display());
        std::process::exit(1);
    }
    ensure_state_dir();
    stop_wallpaper();
    if hide_icons {
        hide_desktop_icons();
    } else if icons_hidden() {
        show_desktop_icons();
    }
    let vp = abs.to_string_lossy().to_string();
    fs::write(VIDEO_PATH, &vp).ok();
    save_config(&vp, hide_icons);

    let exe = std::env::current_exe().unwrap();
    let child = Command::new(&exe)
        .arg("run")
        .arg("--path")
        .arg(&vp)
        .args(if hide_icons { vec!["--hide-icons"] } else { vec![] })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to spawn wallpaper process: {e}");
            std::process::exit(1);
        });
    let pid = child.id();
    fs::write(PID_PATH, pid.to_string()).ok();
    println!("wallpaper started (pid: {pid})");
}

pub fn cmd_stop_inner() {
    ensure_state_dir();
    stop_wallpaper();
    if icons_hidden() {
        show_desktop_icons();
    }
    println!("wallpaper stopped");
}

fn cmd_stop() {
    cmd_stop_inner();
}

fn cmd_status() {
    ensure_state_dir();
    let pid = fs::read_to_string(PID_PATH).ok();
    let video = fs::read_to_string(VIDEO_PATH).ok();
    if let Some(p) = pid {
        if let Ok(pid) = p.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                println!("running (pid: {pid})");
                if let Some(v) = video {
                    if let Some(line) = v.lines().next() {
                        println!("video: {line}");
                    }
                }
                if icons_hidden() {
                    println!("desktop icons: hidden");
                }
                return;
            }
        }
        fs::remove_file(PID_PATH).ok();
    }
    println!("not running");
}

fn cmd_run(path: &str, hide_icons: bool) {
    wallpaper::run(path, hide_icons);
}

// ---- autostart ------------------------------------------------------------

fn autostart_plist() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.lwp.wallpaper.plist")
}

pub fn cmd_autostart_on_inner() {
    let exe = std::env::current_exe().unwrap();
    let plist = autostart_plist();
    fs::create_dir_all(plist.parent().unwrap()).ok();
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.lwp.wallpaper</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>autostart-run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
</dict>
</plist>"#,
        exe = exe.display()
    );
    fs::write(&plist, &content).ok();
    let uid = unsafe { libc::getuid() };
    for cmd in [
        vec!["launchctl", "bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()],
        vec!["launchctl", "load", &plist.to_string_lossy()],
    ] {
        let r = Command::new(&cmd[0]).args(&cmd[1..]).output().ok();
        if r.as_ref().map_or(false, |o| o.status.success()) {
            break;
        }
        if r.as_ref().map_or(false, |o| {
            String::from_utf8_lossy(&o.stderr).contains("already")
        }) {
            break;
        }
    }
    println!("autostart enabled ({})", plist.display());
}

fn cmd_autostart_on() {
    cmd_autostart_on_inner();
}

pub fn cmd_autostart_off_inner() {
    let plist = autostart_plist();
    let uid = unsafe { libc::getuid() };
    Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/com.lwp.wallpaper")])
        .output()
        .ok();
    Command::new("launchctl")
        .args(["unload", &plist.to_string_lossy()])
        .output()
        .ok();
    if plist.exists() {
        fs::remove_file(&plist).ok();
        println!("autostart disabled");
    } else {
        println!("autostart not enabled");
    }
}

fn cmd_autostart_off() {
    cmd_autostart_off_inner();
}

fn cmd_autostart_status() {
    if autostart_plist().exists() {
        println!("autostart: enabled");
    } else {
        println!("autostart: disabled");
    }
}

fn cmd_autostart_run() {
    if let Some(cfg) = load_config() {
        if PathBuf::from(&cfg.last_video).is_file() {
            wallpaper::run(&cfg.last_video, cfg.hide_icons);
        }
    }
}

// ---- CLI ------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "lwp", about = "Live Wallpaper Player for macOS")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set a video as desktop wallpaper
    Set {
        path: String,
        #[arg(long)]
        hide_icons: bool,
    },
    /// Stop the current wallpaper
    Stop,
    /// Show wallpaper status
    Status,
    /// Enable login autostart
    AutostartOn,
    /// Disable login autostart
    AutostartOff,
    /// Show autostart status
    AutostartStatus,
    /// Internal: run the wallpaper worker
    #[command(hide = true)]
    Run {
        #[arg(long)]
        path: String,
        #[arg(long)]
        hide_icons: bool,
    },
    /// Internal: run from autostart
    #[command(hide = true)]
    AutostartRun,
    /// Interactive TUI
    #[command(name = "interactive", aliases = ["tui", "i"])]
    Interactive,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Set { path, hide_icons } => cmd_set(&path, hide_icons),
        Commands::Stop => cmd_stop(),
        Commands::Status => cmd_status(),
        Commands::AutostartOn => cmd_autostart_on(),
        Commands::AutostartOff => cmd_autostart_off(),
        Commands::AutostartStatus => cmd_autostart_status(),
        Commands::AutostartRun => cmd_autostart_run(),
        Commands::Run { path, hide_icons } => cmd_run(&path, hide_icons),
        Commands::Interactive => tui::run(),
    }
}
