//! Colour themes for ked.
//!
//! Each theme defines a palette of styles used by the editor and the
//! syntax highlighter.  The [`Theme`] struct holds a colour for every
//! token / UI element.  [`ThemeKind`] is an enum over the nine
//! built-in themes:
//!
//!   Default, Monokai, Solarized, Nord, Gruvbox, Bi, Catppuccin,
//!   TokyoNight, Amber
//!
//! All colours use ratatui's [`Color`] type, which supports named
//! colours, indexed (256-colour) codes, and true-colour RGB.

use ratatui::style::{Color, Style, Modifier};

/// The colour palette for one theme.
#[derive(Debug, Clone)]
pub struct Theme {
    // ── general UI ──
    pub fg:            Color,   // default foreground (plain text)
    pub bg:            Color,   // default background
    pub selection_bg:  Color,   // visual selection background
    pub line_number:   Style,   // gutter line numbers
    pub tilde:         Style,   // "~" for empty rows past EOF
    pub status_bg:     Color,   // status bar background
    pub status_fg:     Color,   // status bar text

    // ── syntax tokens ──
    pub keyword:       Style,   // `fn`, `let`, `match`, `return`, …
    pub builtin:       Style,   // `print`, `len`, `println!`, …
    pub rstype:        Style,   // type names `i32`, `String`, `Vec`, …
    pub function:      Style,   // function / method names
    pub lifetime:      Style,   // lifetime annotations `'a`, `'static`
    pub string:        Style,   // `"..."` and `'...'`
    pub fstring_prefix:Style,   // the `f` before an f-string
    pub comment:       Style,   // `#`, `//`, `///`, `//!`
    pub number:        Style,   // `42`, `3.14`
    pub decorator:     Style,   // `@property`, `#[…]`
    pub operator:      Style,   // `==`, `+=`, `->`, …
    pub punctuation:   Style,   // `(`, `)`, `:`, `,`
}

/// The eighteen built-in theme variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Default,
    Monokai,
    Solarized,
    Nord,
    Gruvbox,
    Bi,
    Catppuccin,
    TokyoNight,
    Amber,
    Dracula,
    OneDark,
    Everforest,
    RosePine,
    Oxocarbon,
    System,
    OpenCode,
    Ayu,
    Kanagawa,
}

impl ThemeKind {
    /// Return the colour palette for this variant.
    pub fn theme(&self) -> Theme {
        match self {
            ThemeKind::Default    => default(),
            ThemeKind::Monokai    => monokai(),
            ThemeKind::Solarized  => solarized(),
            ThemeKind::Nord       => nord(),
            ThemeKind::Gruvbox    => gruvbox(),
            ThemeKind::Bi         => bi(),
            ThemeKind::Catppuccin => catppuccin(),
            ThemeKind::TokyoNight => tokyo_night(),
            ThemeKind::Amber      => amber(),
            ThemeKind::Dracula    => dracula(),
            ThemeKind::OneDark    => one_dark(),
            ThemeKind::Everforest => everforest(),
            ThemeKind::RosePine   => rose_pine(),
            ThemeKind::Oxocarbon  => oxocarbon(),
            ThemeKind::System     => system(),
            ThemeKind::OpenCode   => opencode(),
            ThemeKind::Ayu        => ayu(),
            ThemeKind::Kanagawa   => kanagawa(),
        }
    }

    /// Human-readable name for the `:themes` command.
    pub fn name(&self) -> &'static str {
        match self {
            ThemeKind::Default    => "default",
            ThemeKind::Monokai    => "monokai",
            ThemeKind::Solarized  => "solarized",
            ThemeKind::Nord       => "nord",
            ThemeKind::Gruvbox    => "gruvbox",
            ThemeKind::Bi         => "bi",
            ThemeKind::Catppuccin => "catppuccin",
            ThemeKind::TokyoNight => "tokyonight",
            ThemeKind::Amber      => "amber",
            ThemeKind::Dracula    => "dracula",
            ThemeKind::OneDark    => "onedark",
            ThemeKind::Everforest => "everforest",
            ThemeKind::RosePine   => "rosepine",
            ThemeKind::Oxocarbon  => "oxocarbon",
            ThemeKind::System     => "system",
            ThemeKind::OpenCode   => "opencode",
            ThemeKind::Ayu        => "ayu",
            ThemeKind::Kanagawa   => "kanagawa",
        }
    }

    /// Iterate over all variants (used by `:themes`).
    pub fn all() -> &'static [ThemeKind] {
        &[
            ThemeKind::Default,
            ThemeKind::Monokai,
            ThemeKind::Solarized,
            ThemeKind::Nord,
            ThemeKind::Gruvbox,
            ThemeKind::Bi,
            ThemeKind::Catppuccin,
            ThemeKind::TokyoNight,
            ThemeKind::Amber,
            ThemeKind::Dracula,
            ThemeKind::OneDark,
            ThemeKind::Everforest,
            ThemeKind::RosePine,
            ThemeKind::Oxocarbon,
            ThemeKind::System,
            ThemeKind::OpenCode,
            ThemeKind::Ayu,
            ThemeKind::Kanagawa,
        ]
    }

    /// Parse a case-insensitive theme name from a command.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "default"    => Some(ThemeKind::Default),
            "monokai"    => Some(ThemeKind::Monokai),
            "solarized"  => Some(ThemeKind::Solarized),
            "nord"       => Some(ThemeKind::Nord),
            "gruvbox"    => Some(ThemeKind::Gruvbox),
            "bi"         => Some(ThemeKind::Bi),
            "catppuccin" => Some(ThemeKind::Catppuccin),
            "tokyonight" => Some(ThemeKind::TokyoNight),
            "amber"      => Some(ThemeKind::Amber),
            "dracula"    => Some(ThemeKind::Dracula),
            "onedark"    => Some(ThemeKind::OneDark),
            "everforest" => Some(ThemeKind::Everforest),
            "rosepine"   => Some(ThemeKind::RosePine),
            "oxocarbon"  => Some(ThemeKind::Oxocarbon),
            "system"     => Some(ThemeKind::System),
            "opencode"   => Some(ThemeKind::OpenCode),
            "ayu"        => Some(ThemeKind::Ayu),
            "kanagawa"   => Some(ThemeKind::Kanagawa),
            _           => None,
        }
    }
}

