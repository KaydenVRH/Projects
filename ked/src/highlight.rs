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
    C,
    Conf,
    Md,
    JavaScript,
    Html,
    Plain,
}

/// What kind of token this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Builtin,
    Type,
    Function,
    Lifetime,
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

/// Rust std / prelude type names.
const RS_TYPES: &[&str] = &[
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
    "Pin", "Unpin",
];

/// Rust built-in macros and utility functions.
const RS_BUILTINS: &[&str] = &[
    "dbg", "eprintln", "eprint", "println", "print", "format",
    "assert", "assert_eq", "assert_ne",
    "panic", "unreachable", "unimplemented", "todo",
    "vec", "cfg", "matches",
    "include_str", "include_bytes",
    "concat", "stringify",
    "write", "writeln",
    "file", "line", "column",
    "env", "option_env",
    "compile_error",
    "core", "std", "alloc", "proc_macro",
];

/// Tokenise a Python line into coloured spans.
///
/// This is a simple character-by-character scanner.  It does NOT
/// handle multi-line strings or f-string interpolation inside braces.
pub fn highlight_line(line: &str, theme: &Theme, lang: Lang) -> Vec<Span<'static>> {
    let tokens = match lang {
        Lang::Python => tokenize_python(line),
        Lang::Rust => tokenize_rust(line),
        Lang::C => tokenize_c(line),
        Lang::Conf => tokenize_conf(line),
        Lang::Md => tokenize_markdown(line),
        Lang::JavaScript => tokenize_javascript(line),
        Lang::Html => tokenize_html(line),
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
        TokenKind::Type         => theme.rstype,
        TokenKind::Function     => theme.function,
        TokenKind::Lifetime     => theme.lifetime,
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

        // ── char literal `'x'` / lifetime `'ident` ──
        if ch == '\'' {
            let start = i;
            if i + 2 < n {
                i += 1;
                // escaped char
                if chars[i] == '\\' && i + 1 < n {
                    i += 2;
                } else {
                    i += 1;
                }
                // char literal if closing quote follows
                if i < n && chars[i] == '\'' {
                    i += 1;
                    tokens.push(Token {
                        kind: TokenKind::String,
                        text: chars[start..i].iter().collect(),
                    });
                    continue;
                }
            }
            // Not a char literal — try lifetime: `'ident`
            if i < n && chars[i].is_alphabetic() {
                i += 1;
                while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Lifetime,
                    text: chars[start..i].iter().collect(),
                });
                continue;
            }
            // Lone `'` (odd edge case) — emit as punctuation.
            tokens.push(Token {
                kind: TokenKind::Punctuation,
                text: "'".to_string(),
            });
            i = start + 1;
            continue;
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
            // When we see `(`, check if the previous non-whitespace
            // token was a plain identifier — it's a function call.
            if ch == '(' {
                if let Some(last) = tokens.last_mut() {
                    if last.kind == TokenKind::Plain {
                        last.kind = TokenKind::Function;
                    }
                }
            }
            tokens.push(Token {
                kind: TokenKind::Punctuation,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── identifiers / words (keyword, type, builtin, macro) ──
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
            } else if RS_TYPES.contains(&word.as_str()) {
                TokenKind::Type
            } else if RS_BUILTINS.contains(&word.as_str()) {
                TokenKind::Builtin
            } else {
                // Check if this follows `fn` keyword — then it's a
                // function declaration name.
                let after_fn = tokens.last()
                    .map(|t| t.kind == TokenKind::Keyword && t.text == "fn")
                    .unwrap_or(false);
                if after_fn { TokenKind::Function } else { TokenKind::Plain }
            };
            tokens.push(Token { kind, text: word });
            if is_macro {
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

// ── C tokeniser ──────────────────────────────────────────────────

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do",
    "double", "else", "enum", "extern", "float", "for", "goto", "if",
    "inline", "int", "long", "register", "restrict", "return", "short",
    "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
    "unsigned", "void", "volatile", "while",
    // C11
    "_Alignas", "_Alignof", "_Atomic", "_Bool", "_Complex", "_Generic",
    "_Imaginary", "_Noreturn", "_Static_assert", "_Thread_local",
    // C23
    "alignas", "alignof", "bool", "constexpr", "false", "nullptr",
    "static_assert", "thread_local", "true", "typeof", "typeof_unqual",
];

const C_TYPES: &[&str] = &[
    "size_t", "ssize_t", "int8_t", "int16_t", "int32_t", "int64_t",
    "uint8_t", "uint16_t", "uint32_t", "uint64_t",
    "intptr_t", "uintptr_t", "ptrdiff_t", "wchar_t",
    "FILE", "fpos_t", "time_t", "clock_t", "va_list", "jmp_buf",
    "sig_atomic_t", "div_t", "ldiv_t", "lldiv_t",
];

const C_BUILTINS: &[&str] = &[
    "printf", "fprintf", "sprintf", "snprintf",
    "scanf", "fscanf", "sscanf",
    "puts", "gets", "putchar", "getchar",
    "fopen", "fclose", "fread", "fwrite", "fseek", "ftell", "rewind",
    "fgetc", "fputc", "fgets", "fputs", "fflush",
    "malloc", "calloc", "realloc", "free",
    "memcpy", "memmove", "memset", "memcmp", "memchr",
    "strlen", "strcpy", "strncpy", "strcat", "strncat",
    "strcmp", "strncmp", "strchr", "strrchr", "strstr",
    "strtok", "strdup", "strndup",
    "atoi", "atol", "atof", "strtol", "strtoul", "strtod",
    "abs", "labs", "llabs",
    "qsort", "bsearch",
    "rand", "srand",
    "exit", "abort", "atexit",
    "assert",
    "perror",
    "offsetof",
    "main",
];

fn tokenize_c(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // ── preprocessor directive: `#` at column 0 ──
        if ch == '#' && i == 0 {
            tokens.push(Token {
                kind: TokenKind::Decorator,
                text: chars[i..].iter().collect(),
            });
            break;
        }

        // ── block comment: `/* ... */` ──
        if ch == '/' && i + 1 < n && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < n { i += 2; }
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── line comment: `//` ──
        if ch == '/' && i + 1 < n && chars[i + 1] == '/' {
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[i..].iter().collect(),
            });
            break;
        }

        // ── string literals: `"..."` ──
        if ch == '"' {
            let start = i;
            i += 1;
            while i < n {
                if chars[i] == '\\' && i + 1 < n {
                    i += 2;
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

        // ── char literal: `'x'` ──
        if ch == '\'' {
            let start = i;
            i += 1;
            if i < n && chars[i] == '\\' && i + 1 < n {
                i += 2;
            } else if i < n {
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
            // not a char literal — emit `'` as punctuation
            i = start + 1;
            tokens.push(Token {
                kind: TokenKind::Punctuation,
                text: "'".to_string(),
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
                if nxt == 'x' || nxt == 'X' || nxt == 'b' || nxt == 'B' {
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
            while i < n
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == 'e' || chars[i] == 'E'
                    || chars[i] == 'x' || chars[i] == 'X'
                    || chars[i] == 'f' || chars[i] == 'F'
                    || chars[i] == 'u' || chars[i] == 'U'
                    || chars[i] == 'l' || chars[i] == 'L')
            {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── two-char operators ──
        if i + 1 < n {
            let two = format!("{}{}", ch, chars[i + 1]);
            let op2 = matches!(
                two.as_str(),
                "==" | "!=" | "<=" | ">=" | "&&" | "||"
                    | "->" | "++" | "--"
                    | "+=" | "-=" | "*=" | "/=" | "%="
                    | "&=" | "|=" | "^=" | "<<"
                    | ">>" | "##"
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
                "<<=" | ">>=" | "..."
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
                | '&' | '|' | '^' | '~' | ':' | '.' | '?'
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
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '\\'
        ) {
            tokens.push(Token {
                kind: TokenKind::Punctuation,
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
            let word: String = chars[start..i].iter().collect();
            let kind = if C_KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if C_TYPES.contains(&word.as_str()) {
                TokenKind::Type
            } else if C_BUILTINS.contains(&word.as_str()) {
                TokenKind::Builtin
            } else {
                TokenKind::Plain
            };
            tokens.push(Token { kind, text: word });
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

// ── Markdown tokeniser ────────────────────────────────────────────

fn tokenize_markdown(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // ATX heading: `# ` … `###### `
        if (ch == '#' && i == 0)
            || (ch == '#' && i > 0 && tokens.is_empty() && chars[..i].iter().all(|c| c.is_whitespace()))
        {
            let start = i;
            while i < n && chars[i] == '#' { i += 1; }
            if i < n && chars[i] == ' ' { i += 1; }
            // rest of line as heading
            tokens.push(Token {
                kind: TokenKind::Decorator,
                text: chars[start..i].iter().collect(),
            });
            if i < n {
                tokens.push(Token {
                    kind: TokenKind::Function,
                    text: chars[i..].iter().collect(),
                });
            }
            break;
        }

        // Blockquote: `> ` at line start
        if ch == '>' && (i == 0 || chars[..i].iter().all(|c| c.is_whitespace())) {
            let start = i;
            i += 1;
            while i < n && chars[i] == ' ' { i += 1; }
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[start..i].iter().collect(),
            });
            if i < n {
                tokens.push(Token {
                    kind: TokenKind::Plain,
                    text: chars[i..].iter().collect(),
                });
            }
            break;
        }

        // Horizontal rule: `---`, `***`, `___` (3+ chars, only those)
        if (ch == '-' || ch == '*' || ch == '_')
            && (i == 0 || chars[..i].iter().all(|c| c.is_whitespace()))
        {
            let c = ch;
            let start = i;
            let mut count = 0;
            while i < n && chars[i] == c { count += 1; i += 1; }
            while i < n && chars[i] == ' ' { i += 1; }
            if count >= 3 && (i == n || chars[i] == '\n' || chars[i] == '\r') {
                tokens.push(Token {
                    kind: TokenKind::Operator,
                    text: chars[start..i].iter().collect(),
                });
                break;
            }
            i = start; // not a HR — fall through
        }

        // List marker: `- `, `* `, `+ `, `N. ` at line start
        if (ch == '-' || ch == '*' || ch == '+')
            && (i == 0 || chars[..i].iter().all(|c| c.is_whitespace()))
            && i + 1 < n && chars[i + 1] == ' '
        {
            tokens.push(Token {
                kind: TokenKind::Operator,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }
        if ch.is_ascii_digit()
            && i + 1 < n && chars[i + 1] == '.'
            && i + 2 < n && chars[i + 2] == ' '
            && (i == 0 || chars[..i].iter().all(|c| c.is_whitespace()))
        {
            let start = i;
            while i < n && chars[i].is_ascii_digit() { i += 1; }
            i += 1; // skip '.'
            tokens.push(Token {
                kind: TokenKind::Operator,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // Inline code: backtick pair
        if ch == '`' {
            let start = i;
            i += 1;
            while i < n && chars[i] != '`' { i += 1; }
            if i < n { i += 1; } // skip closing backtick
            tokens.push(Token {
                kind: TokenKind::String,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // Link: `[text](url)`
        if ch == '[' {
            let start = i;
            i += 1;
            while i < n && chars[i] != ']' { i += 1; }
            if i < n {
                i += 1; // skip ']'
                if i < n && chars[i] == '(' {
                    // link text
                    tokens.push(Token {
                        kind: TokenKind::Function,
                        text: chars[start..i].iter().collect(),
                    });
                    let url_start = i;
                    i += 1;
                    while i < n && chars[i] != ')' { i += 1; }
                    if i < n { i += 1; }
                    tokens.push(Token {
                        kind: TokenKind::Operator,
                        text: chars[url_start..i].iter().collect(),
                    });
                    continue;
                }
            }
            i = start; // not a link — fall through
        }

        // Image: `![alt](url)`
        if ch == '!' && i + 1 < n && chars[i + 1] == '[' {
            let start = i;
            i += 2;
            while i < n && chars[i] != ']' { i += 1; }
            if i < n {
                i += 1;
                if i < n && chars[i] == '(' {
                    tokens.push(Token {
                        kind: TokenKind::Decorator,
                        text: chars[start..i].iter().collect(),
                    });
                    let url_start = i;
                    i += 1;
                    while i < n && chars[i] != ')' { i += 1; }
                    if i < n { i += 1; }
                    tokens.push(Token {
                        kind: TokenKind::String,
                        text: chars[url_start..i].iter().collect(),
                    });
                    continue;
                }
            }
            i = start;
        }

        // Bold `**` or `__`
        if (ch == '*' || ch == '_') && i + 1 < n && chars[i + 1] == ch {
            let c = ch;
            let start = i;
            i += 2;
            while i + 1 < n && !(chars[i] == c && chars[i + 1] == c) { i += 1; }
            if i + 1 < n { i += 2; }
            tokens.push(Token {
                kind: TokenKind::Builtin,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // Italic `*` or `_`
        if ch == '*' || ch == '_' {
            let c = ch;
            let start = i;
            i += 1;
            while i < n && chars[i] != c { i += 1; }
            if i < n { i += 1; }
            tokens.push(Token {
                kind: TokenKind::String,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── fallthrough: plain text ──
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

/// Common boolean / null keywords in config files.
const CONF_KW: &[&str] = &["true", "false", "null", "on", "off", "yes", "no"];

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
            let word: String = chars[start..i].iter().collect();
            let kind = if CONF_KW.contains(&word.as_str()) {
                TokenKind::Keyword
            } else {
                TokenKind::Plain
            };
            tokens.push(Token { kind, text: word });
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

// ── JavaScript / TypeScript tokeniser ─────────────────────────────

const JS_KEYWORDS: &[&str] = &[
    "function", "var", "let", "const", "if", "else", "for", "while",
    "do", "switch", "case", "break", "continue", "return", "import",
    "export", "from", "class", "extends", "new", "this", "super",
    "try", "catch", "finally", "throw", "typeof", "instanceof",
    "in", "of", "async", "await", "yield", "delete", "void",
    "true", "false", "null", "undefined",
    "type", "interface", "enum", "implements", "namespace", "declare",
    "abstract", "private", "protected", "public", "readonly", "static",
    "keyof", "as", "is", "infer", "never", "unknown", "any",
];

const JS_BUILTINS: &[&str] = &[
    "console", "document", "window", "Math", "JSON", "Array",
    "Object", "String", "Number", "Boolean", "Date", "RegExp",
    "Map", "Set", "Promise", "setTimeout", "setInterval",
    "parseInt", "parseFloat", "isNaN", "fetch", "require",
];

fn tokenize_javascript(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // ── block comment: `/* ... */` ──
        if ch == '/' && i + 1 < n && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < n { i += 2; }
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── line comment: `//` ──
        if ch == '/' && i + 1 < n && chars[i + 1] == '/' {
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[i..].iter().collect(),
            });
            break;
        }

        // ── template literal: `` `...` `` ──
        if ch == '`' {
            let start = i;
            i += 1;
            while i < n {
                if chars[i] == '\\' && i + 1 < n {
                    i += 2;
                    continue;
                }
                if chars[i] == '`' {
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

        // ── numbers ──
        if ch.is_ascii_digit()
            || (ch == '.' && i + 1 < n && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < n
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
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

        // ── three-char operators ──
        if i + 2 < n {
            let three = format!("{}{}{}", ch, chars[i + 1], chars[i + 2]);
            if matches!(three.as_str(), "===" | "!==" | "<<=" | ">>=" | ">>>" | "**=") {
                tokens.push(Token {
                    kind: TokenKind::Operator,
                    text: three,
                });
                i += 3;
                continue;
            }
        }

        // ── two-char operators ──
        if i + 1 < n {
            let two = format!("{}{}", ch, chars[i + 1]);
            if matches!(
                two.as_str(),
                "==" | "!=" | "<=" | ">=" | "&&" | "||" | "=>" | "??"
                    | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|="
                    | "^=" | "<<" | ">>" | "++" | "--" | "**" | "?."
            ) {
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
                | '&' | '|' | '^' | '~' | ':' | '.' | '?'
        ) {
            tokens.push(Token {
                kind: TokenKind::Operator,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── punctuation ──
        if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';') {
            if ch == '(' {
                if let Some(last) = tokens.last_mut() {
                    if last.kind == TokenKind::Plain {
                        last.kind = TokenKind::Function;
                    }
                }
            }
            tokens.push(Token {
                kind: TokenKind::Punctuation,
                text: ch.to_string(),
            });
            i += 1;
            continue;
        }

        // ── identifiers ──
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            let start = i;
            while i < n
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = if JS_KEYWORDS.contains(&word.as_str()) {
                TokenKind::Keyword
            } else if JS_BUILTINS.contains(&word.as_str()) {
                TokenKind::Builtin
            } else {
                TokenKind::Plain
            };
            tokens.push(Token { kind, text: word });
            continue;
        }

        let start = i;
        i += 1;
        tokens.push(Token {
            kind: TokenKind::Plain,
            text: chars[start..i].iter().collect(),
        });
    }

    tokens
}

// ── HTML tokeniser ────────────────────────────────────────────────

const HTML_TAGS: &[&str] = &[
    "html", "head", "body", "div", "span", "p", "a", "img",
    "ul", "ol", "li", "table", "tr", "td", "th", "form",
    "input", "button", "select", "option", "textarea",
    "h1", "h2", "h3", "h4", "h5", "h6", "meta", "link",
    "script", "style", "title", "header", "footer", "nav",
    "section", "article", "aside", "main", "br", "hr",
    "strong", "em", "code", "pre", "blockquote",
];

fn tokenize_html(line: &str) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // ── comment: `<!-- ... -->` ──
        if ch == '<' && i + 3 < n
            && chars[i + 1] == '!'
            && chars[i + 2] == '-'
            && chars[i + 3] == '-'
        {
            let start = i;
            i += 4;
            while i + 2 < n
                && !(chars[i] == '-' && chars[i + 1] == '-' && chars[i + 2] == '>')
            {
                i += 1;
            }
            if i + 2 < n { i += 3; }
            tokens.push(Token {
                kind: TokenKind::Comment,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── tag: `<tagname` … `>` ──
        if ch == '<' {
            let start = i;
            i += 1;
            while i < n && chars[i] != '>' && chars[i] != ' ' && chars[i] != '\t' && chars[i] != '\n' {
                i += 1;
            }
            let tag = chars[start + 1..i].iter().collect::<String>().to_lowercase();
            let is_kw = HTML_TAGS.contains(&tag.as_str()) || tag.starts_with('/');
            tokens.push(Token {
                kind: if is_kw { TokenKind::Keyword } else { TokenKind::Plain },
                text: chars[start..i].iter().collect(),
            });
            // attributes
            while i < n && chars[i] != '>' {
                if chars[i] == '"' || chars[i] == '\'' {
                    let q = chars[i];
                    let attr_start = i;
                    i += 1;
                    while i < n && chars[i] != q { i += 1; }
                    if i < n { i += 1; }
                    tokens.push(Token {
                        kind: TokenKind::String,
                        text: chars[attr_start..i].iter().collect(),
                    });
                } else {
                    i += 1;
                }
            }
            // closing `>`
            if i < n && chars[i] == '>' {
                tokens.push(Token {
                    kind: TokenKind::Keyword,
                    text: ">".to_string(),
                });
                i += 1;
            }
            continue;
        }

        let start = i;
        i += 1;
        tokens.push(Token {
            kind: TokenKind::Plain,
            text: chars[start..i].iter().collect(),
        });
    }

    tokens
}
