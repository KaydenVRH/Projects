# Workspace notes

This repo (Projects) contains several personal projects, most notably
`ked/` — a vim-like TUI editor in Rust.

## Telling the user you are done, or stuck

The desktop shell is `kshell`, which has its own notification centre. Do not
assume the user is watching the terminal: say so there when a task finishes, and
especially when you are blocked on them.

```sh
~/dotfiles/scripts/agent-notify.sh done      opencode "ked builds and tests pass"
~/dotfiles/scripts/agent-notify.sh attention opencode "need a decision on X"
~/dotfiles/scripts/agent-notify.sh error     opencode "cross-build failed"
```

- **Always** send `done` when you finish a piece of work — a card is the signal
  that you are done, not a courtesy. `attention` the moment you need an answer or
  an approval, `error` when you stopped without finishing.
- Start the turn with `~/dotfiles/scripts/agent-notify.sh stamp <agent>` and
  short turns stay quiet (under 8 s by default); `attention` always speaks.
- Prefix with `KSHELL_NOTIFY_ALWAYS=1` so it is never suppressed: a terminal can
  be frontmost while your pane is not the one on screen (several agents share one
  window), so focus is not a good reason to stay quiet:
  `KSHELL_NOTIFY_ALWAYS=1 ~/dotfiles/scripts/agent-notify.sh done opencode "..."`.
- It falls back to a macOS banner if kshell isn't running, so it is always safe
  to call.
- The project name comes from the working directory, so call it from the
  project you were working in.
- OpenCode and Claude Code already do this automatically through their hooks;
  this section is for every other agent or harness.

## ked — build & release procedure

When work is done on ked, ALWAYS finish with:

1. `cargo test` (in `ked/`) — must be green
2. `cargo build --release` — the user's symlink `~/.local/bin/ked`
   points at `ked/target/release/ked`, so the release build is what
   they actually run
3. Cross-compile release binaries into `ked-binaries/` (the user wants
   these refreshed whenever ked is rebuilt):

```sh
cd ked
cargo build --release
cargo zigbuild --release --target x86_64-unknown-linux-musl
cargo zigbuild --release --target aarch64-unknown-linux-musl
cp target/release/ked                              ../ked-binaries/macos-arm64/ked
cp target/x86_64-unknown-linux-musl/release/ked    ../ked-binaries/linux-x86_64/ked
cp target/aarch64-unknown-linux-musl/release/ked   ../ked-binaries/linux-arm64/ked
chmod +x ../ked-binaries/*/ked
```

- Linux cross-builds use zig + cargo-zigbuild (both installed via
  brew/cargo). musl targets → static binaries that run on any distro.
- If `ked-binaries/README.md` changes are needed, update it too.

## Committing

- **Commit and push when the change is substantial.** The user's standing
  instruction: a big change (a feature, a fix with tests, a refactor) lands as a
  commit on `main` and is pushed, without asking first.
- Group by intent, not by timestamp: one commit for the shell fix, one for the
  editor feature, one for chores. Never mix unrelated projects in one commit.
- **No secrets, ever.** `.env`, tokens, keys, credentials, private URLs — none of
  them belong in a commit. Check `git diff` before staging; if something looks
  like a credential, stop and ask.
- Small things (a typo, a one-line comment) can ride along with the next real
  change rather than getting a commit of their own.

## Other projects

- `yt-tui/` — terminal YouTube player (Rust). `cargo test` in `yt-tui/`;
  `~/.local/bin/yt-tui` points at `yt-tui/target/release/yt-tui`, so
  finish with `cargo build --release`.  (`pked/` and `lked/` no longer exist.)
- The git repo is the whole workspace (remote:
  github.com/KaydenVRH/Projects, branch `main`).