// ── helper: shorthand for a plain fg colour (no bg, no modifier) ──
const fn fg(c: Color) -> Style {
    Style::new().fg(c)
}
const fn fg_bold(c: Color) -> Style {
    Style::new().fg(c).add_modifier(Modifier::BOLD)
}

// ── theme definitions ────────────────────────────────────────────

fn default() -> Theme {
    Theme {
        fg:            Color::White,
        bg:            Color::Reset,
        selection_bg:  Color::DarkGray,
        line_number:   Style::new().fg(Color::DarkGray),
        tilde:         Style::new().fg(Color::DarkGray),
        status_bg:     Color::DarkGray,
        status_fg:     Color::White,
        keyword:       fg(Color::Blue),
        builtin:       fg(Color::Cyan),
        rstype:        fg(Color::Yellow),
        function:      fg(Color::White),
        lifetime:      fg(Color::Rgb(0x5e, 0xbc, 0xce)),
        string:        fg(Color::Green),
        fstring_prefix:fg_bold(Color::Green),
        comment:       fg(Color::Rgb(120, 120, 120)),
        number:        fg(Color::Yellow),
        decorator:     fg(Color::Magenta),
        operator:      fg(Color::White),
        punctuation:   fg(Color::White),
    }
}

fn monokai() -> Theme {
    // Monokai: dark bg, high saturation pastels
    Theme {
        fg:            Color::Rgb(0xd6, 0xd6, 0xd6),
        bg:            Color::Rgb(0x27, 0x28, 0x22),
        selection_bg:  Color::Rgb(0x49, 0x43, 0x3e),
        line_number:   Style::new().fg(Color::Rgb(0x75, 0x75, 0x5e)),
        tilde:         Style::new().fg(Color::Rgb(0x75, 0x75, 0x5e)),
        status_bg:     Color::Rgb(0x75, 0x75, 0x5e),
        status_fg:     Color::Rgb(0x27, 0x28, 0x22),
        keyword:       fg(Color::Rgb(0xf9, 0x26, 0x72)),  // pink
        builtin:       fg(Color::Rgb(0xae, 0x81, 0xff)),  // purple
        rstype:        fg(Color::Rgb(0xa6, 0xe2, 0x2e)),  // green
        function:      fg(Color::Rgb(0xe6, 0xdb, 0x74)),  // yellow
        lifetime:      fg(Color::Rgb(0x66, 0xd9, 0xef)),  // cyan
        string:        fg(Color::Rgb(0xe6, 0xdb, 0x74)),  // yellow
        fstring_prefix:fg_bold(Color::Rgb(0xe6, 0xdb, 0x74)),
        comment:       fg(Color::Rgb(0x75, 0x75, 0x5e)),
        number:        fg(Color::Rgb(0xae, 0x81, 0xff)),  // purple
        decorator:     fg(Color::Rgb(0xa6, 0xe2, 0x2e)),  // green
        operator:      fg(Color::Rgb(0xf9, 0x26, 0x72)),  // pink
        punctuation:   fg(Color::Rgb(0xd6, 0xd6, 0xd6)),
    }
}

fn solarized() -> Theme {
    // Solarized: low-contrast, warm/cool balanced
    Theme {
        fg:            Color::Rgb(0x65, 0x7b, 0x83),
        bg:            Color::Rgb(0xfd, 0xf6, 0xe3),
        selection_bg:  Color::Rgb(0xee, 0xe8, 0xd5),
        line_number:   Style::new().fg(Color::Rgb(0x93, 0xa1, 0xa1)),
        tilde:         Style::new().fg(Color::Rgb(0x93, 0xa1, 0xa1)),
        status_bg:     Color::Rgb(0x07, 0x36, 0x42),
        status_fg:     Color::Rgb(0xfd, 0xf6, 0xe3),
        keyword:       fg(Color::Rgb(0xcb, 0x4b, 0x16)),  // orange
        builtin:       fg(Color::Rgb(0x2a, 0xa1, 0x98)),  // cyan
        rstype:        fg(Color::Rgb(0xb5, 0x89, 0x00)),  // yellow
        function:      fg(Color::Rgb(0x26, 0x8b, 0xd2)),  // blue
        lifetime:      fg(Color::Rgb(0x2a, 0xa1, 0x98)),  // cyan
        string:        fg(Color::Rgb(0x2d, 0xc1, 0x00)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0x2d, 0xc1, 0x00)),
        comment:       fg(Color::Rgb(0x93, 0xa1, 0xa1)),
        number:        fg(Color::Rgb(0xd3, 0x36, 0x82)),  // magenta
        decorator:     fg(Color::Rgb(0x6c, 0x71, 0xc4)),  // violet
        operator:      fg(Color::Rgb(0x58, 0x6e, 0x75)),
        punctuation:   fg(Color::Rgb(0x65, 0x7b, 0x83)),
    }
}

