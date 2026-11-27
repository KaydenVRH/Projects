#!/usr/bin/env bash
# Restart the kshell launch agent after a rebuild.
#
# `launchctl kickstart -k` can leave kshell dead after the binary changes:
# launchd keeps the old binary's code requirement ("needs LWCR update") and the
# spawn fails with OS_REASON_CODESIGNING. Unloading and reloading the agent
# makes it take the new one.
set -euo pipefail
label=dev.kayden.kshell
plist="$HOME/Library/LaunchAgents/$label.plist"
launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
# bootout returns before the old instance has gone; bootstrapping too soon
# fails with "5: Input/output error", so keep trying for a few seconds.
for _ in $(seq 1 20); do
  launchctl bootstrap "gui/$(id -u)" "$plist" 2>/dev/null && break
  sleep 0.5
done
for _ in $(seq 1 20); do
  if launchctl print "gui/$(id -u)/$label" 2>/dev/null | grep -q "state = running"; then
    echo "kshell running"
    exit 0
  fi
  sleep 0.5
done
echo "kshell did not start — see ~/Library/Logs/kshell/kshell.log" >&2
exit 1
