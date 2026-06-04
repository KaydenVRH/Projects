#!/usr/bin/env python3
"""bnner — send notifications to terminal or macOS Notification Center.

Usage:
    bnner <message>
    bnner --term <message>
    bnner --timer 30 "pasta is ready"
    bnner --watch make test
    bnner --title "build" --watch cc main.c
"""

import argparse
import os
import shlex
import subprocess
import sys
import time


def _notify_macos(title, message):
    escaped = message.replace('"', r'\"')
    etitle = title.replace('"', r'\"')
    script = f'display notification "{escaped}" with title "{etitle}"'
    subprocess.run(["osascript", "-e", script], capture_output=True)


def _notify_term(title, message):
    text = f"{title}: {message}" if title != "bnner" else message
    tmux = os.environ.get("TMUX")
    if tmux:
        subprocess.run(["tmux", "display-message", text],
                       capture_output=True)
        sys.stdout.write(f"\033Ptmux;\033]9;{text}\007\033\\")
    else:
        sys.stdout.write(f"\033]9;{text}\007")
    sys.stdout.flush()


def _watch(cmd, title, term):
    proc = subprocess.run(cmd, capture_output=True, text=True)
    rc = proc.returncode
    msg = f"exited with code {rc}" if rc != 0 else "done"
    suffix = f" ({proc.stderr.strip()[:60]})" if rc != 0 and proc.stderr.strip() else ""
    body = f"{shlex.join(cmd)}: {msg}{suffix}"
    (notify := _notify_term if term else _notify_macos)(title, body)
    sys.exit(rc)


def main():
    parser = argparse.ArgumentParser(
        prog="bnner",
        description="Send notifications to terminal or macOS."
    )
    parser.add_argument("message", nargs="*", help="message text")
    parser.add_argument("--term", action="store_true",
                        help="send notification to terminal instead of macOS")
    parser.add_argument("--timer", type=int, default=None,
                        help="wait N seconds before notifying")
    parser.add_argument("--title", default="bnner",
                        help="notification title (default: bnner)")
    parser.add_argument("--watch", nargs=argparse.REMAINDER,
                        help="run a command and notify when it finishes")
    args = parser.parse_args()

    notify = _notify_term if args.term else _notify_macos

    if args.watch:
        _watch(args.watch, args.title, args.term)
        return

    message = " ".join(args.message) if args.message else ""
    if not message:
        parser.print_help()
        sys.exit(1)

    if args.timer is not None:
        try:
            time.sleep(args.timer)
        except KeyboardInterrupt:
            notify(args.title, "timer cancelled")
            sys.exit(1)

    notify(args.title, message)


if __name__ == "__main__":
    main()