fn nord() -> Theme {
    // Nord: arctic, bluish with high readability
    Theme {
        fg:            Color::Rgb(0xd8, 0xde, 0xe9),
        bg:            Color::Rgb(0x2e, 0x34, 0x40),
        selection_bg:  Color::Rgb(0x43, 0x4c, 0x5e),
        line_number:   Style::new().fg(Color::Rgb(0x4c, 0x56, 0x6a)),
        tilde:         Style::new().fg(Color::Rgb(0x4c, 0x56, 0x6a)),
        status_bg:     Color::Rgb(0x3b, 0x42, 0x52),
        status_fg:     Color::Rgb(0xd8, 0xde, 0xe9),
        keyword:       fg(Color::Rgb(0x81, 0xa1, 0xc1)),  // blue
        builtin:       fg(Color::Rgb(0x88, 0xc0, 0xd0)),  // light blue
        rstype:        fg(Color::Rgb(0xeb, 0xcb, 0x8b)),  // yellow
        function:      fg(Color::Rgb(0xd8, 0xde, 0xe9)),  // white
        lifetime:      fg(Color::Rgb(0x88, 0xc0, 0xd0)),  // light blue
        string:        fg(Color::Rgb(0xa3, 0xbe, 0x8c)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xa3, 0xbe, 0x8c)),
        comment:       fg(Color::Rgb(0x61, 0x6e, 0x88)),
        number:        fg(Color::Rgb(0xb4, 0x8e, 0xad)),  // purple
        decorator:     fg(Color::Rgb(0xd0, 0x87, 0x70)),  // orange
        operator:      fg(Color::Rgb(0xd8, 0xde, 0xe9)),
        punctuation:   fg(Color::Rgb(0xd8, 0xde, 0xe9)),
    }
}

fn gruvbox() -> Theme {
    // Gruvbox: warm retro, yellow/green/pink accents
    Theme {
        fg:            Color::Rgb(0xeb, 0xdb, 0xb2),
        bg:            Color::Rgb(0x28, 0x28, 0x28),
        selection_bg:  Color::Rgb(0x50, 0x49, 0x45),
        line_number:   Style::new().fg(Color::Rgb(0x50, 0x49, 0x45)),
        tilde:         Style::new().fg(Color::Rgb(0x50, 0x49, 0x45)),
        status_bg:     Color::Rgb(0x50, 0x49, 0x45),
        status_fg:     Color::Rgb(0xeb, 0xdb, 0xb2),
        keyword:       fg(Color::Rgb(0xfb, 0x49, 0x34)),  // red
        builtin:       fg(Color::Rgb(0x8e, 0xc0, 0x7c)),  // green
        rstype:        fg(Color::Rgb(0xd3, 0x86, 0x9b)),  // magenta
        function:      fg(Color::Rgb(0xeb, 0xdb, 0xb2)),  // fg
        lifetime:      fg(Color::Rgb(0x83, 0xa5, 0x98)),  // aqua
        string:        fg(Color::Rgb(0xb8, 0xbb, 0x26)),  // yellow-green
        fstring_prefix:fg_bold(Color::Rgb(0xb8, 0xbb, 0x26)),
        comment:       fg(Color::Rgb(0x92, 0x83, 0x74)),
        number:        fg(Color::Rgb(0xd3, 0x86, 0x9b)),  // magenta
        decorator:     fg(Color::Rgb(0xfe, 0x80, 0x19)),  // orange
        operator:      fg(Color::Rgb(0xeb, 0xdb, 0xb2)),
        punctuation:   fg(Color::Rgb(0xeb, 0xdb, 0xb2)),
    }
}

fn bi() -> Theme {
    // Bi pride flag inspired: pink #D70270, purple #734F96, blue #0038A8
    // Dark bg with pink/purple/blue accents.
    Theme {
        fg:            Color::Rgb(0xe6, 0xdf, 0xf0),  // light lavender
        bg:            Color::Rgb(0x0d, 0x0a, 0x14),  // deep purple-black
        selection_bg:  Color::Rgb(0x2a, 0x1a, 0x3a),
        line_number:   Style::new().fg(Color::Rgb(0x4a, 0x3a, 0x60)),
        tilde:         Style::new().fg(Color::Rgb(0x4a, 0x3a, 0x60)),
        status_bg:     Color::Rgb(0xd7, 0x02, 0x70),  // bi-pink
        status_fg:     Color::Rgb(0xff, 0xff, 0xff),
        keyword:       fg(Color::Rgb(0xd7, 0x02, 0x70)),  // bi-pink
        builtin:       fg(Color::Rgb(0xb0, 0x7c, 0xc6)),  // purple
        rstype:        fg(Color::Rgb(0x5b, 0x9b, 0xd5)),  // soft blue
        function:      fg(Color::Rgb(0xd4, 0xc5, 0xe8)),  // soft lavender
        lifetime:      fg(Color::Rgb(0x7e, 0xc8, 0xe3)),  // light cyan
        string:        fg(Color::Rgb(0xe8, 0x8a, 0xb0)),  // rose pink
        fstring_prefix:fg_bold(Color::Rgb(0xe8, 0x8a, 0xb0)),
        comment:       fg(Color::Rgb(0x60, 0x50, 0x7a)),  // muted purple
        number:        fg(Color::Rgb(0xb0, 0x7c, 0xc6)),  // purple
        decorator:     fg(Color::Rgb(0x5b, 0x9b, 0xd5)),  // soft blue
        operator:      fg(Color::Rgb(0xe6, 0xdf, 0xf0)),
        punctuation:   fg(Color::Rgb(0xe6, 0xdf, 0xf0)),
    }
}

