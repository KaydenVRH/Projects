//! Colour themes for ked.
//!
//! Each theme defines a palette of styles used by the editor and the
//! syntax highlighter.  The [`Theme`] struct holds a colour for every
//! token / UI element; palettes follow the canonical Neovim theme
//! colours (gruvbox, catppuccin, tokyonight, nord, dracula, …).
//!
//! [`ThemeKind`] is an enum over the nineteen built-in themes.  All
//! colours use ratatui's [`Color`] type, which supports named
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
    pub constant:      Style,   // `true`, `false`, `null`, `None`
    pub decorator:     Style,   // `@property`, `#[…]`
    pub property:      Style,   // struct fields, object properties
    pub operator:      Style,   // `==`, `+=`, `->`, …
    pub punctuation:   Style,   // `(`, `)`, `:`, `,`
}

/// The nineteen built-in theme variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Default,
    Monokai,
    Solarized,
    Nord,
    Gruvbox,
    Bi,
    BlueLagoon,
    Catppuccin,
    TokyoNight,
    Amber,
    Dracula,
    OneDark,
    Everforest,
    RosePine,
    Oxocarbon,
    Ayu,
    Kanagawa,
    Palenight,
    DarkPlus,
    Moonlight,
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
            ThemeKind::BlueLagoon => blue_lagoon(),
            ThemeKind::Catppuccin => catppuccin(),
            ThemeKind::TokyoNight => tokyo_night(),
            ThemeKind::Amber      => amber(),
            ThemeKind::Dracula    => dracula(),
            ThemeKind::OneDark    => one_dark(),
            ThemeKind::Everforest => everforest(),
            ThemeKind::RosePine   => rose_pine(),
            ThemeKind::Oxocarbon  => oxocarbon(),
            ThemeKind::Ayu        => ayu(),
            ThemeKind::Kanagawa   => kanagawa(),
            ThemeKind::Palenight  => palenight(),
            ThemeKind::DarkPlus   => dark_plus(),
            ThemeKind::Moonlight  => moonlight(),
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
            ThemeKind::BlueLagoon => "blue-lagoon",
            ThemeKind::Catppuccin => "catppuccin",
            ThemeKind::TokyoNight => "tokyonight",
            ThemeKind::Amber      => "amber",
            ThemeKind::Dracula    => "dracula",
            ThemeKind::OneDark    => "onedark",
            ThemeKind::Everforest => "everforest",
            ThemeKind::RosePine   => "rosepine",
            ThemeKind::Oxocarbon  => "oxocarbon",
            ThemeKind::Ayu        => "ayu",
            ThemeKind::Kanagawa   => "kanagawa",
            ThemeKind::Palenight  => "palenight",
            ThemeKind::DarkPlus   => "darkplus",
            ThemeKind::Moonlight  => "moonlight",
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
            ThemeKind::BlueLagoon,
            ThemeKind::Catppuccin,
            ThemeKind::TokyoNight,
            ThemeKind::Amber,
            ThemeKind::Dracula,
            ThemeKind::OneDark,
            ThemeKind::Everforest,
            ThemeKind::RosePine,
            ThemeKind::Oxocarbon,
            ThemeKind::Ayu,
            ThemeKind::Kanagawa,
            ThemeKind::Palenight,
            ThemeKind::DarkPlus,
            ThemeKind::Moonlight,
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
            "bluelagoon" | "blue-lagoon" | "blue_lagoon" => Some(ThemeKind::BlueLagoon),
            "catppuccin" => Some(ThemeKind::Catppuccin),
            "tokyonight" => Some(ThemeKind::TokyoNight),
            "amber"      => Some(ThemeKind::Amber),
            "dracula"    => Some(ThemeKind::Dracula),
            "onedark"    => Some(ThemeKind::OneDark),
            "everforest" => Some(ThemeKind::Everforest),
            "rosepine"   => Some(ThemeKind::RosePine),
            "oxocarbon"  => Some(ThemeKind::Oxocarbon),
            "ayu"        => Some(ThemeKind::Ayu),
            "kanagawa"   => Some(ThemeKind::Kanagawa),
            "palenight"  => Some(ThemeKind::Palenight),
            "darkplus"   => Some(ThemeKind::DarkPlus),
            "moonlight"  => Some(ThemeKind::Moonlight),
            _            => None,
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
    // VS Code Dark+ inspired: dark gray bg, blue keywords, warm strings
    Theme {
        fg:            Color::Rgb(0xd4, 0xd4, 0xd4),
        bg:            Color::Rgb(0x28, 0x28, 0x28),
        selection_bg:  Color::Rgb(0x3c, 0x3c, 0x3c),
        line_number:   Style::new().fg(Color::Rgb(0x6e, 0x6e, 0x6e)),
        tilde:         Style::new().fg(Color::Rgb(0x6e, 0x6e, 0x6e)),
        status_bg:     Color::Rgb(0x1e, 0x1e, 0x1e),
        status_fg:     Color::Rgb(0xd4, 0xd4, 0xd4),
        keyword:       fg(Color::Rgb(0x56, 0x9c, 0xd6)),  // blue
        builtin:       fg(Color::Rgb(0x4e, 0xc9, 0xb0)),  // teal
        rstype:        fg(Color::Rgb(0x4e, 0xc9, 0xb0)),  // teal
        function:      fg(Color::Rgb(0xdc, 0xdc, 0xaa)),  // yellow
        lifetime:      fg(Color::Rgb(0x4e, 0xc9, 0xb0)),
        string:        fg(Color::Rgb(0xce, 0x91, 0x78)),  // orange
        fstring_prefix:fg_bold(Color::Rgb(0xce, 0x91, 0x78)),
        comment:       fg(Color::Rgb(0x6a, 0x99, 0x56)),  // green
        number:        fg(Color::Rgb(0xb5, 0xce, 0xa8)),  // light green
        constant:      fg(Color::Rgb(0x4f, 0xc1, 0xff)),  // light blue
        decorator:     fg(Color::Rgb(0xdc, 0xdc, 0xaa)),
        property:      fg(Color::Rgb(0x9c, 0xdc, 0xfe)),  // VS Code property blue
        operator:      fg(Color::Rgb(0xd4, 0xd4, 0xd4)),
        punctuation:   fg(Color::Rgb(0xd4, 0xd4, 0xd4)),
    }
}

