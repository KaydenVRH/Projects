#!/usr/bin/env bash

if [ "$SENDER" = "media_change" ] || [ "$SENDER" = "routine" ]; then
  APP="$INFO"

  case "$APP" in
    "Spotify")
      TRACK=$(osascript -e 'tell application "Spotify" to if player state is playing then return name of current track & " · " & artist of current track')
      ;;
    "Music"|"music")
      TRACK=$(osascript -e 'tell application "Music" to if player state is playing then return name of current track & " · " & artist of current track')
      ;;
    *)
      if command -v nowplaying-cli &>/dev/null; then
        TITLE=$(nowplaying-cli get title 2>/dev/null)
        ARTIST=$(nowplaying-cli get artist 2>/dev/null)
        if [ -n "$TITLE" ] && [ "$TITLE" != "(null)" ]; then
          if [ -n "$ARTIST" ] && [ "$ARTIST" != "(null)" ]; then
            TRACK="$TITLE · $ARTIST"
          else
            TRACK="$TITLE"
          fi
        else
          TRACK=""
        fi
      else
        TRACK=""
      fi
      ;;
  esac

  if [ -z "$TRACK" ]; then
    sketchybar --set "$NAME" drawing=off
  else
    sketchybar --set "$NAME" \
      drawing=on \
      label="$TRACK" \
      background.image="media.artwork" \
      background.image.scale=0.7 \
      background.image.corner_radius=4 \
      background.drawing=on \
      background.color=0x00000000
  fi
fi
