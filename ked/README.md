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
highlighting, 30 themes, animated colour effects, a fuzzy finder, a
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
- **System clipboard** — yanks copy to the system clipboard via
  OSC 52 (kitty, wezterm, foot, iTerm2); pasting from the terminal
  uses bracketed paste and is always inserted literally — in insert
  mode at the cursor, in normal mode like `p`, never as commands
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
  unsaved edits), ked reloads it and keeps your cursor. Changed lines
  get a right-gutter indicator — green `▎` for added lines, amber for
  edited ones — that fades out after a few seconds
- **Save** — `Ctrl+S` or `:w`; new buffers get a name with `:w <name>`

### Tools & toys

- **Shell** — `Ctrl+J` (or `:shell`) pops up a real PTY running `$SHELL`.
  Full-screen programs work properly (htop, btop, vim, less, fzf, …):
  a real terminal emulator handles cursor addressing, the alternate
  screen, scrolling, colours and resizing.  Type `exit` to close
- **Run code** — `Ctrl+E` runs the buffer: `.py` via `python3`,
  `.rs` via `rustc`, `.c`/`.h` via `cc`, `.go` via `go run`; output
  appears in an overlay
- **Music player** — `Ctrl+M` scans your music dir (`.mp3`) and plays
  a playlist through `mpv` with auto‑advance and looping
  (`Enter` play, `s` stop, `l` loop, `Esc` close while it keeps
  playing)
- **Memories & reminders** — `Ctrl+Shift+N` captures a note/reminder in
  one line (`call mom @tomorrow 9am #family !2`), `Ctrl+Shift+M` opens
  a dashboard grouped into Overdue / Today / Upcoming / Recent, and
  `:today` opens a daily journal.  Entries are plain markdown files
  with TOML frontmatter; due reminders pop a macOS notification (and
  optional sound) whether or not ked is open, via an installed
  LaunchAgent
- **Planner** — `Ctrl+Shift+A` (or `:cal`) opens a month calendar with
  the selected day's agenda beside it.  Reminders show as `•`, notes as
  `·`; move by day/week/month, `n` adds something on that date
  (pre-filled), `Enter` opens an entry, `d`/`s` complete or snooze.
  Repeating reminders appear on their future dates too
- **Markdown preview** — `Ctrl+V` opens a live rendered preview pane
  next to the editor: styled headings, bullets, code blocks, quotes,
  and links, re-rendered as you type and following your cursor.
  Uses the `glow` CLI automatically when it's installed
