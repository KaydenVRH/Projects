use clap::{Parser, Subcommand};
use libc::SIGTERM;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

mod tui;
mod shader;
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

/// Stdio pair for the detached worker, appending to `/tmp/lwp/lwp.log`.
///
/// The worker owns the only thread that can report a playback failure (bad
/// codec, denied screen access), and its stdio used to be discarded, so a
/// failure was silent.  Falling back to `null` keeps `set` working when the
/// log cannot be opened.
fn worker_log() -> (Stdio, Stdio) {
    let path = format!("{STATE_DIR}/lwp.log");
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => match f.try_clone() {
            Ok(f2) => (Stdio::from(f), Stdio::from(f2)),
            Err(_) => (Stdio::null(), Stdio::null()),
        },
        Err(_) => (Stdio::null(), Stdio::null()),
    }
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
    /// "video" (default), "shader" or "builtin"
    #[serde(default)]
    last_kind: Option<String>,
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(HOME_CONFIG_DIR).join("config.json")
}

fn save_config(video: &str, hide: bool, kind: Option<&str>) {
    let p = config_path();
    fs::create_dir_all(p.parent().unwrap()).ok();
    if let Ok(json) = serde_json::to_string(&Config {
        last_video: video.to_string(),
        hide_icons: hide,
        last_kind: kind.map(|k| k.to_string()),
    }) {
        fs::write(&p, &json).ok();
    }
}

fn load_config() -> Option<Config> {
    let p = config_path();
    fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok())
}

// ---- commands -------------------------------------------------------------

pub fn cmd_set_inner(path: &str) -> String {
    let abs = PathBuf::from(path);
    if !abs.is_file() {
        return format!("error: file not found: {}", abs.display());
    }
    ensure_state_dir();
    stop_wallpaper();
    let vp = abs.to_string_lossy().to_string();
    fs::write(VIDEO_PATH, &vp).ok();
    save_config(&vp, icons_hidden(), None);

    let exe = std::env::current_exe().unwrap();
    let (log_out, log_err) = worker_log();
    let child = Command::new(&exe)
        .arg("run")
        .arg("--path")
        .arg(&vp)
        .stdout(log_out)
        .stderr(log_err)
        .stdin(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to spawn wallpaper process: {e}");
            std::process::exit(1);
        });
    let pid = child.id();
    fs::write(PID_PATH, pid.to_string()).ok();
    format!("wallpaper started (pid: {pid})")
}

fn spawn_worker(args: &[String], label: &str) -> String {
    let exe = std::env::current_exe().unwrap();
    let (log_out, log_err) = worker_log();
    let mut cmd = Command::new(&exe);
    for a in args {
        cmd.arg(a);
    }
    let child = cmd
        .stdout(log_out)
        .stderr(log_err)
        .stdin(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to spawn wallpaper process: {e}");
            std::process::exit(1);
        });
    let pid = child.id();
    fs::write(PID_PATH, pid.to_string()).ok();
    format!("{label} started (pid: {pid})")
}

fn resolve(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_relative() {
        std::env::current_dir().unwrap_or_default().join(&p)
    } else {
        p
    }
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
    save_config(&vp, hide_icons, None);

    let exe = std::env::current_exe().unwrap();
    let (log_out, log_err) = worker_log();
    let child = Command::new(&exe)
        .arg("run")
        .arg("--path")
        .arg(&vp)
        .args(if hide_icons { vec!["--hide-icons"] } else { vec![] })
        .stdout(log_out)
        .stderr(log_err)
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

pub fn cmd_stop_inner() -> String {
    ensure_state_dir();
    stop_wallpaper();
    if icons_hidden() {
        show_desktop_icons();
    }
    "wallpaper stopped".to_string()
}

fn cmd_stop() {
    println!("{}", cmd_stop_inner());
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


// ---- shader wallpapers ----------------------------------------------------

fn cmd_shaders() {
    println!("built-in shaders:");
    for name in shader::builtin_names() {
        println!("  {name}");
    }
    println!("\nuse: lwp set-builtin <name>   or   lwp set-shader <file.metal>");
}

fn cmd_shader_check(path: Option<&str>, builtin: Option<&str>, size: &str) -> String {
    let src = match (path, builtin) {
        (_, Some(name)) => match shader::builtin_source(name) {
            Some(s) => s.to_string(),
            None => return format!("error: no built-in shader named '{name}'"),
        },
        (Some(p), None) => {
            let abs = resolve(p);
            match fs::read_to_string(&abs) {
                Ok(s) => s,
                Err(e) => return format!("error: cannot read {}: {e}", abs.display()),
            }
        }
        (None, None) => return "error: give a shader path or --builtin <name>".to_string(),
    };
    let (w, h) = match size.split_once('x') {
        Some((w, h)) => match (w.parse::<u32>(), h.parse::<u32>()) {
            (Ok(w), Ok(h)) => (w, h),
            _ => return format!("error: bad size '{size}', expected WxH"),
        },
        None => return format!("error: bad size '{size}', expected WxH"),
    };
    match shader::check(&src, w, h) {
        Ok(msg) => msg,
        Err(e) => format!("error: {e}"),
    }
}

fn cmd_set_shader(path: &str, hide_icons: bool) {
    let abs = resolve(path);
    let src = match fs::read_to_string(&abs) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", abs.display());
            std::process::exit(1);
        }
    };
    // Compile before tearing down whatever is already playing.
    if let Err(e) = shader::check(&src, 64, 64) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    ensure_state_dir();
    stop_wallpaper();
    if hide_icons {
        hide_desktop_icons();
    } else if icons_hidden() {
        show_desktop_icons();
    }
    let p = abs.to_string_lossy().to_string();
    fs::write(VIDEO_PATH, format!("shader: {p}")).ok();
    save_config(&p, hide_icons, Some("shader"));

    let mut args: Vec<String> = vec!["run-shader".into(), "--path".into(), p];
    if hide_icons {
        args.push("--hide-icons".into());
    }
    println!("{}", spawn_worker(&args, "shader wallpaper"));
}