fn catppuccin() -> Theme {
    // Catppuccin Mocha: warm pastels on deep purple-brown bg
    Theme {
        fg:            Color::Rgb(0xcd, 0xd6, 0xf4),  // text
        bg:            Color::Rgb(0x1e, 0x1e, 0x2e),  // base
        selection_bg:  Color::Rgb(0x45, 0x47, 0x5a),
        line_number:   Style::new().fg(Color::Rgb(0x6c, 0x70, 0x86)),  // overlay1
        tilde:         Style::new().fg(Color::Rgb(0x6c, 0x70, 0x86)),
        status_bg:     Color::Rgb(0x31, 0x32, 0x44),  // surface0
        status_fg:     Color::Rgb(0xcd, 0xd6, 0xf4),
        keyword:       fg(Color::Rgb(0xcb, 0xa6, 0xf7)),  // mauve
        builtin:       fg(Color::Rgb(0x89, 0xb4, 0xfa)),  // blue
        rstype:        fg(Color::Rgb(0xf9, 0xe2, 0xaf)),  // yellow
        function:      fg(Color::Rgb(0xf5, 0xc2, 0xe7)),  // pink
        lifetime:      fg(Color::Rgb(0x94, 0xe2, 0xd5)),  // teal
        string:        fg(Color::Rgb(0xa6, 0xe3, 0xa1)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xa6, 0xe3, 0xa1)),
        comment:       fg(Color::Rgb(0x6c, 0x70, 0x86)),  // overlay1
        number:        fg(Color::Rgb(0xfa, 0xb3, 0x87)),  // peach
        decorator:     fg(Color::Rgb(0xf3, 0x8b, 0xa8)),  // red
        operator:      fg(Color::Rgb(0x89, 0xb4, 0xfa)),  // blue
        punctuation:   fg(Color::Rgb(0xcd, 0xd6, 0xf4)),
    }
}

fn tokyo_night() -> Theme {
    // Tokyo Night: deep blue-black bg, cyan/blue/purple accents
    Theme {
        fg:            Color::Rgb(0xc0, 0xca, 0xf5),  // text
        bg:            Color::Rgb(0x1a, 0x1b, 0x26),  // bg
        selection_bg:  Color::Rgb(0x36, 0x3b, 0x54),
        line_number:   Style::new().fg(Color::Rgb(0x56, 0x5f, 0x89)),  // comment
        tilde:         Style::new().fg(Color::Rgb(0x56, 0x5f, 0x89)),
        status_bg:     Color::Rgb(0x2a, 0x2e, 0x3d),  // surface0
        status_fg:     Color::Rgb(0xc0, 0xca, 0xf5),
        keyword:       fg(Color::Rgb(0xbb, 0x9a, 0xf7)),  // purple
        builtin:       fg(Color::Rgb(0x7d, 0xcf, 0xff)),  // cyan
        rstype:        fg(Color::Rgb(0x7a, 0xa2, 0xf7)),  // blue
        function:      fg(Color::Rgb(0xe0, 0xaf, 0x68)),  // yellow
        lifetime:      fg(Color::Rgb(0x7d, 0xcf, 0xff)),  // cyan
        string:        fg(Color::Rgb(0x9e, 0xce, 0x6a)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0x9e, 0xce, 0x6a)),
        comment:       fg(Color::Rgb(0x56, 0x5f, 0x89)),
        number:        fg(Color::Rgb(0xff, 0x9e, 0x64)),  // orange
        decorator:     fg(Color::Rgb(0xf7, 0x76, 0x8e)),  // red
        operator:      fg(Color::Rgb(0x7d, 0xcf, 0xff)),  // cyan
        punctuation:   fg(Color::Rgb(0xc0, 0xca, 0xf5)),
    }
}

fn amber() -> Theme {
    // Amber Amethyst: deep purple base, warm gold accent
    // Mirrors the kitty amber-amethyst theme.
    Theme {
        fg:            Color::Rgb(0xdd, 0xd6, 0xf0),  // lavender
        bg:            Color::Rgb(0x18, 0x14, 0x25),  // deep purple
        selection_bg:  Color::Rgb(0xf5, 0xc8, 0x42),  // gold
        line_number:   Style::new().fg(Color::Rgb(0x88, 0x80, 0xa0)),
        tilde:         Style::new().fg(Color::Rgb(0x88, 0x80, 0xa0)),
        status_bg:     Color::Rgb(0xf5, 0xc8, 0x42),  // gold
        status_fg:     Color::Rgb(0x18, 0x14, 0x25),  // bg
        keyword:       fg(Color::Rgb(0xf5, 0xc8, 0x42)),  // gold
        builtin:       fg(Color::Rgb(0xb8, 0x9c, 0xf0)),  // medium purple
        rstype:        fg(Color::Rgb(0x9b, 0x7d, 0xd6)),  // medium purple
        function:      fg(Color::Rgb(0xdd, 0xd6, 0xf0)),  // lavender
        lifetime:      fg(Color::Rgb(0xc4, 0xa9, 0xf8)),  // light purple
        string:        fg(Color::Rgb(0xa8, 0x8a, 0xe0)),  // purple
        fstring_prefix:fg_bold(Color::Rgb(0xf5, 0xc8, 0x42)),  // gold
        comment:       fg(Color::Rgb(0x88, 0x80, 0xa0)),
        number:        fg(Color::Rgb(0xc4, 0xa9, 0xf8)),
        decorator:     fg(Color::Rgb(0x7b, 0x5d, 0xb8)),  // deep purple
        operator:      fg(Color::Rgb(0xdd, 0xd6, 0xf0)),
        punctuation:   fg(Color::Rgb(0xdd, 0xd6, 0xf0)),
    }
}

