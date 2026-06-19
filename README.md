# projects

Personal tools built for my daily workflow.

## ked

A vim‑like TUI text editor written in **Rust** with ratatui/crossterm.

- 22 themes, 8 syntax languages, multi‑buffer tabs
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

The original **Python** wallpaper engine — the proof of concept that
lwp-rust was ported from. Uses PyObjC to reach AVFoundation/AppKit
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

## License

Unlicense — public domain.