- **Markdown conceal** — while editing `.md` files, markers are
  rendered away in the buffer itself: `#` heading markers hidden
  (headings styled by level), `**`/`*`/`` ` `` delimiters hidden with
  bold/italic/code applied, `-` shown as `•`, quotes as `│` bars, and
  links shown as text without the URL — all fully editable, with the
  cursor tracking correctly through concealed characters
- **Markdown inline images** — terminals with the kitty graphics
  protocol (kitty, ghostty, wezterm, foot, …) render `![alt](pic.png)`
  images directly in the buffer, sized to the content width; the
  covered source lines are blanked out.  Enter insert mode on an image
  to reveal its markdown source for editing.  Paste a screenshot with
  `:img` or `Ctrl+Shift+V` — it's saved next to the file, inserted as
  a markdown image reference, and rendered immediately
- **System dashboard** — `:sys` shows CPU, memory, disk, battery,
  uptime, and a tail of recent system errors
- **Context bar** — the top bar shows the detected language, buffer
  position, line count, the now-playing song (scrolling), and the
  clock (see `bar_stats`)

### Looks

- **30 themes** — 22 dark (default, mono, monokai, nord, gruvbox, bi,
  blue-lagoon, catppuccin, tokyonight, amber, dracula, onedark,
  everforest, rosepine, oxocarbon, ayu, kanagawa, palenight, darkplus,
  moonlight, osaka-jade, last-horizon) and 8 light (solarized, latte,
  gruvbox-light, one-light, github-light, tokyonight-day, rosepine-dawn,
  everforest-light).
  Palettes follow the canonical Neovim theme colours, with distinct
  roles for keywords, functions, types, strings, constants, and
  properties.  **mono** is pure grayscale: tokens are separated by a
  lightness ramp plus bold/italic styling instead of hue.  Light
  themes pair dark accents with automatic contrast-aware status/selection
  text.  Try `:theme latte` for Catppuccin Latte
- **Theme selector** — `Ctrl+T` list with live preview, Enter applies
- **Colour FX** — animated theme modes from the config: gentle hue
  drift, breathing lightness, or a warm amber-purple wander. Only the
  meaningful tokens move (keywords, functions, types, strings,
  numbers, config keys); plain text, comments, and the UI stay still.
  Search matches pulse with an amplified version of the effect — or,
  with fx off, a gentle shimmer like the active tab
- **Animations** — shimmering tab pills, rainbow status‑bar dashes,
  scrolling overlay titles, an animated splash logo, a music spinner
- **Transparency & opacity** — `transparent = true` lets your
  terminal background show through; `opacity` fades the UI chrome
- **Syntax highlighting** — real tree-sitter parsing for Rust, Python,
  C/C++, JavaScript/TypeScript (incl. JSX/TSX), HTML, CSS, TOML, JSON,
  Bash, Go, and Markdown. Multi-line comments, docstrings, and
  fenced code blocks stay correctly highlighted across lines.
  Language is detected from the shebang line, well-known file names,
  then the extension. Config files (`.conf`/`.ini`/`.cfg`, kitty.conf,
  `.gitconfig`, Xresources, `.desktop`/`.service`) use a hand-rolled
  tokenizer that understands `[sections]`, `key = value` and
  kitty-style `key value` pairs, hex colours, booleans, `$VARS`, and
  `ctrl+shift+…` keybindings

## Install

Needs a recent stable Rust toolchain. macOS and Linux are first‑class.

```sh
cargo build --release
cp target/release/ked ~/.local/bin/        # or: cargo install --path .
```

Or grab a prebuilt binary from
[`ked-binaries/`](../ked-binaries) — static builds for Apple Silicon
macOS, x86_64 Linux, and ARM Linux, with install instructions.

Optional helpers:

- **Nerd Font** — file‑tree icons (``, ``, …) fall back to nothing
  without one
- **glow** — optional, prettier markdown preview when installed
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

Every binding below is just a **default** — override any of them in the
`[keys]` config table (see [Custom keybindings](#custom-keybindings)).
Run `:keys` inside ked to see the effective map.

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
| `Ctrl+V` | markdown preview (`.md` files) |
| `Ctrl+Shift+N` | remember something (quick capture) |
| `Ctrl+Shift+M` | memories / reminders dashboard |
| `Ctrl+Shift+A` | planner / month calendar |
| `Ctrl+S` | save |
| `Ctrl+C` | quit — or, from insert/visual mode, step back to normal mode first |
| `Ctrl+Shift+V` | paste the clipboard image into the buffer as `![](img-….png)` |

### Prompt editing

The `:` / `/` / `Remember:` lines are fully editable with a visible
cursor (and a blinking bar):

| Key | Action |
|-----|--------|
| `←` / `→`, `Home` / `End` | move the cursor |
| `Ctrl+A` / `Ctrl+E` | start / end of line |
| `Backspace` / `Delete` | delete before / at the cursor |
| `Ctrl+W` | delete the word before the cursor |
| `Ctrl+U` | clear the line |
| `Esc` / `Enter` | cancel / submit |

Long input scrolls under the fixed prompt prefix.

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
| Finder | type to filter, arrows or `Ctrl+N`/`Ctrl+P` navigate, Enter open, Esc close |
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
| `:shell` | pop-up terminal (`$SHELL`) |
| `:keys` | show the effective (and customised) keybindings |
| `:mem [text]` | remember text (no text → open the dashboard) |
| `:memories` | reminders / notes dashboard |
| `:cal` / `:planner` | month calendar + day agenda |
| `:today` | open (or create) today's journal entry |
| `:img` | paste the clipboard image into the buffer as `![](img-….png)` |

## Config

`~/.config/ked/config.toml` — every key is optional; missing or broken
files fall back to defaults.

```toml
# ── appearance ──
theme          = "oxocarbon"   # any of the 30 theme names
transparent    = false         # let the terminal background show through
animations     = true          # tab shimmer, rainbow dashes, splash, fx
bar_stats      = true          # context bar: language, buffers, clock
status_bar_top = false         # status bar above the buffer bar
fx_mode        = 0             # 0 off, 1 gentle, 2 breathing, 3 warm
opacity        = 1.0           # UI chrome opacity, 0.0–1.0
alt_scroll     = 5             # cells per ⌥/⌘-arrow jump

