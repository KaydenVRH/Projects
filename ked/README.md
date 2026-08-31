# ked — a vim‑like TUI text editor

```
 _            _
| |          | |
| | _____  __| |
| |/ / _ \/ _` |
|   <  __/ (_| |
|_|\_\___|\__,_|
```

ked is a personal vim‑like text editor that runs entirely in your
terminal. It's a single Rust binary: modal editing, hand‑rolled syntax
highlighting, 19 themes, animated colour effects, a fuzzy finder, a
file tree, a real PTY shell, a music player, and a system dashboard —
all in the alternate screen.

## Features

### Editing

- **Vim‑style modes** — normal, insert, and visual modes with the
  usual motions: `h`/`j`/`k`/`l`, `w`/`b`, `0`/`$`, `g`/`G`,
  `Ctrl+D`/`Ctrl+U` half‑pages, page up/down
- **Undo / redo** — `u` undo, `Ctrl+R` redo. Snapshots store the text
  *and* the cursor, so undo lands you back where you were. A whole
  insert session (a typed run) is one undo step, like vim.
- **Yank / paste** — `yy`, `dd`, `p`/`P`, plus visual‑mode
  yank/delete over a selection
- **Search** — `/` query, `n`/`N` repeat, every match highlighted
- **Smart insert** — Enter preserves indentation, Tab indents,
  `Ctrl+A` pastes the internal clipboard, `Ctrl+C` steps back to
  normal mode
- **UTF‑8 clean** — cursor math is terminal‑cell aware, so wide
  characters, emoji, tabs, and combining marks never push the cursor
  out of sync

### Buffers & files

- **Multi‑buffer tabs** — `Tab`/`Shift+Tab` or `:bn`/`:bp` to cycle;
  each buffer keeps its own cursor, scroll, and undo history
- **Fuzzy file finder** — `Ctrl+P` filters as you type with scoring
  that favours path boundaries; Enter opens in a new buffer
- **File tree** — `Ctrl+F` panel with Nerd Font icons; `j`/`k`
  navigate, `h`/`l` or Enter expand/collapse folders, Enter opens files
- **Auto‑reload** — if the open file changes on disk (and you have no
  unsaved edits), ked reloads it and keeps your cursor
- **Save** — `Ctrl+S` or `:w`; new buffers get a name with `:w <name>`

### Tools & toys

- **Shell** — `Ctrl+J` pops up a real PTY running `$SHELL`, with ANSI
  colours rendered in your current theme; type `exit` to close
- **Run code** — `Ctrl+E` runs the buffer: `.py` via `python3`,
  `.rs` via `rustc`, `.c`/`.h` via `cc`, `.go` via `go run`; output
  appears in an overlay
- **Music player** — `Ctrl+M` scans your music dir (`.mp3`) and plays
  a playlist through `mpv` with auto‑advance and looping
  (`Enter` play, `s` stop, `l` loop, `Esc` close while it keeps
  playing)
- **System dashboard** — `:sys` shows CPU, memory, disk, battery,
  uptime, and a tail of recent system errors
- **System stats bar** — optional CPU / MEM / BATT / clock in the top
  bar (see `bar_stats`)

### Looks

- **19 themes** — default, monokai, solarized, nord, gruvbox, bi,
  catppuccin, tokyonight, amber, dracula, onedark, everforest,
  rosepine, oxocarbon, ayu, kanagawa, palenight, darkplus, moonlight.
  Palettes follow the canonical Neovim theme colours, with distinct
  roles for keywords, functions, types, strings, constants, and
  properties
- **Theme selector** — `Ctrl+T` list with live preview, Enter applies
- **Colour FX** — animated theme modes from the config: gentle hue
  drift, breathing lightness, or a warm amber-purple wander
- **Animations** — shimmering tab pills, rainbow status‑bar dashes,
  scrolling overlay titles, an animated splash logo, a music spinner
- **Transparency & opacity** — `transparent = true` lets your
  terminal background show through; `opacity` fades the UI chrome
- **Syntax highlighting** — real tree-sitter parsing for Rust, Python,
  C/C++, JavaScript/TypeScript (incl. JSX/TSX), HTML, CSS, TOML, JSON,
  Bash, Go, and Markdown. Multi-line comments, docstrings, and
  fenced code blocks stay correctly highlighted across lines.
  Language is detected from the shebang line, well-known file names,
  then the extension; `.conf`/`.ini`/`.cfg` keep a hand-rolled
  tokenizer

## Install

Needs a recent stable Rust toolchain. macOS and Linux are first‑class.

```sh
cargo build --release
cp target/release/ked ~/.local/bin/        # or: cargo install --path .
```

Optional helpers:

- **Nerd Font** — file‑tree icons (``, ``, …) fall back to nothing
  without one
- **mpv** — for the music player (`Ctrl+M`)
- **kitty keyboard protocol** — `Ctrl+M` only works in terminals that
  speak it: kitty, foot, wezterm, iTerm2, ghostty, …; elsewhere it
  arrives as Enter and the binding won't fire
- **python3 / cc / rustup / go** — whatever parts of `Ctrl+E` you use
- **truecolor** — themes are RGB; a 256‑colour terminal still works
  but dithers

## Usage

```
ked              # start with an empty buffer (splash screen)
ked file.rs      # open a file
```

The finder and file tree scan the directory you launched ked from, so
`cd` into a project first.

## Keybinds

### Global

| Key | Action |
|-----|--------|
| `Ctrl+P` | fuzzy file finder |
| `Ctrl+F` | file tree |
| `Ctrl+J` | shell popup (type `exit` to close) |
| `Ctrl+E` | run current file |
| `Ctrl+M` | music player |
| `Ctrl+T` | theme selector |
| `Ctrl+K` | keybinds manual |
| `Ctrl+S` | save |
| `Ctrl+C` | quit — or, from insert/visual mode, step back to normal mode first |

### Normal mode

| Key | Action |
|-----|--------|
| `h`/`j`/`k`/`l` or arrows | move |
| `w`/`b` | next / previous word |
| `0`/`$` | line start / end |
| `g`/`gg`/`G` | first line / last line |
| `Ctrl+D`/`Ctrl+U` | half page down / up |
| `i`/`a`/`I`/`A` | enter insert mode |
| `o`/`O` | open line below / above |
| `x`/`dd` | delete char / line |
| `yy` | yank line |
| `p`/`P` | paste after / before |
| `u` | undo |
| `Ctrl+R` | redo |
| `v` | enter visual mode |
| `/` | search |
| `n`/`N` | next / previous match |
| `Tab`/`Shift+Tab` | next / previous buffer |
| `:` | command mode |
| `⌥`/`⌘` + arrows or `hjkl` | jump cursor (step size = `alt_scroll`) |

### Insert mode

| Key | Action |
|-----|--------|
| `Esc`/`Ctrl+C` | back to normal mode |
| `Enter` | new line, indentation preserved |
| `Tab` | insert 4 spaces |
| `Ctrl+A` | paste the yanked/deleted text |
| arrows | move without leaving insert mode |

### Visual mode

| Key | Action |
|-----|--------|
| `h`/`j`/`k`/`l`, `w`/`b`, `0`/`$`, `Ctrl+D`/`Ctrl+U` | extend / shrink selection |
| `y`/`c` | yank selection |
| `d` | delete selection |
| `i`/`a`/`I`/`A` | edit the selection area |
| `Esc`/`v`/`Ctrl+C` | back to normal mode |

### Overlays

| Overlay | Keys |
|---------|------|
| Finder | type to filter, `j`/`k` or arrows, Enter open, `Esc`/`Ctrl+P` close |
| File tree | `j`/`k`, `h`/`l` or Enter expand/collapse, Enter open, `Esc`/`Ctrl+F` close |
| Theme | `j`/`k`, Enter apply, Esc cancel |
| Music | `j`/`k`, Enter play, `s` stop, `l` loop, Esc close (keeps playing) |
| Run output | any key dismisses |
| Shell | forward everything; type `exit` to close |

### Commands (`:`)

| Command | Action |
|---------|--------|
| `:w [file]` | save (optionally under a new name) |
| `:q` / `:q!` | close buffer, quit if last (force) |
| `:wq` / `:wq!` | save & close (force) |
| `:e <file>` | open file in a new buffer |
| `:bn`/`:bnext`, `:bp`/`:bprev` | switch buffers |
| `:<line>` | jump to line |
| `:theme <name>` | switch theme by name |
| `:sys` | system dashboard |

## Config

`~/.config/ked/config.toml` — every key is optional; missing or broken
files fall back to defaults.

```toml
theme         = "oxocarbon"   # any of the 19 theme names
music_dir     = "~/Music"     # where Ctrl+M scans
filetree_width = 30           # file-tree panel width in columns
status_bar_top = false        # put the status bar above the buffer bar
transparent   = false         # let the terminal background show through
animations    = true          # tab shimmer, rainbow dashes, splash, …
bar_stats     = true          # CPU / MEM / BATT / clock in the top bar
fx_mode       = 0             # colour FX at startup: 0 off, 1 gentle,
                              # 2 breathing, 3 warm
opacity       = 1.0           # UI chrome opacity (0.0 – 1.0)
alt_scroll    = 5             # chars per ⌥/⌘-arrow jump
```

## How it works

ked is one binary with no runtime dependencies beyond the OS.

- **Event loop** (`src/main.rs`) — raw mode + alternate screen, then a
  ~60 fps loop: poll keys → mutate state → render a frame with ratatui.
  The kitty keyboard protocol is pushed at startup (that's what makes
  `Ctrl+M` usable).
- **Editor core** (`src/editor.rs`) — one `Editor` struct owns
  everything: buffers, modes, overlays, undo, theme, stats. The active
  buffer's fields are mirrored onto the editor and swapped out when
  you switch tabs, so each tab keeps its own cursor, scroll, and
  undo history.
- **Cursor math** — buffer positions are UTF‑8 byte offsets, but
  everything on screen is measured in terminal *cells*: wide
  characters (CJK/emoji) are 2 cells, tabs advance to multiples of 8,
  and control characters are dropped so the frame never desyncs.
- **Highlighting** (`src/highlight.rs`) — the buffer is parsed with
  tree-sitter once per edit (cached by content hash); a tree walk
  classifies nodes into ked's token kinds (keywords, functions, types,
  strings, comments, …) and the render loop maps those to theme
  colours every frame, so themes and colour FX stay cheap.
- **Themes & FX** (`src/theme.rs`) — plain RGB palettes; the animated
  FX modes convert every colour through HSL and rotate hue or pulse
  lightness per frame.
- **Shell** (`src/shell.rs`) — a real PTY (`openpty` + `fork` +
  `execvp $SHELL`) with a small ANSI/SGR parser that turns shell
  colours into ratatui styles, so `ls --color`, git diffs, and TUIs
  render correctly.
- **Music** (`src/music.rs`) — a background thread drives `mpv` and
  reports track ends over a channel, so playback advances without
  blocking the editor.
- **System stats** — CPU/mem/battery are collected on a background
  thread every few seconds and cached for the bars and `:sys`.

## Project layout

```
src/
  main.rs       entry point: terminal setup, event loop, kitty protocol
  editor.rs     editor core: modes, key handling, render, undo, scrolling
  highlight.rs  tree-sitter highlighting + language detection
  theme.rs      19 themes + HSL hue rotation for colour FX
  config.rs     ~/.config/ked/config.toml loading
  finder.rs     fuzzy file finder (Ctrl+P)
  filetree.rs   file-tree panel with Nerd Font icons (Ctrl+F)
  music.rs      mpv-backed music player with auto-advance (Ctrl+M)
  shell.rs      PTY shell with ANSI SGR rendering (Ctrl+J)
```

## Dependencies

- [ratatui](https://crates.io/crates/ratatui) — TUI widgets
- [crossterm](https://crates.io/crates/crossterm) — terminal & events
- [tree-sitter](https://crates.io/crates/tree-sitter) — parsing, with
  bundled grammars for the supported languages
- [anyhow](https://crates.io/crates/anyhow) — error handling
- [serde](https://crates.io/crates/serde) + [toml](https://crates.io/crates/toml) — config parsing
- [chrono](https://crates.io/crates/chrono) — clock
- [libc](https://crates.io/crates/libc) — PTY support

Single binary, no runtime dependencies beyond the OS (plus `mpv` for
music and whatever compilers `Ctrl+E` needs).  The first build is slow
(each grammar compiles a generated C parser); subsequent builds are
incremental.

## License

Unlicense — public domain.
