#!/usr/bin/env bash
# Prints "Artist — Title" for the current track, or nothing.
# Works with nowplaying-cli if present, else falls back to Spotify/Music.

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
