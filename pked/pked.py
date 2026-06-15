#!/usr/bin/env python3
"""pked — a vim-like TUI text editor, written in Python."""

import sys, os, signal, atexit, termios, shutil, select, re, threading, queue, subprocess, time, pty, struct, fcntl, errno, base64
from collections import namedtuple

# ── ANSI helpers ─────────────────────────────────────────────────

Color = namedtuple('Color', 'r g b')
def fg(c): return f'\033[38;2;{c.r};{c.g};{c.b}m'
def bg(c): return f'\033[48;2;{c.r};{c.g};{c.b}m'

class Theme:
    def __init__(self, bg, fg, gutter_bg, gutter_fg, status_bg, status_fg, tilde,
                 search_bg=None, search_fg=None, sel_bg=None,
                 kw=None, string=None, cmt=None, num=None, builtin=None, tp=None,
                 decorator=None, operator=None, fn=None):
        self.bg = bg; self.fg = fg
        self.gutter_bg = gutter_bg; self.gutter_fg = gutter_fg
        self.status_bg = status_bg; self.status_fg = status_fg
        self.tilde = tilde
        self.search_bg = search_bg or fg; self.search_fg = search_fg or bg
        self.sel_bg = sel_bg or C(80, 80, 120)
        self.kw = kw or fg
        self.string = string or fg
        self.cmt = cmt or tilde
        self.num = num or fg
        self.builtin = builtin or fg
        self.tp = tp or fg
        self.decorator = decorator or (kw or fg)
        self.operator = operator or tilde
        self.fn = fn or (kw or fg)

C = Color

THEMES = {
    'catppuccin': Theme(
        C(30,30,46), C(205,214,244),
        C(49,53,79), C(108,112,143),
        C(139,170,218), C(30,30,46), C(108,112,143),
        kw=C(203,166,247), string=C(166,227,161), cmt=C(92,95,119),
        num=C(249,226,175), builtin=C(137,180,250), tp=C(243,139,168),
    ),
    'default': Theme(
        C(40,40,40), C(220,220,220),
        C(55,55,55), C(150,150,150),
        C(60,120,200), C(40,40,40), C(100,100,100),
        kw=C(180,140,220), string=C(140,200,140), cmt=C(150,150,150),
        num=C(220,200,140), builtin=C(140,180,220), tp=C(220,160,160),
    ),
    'monokai': Theme(
        C(39,40,34), C(248,248,242),
        C(50,51,44), C(130,130,120),
        C(102,217,239), C(39,40,34), C(90,90,80),
        kw=C(249,38,114), string=C(230,219,116), cmt=C(117,113,94),
        num=C(174,129,255), builtin=C(166,226,46), tp=C(102,217,239),
    ),
    'solarized': Theme(
        C(253,246,227), C(101,123,131),
        C(238,232,213), C(147,161,161),
        C(7,54,66), C(253,246,227), C(147,161,161),
        kw=C(203,75,22), string=C(42,161,152), cmt=C(147,161,161),
        num=C(211,54,130), builtin=C(38,139,210), tp=C(181,137,0),
    ),
    'nord': Theme(
        C(46,52,64), C(216,222,233),
        C(59,66,82), C(76,86,106),
        C(59,66,82), C(216,222,233), C(76,86,106),
        kw=C(129,161,193), string=C(163,190,140), cmt=C(97,110,136),
        num=C(180,142,173), builtin=C(136,192,208), tp=C(235,203,139),
    ),
    'gruvbox': Theme(
        C(40,40,40), C(235,219,178),
        C(80,73,69), C(80,73,69),
        C(80,73,69), C(235,219,178), C(80,73,69),
        kw=C(251,73,52), string=C(184,187,38), cmt=C(146,131,116),
        num=C(211,134,155), builtin=C(142,192,124), tp=C(211,134,155),
    ),
    'bi': Theme(
        C(13,10,20), C(230,223,240),
        C(42,26,58), C(74,58,96),
        C(215,2,112), C(255,255,255), C(74,58,96),
        kw=C(215,2,112), string=C(232,138,176), cmt=C(96,80,122),
        num=C(176,124,198), builtin=C(91,155,213), tp=C(91,155,213),
    ),
    'tokyonight': Theme(
        C(26,27,38), C(192,202,245),
        C(42,46,61), C(86,95,137),
        C(42,46,61), C(192,202,245), C(86,95,137),
        kw=C(187,154,247), string=C(158,206,106), cmt=C(86,95,137),
        num=C(255,158,100), builtin=C(125,207,255), tp=C(122,162,247),
    ),
    'amber': Theme(
        C(24,20,37), C(221,214,240),
        C(50,42,65), C(136,128,160),
        C(245,200,66), C(24,20,37), C(136,128,160),
        kw=C(245,200,66), string=C(168,138,224), cmt=C(136,128,160),
        num=C(196,169,248), builtin=C(184,156,240), tp=C(155,125,214),
    ),
    'dracula': Theme(
        C(40,42,54), C(248,248,242),
        C(52,54,68), C(98,114,164),
        C(98,114,164), C(248,248,242), C(98,114,164),
        kw=C(255,121,198), string=C(241,250,140), cmt=C(98,114,164),
        num=C(189,147,249), builtin=C(139,233,253), tp=C(139,233,253),
    ),
    'onedark': Theme(
        C(40,44,52), C(171,178,191),
        C(51,56,65), C(92,99,112),
        C(40,44,52), C(171,178,191), C(92,99,112),
        kw=C(198,120,221), string=C(152,195,121), cmt=C(92,99,112),
        num=C(209,154,102), builtin=C(97,175,239), tp=C(229,192,123),
    ),
    'everforest': Theme(
        C(45,53,59), C(211,198,170),
        C(56,65,72), C(125,131,121),
        C(56,65,72), C(211,198,170), C(125,131,121),
        kw=C(230,126,128), string=C(167,192,128), cmt=C(125,131,121),
        num=C(223,184,105), builtin=C(127,187,179), tp=C(214,153,156),
    ),
    'rosepine': Theme(
        C(25,23,36), C(224,222,244),
        C(38,35,51), C(111,106,133),
        C(38,35,51), C(224,222,244), C(111,106,133),
        kw=C(196,167,231), string=C(156,207,216), cmt=C(111,106,133),
        num=C(234,154,151), builtin=C(158,206,221), tp=C(235,188,186),
    ),
    'oxocarbon': Theme(
        C(22,22,22), C(210,209,210),
        C(38,38,38), C(82,82,82),
        C(38,38,38), C(210,209,210), C(82,82,82),
        kw=C(190,149,255), string=C(66,190,166), cmt=C(82,82,82),
        num=C(255,126,182), builtin=C(51,176,255), tp=C(8,189,174),
    ),
    'system': Theme(
        C(28,28,30), C(200,200,208),
        C(36,36,40), C(95,95,105),
        C(55,55,62), C(200,200,208), C(95,95,105),
        kw=C(100,140,185), string=C(140,175,125), cmt=C(95,95,105),
        num=C(200,170,125), builtin=C(120,165,200), tp=C(170,150,135),
    ),
    'opencode': Theme(
        C(10,10,10), C(238,238,238),
        C(20,20,20), C(96,96,96),
        C(157,124,216), C(238,238,238), C(128,128,128),
        kw=C(157,124,216), string=C(127,216,143), cmt=C(128,128,128),
        num=C(245,167,66), builtin=C(157,124,216), tp=C(229,192,123),
        decorator=C(178,140,230), operator=C(157,124,216), fn=C(178,140,230),
    ),
}

THEME_LIST = ['catppuccin', 'default', 'monokai', 'solarized', 'nord', 'gruvbox',
              'bi', 'tokyonight', 'amber', 'dracula', 'onedark', 'everforest',
              'rosepine', 'oxocarbon', 'system', 'opencode']

def theme_get(name):
    return THEMES.get(name, THEMES['default'])

# ── rainbow hue rotation ───────────────────────────────────────

def _rot_hue(r, g, b, deg):
    """Rotate the hue of an RGB colour by *deg* degrees.  Greys are given
    a synthetic saturation so the rotation shows up."""
    rn, gn, bn = r / 255.0, g / 255.0, b / 255.0
    mx = max(rn, gn, bn)
    mn = min(rn, gn, bn)
    l = (mx + mn) / 2.0
    if mx == mn:
        s, h = 0.7, deg  # grey: invent saturation so rotation is visible
    else:
        d = mx - mn
        s = d / (2.0 - mx - mn) if l > 0.5 else d / (mx + mn)
        h = (60.0 * ((gn - bn) / d) + 360.0) % 360.0 if mx == rn else \
            (60.0 * ((bn - rn) / d) + 120.0) if mx == gn else \
            (60.0 * ((rn - gn) / d) + 240.0)
    h = (h + deg) % 360.0
    c = (1.0 - abs(2.0 * l - 1.0)) * s
    x = c * (1.0 - abs((h / 60.0) % 2.0 - 1.0))
    m = l - c / 2.0
    if h < 60:    r1, g1, b1 = c, x, 0.0
    elif h < 120: r1, g1, b1 = x, c, 0.0
    elif h < 180: r1, g1, b1 = 0.0, c, x
    elif h < 240: r1, g1, b1 = 0.0, x, c
    elif h < 300: r1, g1, b1 = x, 0.0, c
    else:         r1, g1, b1 = c, 0.0, x
    return C(int((r1 + m) * 255), int((g1 + m) * 255), int((b1 + m) * 255))


def rotate_theme(t, hue):
    """Return a copy of *t* with every colour's hue rotated, except bg."""
    return Theme(
        t.bg,
        _rot_hue(t.fg.r, t.fg.g, t.fg.b, hue),
        _rot_hue(t.gutter_bg.r, t.gutter_bg.g, t.gutter_bg.b, hue),
        _rot_hue(t.gutter_fg.r, t.gutter_fg.g, t.gutter_fg.b, hue),
        _rot_hue(t.status_bg.r, t.status_bg.g, t.status_bg.b, hue),
        _rot_hue(t.status_fg.r, t.status_fg.g, t.status_fg.b, hue),
        _rot_hue(t.tilde.r, t.tilde.g, t.tilde.b, hue),
        search_bg=_rot_hue(t.search_bg.r, t.search_bg.g, t.search_bg.b, hue),
        search_fg=_rot_hue(t.search_fg.r, t.search_fg.g, t.search_fg.b, hue),
        sel_bg=_rot_hue(t.sel_bg.r, t.sel_bg.g, t.sel_bg.b, hue),
        kw=_rot_hue(t.kw.r, t.kw.g, t.kw.b, hue),
        string=_rot_hue(t.string.r, t.string.g, t.string.b, hue),
        cmt=_rot_hue(t.cmt.r, t.cmt.g, t.cmt.b, hue),
        num=_rot_hue(t.num.r, t.num.g, t.num.b, hue),
        builtin=_rot_hue(t.builtin.r, t.builtin.g, t.builtin.b, hue),
        tp=_rot_hue(t.tp.r, t.tp.g, t.tp.b, hue),
        decorator=_rot_hue(t.decorator.r, t.decorator.g, t.decorator.b, hue),
        operator=_rot_hue(t.operator.r, t.operator.g, t.operator.b, hue),
        fn=_rot_hue(t.fn.r, t.fn.g, t.fn.b, hue),
    )


def _pulse_color(c, t):
    """Oscillate a colour's lightness sinusoidally (breathing effect)."""
    import math
    r, g, b = c.r / 255.0, c.g / 255.0, c.b / 255.0
    mx = max(r, g, b)
    mn = min(r, g, b)
    l = (mx + mn) / 2.0
    s = 0.0 if mx == mn else ((mx - mn) / (2.0 - mx - mn) if l > 0.5 else (mx - mn) / (mx + mn))
    factor = 0.3 + 0.6 * (0.5 + 0.5 * math.sin(t * 2.0))
    l = factor
    c2 = (1.0 - abs(2.0 * l - 1.0)) * s
    x = c2 * (1.0 - abs((0.0) % 2.0 - 1.0))
    m = l - c2 / 2.0
    return C(int((c2 + m) * 255), int((x + m) * 255), int((m) * 255))


def _fx_pulse(t, elapsed):
    import math
    return Theme(
        t.bg,
        _pulse_color(t.fg, elapsed),
        _pulse_color(t.gutter_bg, elapsed),
        _pulse_color(t.gutter_fg, elapsed),
        _pulse_color(t.status_bg, elapsed),
        _pulse_color(t.status_fg, elapsed),
        _pulse_color(t.tilde, elapsed),
        search_bg=_pulse_color(t.search_bg, elapsed),
        search_fg=_pulse_color(t.search_fg, elapsed),
        sel_bg=_pulse_color(t.sel_bg, elapsed),
        kw=_pulse_color(t.kw, elapsed),
        string=_pulse_color(t.string, elapsed),
        cmt=_pulse_color(t.cmt, elapsed),
        num=_pulse_color(t.num, elapsed),
        builtin=_pulse_color(t.builtin, elapsed),
        tp=_pulse_color(t.tp, elapsed),
        decorator=_pulse_color(t.decorator, elapsed),
        operator=_pulse_color(t.operator, elapsed),
        fn=_pulse_color(t.fn, elapsed),
    )


def _map_hue(r, g, b, lo, hi):
    """Map an RGB colour's hue into the range [lo, hi]."""
    rn, gn, bn = r / 255.0, g / 255.0, b / 255.0
    mx = max(rn, gn, bn)
    mn = min(rn, gn, bn)
    d = mx - mn
    if d == 0:
        return C(int(rn * 255), int(gn * 255), int(bn * 255))
    l = (mx + mn) / 2.0
    s = d / (2.0 - mx - mn) if l > 0.5 else d / (mx + mn)
    if mx == rn:
        h = (60.0 * ((gn - bn) / d) + 360.0) % 360.0
    elif mx == gn:
        h = (60.0 * ((bn - rn) / d) + 120.0)
    else:
        h = (60.0 * ((rn - gn) / d) + 240.0)
    h = lo + (h / 360.0) * (hi - lo)
    c2 = (1.0 - abs(2.0 * l - 1.0)) * s
    x = c2 * (1.0 - abs((h / 60.0) % 2.0 - 1.0))
    m = l - c2 / 2.0
    if h < 60:    r1, g1, b1 = c2, x, 0.0
    elif h < 120: r1, g1, b1 = x, c2, 0.0
    elif h < 180: r1, g1, b1 = 0.0, c2, x
    elif h < 240: r1, g1, b1 = 0.0, x, c2
    elif h < 300: r1, g1, b1 = x, 0.0, c2
    else:         r1, g1, b1 = c2, 0.0, x
    return C(int((r1 + m) * 255), int((g1 + m) * 255), int((b1 + m) * 255))


def _fx_vaporwave(t, elapsed):
    """Cycle hues restricted to pink/purple/blue range (260–330°)."""
    import math
    shift = (elapsed * 25.0) % 70.0
    lo, hi = 260.0 + shift, 330.0 + shift
    return Theme(
        t.bg,
        _map_hue(t.fg.r, t.fg.g, t.fg.b, lo, hi),
        _map_hue(t.gutter_bg.r, t.gutter_bg.g, t.gutter_bg.b, lo, hi),
        _map_hue(t.gutter_fg.r, t.gutter_fg.g, t.gutter_fg.b, lo, hi),
        _map_hue(t.status_bg.r, t.status_bg.g, t.status_bg.b, lo, hi),
        _map_hue(t.status_fg.r, t.status_fg.g, t.status_fg.b, lo, hi),
        _map_hue(t.tilde.r, t.tilde.g, t.tilde.b, lo, hi),
        search_bg=_map_hue(t.search_bg.r, t.search_bg.g, t.search_bg.b, lo, hi),
        search_fg=_map_hue(t.search_fg.r, t.search_fg.g, t.search_fg.b, lo, hi),
        sel_bg=_map_hue(t.sel_bg.r, t.sel_bg.g, t.sel_bg.b, lo, hi),
        kw=_map_hue(t.kw.r, t.kw.g, t.kw.b, lo, hi),
        string=_map_hue(t.string.r, t.string.g, t.string.b, lo, hi),
        cmt=_map_hue(t.cmt.r, t.cmt.g, t.cmt.b, lo, hi),
        num=_map_hue(t.num.r, t.num.g, t.num.b, lo, hi),
        builtin=_map_hue(t.builtin.r, t.builtin.g, t.builtin.b, lo, hi),
        tp=_map_hue(t.tp.r, t.tp.g, t.tp.b, lo, hi),
        decorator=_map_hue(t.decorator.r, t.decorator.g, t.decorator.b, lo, hi),
        operator=_map_hue(t.operator.r, t.operator.g, t.operator.b, lo, hi),
        fn=_map_hue(t.fn.r, t.fn.g, t.fn.b, lo, hi),
    )


def _fx_glitch(t, elapsed):
    """Subtle random hue jitter — changes once per second."""
    import random
    seed = int(elapsed)
    rng = random.Random(seed)
    jitter = rng.uniform(-15, 15)
    return rotate_theme(t, jitter)


FX_NAMES = ['off', 'rainbow', 'pulse', 'vaporwave', 'glitch']

# ── Fuzzy file finder ──

