#!/usr/bin/env bash

USED=$(memory_pressure | awk '/percent/{print $5}' | sed 's/%//')

sketchybar --set "$NAME" label="${USED}%"
