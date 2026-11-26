# projects

Personal tools built for fun.

## ked

A vim‑like TUI text editor written in **Rust** with ratatui/crossterm.

- 53 themes, 14 syntax languages, multi‑buffer tabs
- Fuzzy file finder, file tree with Nerd Font icons
- Built‑in music player (mpv/afplay), PTY shell popup
- System dashboard with CPU/MEM/BATT gauges and log tail
- Configurable via `~/.config/ked/config.toml`
- Cross‑platform (macOS + Linux)

```sh
cd ked && cargo build --release
./target/release/ked [file]
```

[ked/README.md](ked/README.md)

---

## kshell

A scriptable desktop shell for macOS — bars, widgets, panels and
launchers described in TOML and driven by scripts. Written in **Swift**
(AppKit + SwiftUI) with **JavaScriptCore** widgets. It replaces
sketchybar.

- One bar per display, live-reloading `~/.config/kshell/config.toml`
- Native widgets (workspaces via aerospace, volume, Bluetooth, clock) and
  user JavaScript widgets with hot reload
- App, theme and wallpaper launchers; music player with playlists
- Notification centre with a `kshell notify` CLI, used by coding agents
- `kshell trigger` IPC event bus

```sh
cd kshell && ./scripts/run.sh
```

[kshell/README.md](kshell/README.md)

---

## lwp-rust

A live video wallpaper engine for macOS, written in **Rust** using
objc2 bindings to AVFoundation + AppKit. This is the native binary
rewrite of the Python version.

- `lwp set <video>` — play a video as your desktop wallpaper
- `lwp stop` / `lwp status` — control playback
- `lwp tui` — interactive fuzzy file browser (dialoguer)
- `lwp autostart on|off` — LaunchAgent for login persistence
- Plays across all monitors, loops infinitely, no icons covered
- Single statically linked binary, zero Python/runtime deps

```sh
cd lwp-rust && cargo build --release
./target/release/lwp set ~/Videos/waves.mp4
```

Requires macOS (AVFoundation, AppKit, Quartz).

---

## lwp

*Superseded by lwp-rust; kept for reference.* The original **Python**
wallpaper engine — the proof of concept that lwp-rust was ported from. Uses PyObjC to reach AVFoundation/AppKit
and Python's built‑in curses for the TUI browser.

- Same feature set as lwp-rust
- Two‑process architecture: TUI/CLI spawns a detached background worker
- Requires `pip install PyObjC` and Python 3.14+

```sh
cd lwp
python3 -m venv .venv && source .venv/bin/activate
pip install PyObjC
./lwp tui
```

[lwp/README.md](lwp/README.md)

---

## yt-tui

Search and watch YouTube in the terminal, written in **Rust** with
ratatui: real video at the resolution of your window, with sound in sync.

- ffmpeg decodes frames sized to the window; they reach kitty through
  shared memory, so full resolution keeps full frame rate
- Placed with kitty's Unicode placeholders, so it works inside tmux
- mpv plays the audio and drives the clock: seek, pause, volume, live
- Thumbnail previews, audio-only mode, autoplay, mouse support

```sh
cd yt-tui && cargo build --release
./target/release/yt-tui lofi hip hop
```

Needs yt-dlp, ffmpeg and mpv; video needs kitty or Ghostty.

[yt-tui/README.md](yt-tui/README.md)

---

## License

Unlicense — public domain.
