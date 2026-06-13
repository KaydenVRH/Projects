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

- **Vim‑style editing** — normal / insert / visual modes with familiar
  motions (`h`/`j`/`k`/`l`, `w`/`b`, `0`/`$`, `gg`/`G`, page up/down)
- **Syntax highlighting** — Rust, Python, and config files (`.conf`,
  `.ini`, `.cfg`)
- **Fuzzy file finder** — `Ctrl+P`, type to filter, Enter to open
- **Multi‑buffer tabs** — `Tab` / `Shift+Tab` to cycle, `:bn` / `:bp`
- **Pop‑up shell** — `Ctrl+J` opens a real PTY shell inside the editor
- **Music player** — `Ctrl+T` scans `~/Music` for MP3s, plays with
  `afplay` (macOS)
- **Run Python** — `Ctrl+R` runs the current file through `python3`
- **9 themes** — `Ctrl+E` or `:theme <name>`.  Rainbow mode with `:3`
- **Search** — `/` to query, `n` / `N` to repeat, all matches highlighted
- **Undo / redo** — `u` / `Ctrl+R`
- **Yank / paste** — `yy`, `dd`, `p` / `P` (works across buffers)
- **Command mode** — `:w`, `:wq`, `:q`, `:q!`, `:e <file>`
- **Auto‑reload** — detects file changes on disk
- **Splash screen** — shown when no file is open

## Install

```sh
cargo install --path /path/to/ked
# or build manually
cd ked && cargo build --release
cp target/release/ked ~/.local/bin/
```

## Usage

```
ked              # start with an empty buffer
ked file.rs      # open a specific file
```

## Keybinds

| Key       | Action          | Key       | Action          |
|-----------|-----------------|-----------|-----------------|
| `Ctrl+P`  | find file       | `Ctrl+J`  | shell           |
| `Ctrl+R`  | run python      | `Ctrl+T`  | music           |
| `Ctrl+E`  | theme           | `/`       | search          |
| `Tab`     | switch buf      | `Ctrl+S`  | save            |
| `:wq`     | save & quit     | `:q!`     | quit            |

## Dependencies

- [ratatui](https://crates.io/crates/ratatui) — TUI widgets
- [crossterm](https://crates.io/crates/crossterm) — terminal & events
- [anyhow](https://crates.io/crates/anyhow) — error handling
- [libc](https://crates.io/crates/libc) — PTY support (`openpty`)

Only 4 crates — single statically linked Mach-O binary, no runtime
dependencies.

## License

Unlicense — public domain.  Do whatever you want.