fn monokai() -> Theme {
    // Monokai: dark charcoal bg, bright pastels, clean
    Theme {
        fg:            Color::Rgb(0xf8, 0xf8, 0xf2),
        bg:            Color::Rgb(0x27, 0x28, 0x22),
        selection_bg:  Color::Rgb(0x49, 0x48, 0x3e),
        line_number:   Style::new().fg(Color::Rgb(0x90, 0x90, 0x80)),
        tilde:         Style::new().fg(Color::Rgb(0x3e, 0x3d, 0x32)),
        status_bg:     Color::Rgb(0x3e, 0x3d, 0x32),
        status_fg:     Color::Rgb(0xf8, 0xf8, 0xf2),
        keyword:       fg(Color::Rgb(0xf9, 0x26, 0x72)),  // hot pink
        builtin:       fg(Color::Rgb(0xae, 0x81, 0xff)),  // purple
        rstype:        fg(Color::Rgb(0x66, 0xd9, 0xef)),  // cyan
        function:      fg(Color::Rgb(0xa6, 0xe2, 0x2e)),  // lime green
        lifetime:      fg(Color::Rgb(0x66, 0xd9, 0xef)),  // cyan
        string:        fg(Color::Rgb(0xe6, 0xdb, 0x74)),  // warm yellow
        fstring_prefix:fg_bold(Color::Rgb(0xe6, 0xdb, 0x74)),
        comment:       fg(Color::Rgb(0x75, 0x71, 0x5e)),
        number:        fg(Color::Rgb(0xae, 0x81, 0xff)),  // purple
        constant:      fg(Color::Rgb(0xae, 0x81, 0xff)),  // purple
        decorator:     fg(Color::Rgb(0xa6, 0xe2, 0x2e)),  // green
        property:      fg(Color::Rgb(0xf8, 0xf8, 0xf2)),
        operator:      fg(Color::Rgb(0xf9, 0x26, 0x72)),  // pink
        punctuation:   fg(Color::Rgb(0xf8, 0xf8, 0xf2)),
    }
}

