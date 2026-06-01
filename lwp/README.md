# lwp — Live Wallpaper Player for macOS

Minimal video wallpaper engine for macOS. Plays any video file as your desktop background across all monitors, infinitely looping.

## Quick start

```sh
# Add to PATH (one-time)
export PATH="$PATH:/path/to/lwp"

# Set a wallpaper
lwp set ~/Videos/waves.mp4

# Stop it
lwp stop

# Interactive browser
lwp tui
```

## Commands

| Command | Description |
|---------|-------------|
| `lwp set [--hide-icons] <video>` | Set a video as wallpaper |
| `lwp stop` | Stop the current wallpaper |
| `lwp status` | Show what's playing |
| `lwp list [dir]` | List video files in a directory |
| `lwp tui [dir]` | Interactive terminal browser |
| `lwp autostart on\|off\|status` | Manage login autostart |

## Login autostart

```sh
# Enable — your last-set wallpaper plays automatically at login
lwp autostart on

# Check status
lwp autostart status

# Disable
lwp autostart off
```

This creates a LaunchAgent plist at `~/Library/LaunchAgents/com.lwp.wallpaper.plist`. When you log in, it reads `~/.lwp/config.json` (saved automatically whenever you `set` a wallpaper) and starts the player.

## Setup

```sh
git clone <url> lwp
cd lwp
python3 -m venv .venv
source .venv/bin/activate
pip install PyObjC
```

The `.venv` contains native code and must be recreated on each machine.

## How it works

Two-process architecture — the `set`/`tui` command spawns a detached background worker and exits. The worker uses PyObjC to call Apple's `AVFoundation` and `AppKit` directly:
- A borderless `NSWindow` at `kCGDesktopWindowLevel` sits behind desktop icons
- `AVPlayer` loops the video, one per monitor
- Mouse events pass through to the desktop
- `--hide-icons` toggles Finder's `CreateDesktop` preference (needed on macOS 14+)

The TUI uses Python's built-in `curses` — zero extra dependencies.

## Requirements

- macOS (AppKit, AVFoundation, Quartz)
- Python 3.14+
- PyObjC (`pip install PyObjC`)
