//! Colour themes for ked.
//!
//! Each theme defines a palette of styles used by the editor and the
//! syntax highlighter.  The [`Theme`] struct holds a colour for every
//! token / UI element.  [`ThemeKind`] is an enum over the five
//! built-in themes:
//!
//!   Default, Monokai, Solarized, Nord, Gruvbox
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

/// The six built-in theme variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Default,
    Monokai,
    Solarized,
    Nord,
    Gruvbox,
    Bi,
}

impl ThemeKind {
    /// Return the colour palette for this variant.
    pub fn theme(&self) -> Theme {
        match self {
            ThemeKind::Default   => default(),
            ThemeKind::Monokai   => monokai(),
            ThemeKind::Solarized => solarized(),
            ThemeKind::Nord      => nord(),
            ThemeKind::Gruvbox   => gruvbox(),
            ThemeKind::Bi        => bi(),
        }
    }

    /// Human-readable name for the `:themes` command.
    pub fn name(&self) -> &'static str {
        match self {
            ThemeKind::Default   => "default",
            ThemeKind::Monokai   => "monokai",
            ThemeKind::Solarized => "solarized",
            ThemeKind::Nord      => "nord",
            ThemeKind::Gruvbox   => "gruvbox",
            ThemeKind::Bi        => "bi",
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
        ]
    }

    /// Parse a case-insensitive theme name from a command.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "default"   => Some(ThemeKind::Default),
            "monokai"   => Some(ThemeKind::Monokai),
            "solarized" => Some(ThemeKind::Solarized),
            "nord"      => Some(ThemeKind::Nord),
            "gruvbox"   => Some(ThemeKind::Gruvbox),
            "bi"        => Some(ThemeKind::Bi),
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