fn solarized() -> Theme {
    // Solarized Light: low-contrast, warm/cool balanced
    Theme {
        fg:            Color::Rgb(0x65, 0x7b, 0x83),
        bg:            Color::Rgb(0xfd, 0xf6, 0xe3),
        selection_bg:  Color::Rgb(0xee, 0xe8, 0xd5),
        line_number:   Style::new().fg(Color::Rgb(0x93, 0xa1, 0xa1)),
        tilde:         Style::new().fg(Color::Rgb(0x93, 0xa1, 0xa1)),
        status_bg:     Color::Rgb(0x07, 0x36, 0x42),
        status_fg:     Color::Rgb(0xfd, 0xf6, 0xe3),
        keyword:       fg(Color::Rgb(0x85, 0x99, 0x00)),  // green
        builtin:       fg(Color::Rgb(0x26, 0x8b, 0xd2)),  // blue
        rstype:        fg(Color::Rgb(0xb5, 0x89, 0x00)),  // yellow
        function:      fg(Color::Rgb(0x26, 0x8b, 0xd2)),  // blue
        lifetime:      fg(Color::Rgb(0x2a, 0xa1, 0x98)),  // cyan
        string:        fg(Color::Rgb(0x2a, 0xa1, 0x98)),  // cyan
        fstring_prefix:fg_bold(Color::Rgb(0x2a, 0xa1, 0x98)),
        comment:       fg(Color::Rgb(0x93, 0xa1, 0xa1)),
        number:        fg(Color::Rgb(0xd3, 0x36, 0x82)),  // magenta
        constant:      fg(Color::Rgb(0x2a, 0xa1, 0x98)),  // cyan
        decorator:     fg(Color::Rgb(0x6c, 0x71, 0xc4)),  // violet
        property:      fg(Color::Rgb(0x58, 0x6e, 0x75)),
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
        function:      fg(Color::Rgb(0x88, 0xc0, 0xd0)),  // light blue
        lifetime:      fg(Color::Rgb(0x88, 0xc0, 0xd0)),
        string:        fg(Color::Rgb(0xa3, 0xbe, 0x8c)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xa3, 0xbe, 0x8c)),
        comment:       fg(Color::Rgb(0x61, 0x6e, 0x88)),
        number:        fg(Color::Rgb(0xb4, 0x8e, 0xad)),  // purple
        constant:      fg(Color::Rgb(0x88, 0xc0, 0xd0)),  // light blue
        decorator:     fg(Color::Rgb(0xd0, 0x87, 0x70)),  // orange
        property:      fg(Color::Rgb(0xd8, 0xde, 0xe9)),
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
        rstype:        fg(Color::Rgb(0xfa, 0xbd, 0x2f)),  // yellow
        function:      fg(Color::Rgb(0x8e, 0xc0, 0x7c)),  // green
        lifetime:      fg(Color::Rgb(0x83, 0xa5, 0x98)),  // aqua
        string:        fg(Color::Rgb(0xb8, 0xbb, 0x26)),  // yellow-green
        fstring_prefix:fg_bold(Color::Rgb(0xb8, 0xbb, 0x26)),
        comment:       fg(Color::Rgb(0x92, 0x83, 0x74)),
        number:        fg(Color::Rgb(0xd3, 0x86, 0x9b)),  // magenta
        constant:      fg(Color::Rgb(0xd3, 0x86, 0x9b)),  // magenta
        decorator:     fg(Color::Rgb(0xfe, 0x80, 0x19)),  // orange
        property:      fg(Color::Rgb(0x83, 0xa5, 0x98)),  // aqua
        operator:      fg(Color::Rgb(0xfe, 0x80, 0x19)),  // orange
        punctuation:   fg(Color::Rgb(0xeb, 0xdb, 0xb2)),
    }
}