impl Theme {
    /// Return a copy of this theme with every colour's hue rotated by
    /// `hue_offset` degrees (0.0–360.0).  White, black, grey, and
    /// `Color::Reset` are left untouched.
    pub fn with_rainbow(&self, hue_offset: f64) -> Theme {
        let mut t = self.clone();
        t.fg            = rotate_hue(self.fg, hue_offset);
        t.bg            = self.bg; // keep background static
        t.selection_bg  = rotate_hue(self.selection_bg, hue_offset);
        t.line_number   = rot_style(&self.line_number, hue_offset);
        t.tilde         = rot_style(&self.tilde, hue_offset);
        t.status_bg     = rotate_hue(self.status_bg, hue_offset);
        t.status_fg     = rotate_hue(self.status_fg, hue_offset);
        t.keyword       = rot_style(&self.keyword, hue_offset);
        t.builtin       = rot_style(&self.builtin, hue_offset);
        t.rstype        = rot_style(&self.rstype, hue_offset);
        t.function      = rot_style(&self.function, hue_offset);
        t.lifetime      = rot_style(&self.lifetime, hue_offset);
        t.string        = rot_style(&self.string, hue_offset);
        t.fstring_prefix= rot_style(&self.fstring_prefix, hue_offset);
        t.comment       = rot_style(&self.comment, hue_offset);
        t.number        = rot_style(&self.number, hue_offset);
        t.decorator     = rot_style(&self.decorator, hue_offset);
        t.operator      = rot_style(&self.operator, hue_offset);
        t.punctuation   = rot_style(&self.punctuation, hue_offset);
        t
    }
}

fn rot_style(s: &Style, hue: f64) -> Style {
    let fg = s.fg.map(|c| rotate_hue(c, hue));
    let bg = s.bg.map(|c| rotate_hue(c, hue));
    let mut out = *s;
    if let Some(c) = fg { out = out.fg(c); }
    if let Some(c) = bg { out = out.bg(c); }
    out
}

/// Convert HSL to RGB, each 0‑255.  `h` in degrees, `s`/`l` in 0..1.
pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

/// Rotate the hue of an RGB colour by `deg` degrees.  Named and
/// indexed colours are converted to true-colour RGB first so the
/// rotation is visible even for named colours like White, Blue, etc.
fn rotate_hue(c: Color, deg: f64) -> Color {
    let rgb = named_to_rgb(c);
    let (r, g, b) = match rgb {
        Some(x) => x,
        None => return c, // Reset or un-mapped named colour
    };
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);

    // RGB → HSL
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let l = (mx + mn) / 2.0;

    // For achromatic colours (white, black, grey) we invent a
    // moderate saturation so the hue rotation shows up.
    let (s, d) = if mx == mn {
        (0.7, 0.7) // d doesn't matter, we compute h from deg directly
    } else {
        let d = mx - mn;
        let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
        (s, d)
    };

    let h = if mx == mn {
        deg // pure grey – start hue at the rotation angle itself
    } else {
        let h = if mx == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if mx == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h * 60.0 + deg) % 360.0
    };
    let h = if h < 0.0 { h + 360.0 } else { h };

    // HSL → RGB
    let (r, g, b) = hsl_to_rgb(h, s, l);

    Color::Rgb(r, g, b)
}

/// Map a named colour or indexed colour to its conventional 8‑bit RGB
/// value.  Returns `None` for `Color::Reset` (leave untouched).
fn named_to_rgb(c: Color) -> Option<(u8, u8, u8)> {
    Some(match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (255, 0, 0),
        Color::Green => (0, 255, 0),
        Color::Yellow => (255, 255, 0),
        Color::Blue => (0, 0, 255),
        Color::Magenta => (255, 0, 255),
        Color::Cyan => (0, 255, 255),
        Color::Gray => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::LightRed => (255, 128, 128),
        Color::LightGreen => (128, 255, 128),
        Color::LightYellow => (255, 255, 128),
        Color::LightBlue => (128, 128, 255),
        Color::LightMagenta => (255, 128, 255),
        Color::LightCyan => (128, 255, 255),
        Color::White => (255, 255, 255),
        Color::Indexed(i) => ansi_256_to_rgb(i),
        _ => return None,
    })
}