class Finder:
    def __init__(self):
        self.files = []
        self.results = []

    def collect_files(self):
        self.files.clear()
        root = os.getcwd()
        self._walk(root, root, 0)
        self.files.sort()

    def _walk(self, root, directory, depth):
        if depth > 6:
            return
        try:
            entries = os.listdir(directory)
        except PermissionError:
            return
        for name in entries:
            if name.startswith('.') or name == '.git':
                continue
            path = os.path.join(directory, name)
            if os.path.isdir(path):
                self._walk(root, path, depth + 1)
            elif os.path.isfile(path):
                rel = os.path.relpath(path, root)
                self.files.append(rel)

    def search(self, query):
        self.results.clear()
        if not query:
            for f in self.files[:20]:
                self.results.append((f, 0))
            return
        for f in self.files:
            score = self.fuzzy_score(query, f)
            if score is not None:
                self.results.append((f, score))
        self.results.sort(key=lambda x: x[1])
        self.results = self.results[:50]

    @staticmethod
    def fuzzy_score(query, text):
        q = query.lower()
        t = text.lower()
        if not q:
            return 0
        qi = 0
        score = 0
        prev_match = False
        for ti, tc in enumerate(t):
            if qi < len(q) and tc == q[qi]:
                if ti == 0 or t[ti - 1] in '/_-.':
                    score -= 10
                if prev_match:
                    score -= 5
                prev_match = True
                qi += 1
            else:
                prev_match = False
        return score if qi == len(q) else None

# ── File tree ──

class TreeEntry:
    __slots__ = ('name', 'path', 'is_dir', 'depth', 'expanded')
    def __init__(self, name, path, is_dir, depth, expanded=False):
        self.name = name
        self.path = path
        self.is_dir = is_dir
        self.depth = depth
        self.expanded = expanded

# Nerd Font icons — codepoints for common file types
FILE_ICONS = {
    '.py':  '\ue606',  #  nf-dev-python
    '.rs':  '\ue7a8',  #  nf-dev-rust
    '.js':  '\ue781',  #  nf-dev-javascript
    '.ts':  '\ue628',  #  nf-dev-typescript
    '.jsx': '\ue7ba',  #  nf-dev-react
    '.tsx': '\ue7ba',  # 
    '.c':   '\ue61e',  #  nf-dev-c
    '.h':   '\ue61e',
    '.cpp': '\ue61d',  #  nf-dev-cplusplus
    '.hpp': '\ue61d',
    '.html':'\ue736',  #  nf-dev-html5
    '.css': '\ue749',  #  nf-dev-css3
    '.json':'\ue60b',  #  nf-dev-json
    '.md':  '\ue73b',  #  nf-dev-markdown
    '.toml':'\ue615',  #  nf-dev-toml
    '.yml': '\ue615',
    '.yaml':'\ue615',
    '.sh':  '\ue68d',  # nf-dev-terminal
    '.txt': '\uf15b',  #  nf-fa-file
}
FOLDER_CLOSED = '\uf07b'  #  nf-fa-folder
FOLDER_OPEN   = '\uf07c'  #  nf-fa-folder-open
FILE_DEFAULT  = '\uf15b'  #  nf-fa-file

def file_icon(path, is_dir):
    if is_dir:
        return FOLDER_CLOSED
    _, ext = os.path.splitext(path)
    return FILE_ICONS.get(ext.lower(), FILE_DEFAULT)


class FileTree:
    def __init__(self):
        self.entries = []
        self.selected = 0
        self.scroll = 0

    def refresh(self):
        self.entries.clear()
        cwd = os.getcwd()
        root_name = os.path.basename(cwd) or cwd
        self.entries.append(TreeEntry(root_name, cwd, True, 0, True))
        root_path = self.entries[0].path
        children = self._collect_children(root_path, 1)
        self.entries[1:1] = children
        self.selected = 0
        self.scroll = 0

    def _collect_children(self, directory, depth):
        if depth > 5:
            return [TreeEntry('...', directory, True, depth, False)]
        try:
            names = os.listdir(directory)
        except PermissionError:
            return []
        items = []
        for name in names:
            if name == '.git':
                continue
            path = os.path.join(directory, name)
            is_dir = os.path.isdir(path)
            items.append(TreeEntry(name, path, is_dir, depth, False))
        items.sort(key=lambda e: (not e.is_dir, e.name.lower()))
        return items

    def selected_entry(self):
        if 0 <= self.selected < len(self.entries):
            return self.entries[self.selected]
        return None

    def toggle_expand(self):
        entry = self.selected_entry()
        if entry is None or not entry.is_dir:
            return False
        idx = self.selected
        if entry.expanded:
            # Collapse: remove all children at greater depth
            end = idx + 1
            while end < len(self.entries) and self.entries[end].depth > entry.depth:
                end += 1
            del self.entries[idx + 1:end]
            self.entries[idx].expanded = False
        else:
            # Expand: insert children
            children = self._collect_children(entry.path, entry.depth + 1)
            self.entries[idx + 1:idx + 1] = children
            self.entries[idx].expanded = True
        return True

# ── Syntax highlighting ──

# Token types used by the char-by-char scanners
T_PLAIN, T_KW, T_BUILTIN, T_TYPE, T_STRING, T_CMT = \
    'plain', 'kw', 'builtin', 'tp', 'string', 'cmt'
T_NUM, T_DECORATOR, T_OPERATOR, T_FN = \
    'num', 'decorator', 'operator', 'fn'

# ── helper: word-boundary check ────────────────────────────────
def _is_word_char(ch):
    return ch.isalnum() or ch == '_'

def _is_ident_boundary(line, i, j):
    b = (i == 0 or not _is_word_char(line[i - 1]))
    a = (j >= len(line) or not _is_word_char(line[j]))
    return b and a

# ── per-language tokenizer functions ───────────────────────────

def _tokenize_python(line):
    chars = line
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        ch = chars[i]
        # comment
        if ch == '#':
            tokens.append((i, n, T_CMT))
            break
        # f-string prefix before quote
        if ch in 'fF' and i + 1 < n and chars[i + 1] in '"\'':
            tokens.append((i, i + 1, 'string'))
            i += 1
            continue
        # raw/bytes prefix before quote
        if ch in 'rRbB' and i + 1 < n and chars[i + 1] in '"\'':
            i += 1
            continue
        # string
        if ch in '"\'':
            quote = ch
            start = i; i += 1
            while i < n:
                if chars[i] == '\\': i += 2; continue
                if chars[i] == quote: i += 1; break
                i += 1
            tokens.append((start, i, T_STRING))
            continue
        # number
        if ch.isdigit() or (ch == '.' and i + 1 < n and chars[i + 1].isdigit()):
            start = i
            if ch == '0' and i + 1 < n and chars[i + 1] in 'xXoObB':
                i += 2
                while i < n and (chars[i].isdigit() or chars[i] in 'abcdefABCDEF'):
                    i += 1
            else:
                while i < n and (chars[i].isdigit() or chars[i] in '.eE_xXoObB'):
                    i += 1
            tokens.append((start, i, T_NUM))
            continue
        # decorator
        if ch == '@':
            start = i; i += 1
            while i < n and (_is_word_char(chars[i]) or chars[i] == '.'):
                i += 1
            tokens.append((start, i, T_DECORATOR))
            continue
        # multi-char operators
        if i + 1 < n:
            two = chars[i:i + 2]
            if two in ('==', '!=', '<=', '>=', '->', '//', '**',
                       '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=',
                       '>>', '<<', '//=', '**=', '>>=', '<<=', '::'):
                tokens.append((i, i + 2, T_OPERATOR))
                i += 2; continue
        # single-char operators / punctuation
        if ch in '+-*/%=!<>&|^~:.':
            tokens.append((i, i + 1, T_OPERATOR))
            i += 1; continue
        if ch in '()[]{};,\\':
            if ch == '(' and tokens and tokens[-1][2] == T_PLAIN:
                # function call: change last plain token to fn
                s, e, _ = tokens.pop()
                tokens.append((s, e, T_FN))
            i += 1; continue
        # identifier
        if ch.isalpha() or ch == '_':
            start = i
            while i < n and _is_word_char(chars[i]):
                i += 1
            word = chars[start:i]
            if _is_ident_boundary(line, start, i):
                if word in PY_KW: tokens.append((start, i, T_KW))
                elif word in PY_BUILTIN: tokens.append((start, i, T_BUILTIN))
                elif word in PY_TYPE: tokens.append((start, i, T_TYPE))
                else: tokens.append((start, i, T_PLAIN))
            else:
                tokens.append((start, i, T_PLAIN))
            continue
        i += 1
    return tokens


def _tokenize_rust(line):
    chars = line
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        ch = chars[i]
        # line comment
        if ch == '/' and i + 1 < n and chars[i + 1] == '/':
            tokens.append((i, n, T_CMT))
            break
        # raw string r"..." or r#"..."#
        if ch in 'rR':
            saved = i; i += 1; hashes = 0
            while i < n and chars[i] == '#': hashes += 1; i += 1
            if i < n and chars[i] == '"':
                i += 1
                while i < n:
                    if chars[i] == '"':
                        close_hashes = 0
                        j = i + 1
                        while j < n and chars[j] == '#' and close_hashes < hashes:
                            close_hashes += 1; j += 1
                        if close_hashes == hashes:
                            i = j; break
                    i += 1
                tokens.append((saved, i, T_STRING))
                continue
            i = saved
        # string
        if ch == '"':
            start = i; i += 1
            while i < n:
                if chars[i] == '\\': i += 2; continue
                if chars[i] == '"': i += 1; break
                i += 1
            tokens.append((start, i, T_STRING))
            continue
        # char literal or lifetime
        if ch == "'":
            start = i; i += 1
            if i < n and chars[i] == '\\': i += 1
            if i < n: i += 1
            if i < n and chars[i] == "'":
                i += 1
                tokens.append((start, i, T_STRING))
                continue
            # lifetime
            if start + 1 < n and chars[start + 1].isalpha():
                i = start + 1
                while i < n and _is_word_char(chars[i]):
                    i += 1
                tokens.append((start, i, T_TYPE))
                continue
            i = start + 1
            continue
        # attribute #[…] or #![…]
        if ch == '#' and i + 1 < n and chars[i + 1] == '[':
            start = i; i += 2; depth = 1
            while i < n and depth > 0:
                if chars[i] == '[': depth += 1
                elif chars[i] == ']': depth -= 1
                i += 1
            tokens.append((start, i, T_DECORATOR))
            continue
        # number
        if ch.isdigit() or (ch == '.' and i + 1 < n and chars[i + 1].isdigit()):
            start = i
            if ch == '0' and i + 1 < n and chars[i + 1] in 'xXoObB':
                i += 2
                while i < n and (chars[i].isdigit() or chars[i] in 'abcdefABCDEF_'):
                    i += 1
            else:
                while i < n and (chars[i].isdigit() or chars[i] in '.eE_'):
                    i += 1
            tokens.append((start, i, T_NUM))
            continue
        # three-char operators
        if i + 2 < n:
            three = chars[i:i + 3]
            if three in ('<<=', '>>=', '>>>'):
                tokens.append((i, i + 3, T_OPERATOR))
                i += 3; continue
        # two-char operators
        if i + 1 < n:
            two = chars[i:i + 2]
            if two in ('==', '!=', '<=', '>=', '&&', '||', '->', '=>', '..',
                       '::', '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=',
                       '<<', '>>'):
                tokens.append((i, i + 2, T_OPERATOR))
                i += 2; continue
        # single-char operators / punctuation
        if ch in '+-*/%=!<>&|^~:.@':
            tokens.append((i, i + 1, T_OPERATOR))
            i += 1; continue
        if ch in '()[]{};,\\':
            if ch == '(' and tokens and tokens[-1][2] == T_PLAIN:
                s, e, _ = tokens.pop()
                tokens.append((s, e, T_FN))
            i += 1; continue
        # identifier / macro
        if ch.isalpha() or ch == '_':
            start = i
            while i < n and _is_word_char(chars[i]):
                i += 1
            word = chars[start:i]
            is_macro = i < n and chars[i] == '!'
            if _is_ident_boundary(line, start, i):
                if word in RS_KW: tokens.append((start, i, T_KW))
                elif word in RS_TYPE: tokens.append((start, i, T_TYPE))
                elif word in RS_BUILTIN: tokens.append((start, i, T_BUILTIN))
                else:
                    # check if follows 'fn' keyword
                    after_fn = tokens and tokens[-1][2] == T_KW and chars[tokens[-1][0]:tokens[-1][1]] == 'fn'
                    t = T_FN if after_fn else T_PLAIN
                    tokens.append((start, i, t))
            else:
                tokens.append((start, i, T_PLAIN))
            if is_macro:
                tokens.append((i, i + 1, T_BUILTIN))
                i += 1
            continue
        i += 1
    return tokens


def _tokenize_c(line):
    chars = line
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        ch = chars[i]
        # preprocessor at col 0
        if ch == '#' and i == 0:
            tokens.append((i, n, T_DECORATOR))
            break
        # block comment
        if ch == '/' and i + 1 < n and chars[i + 1] == '*':
            start = i; i += 2
            while i + 1 < n and not (chars[i] == '*' and chars[i + 1] == '/'):
                i += 1
            if i + 1 < n: i += 2
            tokens.append((start, i, T_CMT))
            continue
        # line comment
        if ch == '/' and i + 1 < n and chars[i + 1] == '/':
            tokens.append((i, n, T_CMT))
            break
        # string
        if ch == '"':
            start = i; i += 1
            while i < n:
                if chars[i] == '\\': i += 2; continue
                if chars[i] == '"': i += 1; break
                i += 1
            tokens.append((start, i, T_STRING))
            continue
        # char literal
        if ch == "'":
            start = i; i += 1
            if i < n and chars[i] == '\\': i += 1
            if i < n: i += 1
            if i < n and chars[i] == "'": i += 1
            if i > start + 1:
                tokens.append((start, i, T_STRING))
            else:
                i = start + 1
            continue
        # number
        if ch.isdigit() or (ch == '.' and i + 1 < n and chars[i + 1].isdigit()):
            start = i
            if ch == '0' and i + 1 < n and chars[i + 1] in 'xXbB':
                i += 2
                while i < n and chars[i].isalnum():
                    i += 1
            else:
                while i < n and (chars[i].isdigit() or chars[i] in '.eExXfFuUlL'):
                    i += 1
            tokens.append((start, i, T_NUM))
            continue
        # three-char operators
        if i + 2 < n:
            three = chars[i:i + 3]
            if three in ('<<=', '>>=', '...'):
                tokens.append((i, i + 3, T_OPERATOR))
                i += 3; continue
        # two-char operators
        if i + 1 < n:
            two = chars[i:i + 2]
            if two in ('==', '!=', '<=', '>=', '&&', '||', '->', '++', '--',
                       '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=', '<<', '>>', '##'):
                tokens.append((i, i + 2, T_OPERATOR))
                i += 2; continue
        # single-char operators
        if ch in '+-*/%=!<>&|^~:.?':
            tokens.append((i, i + 1, T_OPERATOR))
            i += 1; continue
        if ch in '()[]{};,\\':
            if ch == '(' and tokens and tokens[-1][2] == T_PLAIN:
                s, e, _ = tokens.pop()
                tokens.append((s, e, T_FN))
            i += 1; continue
        # identifier
        if ch.isalpha() or ch == '_':
            start = i
            while i < n and _is_word_char(chars[i]):
                i += 1
            word = chars[start:i]
            if _is_ident_boundary(line, start, i):
                if word in C_KW: tokens.append((start, i, T_KW))
                elif word in C_TYPE: tokens.append((start, i, T_TYPE))
                elif word in C_BUILTIN: tokens.append((start, i, T_BUILTIN))
                else: tokens.append((start, i, T_PLAIN))
            else:
                tokens.append((start, i, T_PLAIN))
            continue
        i += 1
    return tokens


def _tokenize_javascript(line, multiline_cmt=False):
    chars = line
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        ch = chars[i]
        # block comment
        if ch == '/' and i + 1 < n and chars[i + 1] == '*':
            start = i; i += 2
            while i + 1 < n and not (chars[i] == '*' and chars[i + 1] == '/'):
                i += 1
            if i + 1 < n: i += 2
            tokens.append((start, i, T_CMT))
            continue
        # line comment
        if ch == '/' and i + 1 < n and chars[i + 1] == '/':
            tokens.append((i, n, T_CMT))
            break
        # template literal
        if ch == '`':
            start = i; i += 1
            while i < n:
                if chars[i] == '\\': i += 2; continue
                if chars[i] == '`': i += 1; break
                i += 1
            tokens.append((start, i, T_STRING))
            continue
        # string
        if ch in '"\'':
            quote = ch; start = i; i += 1
            while i < n:
                if chars[i] == '\\': i += 2; continue
                if chars[i] == quote: i += 1; break
                i += 1
            tokens.append((start, i, T_STRING))
            continue
        # number
        if ch.isdigit() or (ch == '.' and i + 1 < n and chars[i + 1].isdigit()):
            start = i
            while i < n and (chars[i].isdigit() or chars[i] in '.eExXoObB_'):
                i += 1
            tokens.append((start, i, T_NUM))
            continue
        # three-char operators
        if i + 2 < n:
            three = chars[i:i + 3]
            if three in ('===', '!==', '<<=', '>>=', '>>>', '**='):
                tokens.append((i, i + 3, T_OPERATOR))
                i += 3; continue
        # two-char operators
        if i + 1 < n:
            two = chars[i:i + 2]
            if two in ('==', '!=', '<=', '>=', '&&', '||', '=>', '??',
                       '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=',
                       '<<', '>>', '++', '--', '**', '?.'):
                tokens.append((i, i + 2, T_OPERATOR))
                i += 2; continue
        # single-char operators
        if ch in '+-*/%=!<>&|^~:.?':
            tokens.append((i, i + 1, T_OPERATOR))
            i += 1; continue
        if ch in '()[]{};,':
            if ch == '(' and tokens and tokens[-1][2] == T_PLAIN:
                s, e, _ = tokens.pop()
                tokens.append((s, e, T_FN))
            i += 1; continue
        # identifier
        if ch.isalpha() or ch == '_' or ch == '$':
            start = i
            while i < n and (_is_word_char(chars[i]) or chars[i] == '$'):
                i += 1
            word = chars[start:i]
            if _is_ident_boundary(line, start, i):
                if word in JS_KW: tokens.append((start, i, T_KW))
                elif word in JS_BUILTIN: tokens.append((start, i, T_BUILTIN))
                else: tokens.append((start, i, T_PLAIN))
            else:
                tokens.append((start, i, T_PLAIN))
            continue
        i += 1
    return tokens


