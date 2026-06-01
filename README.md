# lwp — Live Wallpaper Player for macOS

A minimal, no-dependency (beyond PyObjC) video wallpaper engine for macOS. Plays any video file as your desktop background, across all monitors, in an infinite loop.

## Usage

```sh
# Set a video as wallpaper
lwp set [--hide-icons] ~/Videos/waves.mp4

# Stop the current wallpaper
lwp stop

# Show what's currently playing
lwp status

# List video files in a directory
lwp list [dir]

# Interactive TUI browser (arrow keys, / to search, Enter to set)
lwp tui [dir]
```

## Setup

```sh
git clone <repo> lwp
cd lwp
python3 -m venv .venv
source .venv/bin/activate
pip install PyObjC
# optional: add to PATH
echo 'export PATH="$PATH:'"$(pwd)"'"' >> ~/.zshrc
```

The `.venv` contains compiled native code (`.so` files) and must be recreated on each machine.

## How it works

**Two-process architecture.** The `set` (or TUI) command writes the video path to `/tmp/lwp/`, spawns a detached background worker via `subprocess.Popen`, and exits immediately. The worker is the long-lived process that actually shows the wallpaper.

**The wallpaper itself** uses PyObjC to call Apple's native frameworks directly:
- `AVFoundation` — plays the video file, loops it by seeking back to zero on end
- `AppKit` — creates a borderless fullscreen `NSWindow` at `kCGDesktopWindowLevel`, one per monitor, with mouse events ignored so desktop interaction is unaffected
- `AVLayerVideoGravityResizeAspectFill` — video fills the screen, cropping edges if aspect ratios don't match

**Desktop icons** sit above the desktop window level on modern macOS. Pass `--hide-icons` to hide them (toggles `Finder`'s `CreateDesktop` preference).

**The TUI** (`lwp tui`) uses Python's built-in `curses` module — no extra dependencies. It scans the given directory for video files, lets you browse with arrow keys, filter with `/`, and set the current selection as wallpaper with Enter.

## Requirements

- macOS (uses AppKit, AVFoundation, Quartz)
- Python 3.14+
- PyObjC (`pip install PyObjC`)

