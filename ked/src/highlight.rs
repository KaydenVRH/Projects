//! Syntax highlighting for ked.
//!
//! Supports Python (`.py`) and Rust (`.rs`).  The [`highlight_line`]
//! function dispatches to the appropriate tokeniser based on [`Lang`].
//!
//! Token types supported:
//!   - Keywords:  `if`, `else`, `fn`, `let`, `match`, …
//!   - Builtins:  `print`, `len`, `String`, `Vec`, `i32`, …
//!   - Strings:   single/double quoted, raw, byte, f-strings (Python)
//!   - Comments:  `#` / `//` … end of line
//!   - Numbers:   integers, floats, hex/oct/bin literals
//!   - Decorators:`@identifier`, `#[…]`
//!   - Operators: `==`, `!=`, `<=`, `>=`, `+=`, `-=`, …

use ratatui::style::Style;
use ratatui::text::Span;

use crate::theme::Theme;

/// A single token from a line of Python source.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

///Programming language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Python,
    Rust,
    Conf,
    Plain,
}

/// What kind of token this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Builtin,
    String,
    FStringPrefix, // the `f` before an f-string
    Comment,
    Number,
    Decorator,
    Operator,
    Punctuation,
}

/// The set of Python keywords.
const PY_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await",
    "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "while", "with", "yield",
];

/// The set of Python built-in functions.
const PY_BUILTINS: &[&str] = &[
    "abs", "all", "any", "bin", "bool", "bytearray", "bytes", "chr",
    "complex", "dict", "dir", "divmod", "enumerate", "eval", "exec",
    "filter", "float", "format", "frozenset", "getattr", "globals",
    "hasattr", "hash", "hex", "id", "input", "int", "isinstance",
    "issubclass", "iter", "len", "list", "locals", "map", "max",
    "memoryview", "min", "next", "object", "oct", "open", "ord",
    "pow", "print", "property", "range", "repr", "reversed", "round",
    "set", "setattr", "slice", "sorted", "staticmethod", "str",
    "sum", "super", "tuple", "type", "vars", "zip", "__import__",
];

/// Rust keywords (edition-2021 stable).
const RS_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn",
    "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

/// Rust built-in / std-prelude types and common std items.
const RS_BUILTINS: &[&str] = &[
    "bool", "char", "str",
    "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize",
    "f32", "f64",
    "String", "Vec", "Option", "Result", "Box", "Arc", "Rc",
    "Cell", "RefCell", "Cow", "HashMap", "HashSet",
    "VecDeque", "LinkedList", "BTreeMap", "BTreeSet",
    "Path", "PathBuf", "OsString", "OsStr", "CString", "CStr",
    "Mutex", "RwLock", "Duration", "Ordering",
    "Iterator", "IntoIterator", "FromIterator",
    "Clone", "Copy", "Debug", "Display", "Default",
    "Eq", "PartialEq", "Ord", "PartialOrd", "Hash",
    "From", "Into", "TryFrom", "TryInto",
    "Deref", "DerefMut", "Drop",
    "Send", "Sync", "Sized",
    "Fn", "FnMut", "FnOnce",
    "Some", "None", "Ok", "Err",
];

/// Tokenise a Python line into coloured spans.
///
/// This is a simple character-by-character scanner.  It does NOT
/// handle multi-line strings or f-string interpolation inside braces.
pub fn highlight_line(line: &str, theme: &Theme, lang: Lang) -> Vec<Span<'static>> {
    let tokens = match lang {
        Lang::Python => tokenize_python(line),
        Lang::Rust => tokenize_rust(line),
        Lang::Conf => tokenize_conf(line),
        Lang::Plain => vec![Token { kind: TokenKind::Plain, text: line.to_string() }],
    };
    tokens
        .into_iter()
        .map(|t| {
            let style = style_for_kind(t.kind, theme);
            Span::styled(t.text, style)
        })
        .collect()
}