fn bi() -> Theme {
    // Deep midnight blue-purple bg, vibrant blue/purple/magenta text
    Theme {
        fg:            Color::Rgb(0xd4, 0xd8, 0xee),
        bg:            Color::Rgb(0x14, 0x11, 0x24),
        selection_bg:  Color::Rgb(0x2e, 0x28, 0x52),
        line_number:   Style::new().fg(Color::Rgb(0x5a, 0x52, 0x82)),
        tilde:         Style::new().fg(Color::Rgb(0x5a, 0x52, 0x82)),
        status_bg:     Color::Rgb(0x0e, 0x0c, 0x1a),
        status_fg:     Color::Rgb(0xc0, 0xa0, 0xf0),
        keyword:       fg(Color::Rgb(0xb0, 0x80, 0xf0)),  // bright purple
        builtin:       fg(Color::Rgb(0x68, 0xa8, 0xf8)),  // vivid blue
        rstype:        fg(Color::Rgb(0xf0, 0x80, 0xc0)),  // magenta
        function:      fg(Color::Rgb(0x68, 0xa8, 0xf8)),  // blue
        lifetime:      fg(Color::Rgb(0x88, 0xc8, 0xf8)),  // light blue
        string:        fg(Color::Rgb(0x90, 0xd0, 0xc0)),  // teal
        fstring_prefix:fg_bold(Color::Rgb(0x90, 0xd0, 0xc0)),
        comment:       fg(Color::Rgb(0x5a, 0x52, 0x82)),
        number:        fg(Color::Rgb(0xf0, 0xa0, 0xd0)),  // pink
        constant:      fg(Color::Rgb(0xf0, 0xa0, 0xd0)),  // pink
        decorator:     fg(Color::Rgb(0xb0, 0x80, 0xf0)),  // purple
        property:      fg(Color::Rgb(0x68, 0xa8, 0xf8)),  // blue
        operator:      fg(Color::Rgb(0xc0, 0xa0, 0xf0)),
        punctuation:   fg(Color::Rgb(0xd4, 0xd8, 0xee)),
    }
}

fn blue_lagoon() -> Theme {
    // Blue Lagoon (kayden's kitty theme): baby-blue accents, mint
    // strings, peach numbers on a deep blue-black base.
    Theme {
        fg:            Color::Rgb(0xe8, 0xf0, 0xf8),
        bg:            Color::Rgb(0x0e, 0x14, 0x19),
        selection_bg:  Color::Rgb(0x1a, 0x2a, 0x38),
        line_number:   Style::new().fg(Color::Rgb(0x55, 0x70, 0x8c)),
        tilde:         Style::new().fg(Color::Rgb(0x26, 0x38, 0x4a)),
        status_bg:     Color::Rgb(0x14, 0x1e, 0x28),
        status_fg:     Color::Rgb(0xe8, 0xf0, 0xf8),
        keyword:       fg(Color::Rgb(0x6b, 0xa3, 0xff)),  // blue
        builtin:       fg(Color::Rgb(0x7e, 0xcf, 0xd4)),  // teal
        rstype:        fg(Color::Rgb(0x89, 0xcf, 0xf0)),  // baby blue
        function:      fg(Color::Rgb(0x8b, 0xba, 0xff)),  // light blue
        lifetime:      fg(Color::Rgb(0x7e, 0xcf, 0xd4)),
        string:        fg(Color::Rgb(0xa8, 0xe6, 0xcf)),  // mint
        fstring_prefix:fg_bold(Color::Rgb(0xa8, 0xe6, 0xcf)),
        comment:       fg(Color::Rgb(0x56, 0x71, 0x8c)),  // muted slate
        number:        fg(Color::Rgb(0xff, 0xd3, 0xb6)),  // peach
        constant:      fg(Color::Rgb(0xff, 0xd3, 0xb6)),  // peach
        decorator:     fg(Color::Rgb(0xc3, 0xae, 0xdb)),  // lavender
        property:      fg(Color::Rgb(0x89, 0xcf, 0xf0)),  // baby blue
        operator:      fg(Color::Rgb(0xb9, 0xca, 0xdb)),
        punctuation:   fg(Color::Rgb(0x8b, 0xa0, 0xb0)),
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
        function:      fg(Color::Rgb(0x89, 0xb4, 0xfa)),  // blue
        lifetime:      fg(Color::Rgb(0x94, 0xe2, 0xd5)),  // teal
        string:        fg(Color::Rgb(0xa6, 0xe3, 0xa1)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xa6, 0xe3, 0xa1)),
        comment:       fg(Color::Rgb(0x6c, 0x70, 0x86)),  // overlay1
        number:        fg(Color::Rgb(0xfa, 0xb3, 0x87)),  // peach
        constant:      fg(Color::Rgb(0xfa, 0xb3, 0x87)),  // peach
        decorator:     fg(Color::Rgb(0xf3, 0x8b, 0xa8)),  // red
        property:      fg(Color::Rgb(0xb4, 0xbe, 0xfe)),  // lavender
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
        function:      fg(Color::Rgb(0x7a, 0xa2, 0xf7)),  // blue
        lifetime:      fg(Color::Rgb(0x7d, 0xcf, 0xff)),  // cyan
        string:        fg(Color::Rgb(0x9e, 0xce, 0x6a)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0x9e, 0xce, 0x6a)),
        comment:       fg(Color::Rgb(0x56, 0x5f, 0x89)),
        number:        fg(Color::Rgb(0xff, 0x9e, 0x64)),  // orange
        constant:      fg(Color::Rgb(0xff, 0x9e, 0x64)),  // orange
        decorator:     fg(Color::Rgb(0xf7, 0x76, 0x8e)),  // red
        property:      fg(Color::Rgb(0x7a, 0xa2, 0xf7)),  // blue
        operator:      fg(Color::Rgb(0x7d, 0xcf, 0xff)),  // cyan
        punctuation:   fg(Color::Rgb(0xc0, 0xca, 0xf5)),
    }
}

