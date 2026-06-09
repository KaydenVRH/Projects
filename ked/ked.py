#!/usr/bin/env python3
import curses
import sys
import os
import datetime
import subprocess

PYTHON_KEYWORDS = {
    'False', 'None', 'True', 'and', 'as', 'assert', 'async', 'await',
    'break', 'class', 'continue', 'def', 'del', 'elif', 'else', 'except',
    'finally', 'for', 'from', 'global', 'if', 'import', 'in', 'is',
    'lambda', 'nonlocal', 'not', 'or', 'pass', 'raise', 'return',
    'try', 'while', 'with', 'yield',
}

PYTHON_BUILTINS = {
    'print', 'len', 'range', 'int', 'str', 'float', 'list', 'dict',
    'set', 'tuple', 'type', 'open', 'input', 'super',
    'enumerate', 'zip', 'map', 'filter', 'sorted', 'reversed',
    'abs', 'min', 'max', 'sum', 'any', 'all', 'hex', 'oct', 'bin',
    'ord', 'chr', 'repr', 'iter', 'next', 'property',
    'object', 'Exception', 'ValueError', 'TypeError', 'KeyError',
    'IndexError', 'AttributeError', 'ImportError', 'FileNotFoundError',
    'self',
}

THEME = {
    'keyword_fg': curses.COLOR_CYAN,
    'string_fg': curses.COLOR_GREEN,
    'comment_fg': curses.COLOR_RED,
    'number_fg': curses.COLOR_YELLOW,
    'builtin_fg': curses.COLOR_MAGENTA,
    'decorator_fg': curses.COLOR_YELLOW,
    'status_fg': curses.COLOR_WHITE,
    'status_bg': curses.COLOR_BLUE,
    'gutter_fg': -1,
    'gutter_bg': -1,
    'find_fg': -1,
    'find_bg': 236,
    'find_sel_fg': curses.COLOR_WHITE,
    'find_sel_bg': curses.COLOR_BLUE,
}

COLOR_PAIRS = {
    'keyword': 1,
    'string': 2,
    'comment': 3,
    'number': 4,
    'builtin': 5,
    'decorator': 6,
}


def setup_colors(theme=None):
    if theme is None:
        theme = THEME
    curses.init_pair(1, theme['keyword_fg'], -1)
    curses.init_pair(2, theme['string_fg'], -1)
    curses.init_pair(3, theme['comment_fg'], -1)
    curses.init_pair(4, theme['number_fg'], -1)
    curses.init_pair(5, theme['builtin_fg'], -1)
    curses.init_pair(6, theme['decorator_fg'], -1)
    curses.init_pair(7, theme['status_fg'], theme['status_bg'])
    curses.init_pair(8, theme['gutter_fg'], theme['gutter_bg'])
    curses.init_pair(9, theme['find_fg'], theme['find_bg'])
    curses.init_pair(10, theme['find_sel_fg'], theme['find_sel_bg'])


def highlight_python(line):
    segments = []
    i = 0
    while i < len(line):
        if line[i] == '#':
            segments.append(('comment', line[i:]))
            break
        if line[i:i + 3] in ('"""', "'''"):
            close = line.find(line[i:i + 3], i + 3)
            if close == -1:
                segments.append(('string', line[i:]))
                break
            segments.append(('string', line[i:close + 3]))
            i = close + 3
            continue
        if line[i] in ('"', "'"):
            quote = line[i]
            j = i + 1
            while j < len(line):
                if line[j] == '\\':
                    j += 2
                    continue
                if line[j] == quote:
                    j += 1
                    break
                j += 1
            segments.append(('string', line[i:j]))
            i = j
            continue
        if line[i].isdigit():
            j = i
            while j < len(line) and (line[j].isalnum() or line[j] in '_.'):
                j += 1
            segments.append(('number', line[i:j]))
            i = j
            continue
        if line[i].isalpha() or line[i] == '_':
            j = i
            while j < len(line) and (line[j].isalnum() or line[j] == '_'):
                j += 1
            word = line[i:j]
            if word in PYTHON_KEYWORDS:
                segments.append(('keyword', word))
            elif word in PYTHON_BUILTINS:
                segments.append(('builtin', word))
            else:
                segments.append(('default', word))
            i = j
            continue
        if line[i] == '@':
            j = i + 1
            while j < len(line) and (line[j].isalnum() or line[j] in '_.'):
                j += 1
            segments.append(('decorator', line[i:j]))
            i = j
            continue
        segments.append(('default', line[i]))
        i += 1
    return segments


