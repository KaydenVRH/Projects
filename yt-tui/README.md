# yt-tui

Search and watch YouTube in the terminal — real video, at the resolution
of your window, with sound in sync. Written in **Rust** with ratatui.

```sh
cargo build --release
./target/release/yt-tui                      # search from inside
./target/release/yt-tui lofi hip hop         # start with a search
./target/release/yt-tui https://youtu.be/…   # play a link straight away
```

Needs `yt-dlp`, `ffmpeg` and `mpv` (`brew install yt-dlp ffmpeg mpv`), and
a terminal that speaks the kitty graphics protocol (kitty, Ghostty) for
the picture. Works inside tmux with `set -g allow-passthrough on`.
Anywhere else it plays audio only, with the same controls.

## Keys

| Results | | Player | |
|---|---|---|---|
| `/` | search (or paste a link) | `space` / `k` | pause |
| `j` `k` / `↑` `↓` | move | `←` `→` / `h` `l` | seek 5 s |
| `d` `u` | page | `H` `L` | seek 30 s |
| `g` `G` | top / bottom | `0`–`9` | jump to 0 %–90 % |
| `enter` | play | `↑` `↓` | volume |
| `a` | play audio only | `m` | mute |
| `A` | autoplay the next result | `f` | fullscreen |
| `?` | help | `n` `p` | next / previous result |
| `q` | quit | `q` / `esc` | back to results |

The mouse works too: scroll the results, click one to select it, click
the progress bar to seek.

## Options

```
--fps N          highest frame rate to play (default 30)
--quality H      highest stream height, e.g. 720 or 1080 (default 1080)
--max-width W    widest frame to decode before kitty scales it (default 1920)
--results N      search results to fetch (default 30)
--volume N       starting volume, 0-100 (default 80)
```

| Environment | |
|---|---|
| `YT_TUI_GRAPHICS=on\|off` | force kitty graphics on or off |
| `YT_TUI_TRANSFER=shm\|direct` | how pixels reach the terminal (direct is automatic over SSH) |
| `YT_TUI_PLAYER_CLIENT=name` | yt-dlp player client (default: yt-dlp's own choice) |
| `YT_TUI_LOG=path` | write a debug log |

## How it works

```
yt-dlp ──► stream URLs ──┬─► ffmpeg ──► RGB frames ──► shared memory ──► kitty
                         │      (sized to the window)        ▲
                         └─► mpv (audio) ── position ───────┘ clock
```

- **yt-dlp** searches and resolves streams. Resolution wins over frame
  rate (a 1080p60 stream beats 480p30 — ffmpeg drops the extra frames),
  then h264, which decodes in hardware on every Mac. yt-tui asks for
  yt-dlp's default player clients, so a `player_client` in your yt-dlp
  config can't leave it with storyboards only.
- **ffmpeg** decodes with VideoToolbox and scales each frame to the exact
  pixel size of the picture on screen (capped by `--max-width`; kitty
  upscales the rest on the GPU).
- **Shared memory** carries the pixels: each frame is a POSIX shm object,
  and only a ~100 byte escape sequence crosses the tty. That is what makes
  full resolution at full frame rate possible; base64 over the tty would
  be ~100 MB/s at 1080p30. Over SSH it falls back to chunked base64.
- **Unicode placeholders** place the picture. The image gets a *virtual*
  placement, and the cells it covers are ordinary text (U+10EEEE plus two
  diacritics for row and column, coloured with the image id). tmux treats
  them like any other text, so the video stays in its pane and never
  fights the interface for the cursor.
- **mpv** plays the audio and is the master clock. A frame is shown once
  mpv's position reaches its timestamp; frames that fall behind are
  dropped rather than shown late, so a slow moment costs smoothness,
  never sync. Live streams, which can't seek, run on a wall clock.
- Every graphics command carries `q=2`, so the terminal never replies —
  replies would otherwise arrive on stdin as stray text and phantom keys.

## Layout

```
src/
  main.rs     terminal setup, options, the event loop
  app.rs      state, background jobs, key and mouse handling
  ui.rs       drawing: search, results, preview, player, help
  youtube.rs  yt-dlp: search, link lookup, stream selection
  player.rs   playback: clock, frame pacing, seeking, picture fitting
  video.rs    ffmpeg decoder thread with recycled frame buffers
  audio.rs    mpv over its JSON IPC socket
  gfx.rs      kitty graphics: shm / chunked transfer, tmux, placeholders
  thumbs.rs   thumbnail fetching for the preview pane
  log.rs      opt-in debug log
```

`cargo test` covers the protocol encoding, placeholder layout, picture
fitting, stream selection parsing and the mpv state machine.