fn amber() -> Theme {
    // Amber Amethyst: deep purple base, purple accents, amber highlights
    Theme {
        fg:            Color::Rgb(0xe2, 0xe8, 0xf0),  // white
        bg:            Color::Rgb(0x1e, 0x1a, 0x2e),  // deep purple
        selection_bg:  Color::Rgb(0xa8, 0x55, 0xf7),  // purple
        line_number:   Style::new().fg(Color::Rgb(0x88, 0x80, 0xa0)),
        tilde:         Style::new().fg(Color::Rgb(0x88, 0x80, 0xa0)),
        status_bg:     Color::Rgb(0x2a, 0x24, 0x40),  // darker purple
        status_fg:     Color::Rgb(0xfb, 0xbf, 0x24),  // amber
        keyword:       fg(Color::Rgb(0xa8, 0x55, 0xf7)),  // purple
        builtin:       fg(Color::Rgb(0x81, 0x8c, 0xf8)),  // blue
        rstype:        fg(Color::Rgb(0x22, 0xd3, 0xee)),  // cyan
        function:      fg(Color::Rgb(0x81, 0x8c, 0xf8)),  // blue
        lifetime:      fg(Color::Rgb(0x22, 0xd3, 0xee)),  // cyan
        string:        fg(Color::Rgb(0x34, 0xd3, 0x99)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0x34, 0xd3, 0x99)),
        comment:       fg(Color::Rgb(0x88, 0x80, 0xa0)),
        number:        fg(Color::Rgb(0xfb, 0xbf, 0x24)),  // amber
        constant:      fg(Color::Rgb(0xfb, 0xbf, 0x24)),  // amber
        decorator:     fg(Color::Rgb(0xa8, 0x55, 0xf7)),  // purple
        property:      fg(Color::Rgb(0x81, 0x8c, 0xf8)),  // blue
        operator:      fg(Color::Rgb(0xe2, 0xe8, 0xf0)),
        punctuation:   fg(Color::Rgb(0xe2, 0xe8, 0xf0)),
    }
}

