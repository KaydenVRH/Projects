#!/usr/bin/env python3
"""lwp - Live Wallpaper Player for macOS.

Usage:
    lwp set [--hide-icons] <video>    Set video as wallpaper
    lwp stop                           Stop wallpaper
    lwp status                         Show status
    lwp list [dir]                     List video files
"""

import argparse
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

STATE_DIR = Path("/tmp/lwp")
PID_FILE = STATE_DIR / "pid"
VIDEO_FILE = STATE_DIR / "current_video.txt"
ICONS_STATE_FILE = STATE_DIR / "icons_hidden"
VIDEO_EXTS = {"*.mp4", "*.mov", "*.m4v", "*.avi", "*.mkv", "*.webm"}
CONFIG_DIR = Path.home() / ".lwp"
CONFIG_FILE = CONFIG_DIR / "config.json"


def _ensure_state_dir():
    STATE_DIR.mkdir(parents=True, exist_ok=True)


def _icons_hidden():
    return ICONS_STATE_FILE.exists()


def _hide_desktop_icons():
    subprocess.run(
        ["defaults", "write", "com.apple.finder", "CreateDesktop", "-bool", "false"],
        capture_output=True,
    )
    subprocess.run(["killall", "Finder"], capture_output=True)
    ICONS_STATE_FILE.write_text("1")


def _show_desktop_icons():
    subprocess.run(
        ["defaults", "write", "com.apple.finder", "CreateDesktop", "-bool", "true"],
        capture_output=True,
    )
    subprocess.run(["killall", "Finder"], capture_output=True)
    ICONS_STATE_FILE.unlink(missing_ok=True)


def _list_videos(directory="."):
    dir_path = Path(directory).expanduser().resolve()
    videos = []
    for ext in VIDEO_EXTS:
        videos.extend(dir_path.glob(ext))
        videos.extend(dir_path.glob(ext.upper()))
    return sorted(videos)


def _get_running_video():
    if VIDEO_FILE.exists():
        return VIDEO_FILE.read_text().strip()
    return None


def _save_config(video_path, hide_icons):
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    CONFIG_FILE.write_text(json.dumps({"last_video": video_path, "hide_icons": hide_icons}))


def _load_config():
    if CONFIG_FILE.exists():
        try:
            return json.loads(CONFIG_FILE.read_text())
        except (json.JSONDecodeError, OSError):
            pass
    return {}


def _stop_wallpaper():
    if PID_FILE.exists():
        pid_str = PID_FILE.read_text().strip()
        if pid_str:
            try:
                pid = int(pid_str)
                os.kill(pid, signal.SIGTERM)
                for _ in range(20):
                    try:
                        os.kill(pid, 0)
                        time.sleep(0.1)
                    except ProcessLookupError:
                        break
                else:
                    os.kill(pid, signal.SIGKILL)
            except (ProcessLookupError, ValueError, OSError):
                pass
        PID_FILE.unlink(missing_ok=True)


def _run_wallpaper_process(video_path, hide_icons=False, quiet=False):
    _ensure_state_dir()
    log_path = STATE_DIR / "lwp.log"
    venv_python = Path(__file__).resolve().parent / ".venv" / "bin" / "python3"
    python = venv_python if venv_python.exists() else sys.executable
    args = [python, __file__, "_run", "--path", video_path]
    if hide_icons:
        args.append("--hide-icons")
    with open(log_path, "a") as logf:
        logf.write(f"\n--- starting wallpaper with {video_path} ---\n")
        proc = subprocess.Popen(
            args,
            stdout=subprocess.DEVNULL,
            stderr=logf,
            stdin=subprocess.DEVNULL,
        )
    PID_FILE.write_text(str(proc.pid))
    if not quiet:
        print(f"wallpaper started (pid: {proc.pid})")


def cmd_set(video_path, hide_icons=False):
    video_path = os.path.abspath(video_path)
    if not os.path.isfile(video_path):
        print(f"error: file not found: {video_path}", file=sys.stderr)
        sys.exit(1)
    _ensure_state_dir()
    _stop_wallpaper()
    if hide_icons:
        _hide_desktop_icons()
    elif _icons_hidden():
        _show_desktop_icons()
    VIDEO_FILE.write_text(video_path + "\n")
    _save_config(video_path, hide_icons)
    _run_wallpaper_process(video_path, hide_icons)


