#!/usr/bin/env bash
# Build kshell (release by default).
set -euo pipefail
cd "$(dirname "$0")/.."
CONFIG="${1:-release}"
echo "Building kshell ($CONFIG)…"
swift build -c "$CONFIG"
echo "Done: .build/$CONFIG/kshell"