def find_files(root, max_depth=10, max_files=5000):
    exclude = {'.git', '__pycache__', 'node_modules', '.venv', 'venv', '.env',
               '.mypy_cache', '.pytest_cache', '.egg-info', 'dist', 'build',
               '.opencode', '.vscode', '.idea', 'target', '.next', '.turbo'}
    results = []
    root = os.path.abspath(root)
    if not os.path.isdir(root):
        return results
    for dirpath, dirnames, filenames in os.walk(root):
        depth = 0 if dirpath == root else dirpath[len(root):].count(os.sep)
        if depth > max_depth:
            dirnames.clear()
            continue
        dirnames[:] = [d for d in dirnames if d not in exclude and not d.startswith('.')]
        for f in filenames:
            if f.startswith('.'):
                continue
            full = os.path.join(dirpath, f)
            results.append(os.path.relpath(full, root))
            if len(results) >= max_files:
                return results
    return results


def fuzzy_score(query, text):
    q = query.lower()
    t = text.lower()
    qi = 0
    score = 0
    prev = -10
    for ti, tc in enumerate(t):
        if qi < len(q) and tc == q[qi]:
            if ti > 0 and t[ti - 1] in '/-_ .':
                score += 50
            gap = ti - prev - 1
            score += max(0, 10 - gap)
            prev = ti
            qi += 1
            score += 1
    if qi == len(q):
        score -= len(t) * 0.5
        return score
    return None


def fuzzy_finder(stdscr, root):
    files = find_files(root)
    if not files:
        return None

    query = ''
    selected = 0
    offset = 0

    while True:
        h, w = stdscr.getmaxyx()

        scored = []
        if query:
            for f in files:
                s = fuzzy_score(query, f)
                if s is not None:
                    scored.append((s, f))
            scored.sort(key=lambda x: -x[0])
            results = [f for _, f in scored]
        else:
            results = files[:]

        if not results:
            selected = 0
        elif selected >= len(results):
            selected = len(results) - 1

        max_visible = h - 2
        if selected < offset:
            offset = selected
        elif selected >= offset + max_visible:
            offset = selected - max_visible + 1

        for i in range(h):
            stdscr.addstr(i, 0, " " * (w - 1), curses.color_pair(9))

        matches = len(results) if query else len(files)
        s = '' if matches == 1 else 'es'
        label = f" <find>  <{matches} match{s}> "
        stdscr.addstr(0, 0, label[:w - 1], curses.A_BOLD | curses.color_pair(9))

        visible = results[offset:offset + max_visible]
        for i, f in enumerate(visible):
            display = f
            m = w - 4
            if len(display) > m:
                display = "..." + display[-(m - 3):]
            if offset + i == selected:
                stdscr.addstr(i + 1, 0, "  " + display, curses.color_pair(10))
            else:
                stdscr.addstr(i + 1, 0, "  " + display, curses.color_pair(9))

        prompt = f"> {query}"
        stdscr.addstr(h - 1, 0, prompt.ljust(w - 1), curses.color_pair(7))
        stdscr.move(h - 1, min(len(prompt), w - 1))

        key = stdscr.getch()
        if key == curses.KEY_RESIZE:
            continue
        if key == 27:
            return None
        elif key == 10 and results:
            return results[selected]
        elif key == curses.KEY_UP:
            selected = max(0, selected - 1)
        elif key == curses.KEY_DOWN:
            selected = min(len(results) - 1, selected + 1)
        elif key == curses.KEY_NPAGE:
            selected = min(len(results) - 1, selected + max_visible)
        elif key == curses.KEY_PPAGE:
            selected = max(0, selected - max_visible)
        elif key == curses.KEY_HOME:
            selected = 0
            offset = 0
        elif key == curses.KEY_END:
            selected = len(results) - 1
        elif key in (curses.KEY_BACKSPACE, 127, 8):
            if query:
                query = query[:-1]
                selected = 0
                offset = 0
        elif 32 <= key < 127:
            query += chr(key)
            selected = 0
            offset = 0


def run_python(stdscr, filepath):
    if not filepath:
        return
    h, w = stdscr.getmaxyx()

    try:
        result = subprocess.run(
            ['python3', filepath],
            capture_output=True, text=True, timeout=30
        )
        output = result.stdout + result.stderr
        if result.returncode != 0:
            output += f"\n[exit code: {result.returncode}]"
        if not output:
            output = "[no output]"
    except subprocess.TimeoutExpired:
        output = "[timed out after 30s]"
    except Exception as e:
        output = f"[error: {e}]"

    lines_out = output.splitlines()
    max_visible = h - 3
    scroll = max(0, len(lines_out) - max_visible)

    for i in range(h):
        stdscr.addstr(i, 0, " " * (w - 1), curses.color_pair(9))

    header = " <run>  <output> "
    stdscr.addstr(0, 0, header[:w - 1], curses.A_BOLD | curses.color_pair(9))

    visible = lines_out[scroll:scroll + max_visible]
    for i, line in enumerate(visible):
        stdscr.addstr(i + 1, 0, " " + line[:w - 3] + " " * (w - 3 - min(len(line), w - 3)), curses.color_pair(9))

    footer = " press any key to dismiss "
    stdscr.addstr(h - 1, 0, footer.ljust(w - 1), curses.color_pair(7))
    stdscr.getch()