fn cmd_set_builtin(name: &str, hide_icons: bool) {
    let src = match shader::builtin_source(name) {
        Some(s) => s,
        None => {
            eprintln!(
                "error: no built-in shader named '{name}' (have: {})",
                shader::builtin_names().join(", ")
            );
            std::process::exit(1);
        }
    };
    if let Err(e) = shader::check(src, 64, 64) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    ensure_state_dir();
    stop_wallpaper();
    if hide_icons {
        hide_desktop_icons();
    } else if icons_hidden() {
        show_desktop_icons();
    }
    fs::write(VIDEO_PATH, format!("builtin: {name}")).ok();
    save_config(name, hide_icons, Some("builtin"));

    let mut args: Vec<String> = vec!["run-builtin".into(), "--name".into(), name.into()];
    if hide_icons {
        args.push("--hide-icons".into());
    }
    println!("{}", spawn_worker(&args, "shader wallpaper"));
}

fn cmd_run_shader(path: &str, hide_icons: bool) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("shader error: cannot read {path}: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = shader::run(&src, hide_icons) {
        eprintln!("shader error: {e}");
        std::process::exit(1);
    }
}

fn cmd_run_builtin(name: &str, hide_icons: bool) {
    let src = match shader::builtin_source(name) {
        Some(s) => s,
        None => {
            eprintln!("shader error: no built-in shader named '{name}'");
            std::process::exit(1);
        }
    };
    if let Err(e) = shader::run(src, hide_icons) {
        eprintln!("shader error: {e}");
        std::process::exit(1);
    }
}

// ---- autostart ------------------------------------------------------------

fn autostart_plist() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.lwp.wallpaper.plist")
}

pub fn cmd_autostart_on_inner() -> String {
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
    format!("autostart enabled ({})", plist.display())
}

fn cmd_autostart_on() {
    println!("{}", cmd_autostart_on_inner());
}

pub fn cmd_autostart_off_inner() -> String {
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
        "autostart disabled".to_string()
    } else {
        "autostart not enabled".to_string()
    }
}

fn cmd_autostart_off() {
    println!("{}", cmd_autostart_off_inner());
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
        match cfg.last_kind.as_deref() {
            Some("shader") => {
                if PathBuf::from(&cfg.last_video).is_file() {
                    cmd_run_shader(&cfg.last_video, cfg.hide_icons);
                }
            }
            Some("builtin") => cmd_run_builtin(&cfg.last_video, cfg.hide_icons),
            _ => {
                if PathBuf::from(&cfg.last_video).is_file() {
                    wallpaper::run(&cfg.last_video, cfg.hide_icons);
                }
            }
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
    /// Set a Metal shader (.metal) as the wallpaper
    #[command(name = "set-shader")]
    SetShader {
        path: String,
        #[arg(long)]
        hide_icons: bool,
    },
    /// Set a built-in shader as the wallpaper
    #[command(name = "set-builtin")]
    SetBuiltin {
        name: String,
        #[arg(long)]
        hide_icons: bool,
    },
    /// List built-in shaders
    Shaders,
    /// Compile a shader and render one frame offscreen to prove it works
    #[command(name = "shader-check")]
    ShaderCheck {
        path: Option<String>,
        #[arg(long)]
        builtin: Option<String>,
        #[arg(long, default_value = "320x200")]
        size: String,
    },
    /// Internal: shader wallpaper worker
    #[command(name = "run-shader", hide = true)]
    RunShader {
        #[arg(long)]
        path: String,
        #[arg(long)]
        hide_icons: bool,
    },
    /// Internal: built-in shader worker
    #[command(name = "run-builtin", hide = true)]
    RunBuiltin {
        #[arg(long)]
        name: String,
        #[arg(long)]
        hide_icons: bool,
    },
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
        Commands::SetShader { path, hide_icons } => cmd_set_shader(&path, hide_icons),
        Commands::SetBuiltin { name, hide_icons } => cmd_set_builtin(&name, hide_icons),
        Commands::Shaders => cmd_shaders(),
        Commands::ShaderCheck { path, builtin, size } => {
            println!("{}", cmd_shader_check(path.as_deref(), builtin.as_deref(), &size))
        }
        Commands::RunShader { path, hide_icons } => cmd_run_shader(&path, hide_icons),
        Commands::RunBuiltin { name, hide_icons } => cmd_run_builtin(&name, hide_icons),
        Commands::Interactive => tui::run().unwrap_or_else(|e| eprintln!("tui error: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_log_creates_the_log_file() {
        ensure_state_dir();
        let path = format!("{STATE_DIR}/lwp.log");
        let _ = fs::remove_file(&path);
        let (_out, _err) = worker_log();
        assert!(PathBuf::from(&path).exists(), "worker_log should create {path}");
    }
}