def cmd_stop(quiet=False):
    _ensure_state_dir()
    _stop_wallpaper()
    if _icons_hidden():
        _show_desktop_icons()
    if not quiet:
        print("wallpaper stopped")


def cmd_status():
    _ensure_state_dir()
    pid = None
    video = None
    if PID_FILE.exists():
        pid = PID_FILE.read_text().strip()
    if VIDEO_FILE.exists():
        video = VIDEO_FILE.read_text().strip()
    if pid:
        try:
            os.kill(int(pid), 0)
            print(f"running (pid: {pid})")
            if video:
                print(f"video: {video}")
            if _icons_hidden():
                print("desktop icons: hidden")
            return
        except (ProcessLookupError, ValueError, OSError):
            pass
    print("not running")
    PID_FILE.unlink(missing_ok=True)


def cmd_list(directory="."):
    videos = _list_videos(directory)
    if videos:
        for v in videos:
            print(v)
    else:
        print(f"no video files found in {Path(directory).resolve()}")


def _wallpaper_main(video_path, hide_icons):
    try:
        import AppKit
        import AVFoundation
    except ImportError as e:
        print(f"error: PyObjC required (pip install PyObjC)\n  ({e})", file=sys.stderr)
        sys.exit(1)

    log_path = STATE_DIR / "lwp.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log = open(log_path, "w")
    def logmsg(msg):
        log.write(f"{msg}\n")
        log.flush()

    try:
        from Quartz import kCGDesktopWindowLevel, kCGDesktopIconWindowLevel
        window_level = kCGDesktopWindowLevel
        logmsg(f"window level raw: {window_level}")
    except ImportError:
        window_level = -2147483648

    app = AppKit.NSApplication.sharedApplication()
    app.setActivationPolicy_(AppKit.NSApplicationActivationPolicyAccessory)

    class WallpaperDelegate(AppKit.NSObject):
        def applicationDidFinishLaunching_(self, notification):
            logmsg("did finish launching")

            screens = AppKit.NSScreen.screens()
            logmsg(f"screens: {len(screens)}")
            self.windows = []
            self.players = []

            for screen in screens:
                frame = screen.frame()
                logmsg(f"  screen frame: {frame}")

                win = AppKit.NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
                    frame,
                    AppKit.NSWindowStyleMaskBorderless,
                    AppKit.NSBackingStoreBuffered,
                    False,
                )
                win.setTitle_("Live Wallpaper")
                win.setOpaque_(True)
                win.setBackgroundColor_(AppKit.NSColor.blackColor())
                win.setLevel_(window_level)
                win.setCollectionBehavior_(
                    AppKit.NSWindowCollectionBehaviorCanJoinAllSpaces
                    | AppKit.NSWindowCollectionBehaviorStationary
                    | AppKit.NSWindowCollectionBehaviorIgnoresCycle
                )
                win.setIgnoresMouseEvents_(True)

                url = AppKit.NSURL.fileURLWithPath_(video_path)
                player = AVFoundation.AVPlayer.alloc().initWithURL_(url)
                player.setActionAtItemEnd_(AVFoundation.AVPlayerActionAtItemEndNone)
                player.setVolume_(0.0)

                video_view = AppKit.NSView.alloc().initWithFrame_(frame)
                video_view.setWantsLayer_(True)
                video_view.setAutoresizingMask_(
                    AppKit.NSViewWidthSizable | AppKit.NSViewHeightSizable
                )

                player_layer = AVFoundation.AVPlayerLayer.alloc().init()
                player_layer.setFrame_(video_view.bounds())
                player_layer.setAutoresizingMask_(
                    AppKit.NSViewWidthSizable | AppKit.NSViewHeightSizable
                )
                player_layer.setVideoGravity_(
                    AVFoundation.AVLayerVideoGravityResizeAspectFill
                )
                player_layer.setPlayer_(player)
                video_view.layer().addSublayer_(player_layer)

                win.setContentView_(video_view)
                win.makeKeyAndOrderFront_(None)

                player.play()

                self.windows.append(win)
                self.players.append(player)

            nc = AppKit.NSNotificationCenter.defaultCenter()
            for p in self.players:
                nc.addObserver_selector_name_object_(
                    self,
                    "playerItemDidReachEnd:",
                    AVFoundation.AVPlayerItemDidPlayToEndTimeNotification,
                    p.currentItem(),
                )
            logmsg("wallpaper running")

        def playerItemDidReachEnd_(self, notification):
            for p in self.players:
                p.seekToTime_(AVFoundation.kCMTimeZero)
                p.play()

    delegate = WallpaperDelegate.alloc().init()
    app.setDelegate_(delegate)

    signal.signal(signal.SIGTERM, lambda *_: app.terminate_(None))

    def cleanup():
        if hide_icons:
            try:
                subprocess.run(
                    ["defaults", "write", "com.apple.finder", "CreateDesktop", "-bool", "true"],
                    capture_output=True,
                )
                subprocess.run(["killall", "Finder"], capture_output=True)
            except Exception:
                pass

    import atexit
    atexit.register(cleanup)

    app.run()