def main(stdscr):
    curses.raw()
    curses.cbreak()
    stdscr.keypad(True)
    curses.start_color()
    curses.use_default_colors()
    setup_colors()

    filepath = sys.argv[1] if len(sys.argv) > 1 else None
    lines = (open(filepath).read().splitlines() or ['']) if filepath else ['']
    modified = False
    y = x = top = 0
    mode = 'normal'
    yank_buffer = None

    while True:
        h, w = stdscr.getmaxyx()
        stdscr.clear()

        gutter_width = len(str(len(lines))) + 2
        is_python = filepath is None or filepath.endswith('.py')

        for i, line in enumerate(lines[top:top + h - 1]):
            line_num = top + i + 1
            gutter_str = f"{line_num:>{gutter_width - 1}} "
            try:
                stdscr.addstr(i, 0, gutter_str, curses.color_pair(8))
            except curses.error:
                pass
            text_x = gutter_width
            segments = highlight_python(line) if is_python else [('default', line)]
            for style, text in segments:
                if text_x >= w:
                    break
                text = text[:w - text_x]
                if not text:
                    break
                attr = curses.color_pair(COLOR_PAIRS.get(style, 0))
                try:
                    stdscr.addstr(i, text_x, text, attr)
                except curses.error:
                    pass
                text_x += len(text)

        mode_label = "INSERT" if mode == 'insert' else "NORMAL"
        parts = [
            "<ked>",
            f"<{os.path.basename(filepath or 'untitled')}>",
            f"<{mode_label}>",
        ]
        if modified:
            parts.append("<+>")
        parts.append(f"<{y + 1}:{x + 1}>")
        left = " ".join(parts)
        right = f"<{datetime.datetime.now().strftime('%H:%M')}>"
        gap = w - len(left) - len(right) - 1
        if gap < 1:
            gap = 1
        status = left + " " * gap + right
        stdscr.addstr(h - 1, 0, status[:w - 1], curses.A_BOLD | curses.color_pair(7))

        stdscr.move(min(y - top, h - 2), min(x + gutter_width, w - 1))
        key = stdscr.getch()

        if key == 16:  # Ctrl+P — fuzzy file finder
            root = os.path.dirname(os.path.abspath(filepath)) if filepath else os.getcwd()
            chosen = fuzzy_finder(stdscr, root)
            if chosen:
                path = os.path.join(root, chosen)
                try:
                    with open(path) as f:
                        lines = f.read().splitlines()
                    filepath = path
                    if not lines:
                        lines = ['']
                    modified = False
                    y = x = top = 0
                except (IOError, OSError):
                    pass
            continue

        if key == 18:  # Ctrl+R — run file
            if filepath:
                save_file(filepath, lines)
                modified = False
                run_python(stdscr, filepath)
            else:
                h, w = stdscr.getmaxyx()
                msg = " <run>  <no file to run — save with :w filename> "
                stdscr.addstr(h - 1, 0, msg.ljust(w - 1), curses.color_pair(7))
                stdscr.getch()
            continue

        if mode == 'normal':
            if key == ord(':'):
                cmd = prompt_cmd(stdscr, h, w)
                parts = cmd.split(maxsplit=1)
                action = parts[0]
                arg = parts[1] if len(parts) > 1 else None
                if action in ('w', 'wq'):
                    if arg:
                        filepath = arg
                    if filepath:
                        save_file(filepath, lines)
                        modified = False
                    if action == 'wq':
                        break
                elif action == 'q':
                    if not modified:
                        break
                elif action == 'q!':
                    break
                continue

            elif key == ord('i'):
                mode = 'insert'
            elif key == ord('a'):
                if x < len(lines[y]):
                    x += 1
                mode = 'insert'
            elif key == ord('A'):
                x = len(lines[y])
                mode = 'insert'
            elif key == ord('I'):
                x = 0
                mode = 'insert'
            elif key == ord('o'):
                lines.insert(y + 1, '')
                y += 1
                x = 0
                modified = True
                mode = 'insert'
            elif key == ord('O'):
                lines.insert(y, '')
                modified = True
                mode = 'insert'

            elif key == ord('x'):
                if lines[y]:
                    lines[y] = lines[y][:x] + lines[y][x + 1:]
                    modified = True
            elif key == ord('d'):
                next_key = stdscr.getch()
                if next_key == ord('d'):
                    yank_buffer = lines[y]
                    del lines[y]
                    modified = True
                    if not lines:
                        lines = ['']
                    if y >= len(lines):
                        y = len(lines) - 1
                    x = min(x, len(lines[y]))
            elif key == ord('y'):
                next_key = stdscr.getch()
                if next_key == ord('y'):
                    yank_buffer = lines[y]
            elif key == ord('p'):
                if yank_buffer is not None:
                    lines.insert(y + 1, yank_buffer)
                    modified = True
            elif key == ord('P'):
                if yank_buffer is not None:
                    lines.insert(y, yank_buffer)
                    modified = True

            elif key == ord('h') and x > 0:
                x -= 1
            elif key == ord('j') and y < len(lines) - 1:
                y += 1
            elif key == ord('k') and y > 0:
                y -= 1
            elif key == ord('l') and x < len(lines[y]):
                x += 1
            elif key == ord('0'):
                x = 0
            elif key == ord('$'):
                x = len(lines[y])
            elif key == ord('G'):
                y = len(lines) - 1
                x = min(x, len(lines[y]))
            elif key == ord('w'):
                i = x + 1
                while i < len(lines[y]) and lines[y][i].isspace():
                    i += 1
                if i < len(lines[y]):
                    x = i
                else:
                    x = len(lines[y])
            elif key == ord('b') and x > 0:
                i = x - 1
                while i > 0 and lines[y][i].isspace():
                    i -= 1
                x = i

            elif key == curses.KEY_UP and y > 0:
                y -= 1
            elif key == curses.KEY_DOWN and y < len(lines) - 1:
                y += 1
            elif key == curses.KEY_LEFT and x > 0:
                x -= 1
            elif key == curses.KEY_RIGHT and x < len(lines[y]):
                x += 1
            elif key == curses.KEY_PPAGE:
                top = max(0, top - h + 2)
                y = max(0, y - h + 2)
            elif key == curses.KEY_NPAGE:
                top = min(len(lines) - 1, top + h - 2)
                y = min(len(lines) - 1, y + h - 2)
            elif key == curses.KEY_HOME:
                x = 0
            elif key == curses.KEY_END:
                x = len(lines[y])
            elif key == curses.KEY_RESIZE:
                continue

        elif mode == 'insert':
            if key == 27:
                mode = 'normal'
                if x > 0:
                    x -= 1
            elif key == curses.KEY_UP and y > 0:
                y -= 1
            elif key == curses.KEY_DOWN and y < len(lines) - 1:
                y += 1
            elif key == curses.KEY_LEFT and x > 0:
                x -= 1
            elif key == curses.KEY_RIGHT and x < len(lines[y]):
                x += 1
            elif key == curses.KEY_PPAGE:
                top = max(0, top - h + 2)
                y = max(0, y - h + 2)
            elif key == curses.KEY_NPAGE:
                top = min(len(lines) - 1, top + h - 2)
                y = min(len(lines) - 1, y + h - 2)
            elif key == curses.KEY_HOME:
                x = 0
            elif key == curses.KEY_END:
                x = len(lines[y])
            elif key == curses.KEY_RESIZE:
                continue
            elif 32 <= key < 127:
                lines[y] = lines[y][:x] + chr(key) + lines[y][x:]
                x += 1
                modified = True
            elif key in (curses.KEY_BACKSPACE, 127, 8):
                if x > 0:
                    lines[y] = lines[y][:x - 1] + lines[y][x:]
                    x -= 1
                    modified = True
                elif y > 0:
                    x = len(lines[y - 1])
                    lines[y - 1] += lines[y]
                    del lines[y]
                    y -= 1
                    modified = True
            elif key == 10 or key == ord('\n'):
                lines.insert(y + 1, lines[y][x:])
                lines[y] = lines[y][:x]
                y += 1
                x = 0
                modified = True
            elif key == 9:
                lines[y] = lines[y][:x] + '    ' + lines[y][x:]
                x += 4
                modified = True

        # Clamp x
        if x < 0:
            x = 0
        elif x > len(lines[y]):
            x = len(lines[y])

        # Scroll
        if y < top:
            top = y
        elif y >= top + h - 1:
            top = y - h + 2


def save_file(path, lines):
    with open(path, 'w') as f:
        f.write('\n'.join(lines))


def prompt_confirm(stdscr, h, w, msg):
    stdscr.addstr(h - 1, 0, (msg + " (y/n)").ljust(w - 1), curses.color_pair(7))
    stdscr.clrtoeol()
    while True:
        key = stdscr.getch()
        if key == ord('y'):
            return True
        if key == ord('n'):
            return False


def prompt_cmd(stdscr, h, w):
    buf = ''
    while True:
        stdscr.addstr(h - 1, 0, (":" + buf).ljust(w - 1), curses.color_pair(7))
        key = stdscr.getch()
        if key == 10:
            return buf
        if key in (curses.KEY_BACKSPACE, 127, 8) and buf:
            buf = buf[:-1]
        elif 32 <= key < 127:
            buf += chr(key)


if __name__ == '__main__':
    curses.wrapper(main)
