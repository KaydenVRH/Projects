# ked — a vim‑like TUI text editor

```
 _            _
| |          | |
| | _____  __| |
| |/ / _ \/ _` |
|   <  __/ (_| |
|_|\_\___|\__,_|
```

## Features

- **Vim‑style editing** — normal / insert / visual modes with
  `h`/`j`/`k`/`l`, `w`/`b`, `0`/`$`, `gg`/`G`, page up/down
- **Syntax highlighting** — Rust, Python, C/C++, JavaScript, TypeScript,
  HTML, CSS, Markdown, and config files (`.conf`, `.ini`, `.cfg`, `.toml`)
- **Fuzzy file finder** — `Ctrl+P`, type to filter, Enter to open
- **File tree** — `Ctrl+F` with Nerd Font icons and folder expand/collapse
- **Multi‑buffer tabs** — `Tab` / `Shift+Tab` to cycle, `:bn` / `:bp`
- **Shell** — `Ctrl+J` PTY shell popup
- **Music player** — `Ctrl+T` scans for audio files, playlist with
  auto‑advance (macOS: `afplay`, Linux: `mpv`)
- **Run code** — `Ctrl+R` runs `.py`, `.rs`, `.c`, `.h`, `.go` files
- **22 themes** — default, monokai, solarized, nord, gruvbox, bi,
  catppuccin, tokyonight, amber, dracula, onedark, everforest,
  rosepine, oxocarbon, system, opencode, ayu, kanagawa, templeos,
  palenight, darkplus, moonlight, poimandres
- **System dashboard** — `:sys` shows CPU, memory, disk, battery,
  uptime, and system log tail
- **Search** — `/` to query, `n` / `N` to repeat, all matches highlighted
- **Undo / redo** — `u` to undo
- **Yank / paste** — `yy`, `dd`, `p` / `P`
- **Command mode** — `:w`, `:wq`, `:wq!`, `:q`, `:q!`, `:e <file>`,
  `:<line>`, `:theme <name>`
- **Config file** — `~/.config/ked/config.toml` with theme, transparency,
  animations, bar stats, music dir, filetree width
- **Auto‑reload** — detects external file changes on disk
- **Splash screen** — animated logo when no file is open
- **System stats bar** — CPU, memory, battery, clock in the buffer bar
- **Transparent background** — `transparent = true` in config
- **Scrollable overlays** — theme selector, file finder, music player
  all auto‑scroll with selection

## Install

```sh
cargo build --release
cp target/release/ked ~/.local/bin/
```

## Usage

```
ked              # start with an empty buffer
ked file.rs      # open a specific file
```

## Keybinds

| Key         | Action         | Key         | Action         |
|-------------|----------------|-------------|----------------|
| `Ctrl+P`    | find file      | `Ctrl+F`    | file tree      |
| `Ctrl+J`    | shell          | `Ctrl+R`    | run file       |
| `Ctrl+T`    | music          | `Ctrl+E`    | theme selector |
| `Ctrl+K`    | keybinds       | `Ctrl+S`    | save           |
| `/`         | search         | `Tab`       | next buffer    |
| `h`/`j`/`k`/`l` | move       | `i`/`a`/`A` | insert mode    |
| `v`         | visual mode    | `u`         | undo           |
| `yy`        | yank line      | `dd`        | delete line    |
| `p`/`P`     | paste          | `:wq`       | save & quit    |

## Config

```toml
# ~/.config/ked/config.toml
theme = "oxocarbon"
music_dir = "/path/to/music"
filetree_width = 30
transparent = false
animations = true
bar_stats = true
```

## Dependencies

- [ratatui](https://crates.io/crates/ratatui) — TUI widgets
- [crossterm](https://crates.io/crates/crossterm) — terminal & events
- [anyhow](https://crates.io/crates/anyhow) — error handling
- [serde](https://crates.io/crates/serde) + [toml](https://crates.io/crates/toml) — config parsing
- [chrono](https://crates.io/crates/chrono) — clock
- [libc](https://crates.io/crates/libc) — PTY support

Single statically linked binary, no runtime dependencies beyond the OS.

## License

Unlicense — public domain.
