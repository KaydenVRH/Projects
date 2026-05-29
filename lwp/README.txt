Copy the lwp directory as-is, then on the other Mac run:

    cd lwp
    python3 -m venv .venv
    source .venv/bin/activate
    pip install PyObjC
    echo 'export PATH="$PATH:'"$(pwd)"'"' >> ~/.zshrc
    source ~/.zshrc

The .venv has compiled native code so don't rely on copying it between Macs — always recreate it with the commands above. Everything else (lwp.py, lwp.sh, the symlink) is portable.

USAGE:
    lwp set [--hide-icons] <video>    Set video as wallpaper
    lwp stop                           Stop wallpaper
    lwp status                         Show status
    lwp list [dir]                     List video files
    lwp tui [dir]                      Interactive TUI browser

The 'tui' command opens a terminal UI to browse videos in a directory
(arrow keys to navigate, Enter to set, / to search).


