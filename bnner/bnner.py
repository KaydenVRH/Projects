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
import json
import os
import shlex
import signal
import subprocess
import sys
import time
from pathlib import Path

STATE_DIR = Path("/tmp/bnner")


def _save_state(info):
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    (STATE_DIR / f"{os.getpid()}.json").write_text(json.dumps(info))


def _clear_state():
    (STATE_DIR / f"{os.getpid()}.json").unlink(missing_ok=True)


def _list_states():
    states = []
    for f in STATE_DIR.glob("*.json"):
        try:
            data = json.loads(f.read_text())
            pid = data.get("pid", 0)
            os.kill(pid, 0)
            data["elapsed"] = time.time() - data.get("started_at", time.time())
            states.append(data)
        except (ProcessLookupError, OSError, json.JSONDecodeError):
            f.unlink(missing_ok=True)
    return sorted(states, key=lambda s: s.get("started_at", 0))


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
    _save_state({"pid": os.getpid(), "type": "watch", "title": title,
                 "cmd": cmd, "started_at": time.time()})
    try:
        proc = subprocess.run(cmd)
    finally:
        _clear_state()
    rc = proc.returncode
    msg = f"exited with code {rc}" if rc != 0 else "done"
    body = f"{shlex.join(cmd)}: {msg}"
    (notify := _notify_term if term else _notify_macos)(title, body)
    sys.exit(rc)


def cmd_monitor():
    import curses

    def _draw(stdscr):
        curses.use_default_colors()
        if curses.has_colors():
            curses.init_pair(1, curses.COLOR_WHITE, curses.COLOR_BLUE)
            curses.init_pair(2, curses.COLOR_BLACK, curses.COLOR_WHITE)
            curses.init_pair(3, curses.COLOR_GREEN, -1)
            curses.init_pair(4, curses.COLOR_CYAN, -1)
            curses.init_pair(5, curses.COLOR_YELLOW, -1)
            curses.init_pair(6, curses.COLOR_RED, -1)
        curses.curs_set(0)
        curses.halfdelay(5)

        selected = 0
        while True:
            height, width = stdscr.getmaxyx()
            stdscr.clear()

            h = " bnner monitor "
            stdscr.attron(curses.color_pair(1) if curses.has_colors() else curses.A_REVERSE)
            stdscr.addstr(0, 0, " " * width)
            stdscr.addstr(0, 0, h[:width - 1])
            stdscr.attroff(curses.color_pair(1) if curses.has_colors() else curses.A_REVERSE)

            states = _list_states()
            if selected >= len(states):
                selected = max(0, len(states) - 1)

            if not states:
                if curses.has_colors():
                    stdscr.attron(curses.color_pair(4))
                stdscr.addstr(2, 2, "no running bnnners")
                if curses.has_colors():
                    stdscr.attroff(curses.color_pair(4))

            for i, s in enumerate(states):
                y = 2 + i
                if y >= height - 1:
                    break
                pid = s.get("pid", 0)
                stype = s.get("type", "?")
                title = s.get("title", "bnner")
                elapsed = int(s.get("elapsed", 0))

                if stype == "timer":
                    dur = s.get("duration", 0)
                    remaining = max(0, dur - elapsed)
                    label = f" [{remaining}s] {title}: {s.get('message', '')}"
                else:
                    cmd = shlex.join(s.get("cmd", []))
                    label = f" [{elapsed}s] {title}: {cmd}"

                if len(label) > width - 1:
                    label = label[:width - 4] + "..."

                is_sel = i == selected
                if is_sel:
                    style = curses.A_REVERSE
                    if curses.has_colors():
                        style |= curses.color_pair(2)
                elif curses.has_colors():
                    style = curses.color_pair(3) if stype == "timer" else curses.color_pair(4)
                else:
                    style = 0

                prefix = " >" if is_sel else "  "
                stdscr.attron(style)
                stdscr.addstr(y, 0, f"{prefix} {label}")
                stdscr.attroff(style)

            keys = "k:kill  q:quit"
            safe_w = max(1, width - 1)
            bar_style = curses.color_pair(5) if curses.has_colors() else curses.A_REVERSE
            stdscr.attron(bar_style | curses.A_BOLD)
            stdscr.addstr(height - 1, 0, " " * safe_w)
            stdscr.addstr(height - 1, 0, keys[:safe_w])
            stdscr.attroff(bar_style | curses.A_BOLD)

            stdscr.refresh()
            key = stdscr.getch()

            if key == ord("q"):
                break
            elif key == ord("j") or key == curses.KEY_DOWN:
                if states:
                    selected = min(selected + 1, len(states) - 1)
            elif key == ord("k") or key == curses.KEY_UP:
                selected = max(selected - 1, 0)
            elif key == ord("K"):
                if states:
                    pid = states[selected].get("pid", 0)
                    try:
                        os.kill(pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass

    curses.wrapper(_draw)


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
    if len(sys.argv) > 1 and sys.argv[1] == "monitor":
        if "-h" in sys.argv or "--help" in sys.argv:
            print("usage: bnner monitor\n\nShow running timers and watches.")
            return
        cmd_monitor()
        return

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
        _save_state({"pid": os.getpid(), "type": "timer", "title": args.title,
                     "message": message, "started_at": time.time(),
                     "duration": args.timer})
        try:
            time.sleep(args.timer)
        except KeyboardInterrupt:
            _clear_state()
            notify(args.title, "timer cancelled")
            sys.exit(1)
        _clear_state()

    notify(args.title, message)


if __name__ == "__main__":
    main()
