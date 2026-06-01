#!/usr/bin/env bash

IP=$(ipconfig getifaddr en0 2>/dev/null)

if [ -n "$IP" ]; then
  sketchybar --set "$NAME" icon= label="on" icon.color=0xff39ff14
else
  sketchybar --set "$NAME" icon= label="off" icon.color=0x4439ff14
fi
