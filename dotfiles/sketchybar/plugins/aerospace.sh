#!/usr/bin/env bash

if [ -z "$1" ]; then
    ws=${NAME#space.}
else
    ws=$1
fi

focused=$(aerospace list-workspaces --focused 2>/dev/null)
windows=$(aerospace list-windows --workspace "$ws" 2>/dev/null)

if [ "$ws" = "$focused" ]; then
    sketchybar --set $NAME drawing=on label.highlight=on
elif [ -n "$windows" ]; then
    sketchybar --set $NAME drawing=on label.highlight=off
else
    sketchybar --set $NAME drawing=off
fi
