# ked-binaries

Prebuilt releases of [ked](../ked), refreshed whenever the editor is
rebuilt. One static binary per platform — no runtime dependencies
beyond the OS (the Linux builds are static musl, so they run on any
Linux distribution).

```
macos-arm64/ked     Apple Silicon macOS
linux-arm64/ked     ARM Linux (aarch64, static musl)
linux-x86_64/ked    Intel/AMD Linux (x86_64, static musl)
```

## Install

Pick your platform's binary, make it executable, and put it on your
`PATH`:

### macOS (Apple Silicon)

```sh
curl -L -o ked https://github.com/KaydenVRH/Projects/raw/main/ked-binaries/macos-arm64/ked
chmod +x ked
mv ked ~/.local/bin/        # or /usr/local/bin/
```

### Linux (x86_64 — most PCs and servers)

```sh
curl -L -o ked https://github.com/KaydenVRH/Projects/raw/main/ked-binaries/linux-x86_64/ked
chmod +x ked
sudo mv ked /usr/local/bin/
```

### Linux (arm64 — Raspberry Pi 4/5, ARM servers)

```sh
curl -L -o ked https://github.com/KaydenVRH/Projects/raw/main/ked-binaries/linux-arm64/ked
chmod +x ked
sudo mv ked /usr/local/bin/
```

`~/.local/bin` and `/usr/local/bin` are usually already on `PATH`; if
not, add `export PATH="$HOME/.local/bin:$PATH"` to your shell config.

Then just run `ked` (optionally `ked some-file.rs`).

## Optional runtime helpers

- **mpv** — music player (`Ctrl+M`)
- **python3 / cc / rustup / go** — whichever parts of `Ctrl+E` (run
  file) you use
- **Nerd Font** — prettier file-tree icons
- A terminal with the **kitty keyboard protocol** (kitty, foot,
  wezterm, iTerm2, ghostty, …) for the `Ctrl+M` binding

## Notes

- The Linux binaries are statically linked against musl — they run on
  glibc and musl systems alike
- macos binary is unsigned; first launch: right-click → Open, or
  `xattr -d com.apple.quarantine ked` after downloading
- These are built with `cargo build --release` and
  `cargo zigbuild --release --target {aarch64,x86_64}-unknown-linux-musl`