/// Map an ANSI 256‑colour index to its RGB value.
fn ansi_256_to_rgb(i: u8) -> (u8, u8, u8) {
    // 0–7: standard ANSI colours
    // 8–15: bright ANSI colours
    // 16–231: 6×6×6 colour cube
    // 232–255: grey ramp
    match i {
        0   => (0, 0, 0),
        1   => (128, 0, 0),
        2   => (0, 128, 0),
        3   => (128, 128, 0),
        4   => (0, 0, 128),
        5   => (128, 0, 128),
        6   => (0, 128, 128),
        7   => (192, 192, 192),
        8   => (128, 128, 128),
        9   => (255, 0, 0),
        10  => (0, 255, 0),
        11  => (255, 255, 0),
        12  => (0, 0, 255),
        13  => (255, 0, 255),
        14  => (0, 255, 255),
        15  => (255, 255, 255),
        16..=231 => {
            let n = i - 16;
            let r = (n / 36) * 51 + 0;  // multiply by 255/5
            let g = ((n % 36) / 6) * 51 + 0;
            let b = (n % 6) * 51 + 0;
            (r as u8, g as u8, b as u8)
        }
        232..=255 => {
            let grey = (i - 232) * 11 + 8;
            (grey, grey, grey)
        }
    }
}

// ── new themes (matching pked) ────────────────────────────────

fn dracula() -> Theme {
    Theme {
        fg:            Color::Rgb(0xf8, 0xf8, 0xf2),
        bg:            Color::Rgb(0x28, 0x2a, 0x36),
        selection_bg:  Color::Rgb(0x44, 0x47, 0x5a),
        line_number:   Style::new().fg(Color::Rgb(0x62, 0x72, 0xa4)),
        tilde:         Style::new().fg(Color::Rgb(0x62, 0x72, 0xa4)),
        status_bg:     Color::Rgb(0x62, 0x72, 0xa4),
        status_fg:     Color::Rgb(0xf8, 0xf8, 0xf2),
        keyword:       fg(Color::Rgb(0xff, 0x79, 0xc6)),
        builtin:       fg(Color::Rgb(0x8b, 0xe9, 0xfd)),
        rstype:        fg(Color::Rgb(0x8b, 0xe9, 0xfd)),
        function:      fg(Color::Rgb(0x50, 0xfa, 0x7b)),
        lifetime:      fg(Color::Rgb(0x8b, 0xe9, 0xfd)),
        string:        fg(Color::Rgb(0xf1, 0xfa, 0x8c)),
        fstring_prefix:fg_bold(Color::Rgb(0xf1, 0xfa, 0x8c)),
        comment:       fg(Color::Rgb(0x62, 0x72, 0xa4)),
        number:        fg(Color::Rgb(0xbd, 0x93, 0xf9)),
        decorator:     fg(Color::Rgb(0xff, 0x79, 0xc6)),
        operator:      fg(Color::Rgb(0xff, 0x79, 0xc6)),
        punctuation:   fg(Color::Rgb(0xf8, 0xf8, 0xf2)),
    }
}

fn one_dark() -> Theme {
    Theme {
        fg:            Color::Rgb(0xab, 0xb2, 0xbf),
        bg:            Color::Rgb(0x28, 0x2c, 0x34),
        selection_bg:  Color::Rgb(0x3e, 0x44, 0x51),
        line_number:   Style::new().fg(Color::Rgb(0x5c, 0x63, 0x70)),
        tilde:         Style::new().fg(Color::Rgb(0x5c, 0x63, 0x70)),
        status_bg:     Color::Rgb(0x28, 0x2c, 0x34),
        status_fg:     Color::Rgb(0xab, 0xb2, 0xbf),
        keyword:       fg(Color::Rgb(0xc6, 0x78, 0xdd)),
        builtin:       fg(Color::Rgb(0x61, 0xaf, 0xef)),
        rstype:        fg(Color::Rgb(0xe5, 0xc0, 0x7b)),
        function:      fg(Color::Rgb(0x61, 0xaf, 0xef)),
        lifetime:      fg(Color::Rgb(0x56, 0xb6, 0xc2)),
        string:        fg(Color::Rgb(0x98, 0xc3, 0x79)),
        fstring_prefix:fg_bold(Color::Rgb(0x98, 0xc3, 0x79)),
        comment:       fg(Color::Rgb(0x5c, 0x63, 0x70)),
        number:        fg(Color::Rgb(0xd1, 0x9a, 0x66)),
        decorator:     fg(Color::Rgb(0xe5, 0xc0, 0x7b)),
        operator:      fg(Color::Rgb(0x56, 0xb6, 0xc2)),
        punctuation:   fg(Color::Rgb(0xab, 0xb2, 0xbf)),
    }
}

fn everforest() -> Theme {
    Theme {
        fg:            Color::Rgb(0xd3, 0xc6, 0xaa),
        bg:            Color::Rgb(0x2d, 0x35, 0x3b),
        selection_bg:  Color::Rgb(0x38, 0x41, 0x48),
        line_number:   Style::new().fg(Color::Rgb(0x7d, 0x83, 0x79)),
        tilde:         Style::new().fg(Color::Rgb(0x7d, 0x83, 0x79)),
        status_bg:     Color::Rgb(0x38, 0x41, 0x48),
        status_fg:     Color::Rgb(0xd3, 0xc6, 0xaa),
        keyword:       fg(Color::Rgb(0xe6, 0x7e, 0x80)),
        builtin:       fg(Color::Rgb(0x7f, 0xbb, 0xb3)),
        rstype:        fg(Color::Rgb(0xd6, 0x99, 0x9c)),
        function:      fg(Color::Rgb(0xa7, 0xc0, 0x80)),
        lifetime:      fg(Color::Rgb(0x7f, 0xbb, 0xb3)),
        string:        fg(Color::Rgb(0xa7, 0xc0, 0x80)),
        fstring_prefix:fg_bold(Color::Rgb(0xa7, 0xc0, 0x80)),
        comment:       fg(Color::Rgb(0x7d, 0x83, 0x79)),
        number:        fg(Color::Rgb(0xdf, 0xb8, 0x69)),
        decorator:     fg(Color::Rgb(0xe6, 0x98, 0x75)),
        operator:      fg(Color::Rgb(0xd3, 0xc6, 0xaa)),
        punctuation:   fg(Color::Rgb(0xd3, 0xc6, 0xaa)),
    }
}