def _tokenize_markdown(line):
    chars = line
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        ch = chars[i]
        # heading # … ######
        if ch == '#' and all(c in '#' for c in chars[:i] if not c.isspace()):
            start = i
            while i < n and chars[i] == '#': i += 1
            if i < n and chars[i] == ' ': i += 1
            tokens.append((start, i, T_DECORATOR))
            if i < n:
                tokens.append((i, n, T_FN))
            break
        # blockquote
        if ch == '>' and (i == 0 or chars[i - 1].isspace()):
            start = i; i += 1
            while i < n and chars[i] == ' ': i += 1
            tokens.append((start, i, T_CMT))
            if i < n:
                tokens.append((i, n, T_PLAIN))
            break
        # horizontal rule --- *** ___
        if ch in '-*_':
            c = ch; start = i; cnt = 0
            while i < n and chars[i] == c: cnt += 1; i += 1
            if cnt >= 3 and i == n:
                tokens.append((start, i, T_OPERATOR))
                break
            i = start
        # unordered list marker
        if ch in '-*+' and (i == 0 or chars[i - 1].isspace()) and i + 1 < n and chars[i + 1] == ' ':
            tokens.append((i, i + 1, T_OPERATOR))
            i += 1; continue
        # ordered list marker
        if ch.isdigit() and (i == 0 or chars[i - 1].isspace()):
            j = i
            while j < n and chars[j].isdigit(): j += 1
            if j < n and chars[j] == '.' and j + 1 < n and chars[j + 1] == ' ':
                tokens.append((i, j + 1, T_OPERATOR))
                i = j + 1; continue
        # inline code
        if ch == '`':
            start = i; i += 1
            while i < n and chars[i] != '`': i += 1
            if i < n: i += 1
            tokens.append((start, i, T_STRING))
            continue
        # link [text](url)
        if ch == '[':
            start = i; i += 1
            while i < n and chars[i] != ']': i += 1
            if i < n:
                i += 1
                if i < n and chars[i] == '(':
                    tokens.append((start, i, T_FN))
                    url_start = i; i += 1
                    while i < n and chars[i] != ')': i += 1
                    if i < n: i += 1
                    tokens.append((url_start, i, T_OPERATOR))
                    continue
            i = start
        # image ![alt](url)
        if ch == '!' and i + 1 < n and chars[i + 1] == '[':
            start = i; i += 2
            while i < n and chars[i] != ']': i += 1
            if i < n:
                i += 1
                if i < n and chars[i] == '(':
                    tokens.append((start, i, T_DECORATOR))
                    url_start = i; i += 1
                    while i < n and chars[i] != ')': i += 1
                    if i < n: i += 1
                    tokens.append((url_start, i, T_STRING))
                    continue
            i = start
        # bold ** or __
        if ch in '*_' and i + 1 < n and chars[i + 1] == ch:
            c = ch; start = i; i += 2
            while i + 1 < n and not (chars[i] == c and chars[i + 1] == c): i += 1
            if i + 1 < n: i += 2
            tokens.append((start, i, T_BUILTIN))
            continue
        # italic * or _
        if ch in '*_':
            c = ch; start = i; i += 1
            while i < n and chars[i] != c: i += 1
            if i < n: i += 1
            tokens.append((start, i, T_STRING))
            continue
        i += 1
    return tokens


def _tokenize_conf(line):
    chars = line
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        ch = chars[i]
        # string
        if ch in '"\'':
            quote = ch; start = i; i += 1
            while i < n:
                if chars[i] == '\\': i += 2; continue
                if chars[i] == quote: i += 1; break
                i += 1
            tokens.append((start, i, T_STRING))
            continue
        # #: comment or hex colour
        if ch == '#':
            preceded_by_space = i == 0 or chars[i - 1].isspace()
            if preceded_by_space and i + 1 < n and chars[i + 1].isalnum():
                start = i; i += 1
                hex_start = i
                while i < n and chars[i] in '0123456789abcdefABCDEF':
                    i += 1
                hlen = i - hex_start
                if hlen in (3, 4, 6, 8) and (i == n or chars[i].isspace()):
                    tokens.append((start, i, T_NUM))
                    continue
                i = start
            tokens.append((i, n, T_CMT))
            break
        # number
        if ch.isdigit():
            start = i
            while i < n and (chars[i].isdigit() or chars[i] in '._'):
                i += 1
            tokens.append((start, i, T_NUM))
            continue
        # operators / punctuation
        if ch in '+-=:;,.~><':
            tokens.append((i, i + 1, T_OPERATOR))
            i += 1; continue
        i += 1
    return tokens


# ── keyword / type / builtin sets ───────────────────────────────

PY_KW = {'def', 'class', 'if', 'elif', 'else', 'for', 'while', 'return',
         'import', 'from', 'as', 'try', 'except', 'finally', 'with',
         'pass', 'break', 'continue', 'and', 'or', 'not', 'in', 'is',
         'lambda', 'yield', 'raise', 'global', 'nonlocal', 'assert',
         'del', 'async', 'await', 'True', 'False', 'None'}

PY_BUILTIN = {'print', 'len', 'range', 'int', 'str', 'float', 'list', 'dict',
              'set', 'tuple', 'type', 'open', 'input', 'super', 'isinstance',
              'hasattr', 'getattr', 'setattr', 'map', 'filter', 'zip',
              'enumerate', 'sorted', 'reversed', 'abs', 'min', 'max', 'sum',
              'any', 'all', 'repr', 'eval', 'exec', 'dir', 'vars', 'id',
              'hex', 'oct', 'bin', 'ord', 'chr', 'format'}

PY_TYPE = {'int', 'str', 'float', 'list', 'dict', 'set', 'tuple', 'bool',
           'bytes', 'bytearray', 'memoryview', 'object', 'Exception',
           'ValueError', 'TypeError', 'KeyError', 'IndexError',
           'AttributeError', 'NameError', 'SyntaxError', 'RuntimeError',
           'StopIteration', 'ImportError', 'OSError', 'FileNotFoundError'}

JS_KW = {'function', 'var', 'let', 'const', 'if', 'else', 'for', 'while',
         'do', 'switch', 'case', 'break', 'continue', 'return', 'import',
         'export', 'from', 'class', 'extends', 'new', 'this', 'super',
         'try', 'catch', 'finally', 'throw', 'typeof', 'instanceof',
         'in', 'of', 'async', 'await', 'yield', 'delete', 'void',
         'true', 'false', 'null', 'undefined'}

JS_BUILTIN = {'console', 'document', 'window', 'Math', 'JSON', 'Array',
              'Object', 'String', 'Number', 'Boolean', 'Date', 'RegExp',
              'Map', 'Set', 'Promise', 'setTimeout', 'setInterval',
              'parseInt', 'parseFloat', 'isNaN', 'fetch', 'require'}

C_KW = {'auto', 'break', 'case', 'char', 'const', 'continue', 'default',
        'do', 'double', 'else', 'enum', 'extern', 'float', 'for', 'goto',
        'if', 'inline', 'int', 'long', 'register', 'restrict', 'return',
        'short', 'signed', 'sizeof', 'static', 'struct', 'switch',
        'typedef', 'union', 'unsigned', 'void', 'volatile', 'while',
        '_Bool', 'bool', 'true', 'false', 'NULL', 'nullptr'}

C_TYPE = {'size_t', 'ssize_t', 'int8_t', 'int16_t', 'int32_t', 'int64_t',
          'uint8_t', 'uint16_t', 'uint32_t', 'uint64_t', 'intptr_t',
          'uintptr_t', 'ptrdiff_t', 'FILE', 'time_t', 'va_list'}

C_BUILTIN = {'printf', 'fprintf', 'sprintf', 'snprintf', 'scanf', 'fscanf',
             'puts', 'fopen', 'fclose', 'fread', 'fwrite', 'fseek', 'ftell',
             'malloc', 'calloc', 'realloc', 'free', 'memcpy', 'memmove',
             'memset', 'memcmp', 'memchr', 'strlen', 'strcpy', 'strcmp',
             'strchr', 'strstr', 'strdup', 'atoi', 'atol', 'atof', 'abs',
             'qsort', 'bsearch', 'exit', 'abort', 'assert', 'perror', 'main'}

RS_KW = {'as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn',
         'else', 'enum', 'extern', 'false', 'fn', 'for', 'if', 'impl', 'in',
         'let', 'loop', 'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return',
         'self', 'Self', 'static', 'struct', 'super', 'trait', 'true', 'type',
         'unsafe', 'use', 'where', 'while'}

RS_TYPE = {'i8', 'i16', 'i32', 'i64', 'i128', 'isize', 'u8', 'u16', 'u32',
           'u64', 'u128', 'usize', 'f32', 'f64', 'bool', 'char', 'str',
           'String', 'Vec', 'Box', 'Option', 'Result', 'Arc', 'Rc',
           'Cell', 'RefCell', 'Cow', 'HashMap', 'HashSet', 'Mutex', 'RwLock',
           'Some', 'None', 'Ok', 'Err', 'Path', 'PathBuf'}

RS_BUILTIN = {'dbg', 'eprintln', 'eprint', 'println', 'print', 'format',
              'assert', 'assert_eq', 'assert_ne', 'panic', 'unreachable',
              'unimplemented', 'todo', 'vec', 'cfg', 'matches', 'include_str',
              'include_bytes', 'concat', 'stringify', 'write', 'writeln',
              'file', 'line', 'column', 'env', 'option_env', 'core', 'std'}

HTML_KW = {'html', 'head', 'body', 'div', 'span', 'p', 'a', 'img',
           'ul', 'ol', 'li', 'table', 'tr', 'td', 'th', 'form',
           'input', 'button', 'select', 'option', 'textarea',
           'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'meta', 'link',
           'script', 'style', 'title', 'header', 'footer', 'nav',
           'section', 'article', 'aside', 'main'}

def _tokenize_html(line):
    """Basic HTML tokenizer: tags, attributes, strings, comments."""
    chars = line
    n = len(chars)
    tokens = []
    i = 0
    while i < n:
        ch = chars[i]
        # comment <!-- ... -->
        if ch == '<' and i + 3 < n and chars[i:i + 4] == '<!--':
            start = i; i += 4
            while i + 2 < n and chars[i:i + 3] != '-->':
                i += 1
            if i + 2 < n: i += 3
            tokens.append((start, i, T_CMT))
            continue
        # tag
        if ch == '<':
            start = i; i += 1
            while i < n and chars[i] not in '> \t\n':
                i += 1
            tag = chars[start + 1:i].lower()
            is_kw = tag in HTML_KW or tag.startswith('/')
            ttype = T_KW if is_kw else T_PLAIN
            tokens.append((start, i, ttype))
            # attributes
            while i < n and chars[i] != '>':
                if chars[i] in '"\'':
                    q = chars[i]; ai = i; i += 1
                    while i < n and chars[i] != q: i += 1
                    if i < n: i += 1
                    tokens.append((ai, i, T_STRING))
                else:
                    i += 1
            if i < n and chars[i] == '>':
                tokens.append((i, i + 1, T_KW))
                i += 1
            continue
        i += 1
    return tokens


# Map file extensions to tokenizer functions
TOKENIZERS = {
    '.py':   _tokenize_python,
    '.rs':   _tokenize_rust,
    '.c':    _tokenize_c,
    '.h':    _tokenize_c,
    '.cpp':  _tokenize_c,
    '.hpp':  _tokenize_c,
    '.js':   _tokenize_javascript,
    '.ts':   _tokenize_javascript,
    '.jsx':  _tokenize_javascript,
    '.tsx':  _tokenize_javascript,
    '.css':  _tokenize_javascript,
    '.html': _tokenize_html,
    '.htm':  _tokenize_html,
    '.md':   _tokenize_markdown,
    '.markdown': _tokenize_markdown,
    '.conf': _tokenize_conf,
    '.ini':  _tokenize_conf,
    '.cfg':  _tokenize_conf,
    '.toml': _tokenize_conf,
}


def get_syntax(filename):
    if not filename:
        return None
    _, ext = os.path.splitext(filename)
    return TOKENIZERS.get(ext.lower())

# ── System dashboard ──────────────────────────────────────────

class SysInfo:
    def __init__(self):
        self.data = {}
        self._last_fetch = 0

    def fetch(self):
        now = time.time()
        if now - self._last_fetch < 0.9:
            return self.data
        self._last_fetch = now
        d = {}
        # host
        d['host'] = os.uname().nodename
        d['user'] = os.environ.get('USER', '?')
        # uptime
        try:
            out = subprocess.check_output(['uptime'], text=True, timeout=2)
            m = re.search(r'up\s+(.+?),\s+\d+\s+users?', out)
            d['uptime'] = m.group(1) if m else '?'
            m = re.search(r'load averages?: ([0-9.]+)\s+([0-9.]+)\s+([0-9.]+)', out)
            if m:
                d['load'] = (float(m.group(1)), float(m.group(2)), float(m.group(3)))
        except Exception:
            d['uptime'] = '?'
        # CPU
        d['cpu'] = self._cpu_pct()
        # memory
        d['mem'] = self._mem_info()
        # disk
        d['disk'] = self._disk_info()
        # battery
        d['batt'] = self._batt_info()
        self.data = d
        return d

    @staticmethod
    def _cpu_pct():
        try:
            out = subprocess.check_output(
                ['top', '-l', '1', '-n', '0', '-s', '0'],
                text=True, timeout=4, stderr=subprocess.DEVNULL
            )
            m = re.search(r'CPU usage:\s+([0-9.]+)%\s+user,\s+([0-9.]+)%\s+sys', out)
            if m:
                return float(m.group(1)) + float(m.group(2))
            # Linux fallback
            with open('/proc/stat') as f:
                line = f.readline()
            parts = line.split()
            if len(parts) >= 5:
                idle = int(parts[4])
                total = sum(int(x) for x in parts[1:])
                return max(0, min(100, (1 - idle / max(total, 1)) * 100))
        except Exception:
            return 0.0

    @staticmethod
    def _mem_info():
        try:
            out = subprocess.check_output(['vm_stat'], text=True, timeout=2)
            pages = {}
            # Parse page size
            m = re.search(r'page size of (\d+)', out)
            page_size = int(m.group(1)) if m else 16384
            # Parse page counts
            for m in re.finditer(r'Pages (\w[\w\s]*?):\s+(\d+)', out):
                key = 'Pages ' + m.group(1).strip()
                pages[key] = int(m.group(2))
            used_pages = pages.get('Pages active', 0) + pages.get('Pages wired down', 0)
            used = used_pages * page_size
            try:
                total = int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True, timeout=2).strip())
            except Exception:
                total = max(used * 2, 1)
            return used, total
        except Exception:
            return 0, 1

    @staticmethod
    def _disk_info():
        try:
            out = subprocess.check_output(['df', '-h', '/'], text=True, timeout=2)
            parts = out.strip().split('\n')[-1].split()
            if len(parts) >= 6:
                return parts[1], parts[2], parts[3], parts[4]  # size, used, avail, pct
        except Exception:
            pass
        return '?', '?', '?', '?'

    @staticmethod
    def _batt_info():
        try:
            out = subprocess.check_output(['pmset', '-g', 'batt'], text=True, timeout=2)
            m = re.search(r'(\d+)%;\s*(\w+)', out)
            if m:
                pct = int(m.group(1))
                status = 'charging' if m.group(2).lower() == 'charging' else 'discharging'
                if 'AC' in out and 'charging' not in out.lower():
                    status = 'on AC'
                if pct == 100 and 'charged' in out.lower():
                    status = 'charged'
                return pct, status
        except Exception:
            pass
        return None, None

IMAGE_EXTS = {'.png', '.jpg', '.jpeg', '.gif', '.bmp', '.webp', '.ico', '.tiff', '.tif'}

def _png_dims(path):
    """Read width/height from a PNG file header. Returns (w, h) or None."""
    try:
        with open(path, 'rb') as f:
            if f.read(8) != b'\x89PNG\r\n\x1a\n':
                return None
            f.read(4)  # chunk length
            if f.read(4) != b'IHDR':
                return None
            w = struct.unpack('>I', f.read(4))[0]
            h = struct.unpack('>I', f.read(4))[0]
            return w, h
    except (OSError, struct.error):
        return None


def kitty_show_image(path, x, y, max_w, max_h):
    """Return ANSI escape sequences to display an image via Kitty protocol.
    Positions at (x, y), scales to fit within max_w x max_h terminal cells."""
    dims = _png_dims(path)
    if dims is None:
        return None
    img_w, img_h = dims
    # Estimate terminal cell size: ~9px wide, ~18px tall (varies by font)
    cell_w, cell_h = 9, 18
    avail_w = max_w * cell_w
    avail_h = max_h * cell_h
    scale = min(avail_w / img_w, avail_h / img_h, 1.0)
    disp_w = int(img_w * scale)
    disp_h = int(img_h * scale)
    # Read and base64-encode the image
    with open(path, 'rb') as f:
        data = base64.b64encode(f.read()).decode()
    # Build kitty escape — chunk if > 4096 bytes of base64
    chunks = []
    chunk_size = 4096
    for i in range(0, len(data), chunk_size):
        m = 1 if i + chunk_size < len(data) else 0
        chunks.append(f'\033_Ga=T,f=100,s={disp_w},v={disp_h},m={m};{data[i:i + chunk_size]}\033\\')
    # Position
    pos = f'\033[{y + 1};{x + 1}H'
    return pos + ''.join(chunks)


