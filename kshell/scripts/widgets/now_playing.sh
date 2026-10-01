#!/usr/bin/env bash
# Prints "Artist — Title" for the current track, or nothing.
# termusic first (it never shows up in macOS' now-playing), then nowplaying-cli,
# then Spotify/Music via AppleScript.

# kshell's own music player comes first: it is the primary backend now and
# mirrors its state to a sidecar for scripts like this one.
sidecar="${HOME}/Library/Caches/kshell/now-playing.json"
if [ -f "$sidecar" ]; then
  line=$(jq -r 'if .title == "" then "" else "\(.artist) — \(.title)" end' "$sidecar" 2>/dev/null)
  if [ -n "$line" ] && [ "$line" != " — " ]; then
    printf '%s\n' "$line" | sed 's/^ — //'
    exit 0
  fi
fi

SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
termusic="$SELF_DIR/../termusic/termusic.sh"
if [ -x "$termusic" ] && "$termusic" available; then
  track="$("$termusic" title 2>/dev/null)"
  if [ -n "$track" ]; then echo "$track"; exit 0; fi
fi

if command -v nowplaying-cli >/dev/null 2>&1; then
  title=$(nowplaying-cli get title 2>/dev/null)
  artist=$(nowplaying-cli get artist 2>/dev/null)
  if [ -n "$title" ]; then
    if [ -n "$artist" ]; then echo "$artist — $title"; else echo "$title"; fi
  fi
  exit 0
fi

osascript -e 'tell application "Spotify"
  if it is running then return (artist of current track) & " — " & (name of current track)
end tell' 2>/dev/null && exit 0

osascript -e 'tell application "Music"
  if it is running then return (artist of current track) & " — " & (name of current track)
end tell' 2>/dev/null
