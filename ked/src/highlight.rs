//! Python syntax highlighting for ked.
//!
//! The [`highlight_line`] function takes a line of source text and a
//! [`Theme`] reference, tokenises the line, and returns `Vec<Span>` —
//! each span coloured according to the token type.
//!
//! Token types supported:
//!   - Keywords:  `if`, `else`, `for`, `while`, `def`, `class`, …
//!   - Builtins:  `print`, `len`, `range`, `int`, `str`, `list`, …
//!   - Strings:   single/double quoted, raw, byte, f-strings
//!   - Comments:  `#` … end of line
//!   - Numbers:   integers, floats, hex/oct/bin literals
//!   - Decorators:`@` followed by an identifier
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

/// What kind of Python token this is.
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

/// The set of Python keywords (from the language reference).
const KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await",
    "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "while", "with", "yield",
];

/// The set of Python built-in functions (commonly used names).
const BUILTINS: &[&str] = &[
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

/// Tokenise a Python line into coloured spans.
///
/// This is a simple character-by-character scanner.  It does NOT
/// handle multi-line strings or f-string interpolation inside braces.
pub fn highlight_line<'a>(line: &str, theme: &Theme) -> Vec<Span<'static>> {
    let tokens = tokenize(line);
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

/// Split a line into tokens using a simple state-machine.
fn tokenize(line: &str) -> Vec<Token> {
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
            let kind = if KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if BUILTINS.contains(&word.as_str()) {
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