# ── Terminal ─────────────────────────────────────────────────────

orig_termios = None

def term_init():
    global orig_termios
    fd = sys.stdin.fileno()
    if not os.isatty(fd): return
    orig_termios = termios.tcgetattr(fd)
    raw = termios.tcgetattr(fd)
    raw[0] &= ~(termios.BRKINT | termios.ICRNL | termios.INPCK | termios.ISTRIP | termios.IXON)
    raw[1] &= ~termios.OPOST
    raw[3] &= ~(termios.ECHO | termios.ICANON | termios.IEXTEN | termios.ISIG)
    raw[6][termios.VMIN] = 0
    raw[6][termios.VTIME] = 1
    termios.tcsetattr(fd, termios.TCSAFLUSH, raw)
    sys.stdout.write('\033[?1049h\033[?25l')
    sys.stdout.flush()

def term_restore():
    global orig_termios
    if orig_termios is None: return
    if not os.isatty(sys.stdin.fileno()): return
    # Kill any playing music
    subprocess.run(['pkill', '-x', 'afplay'], stderr=subprocess.DEVNULL)
    sys.stdout.write('\033[?25h\033[?1049l')
    sys.stdout.flush()
    termios.tcsetattr(sys.stdin.fileno(), termios.TCSAFLUSH, orig_termios)
    orig_termios = None

def term_size():
    try:
        sz = shutil.get_terminal_size()
        return sz.columns, sz.lines
    except:
        return 80, 24

# ── Signal handling ──────────────────────────────────────────────

resize_flag = False
def sigwinch(sig, frame):
    global resize_flag
    resize_flag = True

def crash_handler(sig, frame):
    term_restore()
    sys.exit(128 + sig)

signal.signal(signal.SIGTERM, crash_handler)
signal.signal(signal.SIGSEGV, crash_handler)
signal.signal(signal.SIGBUS, crash_handler)
signal.signal(signal.SIGABRT, crash_handler)
signal.signal(signal.SIGQUIT, crash_handler)
atexit.register(term_restore)

# ── Key codes ───────────────────────────────────────────────────

KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT = 0x1000, 0x1001, 0x1002, 0x1003
KEY_PGUP, KEY_PGDN = 0x1004, 0x1005
KEY_HOME, KEY_END = 0x1006, 0x1007
KEY_DEL = 0x1008
KEY_ENTER = 0x1009
KEY_TAB = 0x100a
KEY_ESC = 0x100b
KEY_BACKSP = 0x100c
KEY_BACKTAB = 0x100d

def KEY_CTRL(c): return 0x3000 + (ord(c) - ord('a') + 1)

keybuf = b''

def read_key():
    global keybuf
    fd = sys.stdin.fileno()

    # Read at least one byte
    if len(keybuf) == 0:
        buf = os.read(fd, 64)
        if not buf: return -1
        keybuf = buf

    b = keybuf[0]
    keybuf = keybuf[1:]

    if b == 0x1b:
        # Escape sequence — try to read more
        if len(keybuf) == 0:
            # Wait briefly for more bytes
            r, _, _ = select.select([fd], [], [], 0.05)
            if r:
                extra = os.read(fd, 32)
                keybuf = extra if len(extra) > 0 else b''
        return parse_escape(b, keybuf)

    if b in (0x7f, 0x08): return KEY_BACKSP
    if b == 0x0d: return KEY_ENTER
    if b == 0x09: return KEY_TAB
    if 1 <= b <= 26: return 0x3000 + b
    return b

def parse_escape(first_byte, buf):
    """Parse an ANSI escape sequence starting with first_byte (ESC).
    buf contains remaining bytes. Returns (keycode, leftover_buf)."""
    global keybuf
    if len(buf) == 0:
        return KEY_ESC

    b1 = buf[0]
    rest = buf[1:]

    if b1 == ord('['):
        if len(rest) == 0: return KEY_ESC
        b2 = rest[0]
        rest = rest[1:]
        # Handle sequences like [1~, [3~, [5~, [6~ (with optional prefix)
        if b2 == ord('1') and len(rest) > 0 and rest[0] == ord('~'):
            keybuf = rest[1:]; return KEY_HOME
        if b2 == ord('3') and len(rest) > 0 and rest[0] == ord('~'):
            keybuf = rest[1:]; return KEY_DEL
        if b2 == ord('4') and len(rest) > 0 and rest[0] == ord('~'):
            keybuf = rest[1:]; return KEY_END
        if b2 == ord('5') and len(rest) > 0 and rest[0] == ord('~'):
            keybuf = rest[1:]; return KEY_PGUP
        if b2 == ord('6') and len(rest) > 0 and rest[0] == ord('~'):
            keybuf = rest[1:]; return KEY_PGDN
        if b2 == ord('7') and len(rest) > 0 and rest[0] == ord('~'):
            keybuf = rest[1:]; return KEY_HOME
        if b2 == ord('8') and len(rest) > 0 and rest[0] == ord('~'):
            keybuf = rest[1:]; return KEY_END
        # Handle single-char sequences like [A, [B, [C, [D, [H, [F
        keybuf = rest
        if b2 == ord('A'): return KEY_UP
        if b2 == ord('B'): return KEY_DOWN
        if b2 == ord('C'): return KEY_RIGHT
        if b2 == ord('D'): return KEY_LEFT
        if b2 == ord('H'): return KEY_HOME
        if b2 == ord('F'): return KEY_END
        if b2 == ord('Z'): return KEY_BACKTAB
        return KEY_ESC

    if b1 == ord('O'):
        keybuf = rest
        if len(rest) == 0: return KEY_ESC
        b2 = rest[0]
        keybuf = rest[1:]
        if b2 == ord('H'): return KEY_HOME
        if b2 == ord('F'): return KEY_END
        if b2 == ord('A'): return KEY_UP
        if b2 == ord('B'): return KEY_DOWN
        if b2 == ord('C'): return KEY_RIGHT
        if b2 == ord('D'): return KEY_LEFT
        return KEY_ESC

    # Not a valid escape — treat as ESC followed by raw byte
    keybuf = buf
    return KEY_ESC

# ── Music Player ──────────────────────────────────────────────────

class MusicPlayer:
    """Inline music player using afplay (macOS built-in)."""

    def __init__(self):
        self.files = []
        self.selected = 0
        self.playing = False
        self.current_song = None
        self.current_index = 0
        self.playlist = []
        self.scan_dir = None

        # Kill any orphaned afplay from a previous session.
        subprocess.run(['pkill', '-x', 'afplay'], capture_output=True)

        self.cmd_queue = queue.Queue()
        self.event_queue = queue.Queue()
        self.thread = threading.Thread(target=self._audio_thread, daemon=True)
        self.thread.start()

    def scan(self, directory):
        """Scan directory tree for *.mp3 files (sorted)."""
        self.files.clear()
        self.scan_dir = directory
        self._walk(directory, 0)
        self.files.sort()
        self.selected = 0
        if self.current_song:
            try:
                canon = os.path.realpath(self.current_song)
                if canon in self.files:
                    self.selected = self.files.index(canon)
            except OSError:
                pass

    def _walk(self, directory, depth):
        if depth > 4:
            return
        try:
            entries = os.listdir(directory)
        except PermissionError:
            return
        for name in entries:
            if name.startswith('.'):
                continue
            path = os.path.join(directory, name)
            if os.path.isdir(path):
                self._walk(path, depth + 1)
            elif os.path.isfile(path) and name.lower().endswith('.mp3'):
                self.files.append(os.path.abspath(path))

    def play(self, index):
        """Start playing the file at `index`, queuing the rest as playlist."""
        if index >= len(self.files):
            return
        self.playlist = list(self.files)
        self.current_index = index
        self.playing = True
        path = self.files[index]
        self.cmd_queue.put(('play', path))

    def stop(self):
        """Stop playback and reset state."""
        self.playing = False
        self.current_song = None
        self.current_index = 0
        self.cmd_queue.put(('stop',))

    def next(self):
        """Move to the next track in the playlist."""
        nxt = self.current_index + 1
        if nxt < len(self.playlist):
            self.play(nxt)
        else:
            self.playing = False
            self.current_song = None

    def poll(self):
        """Check for events from the audio thread. Call once per frame."""
        try:
            while True:
                event = self.event_queue.get_nowait()
                if event[0] == 'started':
                    self.playing = True
                    self.current_song = event[1]
                elif event[0] == 'ended':
                    if self.playing:
                        self.next()
        except queue.Empty:
            pass

    def _audio_thread(self):
        child = None
        while True:
            try:
                cmd = self.cmd_queue.get(timeout=0.1)
                if cmd[0] == 'play':
                    path = cmd[1]
                    if child:
                        child.kill()
                        child.wait()
                    try:
                        child = subprocess.Popen(
                            ['afplay', path],
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL,
                        )
                        self.event_queue.put(('started', path))
                    except Exception:
                        self.event_queue.put(('ended', None))
                        child = None
                elif cmd[0] == 'stop':
                    if child:
                        child.kill()
                        child.wait()
                        child = None
                    self.event_queue.put(('ended', None))
            except queue.Empty:
                pass

            if child:
                ret = child.poll()
                if ret is not None:
                    child = None
                    self.event_queue.put(('ended', None))

# ── Pop-up shell ──────────────────────────────────────────────

# Standard 16 ANSI colours → RGB
_ANSI16 = [
    C(0x1d,0x1f,0x21), C(0xcc,0x66,0x66), C(0xb5,0xbd,0x68), C(0xf0,0xc6,0x74),
    C(0x81,0xa2,0xbe), C(0xb2,0x94,0xbb), C(0x8a,0xbe,0xb7), C(0xc5,0xc8,0xc6),
    C(0x66,0x66,0x66), C(0xd5,0x4e,0x53), C(0xb9,0xca,0x4a), C(0xe6,0xc5,0x47),
    C(0x7a,0xa6,0xda), C(0xc3,0x97,0xd8), C(0x70,0xc0,0xb1), C(0xea,0xea,0xea),
]

def _ansi256_to_rgb(n):
    if n < 16:       return _ANSI16[n]
    if n < 232:
        n -= 16; r = n // 36; g = (n // 6) % 6; b = n % 6
        return C(r * 40 + 55 if r else 0, g * 40 + 55 if g else 0, b * 40 + 55 if b else 0)
    v = (n - 232) * 10 + 8
    return C(v, v, v)


def _parse_sgr(params, style):
    """Apply ANSI SGR parameters to a (fg, bg, bold) tuple.  Returns new tuple."""
    fg, bg, bold = style
    parts = [p for p in params.split(';') if p]
    i = 0
    while i < len(parts):
        try: p = int(parts[i])
        except ValueError: i += 1; continue
        if p == 0:     fg, bg, bold = None, None, False
        elif p == 1:   bold = True
        elif p == 22:  bold = False
        elif 30 <= p <= 37:   fg = _ANSI16[p - 30]
        elif 40 <= p <= 47:   bg = _ANSI16[p - 40]
        elif 90 <= p <= 97:   fg = _ANSI16[p - 90 + 8]
        elif 100 <= p <= 107: bg = _ANSI16[p - 100 + 8]
        elif p == 38:
            i += 1
            if i < len(parts) and parts[i] == '5':
                i += 1
                if i < len(parts):
                    try: fg = _ansi256_to_rgb(int(parts[i]))
                    except ValueError: pass
            elif i < len(parts) and parts[i] == '2':
                i += 1
                if i + 2 < len(parts):
                    try: fg = C(int(parts[i]), int(parts[i+1]), int(parts[i+2]))
                    except ValueError: pass
                    i += 2
        elif p == 48:
            i += 1
            if i < len(parts) and parts[i] == '5':
                i += 1
                if i < len(parts):
                    try: bg = _ansi256_to_rgb(int(parts[i]))
                    except ValueError: pass
            elif i < len(parts) and parts[i] == '2':
                i += 1
                if i + 2 < len(parts):
                    try: bg = C(int(parts[i]), int(parts[i+1]), int(parts[i+2]))
                    except ValueError: pass
                    i += 2
        elif p == 39: fg = None
        elif p == 49: bg = None
        i += 1
    return (fg, bg, bold)


class ShellProcess:
    """PTY-backed shell running inside a popup overlay (Ctrl+J)."""

    def __init__(self):
        self.master = None
        self.slave = None
        self.pid = None
        self.alive = False
        self.output = b''
        self.reader_thread = None
        self.reader_rx = None

    def spawn(self):
        master_fd, slave_fd = os.openpty()
        pid = os.fork()
        if pid == 0:
            # child: set up terminal and exec shell
            os.close(master_fd)
            os.setsid()
            # Set controlling terminal
            try:
                fcntl.ioctl(slave_fd, termios.TIOCSCTTY, 0)
            except OSError:
                pass
            # Raw mode on slave
            attrs = termios.tcgetattr(slave_fd)
            attrs[0] &= ~(termios.BRKINT | termios.ICRNL | termios.INPCK | termios.ISTRIP | termios.IXON)
            attrs[1] &= ~termios.OPOST
            attrs[3] &= ~(termios.ECHO | termios.ICANON | termios.IEXTEN | termios.ISIG)
            attrs[6][termios.VMIN] = 1
            attrs[6][termios.VTIME] = 0
            termios.tcsetattr(slave_fd, termios.TCSANOW, attrs)
            # Redirect stdio
            os.dup2(slave_fd, 0)
            os.dup2(slave_fd, 1)
            os.dup2(slave_fd, 2)
            if slave_fd > 2:
                os.close(slave_fd)
            # Set TERM
            os.environ['TERM'] = 'xterm-256color'
            shell = os.environ.get('SHELL', '/bin/bash')
            os.execvp(shell, [shell])
            os._exit(1)

        # parent — keep both fds open: master for I/O, slave for ioctl
        self.master = master_fd
        self.slave = slave_fd
        self.pid = pid
        self.alive = True

        # Background reader thread
        rx_queue = queue.Queue()
        self.reader_rx = rx_queue

        def _reader():
            while True:
                try:
                    data = os.read(master_fd, 4096)
                except OSError:
                    data = b''
                rx_queue.put(data)
                if not data:
                    break

        self.reader_thread = threading.Thread(target=_reader, daemon=True)
        self.reader_thread.start()
        return self

    def write(self, data):
        if self.master is not None:
            try:
                os.write(self.master, data)
            except OSError:
                pass

    def resize(self, rows, cols):
        if self.master is not None:
            try:
                winsz = struct.pack('HHHH', rows, cols, 0, 0)
                fcntl.ioctl(self.master, termios.TIOCSWINSZ, winsz)
            except OSError:
                pass

    def tick(self):
        if self.reader_rx is None:
            return
        try:
            while True:
                data = self.reader_rx.get_nowait()
                if not data:
                    self.alive = False
                    return
                self.output += data
                # Cap at 128 KB
                if len(self.output) > 128 * 1024:
                    cut = len(self.output) - 64 * 1024
                    nl = self.output.find(b'\n', cut)
                    if nl >= 0:
                        self.output = self.output[nl + 1:]
                    else:
                        self.output = self.output[cut:]
        except queue.Empty:
            pass
        self._force_raw()

    def _force_raw(self):
        """Ensure the slave side stays in raw mode (shell may re-enable echo)."""
        if self.slave is None:
            return
        try:
            attrs = termios.tcgetattr(self.slave)
        except (termios.error, OSError):
            return
        if attrs[3] & termios.ECHO:
            attrs[3] &= ~termios.ECHO
            try:
                termios.tcsetattr(self.slave, termios.TCSANOW, attrs)
            except (termios.error, OSError):
                pass

    def is_alive(self):
        return self.alive

    def kill(self):
        if self.pid:
            try: os.kill(self.pid, signal.SIGKILL)
            except OSError: pass
        self.alive = False
        if self.master is not None:
            try: os.close(self.master)
            except OSError: pass
            self.master = None
        if self.slave is not None:
            try: os.close(self.slave)
            except OSError: pass
            self.slave = None

    def styled_lines(self, fg_color, bg_color):
        """Return a list of lines; each line is a list of (text, fg_Color_or_None, bold)."""
        text = self.output.decode('utf-8', errors='replace')
        lines_out = []
        cur_line = []
        style = (None, None, False)  # fg, bg, bold
        i = 0
        while i < len(text):
            ch = text[i]
            if ch == '\x1b' and i + 1 < len(text) and text[i + 1] == '[':
                j = i + 2
                while j < len(text) and not (0x40 <= ord(text[j]) <= 0x7e):
                    j += 1
                if j < len(text):
                    term = text[j]
                    params = text[i + 2:j]
                    if term == 'm':
                        style = _parse_sgr(params, style)
                    elif term == 'J' and params in ('2', '3'):
                        lines_out.clear()
                        cur_line.clear()
                    i = j + 1
                    continue
            elif ch == '\r':
                cur_line.clear()
            elif ch == '\n':
                lines_out.append(cur_line)
                cur_line = []
            elif ch == '\x08':
                if cur_line:
                    cur_line.pop()
            elif ch == '\t':
                spaces = 8 - (sum(len(t) for t, _, _ in cur_line) % 8)
                cur_line.append((' ' * spaces, style[0], style[2]))
            elif ch.isprintable() or ch in ' \t':
                cur_line.append((ch, style[0], style[2]))
            i += 1
        if cur_line:
            lines_out.append(cur_line)
        return lines_out


# ── Config ──────────────────────────────────────────────────────