def cmd_tui(directory="."):
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

        videos = _list_videos(directory)
        selected = 0
        search = ""
        searching = False
        status_msg = ""
        msg_type = "info"

        def get_filtered():
            if search:
                return [v for v in videos if search.lower() in v.name.lower()]
            return list(videos)

        while True:
            height, width = stdscr.getmaxyx()
            if height < 6 or width < 40:
                stdscr.clear()
                stdscr.addstr(0, 0, "Terminal too small (min 40x6)")
                stdscr.refresh()
                stdscr.getch()
                continue

            filtered = get_filtered()
            if selected >= len(filtered):
                selected = max(0, len(filtered) - 1)

            stdscr.clear()

            # Header
            h = f" lwp - Live Wallpaper Player  ({len(videos)} videos) "
            stdscr.attron(curses.color_pair(1) if curses.has_colors() else curses.A_REVERSE)
            stdscr.addstr(0, 0, " " * width)
            stdscr.addstr(0, 0, h[:width - 1])
            stdscr.attroff(curses.color_pair(1) if curses.has_colors() else curses.A_REVERSE)

            # Status line - currently playing
            running = _get_running_video()
            hid = _icons_hidden()
            line = 1
            if running:
                try:
                    rn = os.path.basename(running)
                except Exception:
                    rn = running
                t = f" >> Now: {rn}"
                if hid:
                    t += "  [icons hidden]"
                if curses.has_colors():
                    stdscr.attron(curses.color_pair(3))
                stdscr.addstr(line, 0, t[:width - 1])
                if curses.has_colors():
                    stdscr.attroff(curses.color_pair(3))
                line += 1

            # Search input bar
            if searching:
                s = f" Search: {search}"
                if curses.has_colors():
                    stdscr.attron(curses.color_pair(4))
                stdscr.addstr(line, 0, s[:width - 1])
                if curses.has_colors():
                    stdscr.attroff(curses.color_pair(4))
                line += 1

            # Video list
            list_start = line + 1
            list_height = height - 2 - list_start

            if not filtered:
                msg = "no videos found" if not search else "no matches for filter"
                if curses.has_colors():
                    stdscr.attron(curses.color_pair(4))
                stdscr.addstr(list_start, 2, msg)
                if not search:
                    stdscr.addstr(list_start + 1, 2, "press 'r' to refresh")
                if curses.has_colors():
                    stdscr.attroff(curses.color_pair(4))

            # Scroll window centered on selection
            scroll = max(0, selected - list_height // 2)
            if scroll + list_height > len(filtered):
                scroll = max(0, len(filtered) - list_height)

            for i, v in enumerate(filtered[scroll:scroll + list_height]):
                y = list_start + i
                idx = scroll + i
                is_sel = idx == selected
                is_playing = False
                if running:
                    try:
                        is_playing = os.path.samefile(str(v), running)
                    except (OSError, FileNotFoundError):
                        pass

                if is_sel:
                    prefix = " >"
                elif is_playing:
                    prefix = " *"
                else:
                    prefix = "  "
                label = f"{prefix} {v.name}"
                if len(label) > width - 1:
                    label = label[:width - 4] + "..."
                if y >= height - 1:
                    continue

                if is_sel:
                    style = curses.A_REVERSE
                    if curses.has_colors():
                        style |= curses.color_pair(2)
                elif is_playing:
                    style = curses.color_pair(3) if curses.has_colors() else curses.A_BOLD
                else:
                    style = curses.color_pair(4) if curses.has_colors() else 0

                stdscr.attron(style)
                stdscr.addstr(y, 0, label)
                stdscr.attroff(style)

            # Status bar (avoid bottom-right corner: max width-1)
            keys = "Enter:Set  s:Stop  i:Icons  /:Search  r:Refresh  q:Quit"
            bar_style = curses.color_pair(5) if curses.has_colors() else curses.A_REVERSE
            stdscr.attron(bar_style | curses.A_BOLD)
            safe_w = max(1, width - 1)
            stdscr.addstr(height - 1, 0, " " * safe_w)
            if status_msg:
                s_style = curses.color_pair(6) if (msg_type == "error" and curses.has_colors()) else (curses.color_pair(3) if curses.has_colors() else curses.A_BOLD)
                stdscr.attron(s_style)
                stdscr.addstr(height - 1, 0, status_msg[:safe_w])
                stdscr.attroff(s_style)
            else:
                stdscr.addstr(height - 1, 0, keys[:safe_w])
            stdscr.attroff(bar_style | curses.A_BOLD)

            stdscr.refresh()

            # Input handling
            key = stdscr.getch()

            if searching:
                if key == 27:
                    searching = False
                    search = ""
                elif key in (curses.KEY_BACKSPACE, 127, 8):
                    search = search[:-1]
                elif key in (ord("\n"), curses.KEY_ENTER):
                    searching = False
                elif 32 <= key <= 126:
                    search += chr(key)
                continue

            if key == ord("/"):
                searching = True
                search = ""
                status_msg = ""
            elif key in (ord("q"), 27):
                break
            elif key in (ord("j"), curses.KEY_DOWN):
                if filtered:
                    selected = min(selected + 1, len(filtered) - 1)
            elif key in (ord("k"), curses.KEY_UP):
                selected = max(selected - 1, 0)
            elif key == ord("g"):
                selected = 0
            elif key == ord("G"):
                if filtered:
                    selected = len(filtered) - 1
            elif key == curses.KEY_PPAGE:
                selected = max(selected - list_height, 0) if list_height > 0 else 0
            elif key == curses.KEY_NPAGE:
                if list_height > 0 and filtered:
                    selected = min(selected + list_height, len(filtered) - 1)
            elif key in (ord("\n"), curses.KEY_ENTER):
                if filtered:
                    vp = str(filtered[selected])
                    if not os.path.isfile(vp):
                        status_msg = "error: file not found"
                        msg_type = "error"
                    else:
                        _ensure_state_dir()
                        _stop_wallpaper()
                        if not hid and _icons_hidden():
                            _show_desktop_icons()
                        VIDEO_FILE.write_text(vp + "\n")
                        _save_config(vp, hid)
                        _run_wallpaper_process(vp, hid, quiet=True)
                        status_msg = f"set: {filtered[selected].name}"
                        msg_type = "info"
            elif key == ord("s"):
                cmd_stop(quiet=True)
                status_msg = "wallpaper stopped"
                msg_type = "info"
            elif key == ord("i"):
                if _icons_hidden():
                    _show_desktop_icons()
                    status_msg = "desktop icons shown"
                else:
                    _hide_desktop_icons()
                    status_msg = "desktop icons hidden"
                msg_type = "info"
            elif key == ord("r"):
                videos = _list_videos(directory)
                status_msg = f"refreshed: {len(videos)} videos"
                msg_type = "info"
            else:
                status_msg = ""

    curses.wrapper(_draw)


def _autostart_plist():
    return Path.home() / "Library" / "LaunchAgents" / "com.lwp.wallpaper.plist"


def _enable_autostart():
    script = Path(__file__).resolve()
    venv_python = script.parent / ".venv" / "bin" / "python3"
    python = str(venv_python) if venv_python.exists() else sys.executable

    plist = _autostart_plist()
    plist.parent.mkdir(parents=True, exist_ok=True)

    content = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.lwp.wallpaper</string>
    <key>ProgramArguments</key>
    <array>
        <string>{python}</string>
        <string>{script}</string>
        <string>autostart</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
    <key>StandardOutPath</key>
    <string>{CONFIG_DIR}/autostart.log</string>
    <key>StandardErrorPath</key>
    <string>{CONFIG_DIR}/autostart.log</string>
</dict>
</plist>"""
    plist.write_text(content)
    uid = os.getuid()
    loaded = False
    for cmd in [["launchctl", "bootstrap", f"gui/{uid}", str(plist)],
                ["launchctl", "load", str(plist)]]:
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode == 0 or "already bootstrapped" in result.stderr:
            loaded = True
            break
    if loaded:
        print(f"autostart enabled: {plist}")
    else:
        print(f"error enabling autostart", file=sys.stderr)
        sys.exit(1)


def _disable_autostart():
    plist = _autostart_plist()
    uid = os.getuid()
    subprocess.run(["launchctl", "bootout", f"gui/{uid}/com.lwp.wallpaper"],
                    capture_output=True)
    subprocess.run(["launchctl", "unload", str(plist)],
                    capture_output=True)
    if plist.exists():
        plist.unlink()
        print("autostart disabled")
    else:
        print("autostart not enabled")


def _autostart_status():
    plist = _autostart_plist()
    if plist.exists():
        print("autostart: enabled")
    else:
        print("autostart: disabled")


def _autostart_run():
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    log_path = CONFIG_DIR / "autostart.log"
    log = open(log_path, "w")

    def logmsg(msg):
        log.write(f"{msg}\n")
        log.flush()

    logmsg(f"[{time.strftime('%H:%M:%S')}] autostart run starting")
    config = _load_config()
    video = config.get("last_video", "")
    if not video:
        logmsg("no video configured, exiting")
        sys.exit(0)
    if not os.path.isfile(video):
        logmsg(f"video file not found: {video}")
        sys.exit(0)

    hide_icons = config.get("hide_icons", False)
    logmsg(f"video={video} hide_icons={hide_icons}")
    _ensure_state_dir()
    _stop_wallpaper()
    if hide_icons and not _icons_hidden():
        logmsg("hiding desktop icons")
        _hide_desktop_icons()
    elif not hide_icons and _icons_hidden():
        logmsg("showing desktop icons")
        _show_desktop_icons()
    VIDEO_FILE.write_text(video + "\n")
    _run_wallpaper_process(video, hide_icons, quiet=True)
    logmsg("wallpaper process spawned, exiting")


def cmd_autostart(action):
    if action in ("on", "enable"):
        _enable_autostart()
    elif action in ("off", "disable"):
        _disable_autostart()
    elif action == "status":
        _autostart_status()
    elif action == "run":
        _autostart_run()


def main():
    parser = argparse.ArgumentParser(
        prog="lwp",
        description="Live Wallpaper Player for macOS",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_set = sub.add_parser("set", help="set a video as desktop wallpaper")
    p_set.add_argument("path", help="path to video file")
    p_set.add_argument(
        "--hide-icons",
        action="store_true",
        help="hide desktop icons (needed on macOS 14+)",
    )

    sub.add_parser("stop", help="stop the current wallpaper")
    sub.add_parser("status", help="show wallpaper status")

    p_list = sub.add_parser("list", help="list video files in a directory")
    p_list.add_argument("dir", nargs="?", default=".", help="directory (default: .)")

    p_tui = sub.add_parser("tui", help="browse and select videos interactively")
    p_tui.add_argument("dir", nargs="?", default=".", help="directory (default: .)")

    p_autostart = sub.add_parser("autostart", help="manage login autostart")
    p_autostart.add_argument(
        "action", nargs="?", default="status",
        choices=["on", "off", "enable", "disable", "status", "run"],
        help="on|off|status (default: status)",
    )

    p_run = sub.add_parser("_run", help=argparse.SUPPRESS)
    p_run.add_argument("--path", required=True)
    p_run.add_argument("--hide-icons", action="store_true")

    args = parser.parse_args()

    if args.command == "set":
        cmd_set(args.path, hide_icons=args.hide_icons)
    elif args.command == "stop":
        cmd_stop()
    elif args.command == "status":
        cmd_status()
    elif args.command == "list":
        cmd_list(args.dir)
    elif args.command == "tui":
        cmd_tui(args.dir)
    elif args.command == "autostart":
        cmd_autostart(args.action)
    elif args.command == "_run":
        _wallpaper_main(args.path, hide_icons=args.hide_icons)


if __name__ == "__main__":
    main()