fn rose_pine() -> Theme {
    Theme {
        fg:            Color::Rgb(0xe0, 0xde, 0xf4),
        bg:            Color::Rgb(0x19, 0x17, 0x24),
        selection_bg:  Color::Rgb(0x26, 0x23, 0x33),
        line_number:   Style::new().fg(Color::Rgb(0x6f, 0x6a, 0x85)),
        tilde:         Style::new().fg(Color::Rgb(0x6f, 0x6a, 0x85)),
        status_bg:     Color::Rgb(0x26, 0x23, 0x33),
        status_fg:     Color::Rgb(0xe0, 0xde, 0xf4),
        keyword:       fg(Color::Rgb(0xc4, 0xa7, 0xe7)),
        builtin:       fg(Color::Rgb(0x9e, 0xce, 0xdd)),
        rstype:        fg(Color::Rgb(0xeb, 0xbc, 0xba)),
        function:      fg(Color::Rgb(0xea, 0x9a, 0x97)),
        lifetime:      fg(Color::Rgb(0x9e, 0xce, 0xdd)),
        string:        fg(Color::Rgb(0x9c, 0xcf, 0xd8)),
        fstring_prefix:fg_bold(Color::Rgb(0x9c, 0xcf, 0xd8)),
        comment:       fg(Color::Rgb(0x6f, 0x6a, 0x85)),
        number:        fg(Color::Rgb(0xea, 0x9a, 0x97)),
        decorator:     fg(Color::Rgb(0xc4, 0xa7, 0xe7)),
        operator:      fg(Color::Rgb(0x90, 0x8c, 0xaa)),
        punctuation:   fg(Color::Rgb(0xe0, 0xde, 0xf4)),
    }
}

fn oxocarbon() -> Theme {
    Theme {
        fg:            Color::Rgb(0xd2, 0xd1, 0xd2),
        bg:            Color::Rgb(0x16, 0x16, 0x16),
        selection_bg:  Color::Rgb(0x26, 0x26, 0x26),
        line_number:   Style::new().fg(Color::Rgb(0x52, 0x52, 0x52)),
        tilde:         Style::new().fg(Color::Rgb(0x52, 0x52, 0x52)),
        status_bg:     Color::Rgb(0x26, 0x26, 0x26),
        status_fg:     Color::Rgb(0xd2, 0xd1, 0xd2),
        keyword:       fg(Color::Rgb(0xbe, 0x95, 0xff)),
        builtin:       fg(Color::Rgb(0x33, 0xb0, 0xff)),
        rstype:        fg(Color::Rgb(0x08, 0xbd, 0xae)),
        function:      fg(Color::Rgb(0xbe, 0x95, 0xff)),
        lifetime:      fg(Color::Rgb(0x33, 0xb0, 0xff)),
        string:        fg(Color::Rgb(0x42, 0xbe, 0xa6)),
        fstring_prefix:fg_bold(Color::Rgb(0x42, 0xbe, 0xa6)),
        comment:       fg(Color::Rgb(0x52, 0x52, 0x52)),
        number:        fg(Color::Rgb(0xff, 0x7e, 0xb6)),
        decorator:     fg(Color::Rgb(0xbe, 0x95, 0xff)),
        operator:      fg(Color::Rgb(0xbe, 0x95, 0xff)),
        punctuation:   fg(Color::Rgb(0xd2, 0xd1, 0xd2)),
    }
}

fn system() -> Theme {
    Theme {
        fg:            Color::Rgb(0xc8, 0xc8, 0xd0),
        bg:            Color::Rgb(0x1c, 0x1c, 0x1e),
        selection_bg:  Color::Rgb(0x36, 0x36, 0x40),
        line_number:   Style::new().fg(Color::Rgb(0x5f, 0x5f, 0x69)),
        tilde:         Style::new().fg(Color::Rgb(0x5f, 0x5f, 0x69)),
        status_bg:     Color::Rgb(0x37, 0x37, 0x3e),
        status_fg:     Color::Rgb(0xc8, 0xc8, 0xd0),
        keyword:       fg(Color::Rgb(0x64, 0x8c, 0xb9)),
        builtin:       fg(Color::Rgb(0x78, 0xa5, 0xc8)),
        rstype:        fg(Color::Rgb(0xaa, 0x96, 0x87)),
        function:      fg(Color::Rgb(0x64, 0x8c, 0xb9)),
        lifetime:      fg(Color::Rgb(0x78, 0xa5, 0xc8)),
        string:        fg(Color::Rgb(0x8c, 0xaf, 0x7d)),
        fstring_prefix:fg_bold(Color::Rgb(0x8c, 0xaf, 0x7d)),
        comment:       fg(Color::Rgb(0x5f, 0x5f, 0x69)),
        number:        fg(Color::Rgb(0xc8, 0xaa, 0x7d)),
        decorator:     fg(Color::Rgb(0x64, 0x8c, 0xb9)),
        operator:      fg(Color::Rgb(0x5f, 0x5f, 0x69)),
        punctuation:   fg(Color::Rgb(0xc8, 0xc8, 0xd0)),
    }
}

