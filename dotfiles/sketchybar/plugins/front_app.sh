#!/bin/sh

if [ "$SENDER" = "front_app_switched" ]; then
  sketchybar --set "$NAME" \
    label="$INFO" \
    background.image="app.$INFO" \
    background.image.scale=0.75 \
    background.image.corner_radius=4 \
    background.drawing=on \
    background.color=0x00000000
fi
