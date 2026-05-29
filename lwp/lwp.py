#!/usr/bin/env python3
"""lwp - Live Wallpaper Player for macOS.

Usage:
    lwp set [--hide-icons] <video>    Set video as wallpaper
    lwp stop                           Stop wallpaper
    lwp status                         Show status
    lwp list [dir]                     List video files
"""

import argparse
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


def _run_wallpaper_process(video_path, hide_icons=False):
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
    _run_wallpaper_process(video_path, hide_icons)


def cmd_stop():
    _ensure_state_dir()
    _stop_wallpaper()
    if _icons_hidden():
        _show_desktop_icons()
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
    elif args.command == "_run":
        _wallpaper_main(args.path, hide_icons=args.hide_icons)


if __name__ == "__main__":
    main()