fn dracula() -> Theme {
    Theme {
        fg:            Color::Rgb(0xf8, 0xf8, 0xf2),
        bg:            Color::Rgb(0x28, 0x2a, 0x36),
        selection_bg:  Color::Rgb(0x44, 0x47, 0x5a),
        line_number:   Style::new().fg(Color::Rgb(0x62, 0x72, 0xa4)),
        tilde:         Style::new().fg(Color::Rgb(0x62, 0x72, 0xa4)),
        status_bg:     Color::Rgb(0x62, 0x72, 0xa4),
        status_fg:     Color::Rgb(0xf8, 0xf8, 0xf2),
        keyword:       fg(Color::Rgb(0xff, 0x79, 0xc6)),  // pink
        builtin:       fg(Color::Rgb(0x8b, 0xe9, 0xfd)),  // cyan
        rstype:        fg(Color::Rgb(0x8b, 0xe9, 0xfd)),  // cyan
        function:      fg(Color::Rgb(0x50, 0xfa, 0x7b)),  // green
        lifetime:      fg(Color::Rgb(0x8b, 0xe9, 0xfd)),
        string:        fg(Color::Rgb(0xf1, 0xfa, 0x8c)),  // yellow
        fstring_prefix:fg_bold(Color::Rgb(0xf1, 0xfa, 0x8c)),
        comment:       fg(Color::Rgb(0x62, 0x72, 0xa4)),
        number:        fg(Color::Rgb(0xbd, 0x93, 0xf9)),  // purple
        constant:      fg(Color::Rgb(0xbd, 0x93, 0xf9)),  // purple
        decorator:     fg(Color::Rgb(0xff, 0x79, 0xc6)),  // pink
        property:      fg(Color::Rgb(0x8b, 0xe9, 0xfd)),  // cyan
        operator:      fg(Color::Rgb(0xff, 0x79, 0xc6)),  // pink
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
        keyword:       fg(Color::Rgb(0xc6, 0x78, 0xdd)),  // purple
        builtin:       fg(Color::Rgb(0x61, 0xaf, 0xef)),  // blue
        rstype:        fg(Color::Rgb(0xe5, 0xc0, 0x7b)),  // yellow
        function:      fg(Color::Rgb(0x61, 0xaf, 0xef)),  // blue
        lifetime:      fg(Color::Rgb(0x56, 0xb6, 0xc2)),  // cyan
        string:        fg(Color::Rgb(0x98, 0xc3, 0x79)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0x98, 0xc3, 0x79)),
        comment:       fg(Color::Rgb(0x5c, 0x63, 0x70)),
        number:        fg(Color::Rgb(0xd1, 0x9a, 0x66)),  // orange
        constant:      fg(Color::Rgb(0xd1, 0x9a, 0x66)),  // orange
        decorator:     fg(Color::Rgb(0xe5, 0xc0, 0x7b)),  // yellow
        property:      fg(Color::Rgb(0xe0, 0x6c, 0x75)),  // red
        operator:      fg(Color::Rgb(0x56, 0xb6, 0xc2)),  // cyan
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
        keyword:       fg(Color::Rgb(0xe6, 0x7e, 0x80)),  // red
        builtin:       fg(Color::Rgb(0x7f, 0xbb, 0xb3)),  // teal
        rstype:        fg(Color::Rgb(0xd6, 0x99, 0x9c)),  // pink-red
        function:      fg(Color::Rgb(0xa7, 0xc0, 0x80)),  // green
        lifetime:      fg(Color::Rgb(0x7f, 0xbb, 0xb3)),  // teal
        string:        fg(Color::Rgb(0xa7, 0xc0, 0x80)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xa7, 0xc0, 0x80)),
        comment:       fg(Color::Rgb(0x7d, 0x83, 0x79)),
        number:        fg(Color::Rgb(0xdf, 0xb8, 0x69)),  // yellow
        constant:      fg(Color::Rgb(0xdf, 0xb8, 0x69)),  // yellow
        decorator:     fg(Color::Rgb(0xe6, 0x98, 0x75)),  // orange
        property:      fg(Color::Rgb(0xd3, 0xc6, 0xaa)),
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
        keyword:       fg(Color::Rgb(0xc4, 0xa7, 0xe7)),  // iris
        builtin:       fg(Color::Rgb(0x9c, 0xcf, 0xd8)),  // foam
        rstype:        fg(Color::Rgb(0xeb, 0xbc, 0xba)),  // rose
        function:      fg(Color::Rgb(0xea, 0x9a, 0x97)),  // rose
        lifetime:      fg(Color::Rgb(0x9c, 0xcf, 0xd8)),  // foam
        string:        fg(Color::Rgb(0x9c, 0xcf, 0xd8)),  // foam
        fstring_prefix:fg_bold(Color::Rgb(0x9c, 0xcf, 0xd8)),
        comment:       fg(Color::Rgb(0x6f, 0x6a, 0x85)),
        number:        fg(Color::Rgb(0xea, 0x9a, 0x97)),  // rose
        constant:      fg(Color::Rgb(0xeb, 0x6f, 0x92)),  // love
        decorator:     fg(Color::Rgb(0xc4, 0xa7, 0xe7)),  // iris
        property:      fg(Color::Rgb(0xf6, 0xc1, 0x77)),  // gold
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
        keyword:       fg(Color::Rgb(0xbe, 0x95, 0xff)),  // purple
        builtin:       fg(Color::Rgb(0x33, 0xb0, 0xff)),  // blue
        rstype:        fg(Color::Rgb(0x08, 0xbd, 0xae)),  // teal
        function:      fg(Color::Rgb(0xbe, 0x95, 0xff)),  // purple
        lifetime:      fg(Color::Rgb(0x33, 0xb0, 0xff)),
        string:        fg(Color::Rgb(0x42, 0xbe, 0xa6)),  // teal
        fstring_prefix:fg_bold(Color::Rgb(0x42, 0xbe, 0xa6)),
        comment:       fg(Color::Rgb(0x52, 0x52, 0x52)),
        number:        fg(Color::Rgb(0xff, 0x7e, 0xb6)),  // pink
        constant:      fg(Color::Rgb(0xff, 0x7e, 0xb6)),  // pink
        decorator:     fg(Color::Rgb(0xbe, 0x95, 0xff)),  // purple
        property:      fg(Color::Rgb(0x33, 0xb0, 0xff)),  // blue
        operator:      fg(Color::Rgb(0xbe, 0x95, 0xff)),  // purple
        punctuation:   fg(Color::Rgb(0xd2, 0xd1, 0xd2)),
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
        keyword:       fg(Color::Rgb(0xff, 0x8d, 0x52)),  // orange
        builtin:       fg(Color::Rgb(0x73, 0xb6, 0xd1)),  // blue
        rstype:        fg(Color::Rgb(0xff, 0xc6, 0x6d)),  // yellow
        function:      fg(Color::Rgb(0xff, 0xc6, 0x6d)),  // yellow
        lifetime:      fg(Color::Rgb(0x73, 0xb6, 0xd1)),
        string:        fg(Color::Rgb(0xa6, 0xd2, 0x70)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xa6, 0xd2, 0x70)),
        comment:       fg(Color::Rgb(0x6a, 0x67, 0x5f)),
        number:        fg(Color::Rgb(0xd2, 0xaf, 0x73)),  // orange-yellow
        constant:      fg(Color::Rgb(0xd2, 0xaf, 0x73)),
        decorator:     fg(Color::Rgb(0xff, 0xb5, 0x6a)),  // orange
        property:      fg(Color::Rgb(0xbf, 0xba, 0xae)),
        operator:      fg(Color::Rgb(0xed, 0x93, 0x66)),  // orange
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
        keyword:       fg(Color::Rgb(0xcb, 0x94, 0x8f)),  // red
        builtin:       fg(Color::Rgb(0x8e, 0xa6, 0xb0)),  // blue
        rstype:        fg(Color::Rgb(0xe6, 0xb4, 0x80)),  // yellow
        function:      fg(Color::Rgb(0xe5, 0xc0, 0x80)),  // yellow
        lifetime:      fg(Color::Rgb(0x8e, 0xa6, 0xb0)),
        string:        fg(Color::Rgb(0x98, 0xbb, 0x79)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0x98, 0xbb, 0x79)),
        comment:       fg(Color::Rgb(0x53, 0x56, 0x5a)),
        number:        fg(Color::Rgb(0xdc, 0x9e, 0x80)),  // orange
        constant:      fg(Color::Rgb(0xdc, 0x9e, 0x80)),  // orange
        decorator:     fg(Color::Rgb(0xc7, 0x92, 0x8e)),  // red
        property:      fg(Color::Rgb(0xe6, 0xb4, 0x80)),  // yellow
        operator:      fg(Color::Rgb(0x8e, 0xa6, 0xb0)),  // blue
        punctuation:   fg(Color::Rgb(0xcd, 0xd0, 0xbe)),
    }
}

