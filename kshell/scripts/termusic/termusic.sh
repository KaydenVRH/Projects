#!/usr/bin/env bash
# termusic.sh — query and control the running termusic server.
#
# termusic speaks gRPC over a unix socket ($TMPDIR/termusic.socket). We drive it
# with grpcurl and the protobuf that ships next to this script. Track metadata
# and artwork are read from the audio file itself (which is exactly what the
# termusic TUI does) and cached per track, so polling stays cheap.
#
#   termusic.sh info        JSON: source/state/position/duration/title/artist/album/artwork
#   termusic.sh title       "Artist — Title" (empty when nothing is playing)
#   termusic.sh artwork     prints the path to the cached cover image, if any
#   termusic.sh toggle      play/pause
#   termusic.sh next|prev   skip
#   termusic.sh available   exit 0 when a termusic server is up
set -uo pipefail

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROTO_DIR="$SELF_DIR/proto"
SOCKET="${TMPDIR%/}/termusic.socket"
CACHE="${HOME}/Library/Caches/kshell/media"
GRPCURL="$(command -v grpcurl || true)"
JQ="$(command -v jq || true)"
FFPROBE="$(command -v ffprobe || true)"
FFMPEG="$(command -v ffmpeg || true)"

available() { [ -n "$GRPCURL" ] && [ -n "$JQ" ] && [ -S "$SOCKET" ]; }

rpc() { # rpc <Method> [json]
  local method="$1"; shift
  "$GRPCURL" -unix=true -plaintext -import-path "$PROTO_DIR" -proto player.proto \
    "$SOCKET" "player.MusicPlayer/$method" "$@" 2>/dev/null
}

cache_key() { printf '%s' "$1" | shasum -a 256 | cut -c1-16; }

# Metadata for a track path, cached on disk (tags, with a filename fallback).
track_meta() {
  local path="$1" key out
  key="$(cache_key "$path")"
  out="$CACHE/$key.json"
  mkdir -p "$CACHE"
  if [ ! -f "$out" ]; then
    local raw="{}"
    if [ -n "$FFPROBE" ] && [ -f "$path" ]; then
      raw="$("$FFPROBE" -v quiet -print_format json -show_format "$path" 2>/dev/null)"
      [ -z "$raw" ] && raw="{}"
    fi
    printf '%s' "$raw" | "$JQ" -c --arg file "$(basename "$path")" '
      (.format.tags // {}) as $t
      | def pick($a; $b): ($t[$a] // $t[$b] // "");
      def stem: ($file | sub("\\.[^.]+$"; ""));
      (stem) as $s
      | def split_name:
          if ($s | test(" - ")) then
            { artist: ($s | split(" - ")[0]), title: ($s | split(" - ")[1:]) | join(" - ") }
          else { artist: "", title: $s } end;
        ({
          title:  (pick("title";"TITLE")),
          artist: (pick("artist";"ARTIST") // pick("album_artist";"ALBUMARTIST")),
          album:  (pick("album";"ALBUM"))
        }) as $tag
      | (split_name) as $fb
      | {
          title:  (if $tag.title  == "" then $fb.title  else $tag.title  end),
          artist: (if $tag.artist == "" then $fb.artist else $tag.artist end),
          album:  $tag.album
        }' > "$out" 2>/dev/null || echo '{"title":"","artist":"","album":""}' > "$out"
  fi
  cat "$out"
}

# Cover image for a track path, cached on disk (sidecar image, else embedded art).
artwork() {
  local path="$1" key out dir side
  key="$(cache_key "$path")"
  out="$CACHE/$key.png"
  mkdir -p "$CACHE"
  if [ -f "$out" ]; then printf '%s' "$out"; return 0; fi
  [ -f "$path" ] || return 1

  dir="$(dirname "$path")"
  side="$(ls "$dir"/cover.* "$dir"/folder.* "$dir"/Cover.* "$dir"/Folder.* 2>/dev/null | head -1)"
  if [ -n "$side" ]; then
    if [ -n "$FFMPEG" ]; then
      "$FFMPEG" -v error -i "$side" -y "$out" 2>/dev/null
    else
      cp "$side" "$out" 2>/dev/null
    fi
    [ -s "$out" ] && { printf '%s' "$out"; return 0; }
    rm -f "$out"
  fi

  if [ -n "$FFMPEG" ]; then
    "$FFMPEG" -v error -i "$path" -an -vcodec png -y "$out" 2>/dev/null
    [ -s "$out" ] && { printf '%s' "$out"; return 0; }
    rm -f "$out"
  fi
  return 1
}

info() {
  local prog track idx path meta art state
  prog="$(rpc GetProgress)"; [ -z "$prog" ] && prog="{}"
  track="$(rpc GetPlaylist)"; [ -z "$track" ] && track="{}"
  idx="$(printf '%s' "$prog" | "$JQ" -r '.currentTrackIndex // 0')"
  path="$(printf '%s' "$track" | "$JQ" -r --argjson i "${idx:-0}" \
      '.tracks[$i].id.path // .tracks[$i].id.url // empty')"
  meta='{"title":"","artist":"","album":""}'
  [ -n "$path" ] && meta="$(track_meta "$path")"
  art=""; [ -n "$path" ] && art="$(artwork "$path" || true)"

  printf '%s' "$prog" | "$JQ" -c --argjson meta "$meta" --arg path "$path" --arg art "$art" '
    (.progress // {}) as $p
    | (($p.position // {}).secs // "0" | tonumber) as $ps
    | (($p.position // {}).nanos // 0) as $pn
    | ((($p.totalDuration // {}).secs // "0" | tonumber)) as $ds
    | {
        source: "termusic",
        state: (if (.status // 0) == 1 then "playing"
                elif (.status // 0) == 2 then "paused"
                else "stopped" end),
        position: ($ps + ($pn / 1000000000)),
        duration: $ds,
        volume: (.volume // 0),
        path: $path,
        title: ($meta.title // ""),
        artist: ($meta.artist // ""),
        album: ($meta.album // ""),
        artwork: $art
      }'
}

case "${1:-}" in
  available) available && exit 0 || exit 1 ;;
  info)      available || exit 1; info ;;
  title)
    available || exit 0
    info | "$JQ" -r '
      if .state == "stopped" or .title == "" then ""
      elif .artist != "" then "\(.artist) — \(.title)"
      else .title end'
    ;;
  artwork)
    available || exit 1
    p="${2:-}"
    if [ -z "$p" ]; then
      p="$(info | "$JQ" -r '.path // empty')"
    fi
    [ -n "$p" ] && artwork "$p"
    ;;
  toggle)    available || exit 1; rpc TogglePause >/dev/null ;;
  next)      available || exit 1; rpc SkipNext >/dev/null ;;
  prev)      available || exit 1; rpc SkipPrevious >/dev/null ;;
  *) echo "usage: termusic.sh {info|title|artwork|toggle|next|prev|available}" >&2; exit 2 ;;
esac