def _parse_toml(text):
    """Minimal TOML parser for ked's config: key = \"str\" | 'str' | int | bare."""
    cfg = {}
    for raw in text.split('\n'):
        line = raw.split('#')[0].strip()
        if not line or '=' not in line:
            continue
        key, _, val = line.partition('=')
        key = key.strip()
        val = val.strip()
        if (val.startswith('"') and val.endswith('"')) or \
           (val.startswith("'") and val.endswith("'")):
            cfg[key] = val[1:-1]
        elif val.lower() in ('true', 'false'):
            cfg[key] = val.lower() == 'true'
        elif val.isdigit() or (val.startswith('-') and val[1:].isdigit()):
            cfg[key] = int(val)
        else:
            cfg[key] = val  # bare word or path
    return cfg


def load_config():
    """Load config from ~/.config/ked/config.toml (or ~/.config/pked/config.toml).
    Returns a dict with defaults if neither file is readable."""
    home = os.path.expanduser('~')
    for name in ('ked', 'pked'):
        path = os.path.join(home, '.config', name, 'config.toml')
        try:
            with open(path) as f:
                return _parse_toml(f.read())
        except (FileNotFoundError, PermissionError, OSError):
            continue
    return {}


# ── Pane ───────────────────────────────────────────────────────

class Pane:
    __slots__ = ('buffer_idx', 'cy', 'cx', 'top', 'left')

    def __init__(self, buffer_idx):
        self.buffer_idx = buffer_idx
        self.cy = 0
        self.cx = 0
        self.top = 0
        self.left = 0


# ── Buffer ─────────────────────────────────────────────────────

class Buffer:
    __slots__ = ('lines', 'cy', 'cx', 'top', 'left', 'filename',
                 'modified', 'undo_stack', 'last_mtime', 'shell')

    def __init__(self, lines, filename=None):
        self.lines = lines
        self.cy = 0
        self.cx = 0
        self.top = 0
        self.left = 0
        self.filename = filename
        self.modified = False
        self.undo_stack = []
        self.last_mtime = None
        self.shell = None  # ShellProcess if this is a shell buffer


# ── Editor ───────────────────────────────────────────────────────