fn palenight() -> Theme {
    // Material Palenight
    Theme {
        fg:            Color::Rgb(0xbf, 0xc7, 0xd5),
        bg:            Color::Rgb(0x29, 0x2d, 0x3e),
        selection_bg:  Color::Rgb(0x3e, 0x43, 0x57),
        line_number:   Style::new().fg(Color::Rgb(0x67, 0x6e, 0x95)),
        tilde:         Style::new().fg(Color::Rgb(0x67, 0x6e, 0x95)),
        status_bg:     Color::Rgb(0x1e, 0x21, 0x30),
        status_fg:     Color::Rgb(0xbf, 0xc7, 0xd5),
        keyword:       fg(Color::Rgb(0xc7, 0x92, 0xea)),  // purple
        builtin:       fg(Color::Rgb(0x82, 0xaa, 0xff)),  // blue
        rstype:        fg(Color::Rgb(0xff, 0xcb, 0x6b)),  // yellow
        function:      fg(Color::Rgb(0x82, 0xaa, 0xff)),  // blue
        lifetime:      fg(Color::Rgb(0x89, 0xdd, 0xff)),  // cyan
        string:        fg(Color::Rgb(0xc3, 0xe8, 0x8d)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xc3, 0xe8, 0x8d)),
        comment:       fg(Color::Rgb(0x67, 0x6e, 0x95)),
        number:        fg(Color::Rgb(0xf7, 0x8c, 0x6c)),  // orange
        constant:      fg(Color::Rgb(0xf7, 0x8c, 0x6c)),  // orange
        decorator:     fg(Color::Rgb(0xc7, 0x92, 0xea)),  // purple
        property:      fg(Color::Rgb(0x82, 0xaa, 0xff)),  // blue
        operator:      fg(Color::Rgb(0x89, 0xdd, 0xff)),  // cyan
        punctuation:   fg(Color::Rgb(0xbf, 0xc7, 0xd5)),
    }
}