/// Map a token kind to the theme style for that kind.
fn style_for_kind(kind: TokenKind, theme: &Theme) -> Style {
    match kind {
        TokenKind::Plain        => Style::new().fg(theme.fg),
        TokenKind::Keyword      => theme.keyword,
        TokenKind::Builtin      => theme.builtin,
        TokenKind::String       => theme.string,
        TokenKind::FStringPrefix=> theme.fstring_prefix,
        TokenKind::Comment      => theme.comment,
        TokenKind::Number       => theme.number,
        TokenKind::Decorator    => theme.decorator,
        TokenKind::Operator     => theme.operator,
        TokenKind::Punctuation  => theme.punctuation,
    }
}

// ── tokeniser ────────────────────────────────────────────────────

/// Split a Python line into tokens using a simple state-machine.
fn tokenize_python(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // ── comment: '#' colours the rest of the line ──
        if ch == '#' {
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[i..].iter().collect(),
            });
            break;
        }

        // ── string literals ──
        // Detect f-strings: `f"..."`, `f'...'`, `F"..."`, `rf"..."`, etc.
        if (ch == 'f' || ch == 'F' || ch == 'r' || ch == 'R' || ch == 'b' || ch == 'B')
            && i + 1 < n
        {
            let next = chars[i + 1];
            if next == '"' || next == '\'' {
                // prefix character (f/r/b)
                let prefix_len = if ch == 'r' || ch == 'R' { 1 } else { 1 };
                let is_fstring = ch == 'f' || ch == 'F';
                tokens.push(Token {
                    kind: if is_fstring { TokenKind::FStringPrefix } else { TokenKind::Plain },
                    text: chars[i..i + prefix_len].iter().collect(),
                });
                i += prefix_len;
                // fall through to string parsing below
            }
        }

        // ── quoted strings ──
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut end = i + 1;
            while end < n {
                if chars[end] == quote && (end == 0 || chars[end - 1] != '\\') {
                    end += 1;
                    break;
                }
                end += 1;
            }
            tokens.push(Token {
                kind: TokenKind::String,
                text: chars[i..end].iter().collect(),
            });
            i = end;
            continue;
        }

        // ── numbers (int, float, hex, oct, bin) ──
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < n && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            if ch == '0' && i + 1 < n {
                let nxt = chars[i + 1];
                if nxt == 'x' || nxt == 'X' || nxt == 'o' || nxt == 'O'
                    || nxt == 'b' || nxt == 'B'
                {
                    i += 2;
                    while i < n && chars[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    tokens.push(Token {
                        kind: TokenKind::Number,
                        text: chars[start..i].iter().collect(),
                    });
                    continue;
                }
            }
            while i < n && (chars[i].is_ascii_digit() || chars[i] == '.'
                || chars[i] == 'e' || chars[i] == 'E'
                || chars[i] == 'x' || chars[i] == 'X'
                || chars[i] == 'o' || chars[i] == 'O'
                || chars[i] == 'b' || chars[i] == 'B'
                || chars[i] == '_')
            {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── decorator: @identifier ──
        if ch == '@' {
            let start = i;
            i += 1;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Decorator,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── multi-char operators ──
        if i + 1 < n {
            let two = format!("{}{}", ch, chars[i + 1]);
            let op2 = matches!(
                two.as_str(),
                "==" | "!=" | "<=" | ">=" | "->" | "//"
                    | "**" | "+=" | "-=" | "*=" | "/=" | "%="
                    | "&=" | "|=" | "^=" | ">>" | "<<" | "//="
                    | "**=" | ">>=" | "<<=" | "::"
            );
            if op2 {
                tokens.push(Token {
                    kind: TokenKind::Operator,
                    text: two,
                });
                i += 2;
                continue;
            }
        }

        // ── single-char operators ──
        if matches!(
            ch,
            '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>'
                | '&' | '|' | '^' | '~' | ':' | '.'
        ) {
            tokens.push(Token {
                kind: TokenKind::Operator,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── punctuation ──
        if matches!(
            ch,
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '\\' | '@'
        ) {
            tokens.push(Token {
                kind: TokenKind::Punctuation,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── identifiers / words (may be keyword or builtin) ──
        if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = if PY_KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if PY_BUILTINS.contains(&word.as_str()) {
                TokenKind::Builtin
            } else {
                TokenKind::Plain
            };
            tokens.push(Token { kind, text: word });
            continue;
        }

        // ── anything else: whitespace, non-ASCII, etc. ──
        let start = i;
        i += 1;
        tokens.push(Token {
            kind: TokenKind::Plain,
            text: chars[start..i].iter().collect(),
        });
    }

    tokens
}

// ── Rust tokeniser ────────────────────────────────────────────────

/// Split a Rust line into tokens.
fn tokenize_rust(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // ── comment: `//` colours the rest of the line ──
        if ch == '/' && i + 1 < n && chars[i + 1] == '/' {
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[i..].iter().collect(),
            });
            break;
        }

        // ── string literals: `"..."`, raw `r"..."`, `r#"..."#` ──
        if ch == 'r' || ch == 'R' {
            // raw string: r"...", r#"..."#, r##"..."##, etc.
            let saved = i;
            i += 1; // skip 'r'
            let mut hash_count = 0;
            while i < n && chars[i] == '#' {
                hash_count += 1;
                i += 1;
            }
            if i < n && chars[i] == '"' {
                i += 1;
                while i < n {
                    if chars[i] == '"' {
                        // check that the closing hashes match
                        let mut close_hashes = 0;
                        while close_hashes < hash_count
                            && i + 1 + close_hashes < n
                            && chars[i + 1 + close_hashes] == '#'
                        {
                            close_hashes += 1;
                        }
                        if close_hashes == hash_count {
                            i += 1 + close_hashes;
                            break;
                        }
                    }
                    i += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::String,
                    text: chars[saved..i].iter().collect(),
                });
                continue;
            }
            // Not a raw string — backtrack.
            i = saved;
        }

        if ch == '"' {
            let start = i;
            i += 1;
            while i < n {
                if chars[i] == '\\' && i + 1 < n {
                    i += 2; // skip escaped char
                    continue;
                }
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::String,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── char literal: `'x'`, `'\n'` ──
        if ch == '\'' && i + 2 < n {
            let start = i;
            i += 1;
            if chars[i] == '\\' && i + 1 < n {
                i += 2; // escaped char
            } else {
                i += 1;
            }
            if i < n && chars[i] == '\'' {
                i += 1;
                tokens.push(Token {
                    kind: TokenKind::String,
                    text: chars[start..i].iter().collect(),
                });
                continue;
            }
            // Not a char literal — backtrack (might be lifetime).
            i = start + 1;
        }

        // ── attribute: `#[...]` or `#![...]` ──
        if ch == '#' && i + 1 < n
            && (chars[i + 1] == '[' || chars[i + 1] == '!')
        {
            let start = i;
            i += 1;
            if chars[i] == '!' { i += 1; }
            let mut depth = 1;
            while i < n && depth > 0 {
                if chars[i] == ']' { depth -= 1; }
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Decorator,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── numbers ──
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < n && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            if ch == '0' && i + 1 < n {
                let nxt = chars[i + 1];
                if nxt == 'x' || nxt == 'X' || nxt == 'o' || nxt == 'O'
                    || nxt == 'b' || nxt == 'B'
                {
                    i += 2;
                    while i < n && (chars[i].is_ascii_hexdigit() || chars[i] == '_') {
                        i += 1;
                    }
                    tokens.push(Token {
                        kind: TokenKind::Number,
                        text: chars[start..i].iter().collect(),
                    });
                    continue;
                }
            }
            while i < n
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == 'e' || chars[i] == 'E'
                    || chars[i] == '_')
            {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── two-char operators (Rust) ──
        if i + 1 < n {
            let two = format!("{}{}", ch, chars[i + 1]);
            let op2 = matches!(
                two.as_str(),
                "==" | "!=" | "<=" | ">=" | "&&" | "||"
                    | "->" | "=>" | ".." | "::"
                    | "+=" | "-=" | "*=" | "/=" | "%="
                    | "&=" | "|=" | "^=" | "<<"
                    | ">>"
            );
            if op2 {
                tokens.push(Token {
                    kind: TokenKind::Operator,
                    text: two,
                });
                i += 2;
                continue;
            }
        }

        // ── three-char operators ──
        if i + 2 < n {
            let three = format!("{}{}{}", ch, chars[i + 1], chars[i + 2]);
            let op3 = matches!(
                three.as_str(),
                ">>>" | "<<=" | ">>="
            );
            if op3 {
                tokens.push(Token {
                    kind: TokenKind::Operator,
                    text: three,
                });
                i += 3;
                continue;
            }
        }

        // ── single-char operators ──
        if matches!(
            ch,
            '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>'
                | '&' | '|' | '^' | '~' | ':' | '.' | '@'
        ) {
            // ':' followed by ':' is handled by the two-char check above
            tokens.push(Token {
                kind: TokenKind::Operator,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── punctuation ──
        if matches!(
            ch,
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '\\'
        ) {
            tokens.push(Token {
                kind: TokenKind::Punctuation,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── identifiers / words (keyword, builtin, or macro) ──
        if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // Check for macro call: ident!
            let is_macro = i < n && chars[i] == '!';
            let kind = if is_macro {
                TokenKind::Builtin
            } else if RS_KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if RS_BUILTINS.contains(&word.as_str()) {
                TokenKind::Builtin
            } else {
                TokenKind::Plain
            };
            tokens.push(Token { kind, text: word });
            if is_macro {
                // include the '!'
                tokens.push(Token {
                    kind: TokenKind::Operator,
                    text: "!".to_string(),
                });
                i += 1;
            }
            continue;
        }

        // ── anything else: whitespace, etc. ──
        let start = i;
        i += 1;
        tokens.push(Token {
            kind: TokenKind::Plain,
            text: chars[start..i].iter().collect(),
        });
    }

    tokens
}

// ── Conf-file tokeniser (Kitty, etc.) ─────────────────────────

/// Tokenise a line from a `.conf` file.
///
/// Highlights:
///   - `#` as comment (line start or mid-line prose)
///   - Hex colour values like `#1e1e2e` as numbers
///   - Quoted strings
///   - Numbers (integers, floats)
fn tokenize_conf(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // ── strings: "..." or '...' ──
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = i;
            i += 1;
            while i < n {
                if chars[i] == '\\' && i + 1 < n {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::String,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── `#`: comment or hex color? ──────────────────────
        //
        // Heuristic:
        //   - Line start or preceded by whitespace AND followed
        //     by 3–8 hex digits (then space / EOL) → color value
        //   - Otherwise → rest-of-line comment
        if ch == '#' {
            let preceded_by_space = i == 0
                || chars[i.saturating_sub(1)].is_whitespace();
            let could_be_color = preceded_by_space
                && i + 1 < n
                && chars[i + 1].is_ascii_hexdigit();
            if could_be_color {
                let start = i;
                i += 1; // skip #
                let hex_start = i;
                while i < n && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let hex_len = i - hex_start;
                if (hex_len == 3 || hex_len == 4 || hex_len == 6 || hex_len == 8)
                    && (i == n || chars[i].is_whitespace()
                        || chars[i] == '"' || chars[i] == '\'')
                {
                    tokens.push(Token {
                        kind: TokenKind::Number,
                        text: chars[start..i].iter().collect(),
                    });
                    continue;
                }
                // Not a valid colour — treat as comment from `#`.
                i = hex_start; // backtrack to after #
            }
            // Comment: rest of line.
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[i.saturating_sub(1)..].iter().collect(),
            });
            break;
        }

        // ── numbers ──
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < n && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < n
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == '_')
            {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── punctuation / operators used in conf bindings ──
        if matches!(ch, '+' | '-' | '=' | ':' | ',' | '~' | '>' | '<') {
            tokens.push(Token {
                kind: TokenKind::Operator,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── identifiers / words ──
        if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Plain,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── anything else: whitespace, etc. ──
        let start = i;
        i += 1;
        tokens.push(Token {
            kind: TokenKind::Plain,
            text: chars[start..i].iter().collect(),
        });
    }

    tokens
}
