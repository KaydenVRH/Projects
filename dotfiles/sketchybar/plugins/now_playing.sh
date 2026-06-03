#!/usr/bin/env bash

CACHE="$CONFIG_DIR/plugins/.now_playing_cache"

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
        if [ -n "$TITLE" ] && [ "$TITLE" != "(null)" ] && [ "$TITLE" != "null" ]; then
          if [ -n "$ARTIST" ] && [ "$ARTIST" != "(null)" ] && [ "$ARTIST" != "null" ]; then
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

  HAS_ARTWORK=true
  if [ -z "$TRACK" ]; then
    if pgrep -qx termusic-server 2>/dev/null; then
      FILE=$(lsof -c termusic-server 2>/dev/null | grep -E '\.(mp3|flac|m4a|wav|ogg|aac|opus)$' | sort -k2 -rn | head -1 | grep -oE '/.*\.(mp3|flac|m4a|wav|ogg|aac|opus)')
      if [ -n "$FILE" ]; then
        TRACK=$(basename "$FILE" | sed 's/\.[^.]*$//')
        HAS_ARTWORK=false
      fi
    fi
  fi

  PREV=$(cat "$CACHE" 2>/dev/null || echo "")
  if [ -z "$TRACK" ]; then
    sketchybar --set "$NAME" drawing=off
  elif [ "$TRACK" != "$PREV" ]; then
    echo "$TRACK" > "$CACHE"
    sketchybar --set "$NAME" \
      drawing=on \
      label="$TRACK" \
      background.image="media.artwork" \
      background.image.drawing=$([ "$HAS_ARTWORK" = true ] && echo "on" || echo "off") \
      background.image.scale=0.7 \
      background.image.corner_radius=4 \
      background.drawing=$([ "$HAS_ARTWORK" = true ] && echo "on" || echo "off") \
      background.color=0x00000000
  else
    sketchybar --set "$NAME" drawing=on
  fi
fi
