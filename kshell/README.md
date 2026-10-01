# kshell

A scriptable, moldable desktop shell for macOS — the closest thing to
[Quickshell](https://quickshell.org) that macOS allows. Written in **Swift**
(AppKit + SwiftUI), configured with **TOML**, scriptable with **JavaScriptCore**
(coming).

The goal is a shell you own: bars, widgets, panels and launchers that you
describe in a config file and drive with scripts — not a fixed status bar.

## Status

Early. Today it renders a top bar with a set of built-in widgets, live-reloads
its config, and mirrors a typical sketchybar layout. It is intended to
eventually replace sketchybar.

## Build & run

```sh
./scripts/build.sh          # release build
./scripts/run.sh            # build + run (seeds ~/.config/kshell/config.toml)
```

Or directly:

```sh
swift build && swift run kshell
```

`swift run` / `.build/debug/kshell` runs as an accessory app (no Dock icon)
with one bar per display. Ctrl-C to quit.

## Configuration

`~/.config/kshell/config.toml` (copied from `config/config.example.toml` on
first run). The file is watched — saving reloads the bar live.

```toml
[bar]
height    = 34
edge      = "top"        # top | bottom
margin_x  = 0
blur      = true
background = "#2d353bAA" # RRGGBBAA (alpha last), or 0xAARRGGBB
font      = "Hack Nerd Font"
font_size = 14

[theme]
name       = "Synthwave"  # written by the theme switcher
accent     = "#83c092"    # icons  (primary accent)
highlight  = "#7fbbb3"    # labels (secondary accent)
foreground = "#d3c6aa"
dim        = "#859289"    # muted text
mid        = "#7fbbb3"
background = "#2d353bAA"  # bar background

[[bar.left]]
type = "apple"
icon_hex = "f8ff"
icon_color = "$accent"    # any [theme] color, as a $token

[[bar.right]]
type = "clock"
format = "EEE h:mm a"
```

Colors accept `#RRGGBB`, `#RRGGBBAA` (alpha last) or `0xAARRGGBB` (alpha
first). Icons can be written as a literal `icon = ""` or as an ASCII
codepoint `icon_hex = "f8ff"` (handy because Private-Use glyphs are awkward to
paste).

Any color may instead be a **token** — `$accent`, `$highlight`, `$foreground`,
`$dim`, `$mid`, `$background` — resolved from `[theme]`. That way re-colouring
the whole bar only means rewriting that one section, which is what the theme
switcher does.

## Widgets

| type | fields | notes |
|---|---|---|
| `apple` | `icon_hex`, `icon_font` | Apple logo, drawn from `SF Pro Display` |
| `workspaces` | `workspaces` = "1,2,…", `focused_color`, `unfocused_color`, `hide_empty` | aerospace; click to switch. Discovered live from aerospace when no list is given. `hide_empty` (default `true`) shows only the focused workspace and those holding windows, like sketchybar |
| `chevron` | `icon_hex` | separator glyph |
| `spacer` | — | flexible gap |
| `text` | `icon`, `text`, colors | static |
| `front_app` | — | frontmost app name |
| `clock` | `format`, `interval` | DateFormatter pattern |
| `cpu` / `ram` | `interval` | percent |
| `battery` | `interval` | percent + charge icon |
| `wifi` | `interval` | power-state icon (+ SSID if permitted) |
| `volume` | `interval`, `action` | percent + mute, via CoreAudio |
| `bluetooth` | `interval` | controller power state |
| `js` | `file`/`source`, `interval`, `events` | user JavaScript widget (see below) |
| `script` | `command`, `interval`, `icon`, `separator`, `action` | runs a shell command; stdout becomes the label (or `icon<sep>label`) |

Per-widget overrides: `icon`, `icon_hex`, `icon_color`, `label_color`,
`color`, `padding`, `spacing`, `icon_font`.

Built-in events: `app_launcher_{toggle,open,close}`,
`script_launcher_{toggle,open,close}`, `wallpaper_launcher_{toggle,open,close}`,
`theme_launcher_{toggle,open,close}`, `media_center_{toggle,open,close}`,
`media_{play_pause,next,previous}`, `bar_{hide,show,toggle}`, `config_reload`.

## Events (IPC)

Push events into the running shell from scripts or your window manager:

```sh
kshell trigger media_changed
kshell trigger aerospace_workspace_change FOCUSED_WORKSPACE=2
```

The socket lives at `~/Library/Application Support/kshell/kshell.sock`. The
`workspaces` widget already listens for `aerospace_workspace_change` (and polls
as a fallback). To make workspace switches instant, point aerospace at kshell
instead of sketchybar:

```toml
# ~/.aerospace.toml
exec-on-workspace-change = ['/bin/bash', '-c',
  'kshell trigger aerospace_workspace_change FOCUSED_WORKSPACE=$AEROSPACE_FOCUSED_WORKSPACE']
```

## Panels & launchers

`OverlayPanel` is a reusable, key-capable panel that slides in from a screen
edge; `OverlayLauncher` drives it. A panel is anchored to one edge
(`OverlayEdge`) and sits clear of the screen's edge by the panel margin, which
puts it flush against the frame's inner edge.

While the frame is on, a panel has no material of its own: its glass and tint are
another piece of the frame's, in the frame's own window (two material views in one
window render alike; in different windows they never do), so a panel meets the
frame with no seam. The panel's outline carries caelestia-style concave fillets on
the corners that touch the frame, so it reads as attached, and its own window
draws only content, masked to the same outline. With the frame off, a panel goes
back to its own frosted material and convex corners (`SheetShape`), flush with the
screen's edge.

Either way the slide is stepped frame by frame rather than handed to Core
Animation, with the decelerating curve the media centre uses: the window server
renders an effect view's blur from model geometry, so an animating view frame
would not move it. For the same reason a panel's glass is its own view rather than
part of the frame's mask — a masked backdrop is refreshed lazily, which made the
glass lag the content and snap into place at the end of the slide. Three launchers are built on it:

**App launcher** — search + grid of applications (bottom sheet).
- Open with the Apple logo, or `kshell trigger app_launcher_toggle`.
- `Enter` launches the first match, click to launch, `Esc` closes.

**Script launcher** — search + grid of `.sh` files (bottom sheet).
- Open with `kshell trigger script_launcher_toggle`.
- Configure the directory:
  ```toml
  [script_launcher]
  directory = "~/dotfiles/scripts"
  ```
- A selection runs `bash <script>`.

**Wallpaper launcher** — a left sidebar of preview rows that slides in from the
left edge (rounded right corners).
- Open with `kshell trigger wallpaper_launcher_toggle`.
- `↑`/`↓` browse, `⏎` applies, `Esc` closes; click a row to select it, click
  it again to apply.
- Images are set natively (`NSWorkspace.setDesktopImageURL` on every display);
  videos (`.mp4`/`.mov`) are handed to `lwp` if present.
- Configure the scanned directories:
  ```toml
  [wallpaper_launcher]
  directories = ["~/dotfiles/wallpapers", "~/wallpapers", "~/dotfiles/live-wallpapers"]
  ```

Both accept `*_open` / `*_close` events, scan on open, and use the same `[bar]`
background/blur/corner radius and `[theme]` colors as the bar.

**Theme launcher** — a right sidebar that slides in from the right edge (rounded
left corners) and re-colours your whole setup at once: kitty, neovim, kshell
itself, and the desktop wallpaper.
- Open with `kshell trigger theme_launcher_toggle`.
- `↑`/`↓` browse, `⏎` or click applies, `Esc` closes. It stays open after
  applying so you can flip through themes and try them.
- Applying rewrites only the relevant values in each config (comments and
  layout are preserved): the kitty `include` line (then `SIGUSR1`s kitty to
  reload), `vim.cmd.colorscheme(...)` plus the lualine palette in `init.lua`,
  the `[theme]` section of `~/.config/kshell/config.toml`, and the wallpaper
  via `NSWorkspace` (or `lwp` for videos).
- Define themes in config:
  ```toml
  [theme_launcher]
  kitty_config = "~/dotfiles/kitty/.config/kitty/kitty.conf"
  nvim_init    = "~/dotfiles/nvim/.config/nvim/init.lua"
  shell_config = "~/.config/kshell/config.toml"
  opacity      = 0.7                 # kitty background_opacity default
  blur         = 40                  # kitty background_blur default

  [[theme_launcher.themes]]
  name       = "Synthwave"
  kitty      = "synthwave.conf"                      # kitty include target
  nvim       = "synthwave"                           # nvim colorscheme
  wallpaper  = "~/dotfiles/wallpapers/synthwave.jpg" # optional
  accent     = "#ff2a85"
  highlight  = "#00d4ff"
  foreground = "#e0d0f0"
  dim        = "#9a80aa"
  mid        = "#00d4ff"
  background = "#100018AA"         # bar colour; lower the alpha for more glass
  opacity    = 0.45                # optional per-theme kitty glass
  blur       = 64
  ```
  Any target may be omitted (say, a theme with no wallpaper) and it is skipped.
  `opacity`/`blur` fall back to the `[theme_launcher]` values, so switching away
  from a glassy theme restores the default.

  Shipped in the example config: **Synthwave**, **Everforest**, **Rose Pine**,
  **Catppuccin**, **Monochrome**, **Tokyo Night**, **Gruvbox**, **Nord**,
  **Kanagawa**, **Dracula**, **One Dark** — each with a kitty theme
  (`~/.config/kitty/<name>.conf`), an nvim colorscheme
  (`~/.config/nvim/colors/<name>.lua`) and a wallpaper in
  `~/dotfiles/wallpapers/`. Adding another is just another
  `[[theme_launcher.themes]]` block plus those three files.

  Wallpapers are sourced from [Omarchy](https://github.com/basecamp/omarchy),
  [tokyo-night/wallpapers](https://github.com/tokyo-night/wallpapers),
  [rose-pine/wallpapers](https://github.com/rose-pine/wallpapers) and
  [dracula/wallpaper](https://github.com/dracula/wallpaper).

**Media centre** — a panel that slides down out of the bar. Its glass is its own
vibrancy view *in the frame's window*, with the bar's and the frame's, so the
three read as one continuous surface — no seam where they meet. It lives in a
clip just below the bar, so it waits behind the bar and slides down out of it,
and only its outward corners are rounded.

The slide is stepped frame by frame (content and glass from one clock) rather
than handed to Core Animation. That is deliberate: the window server renders an
effect view's blur from the layer's *model* geometry, so neither an animated mask
nor an animating view frame moves the blur — without the manual step the glass
snapped out at full size and left a bare sheet sitting under the content.
- Shows the current track's artwork, title/artist/album, a progress bar and
  transport controls. `Space`, `←`/`→` and `Esc` work while it is open.
- Open with `kshell trigger media_center_toggle`, or click the bar's
  now-playing widget (the example config wires that widget's `action` to it).
- Alignment and the termusic helper live in config:
  ```toml
  [media]
  termusic = "~/programs/projects/kshell/scripts/termusic/termusic.sh"
  align    = "leading"        # leading | center | trailing
  ```
- **termusic** is supported directly. It plays through its own Rust backend and
  never registers with macOS' now-playing, so kshell talks to it over its gRPC
  unix socket (`$TMPDIR/termusic.socket`) using `grpcurl` and the protobuf in
  `scripts/termusic/proto/`. That helper also reads tags and album art out of
  the audio file and caches them per track, and drives play/pause/skip. Needs
  `grpcurl` (`brew install grpcurl`) plus `jq` and `ffmpeg` for tags/artwork.
- When termusic is idle the macOS now-playing APIs are used instead
  (`nowplaying-cli`, or Spotify/Music via AppleScript for the widget).
- Bindable on their own: `kshell trigger media_play_pause`, `media_next`,
  `media_previous`.

**Music panel** — the shell's own music player: a bottom sheet with a **Search**
tab (YouTube Music, or paste a link), a **Library** tab, and a player bar. It
replaces an external player rather than driving one.

- Searching runs `yt-dlp` and lists results with thumbnails and durations.
  Downloading extracts audio (`m4a` by default), embeds the cover and tags, and
  streams its progress into the panel. Finished tracks appear in the Library.
- Playback happens **inside kshell** through AVFoundation, so the bar's
  now-playing widget, the media centre and this panel all read the same state,
  with real position, seeking and volume. Music keeps playing with the panel
  closed. (The player also mirrors its state to
  `~/Library/Caches/kshell/now-playing.json`, which is how the bar's
  shell-script widget sees it.)
- Playing is AVFoundation's business, so codecs it cannot open (opus, webm) are
  skipped rather than listed — hence m4a as the download format.
- Enter searches, ⏎ in the field runs it, `Esc` closes. Tabs are clickable;
  the library row under the pointer can be played, and trashed.
  ```toml
  [music]
  directory      = "~/Music/kshell"   # where downloads land
  search_results = 15
  format         = "m4a"
  ytdlp          = "yt-dlp"
  ```
- Bindable: `kshell trigger music_launcher_toggle` (or `_open` / `_close`), and
  `music_play_library` to start the whole library. `music_play_pause`,
  `music_next` and `music_previous` drive it directly.

**Screen border** — a bezel around the screen's edges (caelestia style): a frame
that reaches the screen's edge on every side, bounded inside by a rounded
opening, so it thickens into its corners. The bar nests inside the opening at the
top — its margins and corner radius are raised to the frame's thickness and
radius, and all four of its corners are rounded, so they hug the opening's — and
the launcher panels sit flush against the opening's inner edge, leaving no sliver
of wallpaper between them and the border. It lives in the window
manager's outer gap. A sheet hanging off the bar is inset far enough to clear the
bar's rounded corners.

The frame, the bar and any sheet hanging off the bar are cut from **one** vibrancy
view (held by the border's window, with a mask), not several materials that
happen to be configured alike. Two nearby material views never render the same,
which is what used to leave a visible seam where they met.
- Decoration only: its window ignores mouse events, so it never intercepts a
  click meant for something behind it.
- On by default; toggle it at runtime with `kshell trigger border_toggle`
  (or `border_show` / `border_hide`). With it off, the bar goes back to its own
  material view, flush with the screen's edges.
- The shell steps out of the way of full-screen windows: its windows do not join
  a full-screen space, and a window that fills a display within the current space
  (how window managers such as aerospace do full screen) hides the shell on that
  display until it goes away. A merely maximised window keeps the window
  manager's gaps, so it does not count.
- One per display, and it follows `[bar] display`. Config:
  ```toml
  [border]
  enabled   = true
  inset     = 5              # the frame's thickness is inset + thickness
  thickness = 3
  radius    = 20             # rounds the frame's inner opening
  color     = "$background"  # tokens work; "$accent" gives a brighter frame
  blur      = true           # frost the frame with whatever is behind it
  ```

## JavaScript widgets

A widget with `type = "js"` renders whatever a user script produces. Point it at
a file or inline it:

```toml
[[bar.right]]
type     = "js"
file     = "~/programs/projects/kshell/scripts/widgets/git_branch.js"  # or source = "..."
interval = 1                 # how often render() is called (seconds)
events   = "media_changed"   # optional: forward events to onEvent(name, payload)
```

The script defines `render()` (and optionally `onEvent`):

```js
var branch = "";
exec("git rev-parse --abbrev-ref HEAD", 5, function (out) { branch = out; });

function render() {
  return { icon: "\uf126", label: branch, iconColor: "#ff2a85", labelColor: "#00d4ff" };
}
```

**Host API:** `exec(cmd, interval, cb)`, `execOnce(cmd, cb)`, `shell(cmd)`,
`post(event, payload)`, `battery()`, `ram()`, `volume()`, `date(pattern)`,
`log(msg)`.

**`render()` may return:** `icon`, `label`, `iconColor`, `labelColor`, `action`
(shell command on click), `actionEvent` (event on click), `padding`, `flexible`.

Scripts hot-reload when the file changes.

## CLI

```sh
kshell                             # run the shell
kshell trigger <event> [payload]   # push an event (IPC)
kshell reload                      # reload the config
kshell --diagnose                  # screens, notch, sensors
kshell --version | --help
```

Debug helpers: `kshell --render <file.png> [height]` renders the bar offscreen
(handy for checking layout without eyeballing the screen), `kshell --glyphs`
checks each icon codepoint exists in the font, and `kshell --icons` renders the
icon set large.

## Auto-hide & displays

```toml
[bar]
display        = "all"    # all | main — which displays get a bar
autohide       = true     # slide off the edge until the pointer nears it
autohide_peek  = 2        # pixels left visible while hidden
autohide_zone  = 6        # distance from the edge that reveals it
autohide_delay = 0.4      # seconds before hiding after the pointer leaves
```

Bars can also be toggled at runtime: `kshell trigger bar_hide`, `bar_show`,
`bar_toggle`; the frame likewise with `border_toggle`, `border_show`,
`border_hide`. The music panel likewise with `music_launcher_toggle`, and its
player with `music_play_pause`, `music_next`, `music_previous` and
`music_play_library`. Full-screen apps are left alone: the shell hides on a
display while a window fills it, and returns when it does not. Auto-hide applies per bar; the launcher opens on the display the
pointer is on.

## macOS notes

- **Notch:** on notched displays the bar spans the full width (background runs
  behind the notch) while the left/right widget runs are clamped to the two
  areas beside it, so nothing renders behind the cutout. The notch width is
  auto-detected from `NSScreen`; override with `notch_width` in `[bar]`
  (`0` disables the gap). Run `kshell --diagnose` to print per-screen notch info.
- macOS has no layer-shell `exclusiveZone`; the bar floats and the window
  manager must reserve space. With aerospace, set `gaps.outer.top` to the bar
  height.
- Bars are non-activating panels on every Space, including over full-screen
  apps.

## Architecture

```
Sources/KShellCore/
  App/AppController.swift        NSApplication lifecycle (.accessory)
  Shell/ShellController.swift    one bar per screen; config + display changes
  Shell/BarViewModel.swift       appearance + widget runtimes (observable)
  Surfaces/BarPanel.swift        NSPanel: borderless, non-activating, .statusBar
  Surfaces/BarView.swift         SwiftUI left/center/right widget rows
  Widgets/                       Widget protocol, factory, runtimes
  Services/                      ProcessRunner, Shell, SystemMetrics
  Config/                        TOML models, loader + file watching, colors
Sources/kshell/main.swift        entry point
```

## Roadmap

- **M1** bar + widgets + live reload 
- **M2** click actions, `kshell trigger` IPC event bus, overlay panels + app launcher (done)
- **M3** native system widgets (volume, bluetooth) + live aerospace workspace discovery (done)
- **M4** JavaScriptCore widget scripting with hot reload (done)
- **M5** display selection, auto-hide, launcher on active display, CLI polish (done)
- **M6** `.app` bundle + LaunchAgent + `kshell` CLI

## License

Unlicense — public domain.