fn opencode() -> Theme {
    Theme {
        fg:            Color::Rgb(0xee, 0xee, 0xee),
        bg:            Color::Rgb(0x0a, 0x0a, 0x0a),
        selection_bg:  Color::Rgb(0x14, 0x14, 0x14),
        line_number:   Style::new().fg(Color::Rgb(0x60, 0x60, 0x60)),
        tilde:         Style::new().fg(Color::Rgb(0x80, 0x80, 0x80)),
        status_bg:     Color::Rgb(0x9d, 0x7c, 0xd8),
        status_fg:     Color::Rgb(0xee, 0xee, 0xee),
        keyword:       fg(Color::Rgb(0x9d, 0x7c, 0xd8)),
        builtin:       fg(Color::Rgb(0x9d, 0x7c, 0xd8)),
        rstype:        fg(Color::Rgb(0xe5, 0xc0, 0x7b)),
        function:      fg(Color::Rgb(0xb2, 0x8c, 0xe6)),
        lifetime:      fg(Color::Rgb(0x9d, 0x7c, 0xd8)),
        string:        fg(Color::Rgb(0x7f, 0xd8, 0x8f)),
        fstring_prefix:fg_bold(Color::Rgb(0x7f, 0xd8, 0x8f)),
        comment:       fg(Color::Rgb(0x80, 0x80, 0x80)),
        number:        fg(Color::Rgb(0xf5, 0xa7, 0x42)),
        decorator:     fg(Color::Rgb(0xb2, 0x8c, 0xe6)),
        operator:      fg(Color::Rgb(0x9d, 0x7c, 0xd8)),
        punctuation:   fg(Color::Rgb(0xee, 0xee, 0xee)),
    }
}

fn ayu() -> Theme {
    Theme {
        fg:            Color::Rgb(0xbf, 0xba, 0xae),
        bg:            Color::Rgb(0x10, 0x10, 0x12),
        selection_bg:  Color::Rgb(0x19, 0x19, 0x1c),
        line_number:   Style::new().fg(Color::Rgb(0x6a, 0x67, 0x5f)),
        tilde:         Style::new().fg(Color::Rgb(0x6a, 0x67, 0x5f)),
        status_bg:     Color::Rgb(0x19, 0x19, 0x1c),
        status_fg:     Color::Rgb(0xbf, 0xba, 0xae),
        keyword:       fg(Color::Rgb(0xff, 0x8d, 0x52)),
        builtin:       fg(Color::Rgb(0x73, 0xb6, 0xd1)),
        rstype:        fg(Color::Rgb(0xff, 0xc6, 0x6d)),
        function:      fg(Color::Rgb(0xff, 0xc6, 0x6d)),
        lifetime:      fg(Color::Rgb(0x73, 0xb6, 0xd1)),
        string:        fg(Color::Rgb(0xa6, 0xd2, 0x70)),
        fstring_prefix:fg_bold(Color::Rgb(0xa6, 0xd2, 0x70)),
        comment:       fg(Color::Rgb(0x6a, 0x67, 0x5f)),
        number:        fg(Color::Rgb(0xd2, 0xaf, 0x73)),
        decorator:     fg(Color::Rgb(0xff, 0xb5, 0x6a)),
        operator:      fg(Color::Rgb(0xed, 0x93, 0x66)),
        punctuation:   fg(Color::Rgb(0xbf, 0xba, 0xae)),
    }
}

fn kanagawa() -> Theme {
    Theme {
        fg:            Color::Rgb(0xcd, 0xd0, 0xbe),
        bg:            Color::Rgb(0x1d, 0x20, 0x27),
        selection_bg:  Color::Rgb(0x25, 0x28, 0x2f),
        line_number:   Style::new().fg(Color::Rgb(0x53, 0x56, 0x5a)),
        tilde:         Style::new().fg(Color::Rgb(0x53, 0x56, 0x5a)),
        status_bg:     Color::Rgb(0x25, 0x28, 0x2f),
        status_fg:     Color::Rgb(0xcd, 0xd0, 0xbe),
        keyword:       fg(Color::Rgb(0xcb, 0x94, 0x8f)),
        builtin:       fg(Color::Rgb(0x8e, 0xa6, 0xb0)),
        rstype:        fg(Color::Rgb(0xe6, 0xb4, 0x80)),
        function:      fg(Color::Rgb(0xe5, 0xc0, 0x80)),
        lifetime:      fg(Color::Rgb(0x8e, 0xa6, 0xb0)),
        string:        fg(Color::Rgb(0x98, 0xbb, 0x79)),
        fstring_prefix:fg_bold(Color::Rgb(0x98, 0xbb, 0x79)),
        comment:       fg(Color::Rgb(0x53, 0x56, 0x5a)),
        number:        fg(Color::Rgb(0xdc, 0x9e, 0x80)),
        decorator:     fg(Color::Rgb(0xc7, 0x92, 0x8e)),
        operator:      fg(Color::Rgb(0x8e, 0xa6, 0xb0)),
        punctuation:   fg(Color::Rgb(0xcd, 0xd0, 0xbe)),
    }
}