# ── buffers & tools ──
filetree_width = 30            # file-tree panel width in columns
music_dir      = "~/Music"     # where Ctrl+M scans for .mp3 files
inline_images  = true          # render ![alt](path) in .md buffers

# ── memories / reminders / journal ──
[memories]
# dir = "~/notes/memories"     # default: $XDG_DATA_HOME/ked/memories
notify      = true             # macOS notification when a reminder is due
sound       = true             # play a sound alongside it
week_start  = "monday"         # planner: "monday" or "sunday"
# quiet_hours = "22:00-08:00"  # no alerts in this window (empty = never)

# ── keybindings (defaults — uncomment to override) ──
[keys]
# save        = "f2"
# finder      = "alt+p"
# quit        = "ctrl+q"
# left        = "a"            # normal + visual mode
# delete_line = "D"
# search      = "/"
# theme       = "ctrl+t"
# memories    = "ctrl+shift+m"
# quick_note  = "ctrl+shift+n"
# escape      = "ctrl+["       # insert mode
```

The complete generated reference — every option plus **every action and
its current key, commented out so the defaults stay in effect** — lives
in [`config.example.toml`](config.example.toml).  Print it any time
with:

```sh
ked --print-config      # full config template
ked --print-keys        # just the [keys] block
```

Copy it to `~/.config/ked/config.toml` and uncomment what you want to
change.

## Memories, reminders & journal

`Ctrl+Shift+N` (or `:mem <text>`) captures anything you want to
remember.  One line is enough:

```
call mom @tomorrow 9am #family !2
```

- `@…` sets when: `@today`, `@tomorrow`, `@fri`, `@2026-11-02`,
  `@9am`, `@14:30`, `@+2h`, `@daily`, `@weekly`, `@every:3d`
- `#tag` adds tags, `!1`–`!3` sets priority
- `@here` attaches the current file as context
- no `@` → a plain note

`Ctrl+Shift+M` opens the dashboard: **Overdue / Today / Upcoming /
Recent**.  `Enter` opens the entry as a markdown buffer, `n` adds a new
one, `d` marks it done (or advances a repeating reminder), `s` snoozes
an hour, `r` rescans the folder.  `:today` opens (or creates) today's
journal note.

### Planner / calendar

`Ctrl+Shift+A` (or `:cal`) opens a month calendar with the selected
day's agenda beside it (it stacks on narrow terminals).

```
 October 2026                    Monday 26 October 2026
 Mo Tu We Th Fr Sa Su           ───────────────────────
           1  2  3  4            09:30  team standup  #work
  5  6  7  8• 9 10 11           10:00  morning meds  #health
 12 13 14 15 16 17 18           16:00  tea
 19 20 21 22 23 24 25           ·      a plain note
 26[27]28 29 30 31
```

`•` marks a day with reminders, `·` a day with only notes; `[27]` is the
selected day and today is underlined.  Repeating reminders are
projected onto their future dates.

| Key | Action |
|---|---|
| `←` `→` / `h` `l` | previous / next day |
| `↑` `↓` / `k` `j` | previous / next week |
| `PgUp` / `PgDn` | previous / next month |
| `t` | jump to today |
| `Enter` | focus the day's agenda (press again to open an entry) |
| `n` | new entry on the selected date (date pre-filled) |
| `Tab` | switch between calendar and agenda |
| `d` / `s` | done / snooze the selected agenda entry |
| `Esc` | back to the calendar, then close |

Set the first day of the week with `week_start = "sunday"` (default is
`"monday"`) under `[memories]`.

Entries live as plain markdown + TOML frontmatter, one file each, so
you can edit them in ked, `grep` them, or put the folder in git:

```markdown
+++
created = "2026-10-26T09:14"
due = "2026-10-27T09:00"
repeat = "daily"
tags = ["health"]
priority = 2
+++

Call the dentist.
```