fn dark_plus() -> Theme {
    // VS Code Dark+ default
    Theme {
        fg:            Color::Rgb(0xd4, 0xd4, 0xd4),
        bg:            Color::Rgb(0x1e, 0x1e, 0x1e),
        selection_bg:  Color::Rgb(0x26, 0x4f, 0x78),
        line_number:   Style::new().fg(Color::Rgb(0x85, 0x85, 0x85)),
        tilde:         Style::new().fg(Color::Rgb(0x6a, 0x6a, 0x6a)),
        status_bg:     Color::Rgb(0x00, 0x73, 0x3e),
        status_fg:     Color::Rgb(0xff, 0xff, 0xff),
        keyword:       fg(Color::Rgb(0x56, 0x9c, 0xd6)),  // blue
        builtin:       fg(Color::Rgb(0xdc, 0xdc, 0xaa)),  // yellow
        rstype:        fg(Color::Rgb(0x4e, 0xc9, 0xb0)),  // teal
        function:      fg(Color::Rgb(0xdc, 0xdc, 0xaa)),  // yellow
        lifetime:      fg(Color::Rgb(0x4e, 0xc9, 0xb0)),
        string:        fg(Color::Rgb(0xce, 0x91, 0x78)),  // orange
        fstring_prefix:fg_bold(Color::Rgb(0xce, 0x91, 0x78)),
        comment:       fg(Color::Rgb(0x6a, 0x99, 0x56)),  // green
        number:        fg(Color::Rgb(0xb5, 0xce, 0xa8)),  // light green
        constant:      fg(Color::Rgb(0x4f, 0xc1, 0xff)),  // light blue
        decorator:     fg(Color::Rgb(0xdc, 0xdc, 0xaa)),
        property:      fg(Color::Rgb(0x9c, 0xdc, 0xfe)),  // light blue
        operator:      fg(Color::Rgb(0xd4, 0xd4, 0xd4)),
        punctuation:   fg(Color::Rgb(0xd4, 0xd4, 0xd4)),
    }
}

fn moonlight() -> Theme {
    Theme {
        fg:            Color::Rgb(0xc8, 0xd3, 0xf5),
        bg:            Color::Rgb(0x22, 0x24, 0x36),
        selection_bg:  Color::Rgb(0x36, 0x38, 0x4e),
        line_number:   Style::new().fg(Color::Rgb(0x63, 0x65, 0x7e)),
        tilde:         Style::new().fg(Color::Rgb(0x63, 0x65, 0x7e)),
        status_bg:     Color::Rgb(0x19, 0x1a, 0x2a),
        status_fg:     Color::Rgb(0xc8, 0xd3, 0xf5),
        keyword:       fg(Color::Rgb(0xc7, 0x90, 0xe8)),  // purple
        builtin:       fg(Color::Rgb(0x82, 0xaa, 0xff)),  // blue
        rstype:        fg(Color::Rgb(0xff, 0xcc, 0x66)),  // yellow
        function:      fg(Color::Rgb(0x82, 0xaa, 0xff)),  // blue
        lifetime:      fg(Color::Rgb(0x86, 0xe1, 0xfc)),  // cyan
        string:        fg(Color::Rgb(0xc3, 0xe8, 0x8d)),  // green
        fstring_prefix:fg_bold(Color::Rgb(0xc3, 0xe8, 0x8d)),
        comment:       fg(Color::Rgb(0x63, 0x65, 0x7e)),
        number:        fg(Color::Rgb(0xff, 0x96, 0x6c)),  // orange
        constant:      fg(Color::Rgb(0xff, 0x96, 0x6c)),  // orange
        decorator:     fg(Color::Rgb(0xc7, 0x90, 0xe8)),  // purple
        property:      fg(Color::Rgb(0x82, 0xaa, 0xff)),  // blue
        operator:      fg(Color::Rgb(0x86, 0xe1, 0xfc)),  // cyan
        punctuation:   fg(Color::Rgb(0xc8, 0xd3, 0xf5)),
    }
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
