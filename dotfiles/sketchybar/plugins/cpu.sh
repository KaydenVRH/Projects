#!/usr/bin/env bash

CORE_COUNT=$(sysctl -n hw.ncpu)
TOTAL=$(ps -A -o %cpu | awk '{s+=$1} END {printf "%.0f", s / '"$CORE_COUNT"'}')

sketchybar --set "$NAME" label="${TOTAL}%"