The status bar shows `⏰ N due · HH:MM` when reminders are waiting.

### Reminders when ked is closed

Install the bundled macOS LaunchAgent once:

```sh
ked --install-agent     # runs `ked --remind` every 60s via launchd
ked --uninstall-agent   # remove it again
```

A notification and sound fire when an entry comes due (outside
`quiet_hours`), and a `fired` timestamp is written so the editor and the
agent never notify twice.  Without the agent, reminders only fire while
ked is open.

### Command line

```sh
ked --version            # ked 0.1.0
ked --help               # every option
ked --remember "renew passport @2026-12-01 #admin"
ked --due                # overdue + today's reminders
ked --due-count          # just the number (for a shell prompt)
ked --next-due           # just the next time, e.g. 14:30
ked --remind             # one reminder pass (what launchd runs)
ked --install-agent      # reminders while ked is closed (macOS)
ked --uninstall-agent
ked --print-config       # full commented config template
ked --print-keys         # just the [keys] template
```

`--due-count` and `--next-due` print nothing extra, so they work in a
starship/tmux segment:

```toml
# ~/.config/starship.toml
[.custom.ked]
command = "ked --due-count"
when = true
format = "[⏰ $output ]($style)"
```

Unknown options and bad paths are rejected **before** the terminal is
touched, so a typo can't leave your shell in raw mode or the alternate
screen.  `ked some-new-file` opens an empty buffer (saved with `:w`),
like vim.

### Custom keybindings

Actions are named; assign a key spec to override the built-in binding
for that action. The old key is unbound, so it can be reused.

```toml
[keys]
save        = "f2"
finder      = "alt+p"
quit        = "ctrl+q"
left        = "a"            # applies in normal *and* visual mode
delete_line = "D"
search      = "/"
```

Key specs are `[modifiers+]<key>`:

- modifiers: `ctrl`, `alt`/`opt`, `shift`, `super`/`cmd`
- named keys: `tab`, `backtab`, `enter`, `esc`, `space`, `backspace`,
  `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `left`,
  `right`, `up`, `down`, `f1`…`f12`
- case is significant: `G` means shift+g (so `goto_last = "G"`)

Multi-key sequences aren't supported (ked's built-ins are single keys).
An unknown action, an unparseable key, or two actions claiming the same
key produces a startup warning; `:keys` lists every warning and the
resulting map.

| Layer | Action names |
|-------|--------------|
| global | `finder` `run` `save` `quit` `preview` `help` `command` `paste_image` `music` `theme` `shell` `filetree` `memories` `quick_note` `planner` |
| motion (normal+visual) | `left` `down` `up` `right` `word_next` `word_prev` `line_start` `line_end` `half_page_up` `half_page_down` |
| normal | `page_up` `page_down` `goto_first` `goto_last` `visual` `insert_before` `insert_after` `insert_line_start` `insert_line_end` `open_below` `open_above` `delete_char` `delete_line` `yank_line` `paste_after` `paste_before` `undo` `redo` `command_mode` `search` `search_next` `search_prev` `buffer_next` `buffer_prev` |
| insert | `escape` `paste_text` |
| visual | `visual_exit` `visual_yank` `visual_delete` (plus the shared motion + insert actions) |

`insert_before`/`insert_after`/`insert_line_start`/`insert_line_end`
are shared by normal and visual mode. `⌥`/`⌘`+arrows stay hard-wired
unless you bind that exact key.

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
  thread only while `:sys` is open, and cached for the dashboard.

## Project layout

```
src/
  main.rs       entry point: terminal setup, event loop, kitty protocol
  editor.rs     editor core: modes, key handling, render, undo, scrolling
  highlight.rs  tree-sitter highlighting + language detection
  theme.rs      30 themes + HSL hue rotation for colour FX
  config.rs     ~/.config/ked/config.toml loading
  finder.rs     fuzzy file finder (Ctrl+P)
  filetree.rs   file-tree panel with Nerd Font icons (Ctrl+F)
  music.rs      mpv-backed music player with auto-advance (Ctrl+M)
  memory.rs     notes / reminders / journal store + planner + LaunchAgent (Ctrl+Shift+N/M/A)
  shell.rs      real PTY + vt100 terminal emulator (Ctrl+J)
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