class Editor:
    def __init__(self, filename=None):
        # ── load config ──
        cfg = load_config()

        # ── per-buffer state (shortcuts — synced from buffers[current]) ──
        self.lines = []
        self.cy = 0
        self.cx = 0
        self.top = 0
        self.left = 0
        self.filename = None
        self.modified = False
        self.undo_stack = []
        self.last_mtime = None

        # ── multi-buffer / panes ──
        self.buffers = []
        self.panes = []
        self.active_pane = 0

        # ── mode & state ──
        self.mode = 'normal'
        self.state = 'normal'
        self.running = True
        self.cmd_buf = ''
        self.flash = None
        self.needs_clear = True  # first frame should clear the terminal

        # ── theme ──
        theme_name = cfg.get('theme', 'amber').lower()
        self.theme_name = theme_name if theme_name in THEMES else 'amber'
        self.theme_selected = THEME_LIST.index(self.theme_name) if self.theme_name in THEME_LIST else 0

        # ── terminal size cache ──
        self.cache_w = 80
        self.cache_h = 24

        # ── finder ──
        self.finder = Finder()
        self.finder_query = ''
        self.finder_selection = 0

        # ── file tree (Ctrl+F) ──
        self.filetree = FileTree()
        self.filetree_width = max(10, cfg.get('filetree_width', 30))

        # ── layout ──
        sbt = cfg.get('status_bar_top', False)
        if isinstance(sbt, str):
            sbt = sbt.lower() in ('true', '1', 'yes')
        self.status_bar_top = bool(sbt)

        # ── run output (Ctrl+R) ──
        self.run_output = ''

        # ── colour effects (:3) ──
        self.fx_mode = 0
        self.fx_start = 0.0

        # ── pop-up shell (Ctrl+J) ──
        self.shell = None

        # ── image viewer ──
        self.image_path = None

        # ── system dashboard (:sys) ──
        self.sys_info = SysInfo()

        # ── music player ──
        self.music_player = MusicPlayer()
        music_dir = cfg.get('music_dir', '')
        if music_dir and os.path.isdir(music_dir):
            self.music_dir = music_dir
        else:
            default_music = os.path.expanduser('~/Music')
            self.music_dir = default_music if os.path.isdir(default_music) else os.getcwd()

        # ── search ──
        self.search_query = ''
        self.search_results = []  # list of (line, col)
        self.search_idx = 0

        # ── visual mode & clipboard ──
        self.visual_start = None
        self.clipboard = []

        # ── initial buffer ──
        if filename:
            buf = self._buffer_from_file(filename)
            if buf is None:
                buf = Buffer([''])
        else:
            buf = Buffer([''])
        self.buffers.append(buf)
        self.panes.append(Pane(0))
        self._sync_from_current()

    def theme(self):
        return theme_get(self.theme_name)

    # ── multi-buffer / pane sync ──

    def _current_buffer(self):
        return self.buffers[self.panes[self.active_pane].buffer_idx]

    def _sync_to_current(self):
        pane = self.panes[self.active_pane]
        buf = self._current_buffer()
        pane.cy = self.cy
        pane.cx = self.cx
        pane.top = self.top
        pane.left = self.left
        buf.lines = list(self.lines)
        buf.filename = self.filename
        buf.modified = self.modified
        buf.undo_stack = list(self.undo_stack)
        buf.last_mtime = self.last_mtime

    def _sync_from_current(self):
        pane = self.panes[self.active_pane]
        buf = self._current_buffer()
        self.lines = list(buf.lines)
        self.cy = pane.cy
        self.cx = pane.cx
        self.top = pane.top
        self.left = pane.left
        self.filename = buf.filename
        self.modified = buf.modified
        self.undo_stack = list(buf.undo_stack)
        self.last_mtime = buf.last_mtime

    def _switch_pane(self, idx):
        if idx >= len(self.panes) or idx == self.active_pane:
            return
        self._sync_to_current()
        self.active_pane = idx
        self._sync_from_current()
        self.needs_clear = True

    def _switch_buffer(self, idx):
        if idx >= len(self.buffers):
            return
        buf = self._current_buffer()
        if self.panes[self.active_pane].buffer_idx == idx:
            return
        self._sync_to_current()
        self.panes[self.active_pane].buffer_idx = idx
        self._sync_from_current()
        self.needs_clear = True

    def _close_pane(self):
        pane = self.panes[self.active_pane]
        buf_idx = pane.buffer_idx
        del self.panes[self.active_pane]
        if not self.panes and self.buffers:
            # Last pane closed but buffers remain — create pane for first buffer
            self.panes.append(Pane(0))
            self.active_pane = 0
            self._sync_from_current()
            self.needs_clear = True
            return
        if not self.panes:
            return  # editor exits
        self.active_pane = min(self.active_pane, len(self.panes) - 1)
        # If no other pane references this buffer, remove it
        still_used = any(p.buffer_idx == buf_idx for p in self.panes)
        if not still_used:
            del self.buffers[buf_idx]
            for p in self.panes:
                if p.buffer_idx > buf_idx:
                    p.buffer_idx -= 1
        self._sync_from_current()
        self.needs_clear = True

    def _close_buffer(self):
        """Close the active pane's buffer.  If no panes remain, create a
        fresh pane for the next buffer, or exit if no buffers left."""
        buf_idx = self.panes[self.active_pane].buffer_idx
        # Remove all panes referencing this buffer
        i = 0
        while i < len(self.panes):
            if self.panes[i].buffer_idx == buf_idx:
                del self.panes[i]
                if i <= self.active_pane:
                    self.active_pane = max(0, self.active_pane - 1)
            else:
                i += 1
        del self.buffers[buf_idx]
        for p in self.panes:
            if p.buffer_idx > buf_idx:
                p.buffer_idx -= 1
        # If no panes left but buffers remain, create a pane for the first buffer
        if not self.panes and self.buffers:
            self.panes.append(Pane(0))
            self.active_pane = 0
        if not self.panes:
            return  # editor exits
        self.active_pane = min(self.active_pane, len(self.panes) - 1)
        self._sync_from_current()
        self.needs_clear = True

    # ── file operations ──

    @staticmethod
    def _buffer_from_file(path):
        try:
            with open(path, 'r') as f:
                data = f.read()
        except (FileNotFoundError, UnicodeDecodeError, OSError):
            return None
        lines = data.split('\n')
        if data.endswith('\n'):
            lines.pop()
        if not lines:
            lines = ['']
        buf = Buffer(lines, path)
        try:
            buf.last_mtime = os.path.getmtime(path)
        except OSError:
            pass
        return buf

    def open_file(self, path):
        """Open a file (text or image). Returns the state to switch to,
        or None if the file couldn't be opened."""
        _, ext = os.path.splitext(path)
        if ext.lower() in IMAGE_EXTS:
            self.image_path = path
            self.needs_clear = True
            return 'image'
        buf = self._buffer_from_file(path)
        if buf is None:
            return None
        self._sync_to_current()
        self.buffers.append(buf)
        self.panes[self.active_pane].buffer_idx = len(self.buffers) - 1
        self._sync_from_current()
        return 'normal'

    def save(self):
        if not self.filename:
            return
        with open(self.filename, 'w') as f:
            for line in self.lines:
                f.write(line + '\n')
        self.modified = False
        try:
            self.last_mtime = os.path.getmtime(self.filename)
        except OSError:
            pass

    def auto_reload(self):
        """Reload the current file if it changed on disk (only when clean)."""
        if not self.filename or self.modified:
            return
        try:
            cur = os.path.getmtime(self.filename)
        except OSError:
            return
        if self.last_mtime is None:
            self.last_mtime = cur
            return
        if cur == self.last_mtime:
            return
        # File changed externally — reload
        try:
            with open(self.filename, 'r') as f:
                data = f.read()
        except (OSError, UnicodeDecodeError):
            return
        lines = data.split('\n')
        if data.endswith('\n'):
            lines.pop()
        if not lines:
            lines = ['']
        self.lines = lines
        self.cy = min(self.cy, max(0, len(self.lines) - 1))
        if self.lines:
            self.cx = min(self.cx, len(self.lines[self.cy]))
        self.top = min(self.top, max(0, len(self.lines) - 1))
        self.last_mtime = cur

    def is_splash(self):
        buf = self._current_buffer() if self.panes else None
        if buf and buf.shell:
            return False
        return self.filename is None and len(self.lines) == 1 and self.lines[0] == ''

    # ── scrolling ──

    def gutter_width(self):
        n = len(self.lines)
        w = 1
        while n >= 10:
            n //= 10; w += 1
        return w

    def scroll_clamp(self):
        # content area height: subtract status bar + buffer bar
        if self.status_bar_top:
            h = max(1, self.cache_h - 1)  # status bar at top, no buffer bar
        else:
            h = max(1, self.cache_h - 2)  # buffer bar + status bar at bottom
        ft_w = self.filetree_width if self.state == 'filetree' else 0
        if self.cy < 0: self.cy = 0
        if self.cy >= len(self.lines):
            self.cy = max(0, len(self.lines) - 1)
        if self.cy < self.top: self.top = self.cy
        if self.cy >= self.top + h: self.top = self.cy - h + 1
        if self.top < 0: self.top = 0

        if self.lines:
            ll = len(self.lines[self.cy])
            if self.cx < 0: self.cx = 0
            if self.cx > ll: self.cx = ll
        else:
            self.cx = 0

        gutter = self.gutter_width() + 1
        content_w = self.cache_w - ft_w
        visible_cols = content_w - gutter
        if visible_cols < 1:
            visible_cols = 1
        if self.cx < self.left: self.left = self.cx
        if self.cx >= self.left + visible_cols:
            self.left = self.cx - visible_cols + 1
        if self.left < 0: self.left = 0

    # ── undo ──

    def save_undo(self):
        self.undo_stack.append([l for l in self.lines])
        if len(self.undo_stack) > 50:
            self.undo_stack.pop(0)

    def restore_undo(self):
        if not self.undo_stack:
            return False
        self.lines = [l for l in self.undo_stack.pop()]
        if self.cy >= len(self.lines):
            self.cy = max(0, len(self.lines) - 1)
        if self.lines and self.cx > len(self.lines[self.cy]):
            self.cx = len(self.lines[self.cy])
        self.modified = True
        return True

    # ── visual mode ──

    def handle_visual(self, key):
        if key == KEY_ESC or key == ord('v'):
            self.mode = 'normal'
            self.visual_start = None
        elif key == ord(':'):
            self.mode = 'normal'
            self.visual_start = None
            self.state = 'command'
            self.cmd_buf = ''
        elif key in (ord('h'), KEY_LEFT):
            if self.cx > 0: self.cx -= 1
        elif key in (ord('j'), KEY_DOWN):
            if self.cy + 1 < len(self.lines):
                self.cy += 1
                if self.cx > len(self.lines[self.cy]):
                    self.cx = len(self.lines[self.cy])
        elif key in (ord('k'), KEY_UP):
            if self.cy > 0:
                self.cy -= 1
                if self.cx > len(self.lines[self.cy]):
                    self.cx = len(self.lines[self.cy])
        elif key in (ord('l'), KEY_RIGHT):
            if self.lines and self.cx < len(self.lines[self.cy]):
                self.cx += 1
        elif key in (ord('x'), ord('d')):
            self.save_undo()
            self.yank_selection()
            self.delete_selection()
            self.mode = 'normal'
            self.visual_start = None
            self.modified = True
        elif key == ord('y'):
            self.yank_selection()
            self.mode = 'normal'
            self.visual_start = None
        elif key in (ord('0'), KEY_HOME):
            self.cx = 0
        elif key in (ord('$'), KEY_END):
            if self.lines: self.cx = len(self.lines[self.cy])
        elif key == KEY_PGUP:
            self.cy -= self.cache_h - 1
        elif key == KEY_PGDN:
            self.cy += self.cache_h - 1
        elif key in (KEY_CTRL('c'), KEY_CTRL('q')):
            self.running = False
            return False
        self.scroll_clamp()
        return True

    def visual_bounds(self):
        """Returns (y1, x1, y2, x2) with y1 <= y2 and if equal, x1 <= x2."""
        if self.visual_start is None:
            return (0, 0, 0, 0)
        sl, sc = self.visual_start
        el, ec = self.cy, self.cx
        if sl > el or (sl == el and sc > ec):
            return (el, ec, sl, sc)
        return (sl, sc, el, ec)

    def in_selection(self, line, col):
        if self.mode != 'visual':
            return False
        sl, sc, el, ec = self.visual_bounds()
        if line < sl or line > el:
            return False
        if line == sl and line == el:
            return sc <= col <= ec
        if line == sl:
            return col >= sc
        if line == el:
            return col <= ec
        return True

    def yank_selection(self):
        sl, sc, el, ec = self.visual_bounds()
        if sl == el:
            self.clipboard = [self.lines[sl][sc:ec + 1]]
        else:
            lines = [self.lines[sl][sc:]]
            for i in range(sl + 1, el):
                lines.append(self.lines[i])
            lines.append(self.lines[el][:ec + 1])
            self.clipboard = lines

    def delete_selection(self):
        sl, sc, el, ec = self.visual_bounds()
        if sl == el:
            self.lines[sl] = self.lines[sl][:sc] + self.lines[sl][ec + 1:]
        else:
            self.lines[sl] = self.lines[sl][:sc] + self.lines[el][ec + 1:]
            del self.lines[sl + 1:el + 1]
        self.cy = sl
        self.cx = sc

    # ── key handling ──

    def handle_key(self, key):
        prev_state = self.state
        prev_pane = self.active_pane
        prev_buf = self.panes[self.active_pane].buffer_idx if self.panes else 0

        # Global keybindings (work from any state)
        if key == KEY_CTRL('t'):
            self.state = 'music'
            self.music_player.selected = 0
            self.music_player.scan(self.music_dir)
            self.needs_clear = True
            return True
        if key == KEY_CTRL('e'):
            self.state = 'theme'
            self.theme_selected = THEME_LIST.index(self.theme_name) if self.theme_name in THEME_LIST else 0
            self.needs_clear = True
            return True
        if key == KEY_CTRL('p') and self.state != 'finder':
            self.state = 'finder'
            self.finder_query = ''
            self.finder_selection = 0
            self.finder.collect_files()
            self.finder.search('')
            self.needs_clear = True
            return True
        if key == KEY_CTRL('f'):
            if self.state == 'filetree':
                self.state = 'normal'
            else:
                self.filetree.refresh()
                self.state = 'filetree'
            self.needs_clear = True
            return True
        if key == KEY_CTRL('r'):
            if self.filename:
                self.save()
                self.run_output = self._run_python(self.filename)
            else:
                self.run_output = 'No filename.  Save the buffer first (:w <file>).'
            self.state = 'run'
            self.needs_clear = True
            return True
        if key == KEY_CTRL('j'):
            self._open_shell_pane()
            return True
        if key == KEY_CTRL('w'):
            self.state = 'ctrlw'
            return True

        # Clear flash on next keypress
        self.flash = None

        result = True
        if self.state == 'finder':
            result = self.handle_finder(key)
        elif self.state == 'music':
            result = self.handle_music(key)
        elif self.state == 'theme':
            result = self.handle_theme(key)
        elif self.state == 'command':
            result = self.handle_command(key)
        elif self.state == 'search':
            result = self.handle_search(key)
        elif self.state == 'filetree':
            result = self.handle_filetree(key)
        elif self.state == 'run':
            result = self.handle_run(key)
        elif self.state == 'shell':
            result = self.handle_shell(key)
        elif self.state == 'image':
            result = self.handle_image(key)
        elif self.state == 'ctrlw':
            result = self.handle_ctrlw(key)
        elif self.state == 'sysinfo':
            result = self.handle_sysinfo(key)
        elif self.mode == 'insert':
            result = self.handle_insert(key)
        elif self.mode == 'visual':
            result = self.handle_visual(key)
        else:
            # If the active pane has a shell buffer, forward keys to it
            if self.panes and self._current_buffer().shell is not None:
                sh = self._current_buffer().shell
                if sh.is_alive():
                    sh.write(self._key_to_bytes(key))
                    return True
            result = self.handle_normal(key)

        if self.state != prev_state or self.active_pane != prev_pane:
            self.needs_clear = True
        elif self.panes and self.panes[self.active_pane].buffer_idx != prev_buf:
            self.needs_clear = True
        return result

    def handle_command(self, key):
        if key == KEY_ESC:
            self.state = 'normal'
            self.cmd_buf = ''
        elif key == KEY_ENTER:
            self.exec_cmd(self.cmd_buf)
            self.cmd_buf = ''
            if self.state == 'command':
                self.state = 'normal'
        elif key == KEY_BACKSP:
            self.cmd_buf = self.cmd_buf[:-1]
        elif 32 <= key < 128:
            self.cmd_buf += chr(key)
        return True

    # ── search ──

    def handle_search(self, key):
        if key == KEY_ESC:
            self.state = 'normal'
            self.cmd_buf = ''
        elif key == KEY_ENTER:
            self.search_query = self.cmd_buf
            self.cmd_buf = ''
            self.perform_search()
            self.state = 'normal'
        elif key == KEY_BACKSP:
            self.cmd_buf = self.cmd_buf[:-1]
        elif 32 <= key < 128:
            self.cmd_buf += chr(key)
        return True

    # ── theme selector ──

    def handle_theme(self, key):
        if key == KEY_ESC:
            self.state = 'normal'
        elif key == KEY_ENTER:
            self.theme_name = THEME_LIST[self.theme_selected]
            self.state = 'normal'
        elif key in (ord('k'), KEY_UP):
            self.theme_selected = max(0, self.theme_selected - 1)
        elif key in (ord('j'), KEY_DOWN):
            self.theme_selected = min(len(THEME_LIST) - 1, self.theme_selected + 1)
        elif key in (KEY_CTRL('c'), KEY_CTRL('q')):
            self.running = False
            return False
        return True

    def handle_filetree(self, key):
        if key in (KEY_ESC, KEY_CTRL('f')):
            self.state = 'normal'
        elif key == KEY_ENTER:
            entry = self.filetree.selected_entry()
            if entry:
                if entry.is_dir:
                    self.filetree.toggle_expand()
                else:
                    new_state = self.open_file(entry.path)
                    if new_state is not None:
                        self.state = new_state
        elif key in (ord('k'), KEY_UP):
            self.filetree.selected = max(0, self.filetree.selected - 1)
        elif key in (ord('j'), KEY_DOWN):
            mx = len(self.filetree.entries) - 1
            self.filetree.selected = min(mx, self.filetree.selected + 1)
        elif key == KEY_PGUP:
            self.filetree.selected = max(0, self.filetree.selected - 10)
        elif key == KEY_PGDN:
            mx = len(self.filetree.entries) - 1
            self.filetree.selected = min(mx, self.filetree.selected + 10)
        elif key in (ord('h'), KEY_LEFT):
            entry = self.filetree.selected_entry()
            if entry and entry.is_dir and entry.expanded:
                self.filetree.toggle_expand()
        elif key in (ord('l'), KEY_RIGHT):
            entry = self.filetree.selected_entry()
            if entry and entry.is_dir and not entry.expanded:
                self.filetree.toggle_expand()
        elif key in (KEY_CTRL('c'), KEY_CTRL('q')):
            self.running = False
            return False
        return True

    @staticmethod
    def _run_python(path):
        try:
            out = subprocess.run(
                ['python3', path],
                capture_output=True, text=True, timeout=30
            )
            result = out.stdout
            if out.stderr:
                if result:
                    result += '\n'
                result += out.stderr
            return result if result else '(no output)'
        except subprocess.TimeoutExpired:
            return '(timed out after 30 s)'
        except FileNotFoundError:
            return 'python3 not found in PATH'
        except Exception as e:
            return f'Error: {e}'

    def handle_run(self, key):
        self.state = 'normal'
        return True

    def handle_image(self, key):
        self.image_path = None
        self.state = 'normal'
        return True

    def handle_sysinfo(self, key):
        self.state = 'normal'
        return True

    def handle_ctrlw(self, key):
        self.state = 'normal'
        if key in (ord('h'), KEY_LEFT):
            self._switch_pane(max(0, self.active_pane - 1))
        elif key in (ord('l'), KEY_RIGHT):
            self._switch_pane(min(len(self.panes) - 1, self.active_pane + 1))
        elif key == ord('q'):
            self._close_pane()
            if not self.panes:
                self.running = False
                return False
        elif key == ord('v'):
            self._sync_to_current()
            buf_idx = self.panes[self.active_pane].buffer_idx
            new_pane = Pane(buf_idx)
            self.panes.insert(self.active_pane + 1, new_pane)
            self.active_pane += 1
            self._sync_from_current()
            self.needs_clear = True
        elif key == ord('='):
            # Collapse to single pane
            buf_idx = self.panes[self.active_pane].buffer_idx
            self.panes = [Pane(buf_idx)]
            self.active_pane = 0
            self._sync_from_current()
            self.needs_clear = True
        return True

    def _key_to_bytes(self, key):
        """Convert a pked keycode to bytes for the shell PTY."""
        if key == KEY_ENTER:    return b'\r'
        if key == KEY_BACKSP:   return b'\x7f'
        if key == KEY_TAB:      return b'\t'
        if key == KEY_ESC:      return b'\x1b'
        if key == KEY_UP:       return b'\x1b[A'
        if key == KEY_DOWN:     return b'\x1b[B'
        if key == KEY_RIGHT:    return b'\x1b[C'
        if key == KEY_LEFT:     return b'\x1b[D'
        if key == KEY_HOME:     return b'\x1b[H'
        if key == KEY_END:      return b'\x1b[F'
        if key == KEY_DEL:      return b'\x1b[3~'
        if key == KEY_PGUP:     return b'\x1b[5~'
        if key == KEY_PGDN:     return b'\x1b[6~'
        if 0x3001 <= key <= 0x301a:  # Ctrl+A..Ctrl+Z
            return bytes([key - 0x3000])
        if 32 <= key < 127:     return bytes([key])
        return b''

    def handle_shell(self, key):
        if self.shell is None or not self.shell.is_alive():
            self.shell = None
            self.state = 'normal'
            return True
        self.shell.write(self._key_to_bytes(key))
        return True

    def handle_finder(self, key):
        if key in (KEY_ESC, KEY_CTRL('p')):
            self.state = 'normal'
        elif key == KEY_ENTER:
            if self.finder_selection < len(self.finder.results):
                path = self.finder.results[self.finder_selection][0]
                full_path = os.path.join(os.getcwd(), path)
                new_state = self.open_file(full_path)
                self.state = new_state if new_state is not None else 'normal'
        elif key in (ord('k'), KEY_UP):
            self.finder_selection = max(0, self.finder_selection - 1)
        elif key in (ord('j'), KEY_DOWN):
            max_idx = len(self.finder.results) - 1
            self.finder_selection = min(max_idx, self.finder_selection + 1)
        elif key == KEY_PGUP:
            self.finder_selection = max(0, self.finder_selection - 10)
        elif key == KEY_PGDN:
            max_idx = len(self.finder.results) - 1
            self.finder_selection = min(max_idx, self.finder_selection + 10)
        elif key == KEY_BACKSP:
            self.finder_query = self.finder_query[:-1]
            self.finder.search(self.finder_query)
            self.finder_selection = 0
        elif 32 <= key < 128:
            self.finder_query += chr(key)
            self.finder.search(self.finder_query)
            self.finder_selection = 0
        elif key in (KEY_CTRL('c'), KEY_CTRL('q')):
            self.running = False
            return False
        return True

    def handle_music(self, key):
        if key == KEY_ESC:
            self.state = 'normal'
        elif key == KEY_ENTER:
            self.music_player.play(self.music_player.selected)
        elif key in (ord('s'), KEY_BACKSP):
            self.music_player.stop()
        elif key in (ord('k'), KEY_UP):
            self.music_player.selected = max(0, self.music_player.selected - 1)
        elif key in (ord('j'), KEY_DOWN):
            max_idx = len(self.music_player.files) - 1
            self.music_player.selected = min(max_idx, self.music_player.selected + 1)
        elif key == KEY_PGUP:
            self.music_player.selected = max(0, self.music_player.selected - 10)
        elif key == KEY_PGDN:
            max_idx = len(self.music_player.files) - 1
            self.music_player.selected = min(max_idx, self.music_player.selected + 10)
        elif key in (KEY_CTRL('c'), KEY_CTRL('q')):
            self.running = False
            return False
        return True

    def perform_search(self):
        self.search_results = []
        q = self.search_query
        if not q:
            return
        for li, line in enumerate(self.lines):
            start = 0
            while True:
                ci = line.find(q, start)
                if ci == -1:
                    break
                self.search_results.append((li, ci))
                start = ci + 1

        if not self.search_results:
            self.search_idx = 0
            return

        # Find first match at or after cursor
        self.search_idx = 0
        for i, (li, ci) in enumerate(self.search_results):
            if li > self.cy or (li == self.cy and ci >= self.cx):
                self.search_idx = i
                self.go_to_match()
                return
        self.go_to_match()

    def go_to_match(self):
        if not self.search_results:
            return
        self.search_idx %= len(self.search_results)
        li, ci = self.search_results[self.search_idx]
        self.cy = li
        self.cx = ci
        self.top = max(0, li - (self.cache_h - 1) // 2)

    def next_match(self):
        if not self.search_results:
            return
        self.search_idx = (self.search_idx + 1) % len(self.search_results)
        self.go_to_match()

    def prev_match(self):
        if not self.search_results:
            return
        self.search_idx = (self.search_idx - 1) % len(self.search_results)
        self.go_to_match()

    def handle_insert(self, key):
        if key == KEY_ESC:
            self.mode = 'normal'
            if self.lines and self.cx > 0 and self.cx > len(self.lines[self.cy]):
                self.cx = len(self.lines[self.cy])
        elif key == KEY_ENTER:
            self.save_undo()
            right = self.lines[self.cy][self.cx:]
            self.lines[self.cy] = self.lines[self.cy][:self.cx]
            indent = re.match(r'^[ \t]*', self.lines[self.cy]).group()
            self.lines.insert(self.cy + 1, indent)
            self.cy += 1
            self.cx = len(indent)
            self.modified = True
        elif key == KEY_TAB:
            self.save_undo()
            self.lines[self.cy] = self.lines[self.cy][:self.cx] + '    ' + self.lines[self.cy][self.cx:]
            self.cx += 4
            self.modified = True
        elif key == KEY_CTRL('a'):
            if self.clipboard:
                self.save_undo()
                text = ''.join(self.clipboard) if len(self.clipboard) == 1 else '\n'.join(self.clipboard)
                self.lines[self.cy] = self.lines[self.cy][:self.cx] + text + self.lines[self.cy][self.cx:]
                self.cx += len(text)
                self.modified = True
        elif key == KEY_BACKSP:
            self.save_undo()
            if self.cx > 0:
                self.cx -= 1
                self.lines[self.cy] = self.lines[self.cy][:self.cx] + self.lines[self.cy][self.cx + 1:]
                self.modified = True
            elif self.cy > 0:
                prev_len = len(self.lines[self.cy - 1])
                self.lines[self.cy - 1] += self.lines[self.cy]
                del self.lines[self.cy]
                self.cy -= 1
                self.cx = prev_len
                self.modified = True
        elif key == KEY_DEL:
            self.save_undo()
            if self.lines:
                if self.cx < len(self.lines[self.cy]):
                    self.lines[self.cy] = self.lines[self.cy][:self.cx] + self.lines[self.cy][self.cx + 1:]
                    self.modified = True
                elif self.cy + 1 < len(self.lines):
                    self.lines[self.cy] += self.lines[self.cy + 1]
                    del self.lines[self.cy + 1]
                    self.modified = True
        elif key == KEY_UP:
            if self.cy > 0:
                self.cy -= 1
                if self.cx > len(self.lines[self.cy]):
                    self.cx = len(self.lines[self.cy])
        elif key == KEY_DOWN:
            if self.cy + 1 < len(self.lines):
                self.cy += 1
                if self.cx > len(self.lines[self.cy]):
                    self.cx = len(self.lines[self.cy])
        elif key == KEY_LEFT:
            if self.cx > 0: self.cx -= 1
        elif key == KEY_RIGHT:
            if self.lines and self.cx < len(self.lines[self.cy]):
                self.cx += 1
        elif key == KEY_HOME:
            self.cx = 0
        elif key == KEY_END:
            if self.lines: self.cx = len(self.lines[self.cy])
        elif 32 <= key < 128:
            self.save_undo()
            c = chr(key)
            self.lines[self.cy] = self.lines[self.cy][:self.cx] + c + self.lines[self.cy][self.cx:]
            self.cx += 1
            self.modified = True
        return True

    def handle_normal(self, key):
        if key in (ord('h'), KEY_LEFT):
            if self.cx > 0: self.cx -= 1
        elif key in (ord('j'), KEY_DOWN):
            if self.cy + 1 < len(self.lines):
                self.cy += 1
                if self.cx > len(self.lines[self.cy]):
                    self.cx = len(self.lines[self.cy])
        elif key in (ord('k'), KEY_UP):
            if self.cy > 0:
                self.cy -= 1
                if self.cx > len(self.lines[self.cy]):
                    self.cx = len(self.lines[self.cy])
        elif key in (ord('l'), KEY_RIGHT):
            if self.lines and self.cx < len(self.lines[self.cy]):
                self.cx += 1
        elif key in (ord('0'), KEY_HOME):
            self.cx = 0
        elif key in (ord('$'), KEY_END):
            if self.lines: self.cx = len(self.lines[self.cy])
        elif key == KEY_PGUP:
            self.cy -= self.cache_h - 1
        elif key == KEY_PGDN:
            self.cy += self.cache_h - 1
        elif key == ord('i'):
            self.mode = 'insert'
        elif key == ord('a'):
            self.mode = 'insert'
            if self.lines and self.cx < len(self.lines[self.cy]):
                self.cx += 1
        elif key == ord('I'):
            self.cx = 0
            self.mode = 'insert'
        elif key == ord('A'):
            if self.lines: self.cx = len(self.lines[self.cy])
            self.mode = 'insert'
        elif key == ord('o'):
            self.save_undo()
            self.lines.insert(self.cy + 1, '')
            self.cy += 1
            self.cx = 0
            self.mode = 'insert'
            self.modified = True
        elif key == ord('O'):
            self.save_undo()
            self.lines.insert(self.cy, '')
            self.cx = 0
            self.mode = 'insert'
            self.modified = True
        elif key == ord('x'):
            self.save_undo()
            if self.lines and self.cx < len(self.lines[self.cy]):
                self.lines[self.cy] = self.lines[self.cy][:self.cx] + self.lines[self.cy][self.cx + 1:]
                self.modified = True
        elif key == ord('u'):
            self.restore_undo()
        elif key == ord('/'):
            self.state = 'search'
            self.cmd_buf = ''
        elif key == ord('n'):
            self.next_match()
        elif key == ord('N'):
            self.prev_match()
        elif key == ord('v'):
            self.visual_start = (self.cy, self.cx)
            self.mode = 'visual'
        elif key == ord('p'):
            if self.clipboard:
                self.save_undo()
                if len(self.clipboard) == 1:
                    text = self.clipboard[0]
                    self.lines[self.cy] = self.lines[self.cy][:self.cx] + text + self.lines[self.cy][self.cx:]
                    self.cx += len(text)
                else:
                    right = self.lines[self.cy][self.cx:]
                    self.lines[self.cy] = self.lines[self.cy][:self.cx] + self.clipboard[0]
                    for line in self.clipboard[1:]:
                        self.lines.insert(self.cy + 1, line)
                    last_idx = self.cy + len(self.clipboard) - 1
                    self.lines[last_idx] += right
                    self.cy = last_idx
                    self.cx = len(self.clipboard[-1])
                self.modified = True
        elif key == ord('P'):
            if self.clipboard:
                self.save_undo()
                if len(self.clipboard) == 1:
                    text = self.clipboard[0]
                    self.lines[self.cy] = self.lines[self.cy][:self.cx] + text + self.lines[self.cy][self.cx:]
                else:
                    for line in reversed(self.clipboard):
                        self.lines.insert(self.cy, line)
                self.cx = 0
                self.modified = True
        elif key == ord(':'):
            self.state = 'command'
            self.cmd_buf = ''
        elif key == KEY_TAB:
            if len(self.buffers) > 1:
                buf_idx = self.panes[self.active_pane].buffer_idx
                nxt = (buf_idx + 1) % len(self.buffers)
                self._switch_buffer(nxt)
        elif key == KEY_BACKTAB:
            if len(self.buffers) > 1:
                buf_idx = self.panes[self.active_pane].buffer_idx
                prv = (buf_idx - 1) % len(self.buffers)
                self._switch_buffer(prv)
        elif key in (KEY_CTRL('c'), KEY_CTRL('q')):
            if self.modified:
                self.flash = "No write since last change (add ! to override)"
                return True
            self.running = False
            return False
        elif key == KEY_CTRL('s'):
            self.save()
        self.scroll_clamp()
        return True

    # ── commands ──

    def exec_cmd(self, cmd):
        if not cmd: return
        cmd = cmd.strip()
        if cmd == 'q':
            if self.modified:
                self.flash = "No write since last change (add ! to override)"
                return
            self._close_buffer()
            if not self.buffers:
                self.running = False
        elif cmd == 'q!':
            self._close_buffer()
            if not self.buffers:
                self.running = False
        elif cmd == 'w':
            self.save()
        elif cmd == 'wq':
            self.save()
            self._close_buffer()
            if not self.buffers:
                self.running = False
        elif cmd == 'wq!':
            self.save()
            self._close_buffer()
            if not self.buffers:
                self.running = False
        elif cmd.startswith('e '):
            path = cmd[2:].strip()
            if path:
                new_state = self.open_file(path)
                if new_state is None:
                    self.flash = f"can't open '{path}'"
                elif new_state != 'normal':
                    self.state = new_state
        elif cmd in ('bn', 'bnext'):
            if len(self.buffers) > 1:
                buf_idx = self.panes[self.active_pane].buffer_idx
                nxt = (buf_idx + 1) % len(self.buffers)
                self._switch_buffer(nxt)
            else:
                self.flash = "Only one buffer open"
        elif cmd in ('bp', 'bprev'):
            if len(self.buffers) > 1:
                buf_idx = self.panes[self.active_pane].buffer_idx
                prv = (buf_idx - 1) % len(self.buffers)
                self._switch_buffer(prv)
            else:
                self.flash = "Only one buffer open"
        elif cmd == '3':
            self.fx_mode = (self.fx_mode + 1) % len(FX_NAMES)
            self.fx_start = time.time()
            self.flash = f'fx: {FX_NAMES[self.fx_mode]}'
        elif cmd == 'vsp':
            self._sync_to_current()
            buf_idx = self.panes[self.active_pane].buffer_idx
            new_pane = Pane(buf_idx)
            self.panes.insert(self.active_pane + 1, new_pane)
            self.active_pane += 1
            self._sync_from_current()
            self.needs_clear = True
        elif cmd == 'sys':
            self.sys_info._last_fetch = 0  # force refresh
            self.state = 'sysinfo'
            self.needs_clear = True
        elif cmd.startswith('theme'):
            parts = cmd.split()
            if len(parts) > 1:
                name = parts[1].lower()
                if name in THEMES:
                    self.theme_name = name
                else:
                    names = ', '.join(THEME_LIST)
                    self.flash = f"unknown theme '{name}'. themes: {names}"

    # ── system dashboard (:sys) ──

    def render_sysinfo(self, buf, w, h):
        d = self.sys_info.fetch()
        t = self.theme()
        rst = '\033[0m' + fg(t.fg) + bg(t.bg)
        buf.append('\033[0m' + bg(t.bg) + '\033[2J')

        # ── header bar ──
        self.go(buf, 0, 0)
        buf.append(bg(t.status_bg) + fg(t.status_fg) + '\033[1m')
        buf.append(f"  pked system  ─  {d.get('host','?')}  ─  {d.get('user','?')}  (any key to close)")
        buf.append('\033[K' + rst)

        # ── gauges panel ──
        panel_x, panel_w = 3, min(w - 6, 70)
        y = 2
        cpu = d.get('cpu', 0)
        y = self._draw_sys_gauge(buf, y, panel_x, panel_w, 'CPU', cpu, rst, t)
        y += 1
        used, total = d.get('mem', (0, 1))
        mem_pct = (used / max(total, 1)) * 100
        mem_str = f'{self._fmt_bytes(used)} / {self._fmt_bytes(total)}'
        y = self._draw_sys_gauge(buf, y, panel_x, panel_w, 'MEM', mem_pct, rst, t, extra=mem_str)
        y += 1
        # Disk gauge
        size, used_d, avail, pct_str = d.get('disk', ('?', '?', '?', '?'))
        try:
            disk_pct = float(pct_str.replace('%', ''))
        except ValueError:
            disk_pct = 0
        disk_str = f'{used_d} used of {size}  ─  {avail} free'
        y = self._draw_sys_gauge(buf, y, panel_x, panel_w, 'DSK', disk_pct, rst, t, extra=disk_str)
        y += 1
        # Battery gauge
        bpct, bstat = d.get('batt', (None, None))
        if bpct is not None:
            y = self._draw_sys_gauge(buf, y, panel_x, panel_w, 'BATT', bpct, rst, t, extra=bstat or '')
        else:
            self.go(buf, panel_x, y)
            buf.append(rst + '  BATT   no battery (desktop)')
            buf.append('\033[K')
            y += 3
        y += 1

        # ── info line ──
        self.go(buf, panel_x, y)
        uptime = d.get('uptime', '?')
        load = d.get('load')
        load_str = f'  load: {load[0]:.2f} {load[1]:.2f} {load[2]:.2f}' if load else ''
        buf.append(rst + f'  up {uptime}{load_str}')
        buf.append('\033[K')
        buf.append(rst)

    @staticmethod
    def _draw_sys_gauge(buf, y, x, w, label, pct, rst, t, extra=''):
        pct = max(0, min(100, pct))
        bar_w = w - 30
        filled = max(0, int(bar_w * pct / 100))
        # Theme colours for the bar
        bar_fg = fg(t.kw) if pct < 80 else fg(t.builtin)
        empty_fg = fg(t.tilde)
        bar = bar_fg + '█' * filled + empty_fg + '░' * (bar_w - filled) + rst
        pct_str = f'{pct:5.1f}%'
        buf.append(f'\033[{y+1};{x+1}H{rst}  {label.ljust(4)} {bar} {fg(t.fg)}{pct_str}')
        buf.append('\033[K')
        if extra:
            buf.append(f'\033[{y+2};{x+1}H{rst}  {" " * 5} {fg(t.tilde)}{extra}')
            buf.append('\033[K')
        else:
            buf.append(f'\033[{y+2};{x+1}H{rst}')
            buf.append('\033[K')
        return y + 2

    @staticmethod
    def _fmt_bytes(n):
        if n >= 1 << 30:
            return f'{n / (1 << 30):.1f} GB'
        if n >= 1 << 20:
            return f'{n / (1 << 20):.1f} MB'
        return f'{n / (1 << 10):.1f} KB'

    # ── rendering ──

    def _render_pane_shell(self, buf, t, x, y, pw, ph):
        """Render shell output inside a pane rectangle."""
        b = self._current_buffer()
        if b.shell is None:
            return
        # Only resize when dimensions actually change
        if not hasattr(b.shell, '_last_rows') or b.shell._last_rows != ph or b.shell._last_cols != pw:
            b.shell.resize(ph, pw)
            b.shell._last_rows = ph
            b.shell._last_cols = pw
        lines = b.shell.styled_lines(t.fg, t.bg)
        visible = lines[-ph:] if len(lines) > ph else lines
        for li, line in enumerate(visible):
            self.go(buf, x, y + li)
            col = 0
            for text, fgc, bold in line:
                if fgc:
                    buf.append(fg(fgc))
                else:
                    buf.append(fg(t.fg))
                buf.append(bg(t.bg))
                if bold:
                    buf.append('\033[1m')
                for ch in text:
                    if ch.isprintable() or ch == ' ':
                        buf.append(ch)
                        col += 1
                    if col >= pw:
                        break
                buf.append('\033[0m' + fg(t.fg) + bg(t.bg))
                if col >= pw:
                    break
            if col < pw:
                buf.append(' ' * (pw - col))

    def _open_shell_pane(self):
        """Open a new pane with a shell buffer."""
        self._sync_to_current()
        shell_buf = Buffer([''], filename=None)
        shell_buf.shell = ShellProcess().spawn()
        self.buffers.append(shell_buf)
        buf_idx = len(self.buffers) - 1
        new_pane = Pane(buf_idx)
        self.panes.insert(self.active_pane + 1, new_pane)
        self.active_pane += 1
        self._sync_from_current()
        self.needs_clear = True

    def _pane_rects(self, w, h):
        """Return list of (x, y, pw, ph) for each pane in the content area."""
        n = len(self.panes)
        if n == 0:
            return []
        content_y = 1
        content_h = max(1, h - (1 if self.status_bar_top else 2))
        if n == 1:
            return [(0, content_y, w, content_h)]
        # Vertical split: divide width equally
        divider_w = 1
        pw = (w - (n - 1) * divider_w) // n
        rects = []
        x = 0
        for i in range(n):
            if i == n - 1:
                pw = w - x  # last pane gets remainder
            rects.append((x, content_y, pw, content_h))
            x += pw + divider_w
        return rects

    def _render_pane_dividers(self, buf, t, w, h):
        rects = self._pane_rects(w, h)
        for i in range(len(rects) - 1):
            x = rects[i][0] + rects[i][2]  # right edge of pane i
            for row in range(rects[i][1], rects[i][1] + rects[i][3]):
                self.go(buf, x, row)
                buf.append(fg(t.tilde) + bg(t.bg) + '│')

    def go(self, buf, x, y):
        buf.append(f'\033[{y+1};{x+1}H')

    def render(self):
        buf = []
        t = self.theme()
        if self.fx_mode:
            elapsed = time.time() - self.fx_start
            if self.fx_mode == 1:
                t = rotate_theme(t, (elapsed * 40.0) % 360.0)
            elif self.fx_mode == 2:
                t = _fx_pulse(t, elapsed)
            elif self.fx_mode == 3:
                t = _fx_vaporwave(t, elapsed)
            elif self.fx_mode == 4:
                t = _fx_glitch(t, elapsed)
        w, h = self.cache_w, self.cache_h

        self.scroll_clamp()

        if self.needs_clear:
            # Per-row clear instead of \033[2J — tmux handles this
            # incrementally without triggering a full-pane redraw.
            for row in range(h):
                self.go(buf, 0, row)
                buf.append('\033[0m' + bg(t.bg) + '\033[K')
            self.needs_clear = False

        if self.state == 'image':
            self.render_image(buf, w, h)
            buf.append('\033[?25h')
            sys.stdout.write(''.join(buf))
            sys.stdout.flush()
            return
        if self.state == 'sysinfo':
            self.render_sysinfo(buf, w, h)
            buf.append('\033[?25h')
            sys.stdout.write(''.join(buf))
            sys.stdout.flush()
            return

        if self.status_bar_top:
            self.render_status(buf, t, w, h)  # status at row 0
        else:
            self.render_buffer_bar(buf, t, w)
        if self.state == 'filetree':
            self.render_filetree(buf, t, w, h)
        ft_w = self.filetree_width if self.state == 'filetree' else 0
        if self.is_splash():
            self.render_splash(buf, t, w, h, ft_w)
        else:
            rects = self._pane_rects(w - ft_w, h)
            if rects:
                if len(rects) > 1:
                    self._render_pane_dividers(buf, t, w, h)
                saved_pane = self.active_pane
                saved_cy, saved_cx = self.cy, self.cx
                saved_top, saved_left = self.top, self.left
                for pi in range(len(self.panes)):
                    if pi != self.active_pane:
                        self._sync_to_current()
                        self.active_pane = pi
                        self._sync_from_current()
                    x, y, pw, ph = rects[pi]
                    if self._current_buffer().shell:
                        self._render_pane_shell(buf, t, x + ft_w, y, pw, ph)
                    else:
                        self.render_content(buf, t, pw, h, x_offset=x + ft_w)
                if self.active_pane != saved_pane:
                    self._sync_to_current()
                    self.active_pane = saved_pane
                    self._sync_from_current()
                self.cy, self.cx = saved_cy, saved_cx
                self.top, self.left = saved_top, saved_left
        if not self.status_bar_top:
            self.render_status(buf, t, w, h)

        # Cursor — position in the active pane
        if not self.is_splash():
            gutter = self.gutter_width() + 1
            ft_w = self.filetree_width if self.state == 'filetree' else 0
            rects = self._pane_rects(w - ft_w, h)
            px = rects[self.active_pane][0] + ft_w if rects else ft_w
            cy_s = self.cy - self.top + (1 if self.status_bar_top else 1)
            cx_s = self.cx - self.left + gutter + px
            top_bound = 1 if self.status_bar_top else 1
            bot_bound = h - 1 if self.status_bar_top else h - 1
            if top_bound <= cy_s < bot_bound:
                self.go(buf, cx_s, cy_s)

        if self.state == 'theme':
            self.render_theme(buf, t, w, h)
        if self.state == 'finder':
            self.render_finder(buf, t, w, h)
        if self.state == 'music':
            self.render_music(buf, t, w, h)
        if self.state == 'run':
            self.render_run(buf, t, w, h)
        if self.state == 'shell':
            self.render_shell(buf, t, w, h)
        if self.state == 'image':
            self.render_image(buf, w, h)

        buf.append('\033[?25h')
        sys.stdout.write(''.join(buf))
        sys.stdout.flush()

    def render_content(self, buf, t, pane_w, h, x_offset=0):
        text_h = max(1, h - (1 if self.status_bar_top else 2))  # header rows
        gutter = self.gutter_width() + 1
        max_col = max(1, pane_w - gutter)

        for row in range(text_h):
            buf_row = self.top + row
            self.go(buf, x_offset, row + 1)  # +1 for buffer bar
            buf.append(bg(t.bg))

            if buf_row >= len(self.lines):
                buf.append(' ' * (gutter + 1) + fg(t.tilde) + '~')
                if pane_w > gutter + 2:
                    buf.append(' ' * (pane_w - gutter - 2))
                continue

            # Gutter
            num = f'{buf_row + 1:>{gutter - 1}}│'
            buf.append(fg(t.gutter_fg) + num)

            # Line content
            line = self.lines[buf_row]
            raw_len = len(line)
            start_col = self.left

            buf.append(fg(t.fg) + bg(t.bg))
            if start_col >= raw_len:
                buf.append(' ' * max_col)
                continue

            disp = min(raw_len - start_col, max_col)
            end_col = start_col + disp

            # Build content with optional highlights
            syn = get_syntax(self.filename)
            if self.mode == 'visual':
                self.render_visual_content(buf, t, syn, line, start_col, end_col, buf_row)
            elif syn and not self.search_query:
                self.render_syntax_line(buf, t, syn, line, start_col, end_col)
            elif self.search_query and self.search_results:
                self.render_search_line(buf, t, line, start_col, end_col)
            else:
                buf.append(line[start_col:end_col])

            if disp < max_col:
                buf.append(' ' * (max_col - disp))

    def render_search_line(self, buf, t, line, start_col, end_col):
        """Render a segment of `line` from start_col to end_col, highlighting search matches."""
        pos = start_col
        q = self.search_query
        qlen = len(q)
        ci = line.find(q, pos)
        while ci != -1 and ci < end_col and pos < end_col:
            # Text before match
            if ci > pos:
                buf.append(line[pos:ci])
            # The match itself — highlighted
            match_end = min(ci + qlen, end_col)
            buf.append(bg(t.search_bg) + fg(t.search_fg) + line[ci:match_end] + fg(t.fg) + bg(t.bg))
            pos = match_end
            ci = line.find(q, pos)
        if pos < end_col:
            buf.append(line[pos:end_col])

    def render_syntax_line(self, buf, t, syn, line, start_col, end_col):
        visible = line[start_col:end_col]
        tokens = syn(visible)
        pos = 0
        for s, e, typ in tokens:
            if s > pos:
                buf.append(visible[pos:s])
            attr = getattr(t, typ, None)
            if attr:
                buf.append(fg(attr) + bg(t.bg))
            buf.append(visible[s:e])
            if attr:
                buf.append(fg(t.fg) + bg(t.bg))
            pos = e
        if pos < len(visible):
            buf.append(visible[pos:])

    def render_visual_content(self, buf, t, syn, line, start_col, end_col, buf_row):
        """Render syntax-highlighted content with visual-selection overlay on top."""
        visible = line[start_col:end_col]
        tokens = syn(visible) if syn else []
        sl, sc, el, ec = self.visual_bounds()
        # Build a colour map for each character: (fg_Color_or_None, in_selection)
        col_map = []
        for i in range(len(visible)):
            col = start_col + i
            sel = self.in_selection(buf_row, col)
            col_map.append((None, sel))
        # Apply syntax tokens
        for ts, te, ttyp in tokens:
            attr = getattr(t, ttyp, None)
            for i in range(max(0, ts), min(te, len(visible))):
                if col_map[i][0] is None:
                    col_map[i] = (attr, col_map[i][1])
        # Emit
        prev_fg = None
        prev_sel = None
        run = ''
        for i in range(len(visible) + 1):
            if i < len(visible):
                fgc = col_map[i][0]
                sel = col_map[i][1]
            else:
                fgc = None
                sel = None
            if fgc != prev_fg or sel != prev_sel or i == len(visible):
                if run:
                    if prev_sel:
                        buf.append(fg(prev_fg or t.fg) + bg(t.sel_bg))
                    elif prev_fg and prev_fg != t.fg:
                        buf.append(fg(prev_fg) + bg(t.bg))
                    else:
                        buf.append(fg(t.fg) + bg(t.bg))
                    buf.append(run)
                    run = ''
                prev_fg = fgc
                prev_sel = sel
            if i < len(visible):
                run += visible[i]
        buf.append(fg(t.fg) + bg(t.bg))

    def render_filetree(self, buf, t, w, h):
        ft_w = self.filetree_width
        visible_h = h - 2  # rows 1..h-2 are content area
        entries = self.filetree.entries
        sel = self.filetree.selected

        # Auto-scroll
        if sel < self.filetree.scroll:
            self.filetree.scroll = sel
        if sel >= self.filetree.scroll + visible_h:
            self.filetree.scroll = sel - visible_h + 1
        if self.filetree.scroll < 0:
            self.filetree.scroll = 0

        y = 1
        for i in range(self.filetree.scroll, len(entries)):
            if y >= h - 1:
                break
            entry = entries[i]
            self.go(buf, 0, y)

            indent = '  ' * entry.depth
            if entry.is_dir:
                icon = FOLDER_OPEN if entry.expanded else FOLDER_CLOSED
                arrow = '▾ ' if entry.expanded else '▸ '
            else:
                icon = file_icon(entry.path, False)
                arrow = '  '
            visible = f'{indent}{arrow}{icon} {entry.name}'
            max_w = ft_w - 1  # leave 1 col for border
            if len(visible) > max_w:
                visible = visible[:max_w - 1] + '…'
            visible = visible.ljust(max_w)

            if i == sel:
                buf.append(bg(t.status_bg) + fg(t.status_fg) + visible)
            elif entry.is_dir:
                buf.append(fg(t.kw) + bg(t.bg) + visible)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + visible)

            # Vertical border
            self.go(buf, ft_w - 1, y)
            buf.append(fg(t.tilde) + bg(t.bg) + '│')
            y += 1

        # Clear remaining rows
        while y < h - 1:
            self.go(buf, 0, y)
            buf.append(bg(t.bg) + ' ' * (ft_w - 1))
            self.go(buf, ft_w - 1, y)
            buf.append(fg(t.tilde) + bg(t.bg) + '│')
            y += 1

    def render_buffer_bar(self, buf, t, w):
        self.go(buf, 0, 0)
        buf.append('\033[0m' + bg(t.bg))
        active_buf = self.panes[self.active_pane].buffer_idx if self.panes else 0
        if len(self.buffers) <= 1:
            b = self.buffers[0] if self.buffers else None
            if b:
                name = b.filename.rsplit('/', 1)[-1] if b.filename else '[No Name]'
                if b.shell:
                    name = '[shell]'
                mod_flag = ' +' if b.modified else ''
                buf.append(bg(t.status_bg) + fg(t.status_fg) + f' {name}{mod_flag} ')
        else:
            for i, b in enumerate(self.buffers):
                name = b.filename.rsplit('/', 1)[-1] if b.filename else '[No Name]'
                if b.shell:
                    name = '[shell]'
                mod_flag = ' +' if b.modified else ''
                label = f' {name}{mod_flag} '
                if i == active_buf:
                    buf.append(bg(t.status_bg) + fg(t.status_fg) + label + fg(t.fg) + bg(t.bg))
                else:
                    buf.append(fg(t.tilde) + label + fg(t.fg))
                buf.append(fg(t.tilde) + '│')
        buf.append(fg(t.fg) + bg(t.bg) + '\033[K')
        buf.append('\033[0m')

    def render_status(self, buf, t, w, h):
        y = h - 1
        self.go(buf, 0, y)
        buf.append(bg(t.status_bg) + fg(t.status_fg) + '\033[1m')

        if self.flash is not None:
            text = f' {self.flash} '
        elif self.state == 'command':
            text = f':{self.cmd_buf}'
        elif self.state == 'search':
            text = f'/{self.cmd_buf}'
        else:
            mode_str = 'INSERT' if self.mode == 'insert' else 'VISUAL' if self.mode == 'visual' else 'NORMAL'
            fname = self.filename if self.filename else '[No Name]'
            mod_str = ' [+]' if self.modified else ''
            location = f'{self.cy + 1}:{self.cx + 1}'
            tname = self.theme_name.capitalize()
            now_playing = ''
            if self.music_player.playing and self.music_player.current_song:
                name = self.music_player.current_song.rsplit('/', 1)[-1]
                now_playing = f'  ♫ {name}'
            text = f' {mode_str}  {fname}{mod_str}  ─  {location}  ─  {tname}{now_playing} '

        text = text[:w]
        buf.append(text)
        buf.append('\033[K\033[0m')

    def render_splash(self, buf, t, w, h, ft_w=0):
        avail_w = w - ft_w
        figlet = [
            ' _            _ ',
            '| |          | |',
            '| | _____  __| |',
            '| |/ / _ \\/ _` |',
            '|   <  __/ (_| |',
            '|_|\\_\\___|\\__,_|',
        ]
        keybinds = [
            ('Ctrl+P', 'find file'),   ('Ctrl+F', 'file tree'),
            ('Ctrl+J', 'shell pane'),  ('Ctrl+R', 'run python'),
            ('Ctrl+T', 'music'),       ('Ctrl+E', 'theme'),
            ('Ctrl+W', 'pane menu'),   ('Tab',    'switch buf'),
            (':vsp',   'vsplit'),      (':q!',    'quit'),
            (':wq',    'save & quit'), (':3',     'colour fx'),
        ]
        key_w = max(len(k) for k, _ in keybinds) + 2
        pairs = [keybinds[i:i+2] for i in range(0, len(keybinds), 2)]
        total_h = len(figlet) + 3 + len(pairs)
        pad_top = max(0, (h - total_h) // 2)

        buf.append('\033[0m' + fg(t.fg) + bg(t.bg))

        y = 0
        for _ in range(pad_top):
            self.go(buf, ft_w, y); buf.append(bg(t.bg) + '\033[K'); y += 1

        # Figlet — styled with keyword colour
        for line in figlet:
            self.go(buf, ft_w, y); buf.append(bg(t.bg) + '\033[K')
            fx = ft_w + max(0, (avail_w - len(line)) // 2)
            self.go(buf, fx, y)
            buf.append(fg(t.kw) + line + fg(t.fg))
            y += 1

        # Blank line
        self.go(buf, ft_w, y); buf.append(bg(t.bg) + '\033[K'); y += 1

        # Subtitle
        self.go(buf, ft_w, y); buf.append(bg(t.bg) + '\033[K')
        sub = "kayden's editor"
        fx = ft_w + max(0, (avail_w - len(sub)) // 2)
        self.go(buf, fx, y)
        buf.append(fg(t.tilde) + sub + fg(t.fg))
        y += 1

        # Blank line
        self.go(buf, ft_w, y); buf.append(bg(t.bg) + '\033[K'); y += 1

        # Keybinds — two columns, keys in builtin colour, descs in fg
        gap = 3
        max_pair_w = 0
        for pair in pairs:
            pw = sum(key_w + len(d) for _, d in pair) + gap * max(0, len(pair) - 1)
            if pw > max_pair_w:
                max_pair_w = pw
        fx = ft_w + max(0, (avail_w - max_pair_w) // 2)
        for pair in pairs:
            self.go(buf, ft_w, y); buf.append(bg(t.bg))
            self.go(buf, fx, y)
            for ki, (key, desc) in enumerate(pair):
                if ki > 0:
                    buf.append(' ' * gap)
                buf.append(fg(t.builtin) + key + fg(t.fg))
                buf.append(' ' * (key_w - len(key)))
                buf.append(desc)
            buf.append('\033[K')
            y += 1

    # ── theme selector overlay ──

    def render_theme(self, buf, t, w, h):
        n = len(THEME_LIST)
        popup_w = min(40, w - 4)
        inner_w = popup_w - 4
        popup_h = min(n + 2, h - 2) + 2
        popup_x = (w - popup_w) // 2
        popup_y = max(0, (h - popup_h) // 3)

        rst = '\033[0m' + fg(t.fg) + bg(t.bg)
        for row in range(popup_h):
            self.go(buf, popup_x, popup_y + row)
            if row == 0:
                buf.append(fg(t.fg) + bg(t.bg) + '┌')
                title = ' Theme Selector '
                pad = popup_w - 2 - len(title)
                buf.append('─' * (pad // 2) + title + '─' * (pad - pad // 2) + '┐' + rst)
            elif row == popup_h - 1:
                buf.append(fg(t.fg) + bg(t.bg) + '└' + '─' * (popup_w - 2) + '┘' + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)
                buf.append(' ' * (popup_w - 2))
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)

        inner_y = popup_y + 1
        for i, name in enumerate(THEME_LIST):
            if inner_y >= popup_y + popup_h - 1:
                break
            is_current = name == self.theme_name
            is_selected = i == self.theme_selected
            prefix = ' ✓ ' if is_current else '   '
            line = (prefix + name.capitalize()).ljust(inner_w)
            self.go(buf, popup_x + 2, inner_y)
            if is_selected:
                buf.append(fg(t.status_bg) + bg(t.status_fg) + line + rst)
            elif is_current:
                buf.append(fg(t.fg) + bg(t.bg) + '\033[1m' + line + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + line + rst)
            inner_y += 1

    # ── finder overlay ──

    def render_finder(self, buf, t, w, h):
        popup_w = max(40, int(w * 0.6))
        inner_w = popup_w - 2
        popup_h = max(5, int(h * 0.5))
        popup_x = (w - popup_w) // 2
        popup_y = max(0, (h - popup_h) // 3)

        rst = '\033[0m' + fg(t.fg) + bg(t.bg)
        for row in range(popup_h):
            self.go(buf, popup_x, popup_y + row)
            if row == 0:
                buf.append(fg(t.fg) + bg(t.bg) + '┌')
                title = ' Find File '
                pad = popup_w - 2 - len(title)
                buf.append('─' * (pad // 2) + title + '─' * (pad - pad // 2) + '┐' + rst)
            elif row == popup_h - 1:
                buf.append(fg(t.fg) + bg(t.bg) + '└' + '─' * (popup_w - 2) + '┘' + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)
                buf.append(' ' * (popup_w - 2))
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)

        # Query bar (second row)
        query_y = popup_y + 1
        query_text = ' type to search…' if not self.finder_query else f' {self.finder_query}'
        query_display = query_text.ljust(inner_w)[:inner_w]
        self.go(buf, popup_x + 1, query_y)
        buf.append(fg(t.status_fg) + bg(t.status_bg) + query_display + rst)

        # Results list
        inner_y = popup_y + 2
        max_rows = popup_h - 3
        for i, (path, _score) in enumerate(self.finder.results):
            if i >= max_rows:
                break
            line = path.ljust(inner_w)[:inner_w]
            self.go(buf, popup_x + 1, inner_y)
            if i == self.finder_selection:
                buf.append(fg(t.status_bg) + bg(t.status_fg) + line + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + line + rst)
            inner_y += 1

    # ── run output overlay (Ctrl+R) ──

    def render_run(self, buf, t, w, h):
        popup_w = max(30, int(w * 0.7))
        popup_h = max(5, int(h * 0.6))
        popup_x = (w - popup_w) // 2
        popup_y = max(0, (h - popup_h) // 3)
        inner_w = popup_w - 2
        inner_h = popup_h - 2

        rst = '\033[0m' + fg(t.fg) + bg(t.bg)
        # Border
        for row in range(popup_h):
            self.go(buf, popup_x, popup_y + row)
            if row == 0:
                buf.append(fg(t.fg) + bg(t.bg) + '┌')
                title = ' Run Output '
                pad = popup_w - 2 - len(title)
                buf.append('─' * (pad // 2) + title + '─' * (pad - pad // 2) + '┐' + rst)
            elif row == popup_h - 1:
                buf.append(fg(t.fg) + bg(t.bg) + '└' + '─' * (popup_w - 2) + '┘' + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)
                buf.append(' ' * (popup_w - 2))
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)

        # Output text — show last inner_h lines
        lines = self.run_output.split('\n')
        visible = lines[-inner_h:] if len(lines) > inner_h else lines
        for i, line in enumerate(visible):
            self.go(buf, popup_x + 1, popup_y + 1 + i)
            display = line[:inner_w].ljust(inner_w)
            buf.append(fg(t.fg) + bg(t.bg) + display + rst)

    # ── shell overlay (Ctrl+J) ──

    def render_image(self, buf, w, h):
        if self.image_path is None:
            return
        rst = '\033[0m'
        buf.append(rst + '\033[2J')  # clear screen for image
        # Show file name at top
        name = os.path.basename(self.image_path)
        self.go(buf, 0, 0)
        buf.append(f' {name}  (any key to close) ')
        buf.append('\033[K')
        # Render image below — leave 1 row for the header
        seq = kitty_show_image(self.image_path, 0, 1, w, h - 1)
        if seq:
            buf.append(seq)
        else:
            self.go(buf, 0, h // 2)
            buf.append(f'   Cannot display: {name} (PNG only, Kitty terminal required)')
        buf.append(rst)

    # ── shell overlay (Ctrl+J) ──

    def render_shell(self, buf, t, w, h):
        if self.shell is None:
            return
        popup_w = max(40, int(w * 0.92))
        popup_h = max(5, int(h * 0.85))
        popup_x = (w - popup_w) // 2
        popup_y = max(0, (h - popup_h) // 2)
        inner_w = popup_w - 2
        inner_h = popup_h - 2

        rst = '\033[0m' + fg(t.fg) + bg(t.bg)

        # Resize the shell PTY
        self.shell.resize(inner_h, inner_w)

        # Border
        for row in range(popup_h):
            self.go(buf, popup_x, popup_y + row)
            if row == 0:
                buf.append(fg(t.fg) + bg(t.bg) + '┌')
                title = ' Shell — type exit to close '
                pad = popup_w - 2 - len(title)
                buf.append('─' * (pad // 2) + title + '─' * (pad - pad // 2) + '┐' + rst)
            elif row == popup_h - 1:
                buf.append(fg(t.fg) + bg(t.bg) + '└' + '─' * (popup_w - 2) + '┘' + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)
                buf.append(' ' * (popup_w - 2))
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)

        # Shell output — show last inner_h lines
        lines = self.shell.styled_lines(t.fg, t.bg)
        visible = lines[-inner_h:] if len(lines) > inner_h else lines
        for li, line in enumerate(visible):
            self.go(buf, popup_x + 1, popup_y + 1 + li)
            # pad to fill the row
            col = 0
            for text, fgc, bold in line:
                if fgc:
                    buf.append(fg(fgc))
                else:
                    buf.append(fg(t.fg))
                buf.append(bg(t.bg))
                if bold:
                    buf.append('\033[1m')
                for ch in text:
                    # Skip control chars except space
                    if ch.isprintable() or ch == ' ':
                        buf.append(ch)
                        col += 1
                    if col >= inner_w:
                        break
                buf.append('\033[0m' + fg(t.fg) + bg(t.bg))
                if col >= inner_w:
                    break
            if col < inner_w:
                buf.append(' ' * (inner_w - col))

    # ── music player overlay (Ctrl+T) ──

    def render_music(self, buf, t, w, h):
        popup_w = max(40, int(w * 0.6))
        inner_w = popup_w - 2
        popup_h = max(5, int(h * 0.5))
        popup_x = (w - popup_w) // 2
        popup_y = max(0, (h - popup_h) // 3)

        rst = '\033[0m' + fg(t.fg) + bg(t.bg)
        for row in range(popup_h):
            self.go(buf, popup_x, popup_y + row)
            if row == 0:
                buf.append(fg(t.fg) + bg(t.bg) + '┌')
                title = ' Music Player '
                pad = popup_w - 2 - len(title)
                buf.append('─' * (pad // 2) + title + '─' * (pad - pad // 2) + '┐' + rst)
            elif row == popup_h - 1:
                buf.append(fg(t.fg) + bg(t.bg) + '└' + '─' * (popup_w - 2) + '┘' + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)
                buf.append(' ' * (popup_w - 2))
                buf.append(fg(t.fg) + bg(t.bg) + '│' + rst)

        # Status line (second row)
        status_y = popup_y + 1
        if self.music_player.current_song:
            name = self.music_player.current_song.rsplit('/', 1)[-1]
            status_text = f' Now Playing: {name}'
        elif not self.music_player.files:
            status_text = ' No MP3 files found in this directory.'
        else:
            status_text = f' {len(self.music_player.files)} files — Enter=play  s=stop  Esc=close'
        status_display = status_text.ljust(inner_w)[:inner_w]
        self.go(buf, popup_x + 1, status_y)
        buf.append(fg(t.status_fg) + bg(t.status_bg) + status_display + rst)

        # File list
        inner_y = popup_y + 2
        max_rows = popup_h - 3
        for i, path in enumerate(self.music_player.files):
            if i >= max_rows:
                break
            name = path.rsplit('/', 1)[-1]
            is_playing = self.music_player.playing and self.music_player.current_index == i
            prefix = ' ▶ ' if is_playing else '   '
            line = (prefix + name).ljust(inner_w)[:inner_w]
            self.go(buf, popup_x + 1, inner_y)
            if i == self.music_player.selected:
                buf.append(fg(t.status_bg) + bg(t.status_fg) + line + rst)
            elif is_playing:
                buf.append('\033[1m' + fg(t.fg) + bg(t.bg) + line + rst)
            else:
                buf.append(fg(t.fg) + bg(t.bg) + line + rst)
            inner_y += 1

    # ── main loop ──

    def run(self):
        signal.signal(signal.SIGINT, lambda s, f: (
            setattr(self, 'running', False)
        ))
        signal.signal(signal.SIGWINCH, sigwinch)

        while self.running:
            global resize_flag
            if resize_flag:
                resize_flag = False

            self.music_player.poll()

            w, h = term_size()
            self.cache_w = w; self.cache_h = h

            # Tick shells BEFORE render so output is displayed immediately
            for b in self.buffers:
                if b.shell is not None:
                    b.shell.tick()
                    if not b.shell.is_alive():
                        b.shell.kill()
                        b.shell = None

            self.render()

            if len(keybuf) == 0:
                r, _, _ = select.select([sys.stdin], [], [], 0.03)
                if not r:
                    continue

            key = read_key()
            if key >= 0:
                if not self.handle_key(key):
                    break
            self.music_player.poll()
            self.auto_reload()
            # Tick shells again after key handling to catch echo immediately
            for b in self.buffers:
                if b.shell is not None:
                    b.shell.tick()
                    if not b.shell.is_alive():
                        b.shell.kill()
                        b.shell = None

# ── Main ─────────────────────────────────────────────────────────

def main():
    filename = sys.argv[1] if len(sys.argv) > 1 else None
    try:
        term_init()
        editor = Editor(filename)
        editor.run()
    finally:
        term_restore()

if __name__ == '__main__':
    main()
