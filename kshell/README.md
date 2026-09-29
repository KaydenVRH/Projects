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
accent    = "#83c092"    # icons
highlight = "#7fbbb3"    # labels

[[bar.left]]
type = "apple"
icon_hex = "f8ff"

[[bar.right]]
type = "clock"
format = "EEE h:mm a"
```

Colors accept `#RRGGBB`, `#RRGGBBAA` (alpha last) or `0xAARRGGBB` (alpha
first). Icons can be written as a literal `icon = ""` or as an ASCII
codepoint `icon_hex = "f8ff"` (handy because Private-Use glyphs are awkward to
paste).

## Widgets

| type | fields | notes |
|---|---|---|
| `apple` | `icon_hex`, `icon_font` | Apple logo, drawn from `SF Pro Display` |
| `workspaces` | `workspaces` = "1,2,…", `focused_color`, `unfocused_color` | aerospace; click to switch. Discovered live from aerospace when no list is given |
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
(`OverlayEdge`), sits **flush** with that edge, and rounds only the corners
facing away from it (`SheetShape`) so it reads as extending out of the edge.
Frosted with the bar's blur, and it animates in from just past the edge. Three
launchers are built on it:

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
`bar_toggle`. Auto-hide applies per bar; the launcher opens on the display the
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
