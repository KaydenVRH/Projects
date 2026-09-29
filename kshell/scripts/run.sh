#!/usr/bin/env bash
# Run kshell in the foreground (Ctrl-C to quit).
set -euo pipefail
cd "$(dirname "$0")/.."

# Seed a config on first run so there is something to look at.
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/kshell"
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
  mkdir -p "$CONFIG_DIR"
  cp config/config.example.toml "$CONFIG_DIR/config.toml"
  echo "Wrote starter config to $CONFIG_DIR/config.toml"
fi

swift build
exec .build/debug/kshell
