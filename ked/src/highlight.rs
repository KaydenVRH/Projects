//! Syntax highlighting for ked — tree-sitter backed.
//!
//! Language detection looks at (in order):
//!   1. the shebang line (`#!/usr/bin/env python3` …)
//!   2. well-known file names (`Makefile`, `.gitignore`, …)
//!   3. the file extension
//!
//! The [`Highlighter`] parses the whole buffer with tree-sitter once
//! per content change and walks the tree into per-line [`Token`]
//! lists.  The editor turns those tokens into styled spans on every
//! frame with the current theme, so themes and colour FX stay cheap.
//!
//! `.conf` / `.ini` / `.cfg` keep ked's old hand-rolled tokenizer
//! (there's no good grammar for them); everything else falls back to
//! plain text if the parse fails.

use ratatui::style::Style;
use ratatui::text::Span;
use tree_sitter::{Language, Node, Parser};

use crate::theme::Theme;

// ── languages ────────────────────────────────────────────────────

/// The language a buffer is highlighted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    C,
    Cpp,
    JavaScript,
    TypeScript,
    Tsx,
    Html,
    Css,
    Toml,
    Json,
    Bash,
    Go,
    Markdown,
    /// Config files — hand-rolled tokenizer, no grammar.
    Conf,
    Plain,
}

// ── tokens ───────────────────────────────────────────────────────

/// A single token from a line of source.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

/// What kind of token this is (same set as the old hand-rolled
/// tokenizers, so every theme maps unchanged).
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
    Constant,   // true / false / null / None
    Decorator,
    Property,   // struct fields, object properties
    Operator,
    Punctuation,
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
        TokenKind::Constant     => theme.constant,
        TokenKind::Decorator    => theme.decorator,
        TokenKind::Property     => theme.property,
        TokenKind::Operator     => theme.operator,
        TokenKind::Punctuation  => theme.punctuation,
    }
}

// ── language detection ───────────────────────────────────────────

/// Detect the language of a buffer from its filename and first line.
pub fn detect_lang(filename: Option<&str>, first_line: Option<&str>) -> Lang {
    // 1. shebang
    if let Some(line) = first_line {
        if let Some(rest) = line.strip_prefix("#!") {
            let s = rest.trim();
            if s.contains("python") {
                return Lang::Python;
            }
            if s.contains("node") {
                return Lang::JavaScript;
            }
            if s.contains("bash")
                || s.contains("zsh")
                || s.contains("fish")
                || s.contains(" sh")
                || s.ends_with("sh")
            {
                return Lang::Bash;
            }
        }
    }

    // 2. well-known file names (no extension)
    if let Some(name) = filename.and_then(|f| f.rsplit('/').next()) {
        match name.to_lowercase().as_str() {
            ".gitignore" | ".gitattributes" | ".editorconfig" => return Lang::Conf,
            _ => {}
        }
    }

    // 3. extension
    let ext = filename
        .and_then(|f| f.rsplit('.').next())
        .map(|e| e.to_lowercase());
    match ext.as_deref() {
        Some("rs") => Lang::Rust,
        Some("py") | Some("pyw") => Lang::Python,
        Some("c") | Some("h") => Lang::C,
        Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") | Some("hh") => Lang::Cpp,
        Some("js") | Some("mjs") | Some("cjs") => Lang::JavaScript,
        Some("jsx") => Lang::JavaScript,
        Some("ts") | Some("mts") | Some("cts") => Lang::TypeScript,
        Some("tsx") => Lang::Tsx,
        Some("html") | Some("htm") => Lang::Html,
        Some("css") => Lang::Css,
        Some("toml") => Lang::Toml,
        Some("json") => Lang::Json,
        Some("sh") | Some("bash") | Some("zsh") => Lang::Bash,
        Some("go") => Lang::Go,
        Some("md") | Some("markdown") => Lang::Markdown,
        Some("conf") | Some("ini") | Some("cfg") => Lang::Conf,
        _ => Lang::Plain,
    }
}

// ── Highlighter (cached parse) ───────────────────────────────────

/// A parsed buffer: per-line tokens, cached until the content or the
/// language changes.
pub struct Highlighter {
    lang: Lang,
    hash: u64,
    lines: Vec<Vec<Token>>,
}

impl Highlighter {
    /// Parse `source` with the grammar for `lang` (or the fallback
    /// tokenizer when there is none).  `hash` is whatever cheap
    /// fingerprint the editor uses to detect content changes.
    pub fn parse(lang: Lang, source: &str, hash: u64) -> Self {
        let lines = match grammar(lang) {
            Some(l) => parse_with(lang, l, source).unwrap_or_else(|| plain_lines(source)),
            None => fallback_lines(lang, source),
        };
        Highlighter { lang, hash, lines }
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// Content fingerprint this parse belongs to.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Styled spans for one line (empty for lines past the end).
    pub fn spans(&self, row: usize, theme: &Theme) -> Vec<Span<'static>> {
        match self.lines.get(row) {
            Some(tokens) => tokens
                .iter()
                .map(|t| Span::styled(t.text.clone(), style_for_kind(t.kind, theme)))
                .collect(),
            None => Vec::new(),
        }
    }
}

/// The tree-sitter grammar for a language, if we have one.
fn grammar(lang: Lang) -> Option<Language> {
    match lang {
        Lang::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        Lang::Python => Some(tree_sitter_python::LANGUAGE.into()),
        Lang::C => Some(tree_sitter_c::LANGUAGE.into()),
        Lang::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
        Lang::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
        Lang::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        Lang::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        Lang::Html => Some(tree_sitter_html::LANGUAGE.into()),
        Lang::Css => Some(tree_sitter_css::LANGUAGE.into()),
        Lang::Toml => Some(tree_sitter_toml_ng::LANGUAGE.into()),
        Lang::Json => Some(tree_sitter_json::LANGUAGE.into()),
        Lang::Bash => Some(tree_sitter_bash::LANGUAGE.into()),
        Lang::Go => Some(tree_sitter_go::LANGUAGE.into()),
        Lang::Markdown => Some(tree_sitter_md::LANGUAGE.into()),
        Lang::Conf | Lang::Plain => None,
    }
}

// ── fallback tokenizers (no grammar / parse failure) ─────────────

/// One plain token per line.
fn plain_lines(source: &str) -> Vec<Vec<Token>> {
    source
        .lines()
        .map(|l| vec![Token { kind: TokenKind::Plain, text: l.to_string() }])
        .collect()
}

/// Hand-rolled tokenizer for languages without a grammar (conf files),
/// or after a failed parse of anything else.
fn fallback_lines(lang: Lang, source: &str) -> Vec<Vec<Token>> {
    match lang {
        Lang::Conf => source
            .lines()
            .map(|l| tokenize_conf(l).into_iter()
                .map(|t| Token { kind: token_conf_kind(t.kind), text: t.text })
                .collect())
            .collect(),
        _ => plain_lines(source),
    }
}

// ── the tree walk ────────────────────────────────────────────────

/// State carried through a tree walk.
struct WalkState<'a> {
    source: &'a str,
    lines: Vec<Vec<Token>>,
    /// Current output line (incremented as we pass `\n`).
    line: usize,
    /// Byte offset of the end of the last emitted text.
    byte: usize,
}

impl<'a> WalkState<'a> {
    fn new(source: &'a str) -> Self {
        WalkState { source, lines: vec![Vec::new()], line: 0, byte: 0 }
    }

    /// Emit `text` with `kind`, splitting at newlines.
    fn emit(&mut self, kind: TokenKind, text: &str) {
        let mut rest = text;
        while !rest.is_empty() {
            match rest.find('\n') {
                Some(i) => {
                    self.lines[self.line].push(Token { kind, text: rest[..i].to_string() });
                    self.line += 1;
                    if self.lines.len() <= self.line {
                        self.lines.push(Vec::new());
                    }
                    rest = &rest[i + 1..];
                }
                None => {
                    self.lines[self.line].push(Token { kind, text: rest.to_string() });
                    rest = "";
                }
            }
        }
        self.byte += text.len();
    }

    /// Emit the byte range `[start, end)` as one token, first filling
    /// any gap since the last emitted byte with plain text (tree-sitter
    /// doesn't give us whitespace as nodes).
    fn emit_range(&mut self, kind: TokenKind, start: usize, end: usize) {
        if self.byte < start {
            let gap = &self.source[self.byte..start];
            self.emit(TokenKind::Plain, gap);
        }
        let text = &self.source[start..end];
        // f-string prefix: `f"..."` → `f` styled separately.
        if kind == TokenKind::String && text.len() >= 2 {
            let first = text.as_bytes()[0];
            if (first == b'f' || first == b'F')
                && (text.as_bytes()[1] == b'"' || text.as_bytes()[1] == b'\'')
            {
                self.emit(TokenKind::FStringPrefix, &text[..1]);
                self.emit(TokenKind::String, &text[1..]);
                return;
            }
        }
        self.emit(kind, text);
    }
}

fn parse_with(lang: Lang, language: Language, source: &str) -> Option<Vec<Vec<Token>>> {
    let mut parser = Parser::new();
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    let cfg = lang_config(lang);
    let mut st = WalkState::new(source);
    walk(tree.root_node(), cfg, &mut st, None, "");
    // Trailing gap (whitespace after the last token).
    if st.byte < source.len() {
        st.emit(TokenKind::Plain, &source[st.byte..]);
    }
    Some(st.lines)
}

fn walk(
    node: Node,
    cfg: &LangConfig,
    st: &mut WalkState,
    field: Option<&'static str>,
    parent_kind: &'static str,
) {
    let kind = node.kind();

    // Whole node is one token — don't descend.
    if let Some(&(_, tk)) = cfg.opaque.iter().find(|(k, _)| *k == kind) {
        st.emit_range(tk, node.start_byte(), node.end_byte());
        return;
    }

    if node.child_count() == 0 {
        if let Some(tk) = leaf_kind(node, cfg, field, parent_kind, st.source) {
            st.emit_range(tk, node.start_byte(), node.end_byte());
        }
        return;
    }

    for i in 0..node.child_count() {
        let child = node.child(i as u32).unwrap();
        let f = node.field_name_for_child(i as u32);
        walk(child, cfg, st, f, kind);
    }
}

/// Classify a leaf node (or return `None` — its text then becomes
/// part of the plain gap fill).
fn leaf_kind(
    node: Node,
    cfg: &LangConfig,
    field: Option<&str>,
    parent_kind: &str,
    source: &str,
) -> Option<TokenKind> {
    let kind = node.kind();
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");

    // Words first: keywords/builtins/types often appear as anonymous
    // leaves whose kind *is* the word (`fn`, `def`, `package`, …).
    if !text.is_empty() {
        if cfg.constants.contains(&text) {
            return Some(TokenKind::Constant);
        }
        if cfg.keywords.contains(&text) {
            return Some(TokenKind::Keyword);
        }
        if cfg.types.contains(&text) {
            return Some(TokenKind::Type);
        }
        if cfg.builtins.contains(&text) {
            return Some(TokenKind::Builtin);
        }
    }

    if cfg.type_leaf_kinds.contains(&kind) {
        return Some(TokenKind::Type);
    }

    if cfg.ident_kinds.contains(&kind) {
        // The function being called: `foo(...)`.
        if (field == Some("function") || field == Some("constructor"))
            && cfg.call_kinds.contains(&parent_kind)
        {
            return Some(TokenKind::Function);
        }
        // `obj.method()` — method name field is "field" in some
        // grammars (Rust).
        if field == Some("field") && cfg.call_kinds.contains(&parent_kind) {
            return Some(TokenKind::Function);
        }
        // Definition names.
        if (field == Some("name") || field == Some("declarator"))
            && cfg.func_def_kinds.contains(&parent_kind)
        {
            return Some(TokenKind::Function);
        }
        if (field == Some("name") || field == Some("declarator"))
            && cfg.type_def_kinds.contains(&parent_kind)
        {
            return Some(TokenKind::Type);
        }
        // Struct fields / object properties.
        if cfg.property_kinds.contains(&kind) {
            return Some(TokenKind::Property);
        }
        return Some(TokenKind::Plain);
    }

    for (k, tk) in cfg.special_kinds {
        if *k == kind {
            return Some(*tk);
        }
    }

    // Symbol-only leaves: punctuation vs operators.
    if !text.is_empty()
        && text.chars().all(|c| !c.is_alphanumeric() && !c.is_whitespace() && c != '_')
    {
        if text.chars().all(|c| "()[]{};:,".contains(c)) {
            return Some(TokenKind::Punctuation);
        }
        return Some(TokenKind::Operator);
    }

    None
}

// ── per-language configuration ───────────────────────────────────

struct LangConfig {
    /// Node kinds emitted whole, mapped to a token kind
    /// (comments, strings, numbers, decorators, …).
    opaque: &'static [(&'static str, TokenKind)],
    /// Kinds whose `name` field is a function being defined.
    func_def_kinds: &'static [&'static str],
    /// Kinds whose `name` field is a type being defined.
    type_def_kinds: &'static [&'static str],
    /// Kinds with a `function`/`constructor` field (calls).
    call_kinds: &'static [&'static str],
    /// Identifier-like leaf kinds.
    ident_kinds: &'static [&'static str],
    /// Identifier-like leaf kinds that are struct fields / object
    /// properties.
    property_kinds: &'static [&'static str],
    /// Leaf kinds that are always types (`type_identifier`, …).
    type_leaf_kinds: &'static [&'static str],
    /// Leaf kinds with a fixed token kind (HTML tags, CSS units, …).
    special_kinds: &'static [(&'static str, TokenKind)],
    /// Word lists, matched against leaf text.
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    builtins: &'static [&'static str],
    /// `true` / `false` / `null` / … — styled as keywords.
    constants: &'static [&'static str],
}

const CONSTANTS: &[&str] = &["true", "false", "null", "nil"];

// ── word lists ───────────────────────────────────────────────────

const PY_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await",
    "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "while", "with", "yield", "match", "case",
];

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

const RS_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn",
    "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

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

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do",
    "double", "else", "enum", "extern", "float", "for", "goto", "if",
    "inline", "int", "long", "register", "restrict", "return", "short",
    "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
    "unsigned", "void", "volatile", "while",
    "_Alignas", "_Alignof", "_Atomic", "_Bool", "_Complex", "_Generic",
    "_Imaginary", "_Noreturn", "_Static_assert", "_Thread_local",
    "alignas", "alignof", "bool", "constexpr", "false", "nullptr",
    "static_assert", "thread_local", "true", "typeof", "typeof_unqual",
];

const CPP_KEYWORDS: &[&str] = &[
    "class", "namespace", "template", "using", "public", "private",
    "protected", "virtual", "override", "final", "new", "delete", "this",
    "try", "catch", "throw", "typename", "friend", "operator", "explicit",
    "mutable", "noexcept", "consteval", "constinit", "const_cast",
    "dynamic_cast", "reinterpret_cast", "static_cast", "decltype",
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
    "assert", "perror", "offsetof", "main",
];

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
    "get", "set", "satisfies", "module",
];

const JS_BUILTINS: &[&str] = &[
    "console", "document", "window", "Math", "JSON", "Array",
    "Object", "String", "Number", "Boolean", "Date", "RegExp",
    "Map", "Set", "Promise", "setTimeout", "setInterval",
    "parseInt", "parseFloat", "isNaN", "fetch", "require",
];

const GO_KEYWORDS: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer",
    "else", "fallthrough", "for", "func", "go", "goto", "if", "import",
    "interface", "map", "package", "range", "return", "select",
    "struct", "switch", "type", "var",
];

const GO_TYPES: &[&str] = &[
    "string", "bool", "byte", "rune", "error", "any", "comparable",
    "int", "int8", "int16", "int32", "int64",
    "uint", "uint8", "uint16", "uint32", "uint64", "uintptr",
    "float32", "float64", "complex64", "complex128",
];

const GO_BUILTINS: &[&str] = &[
    "append", "cap", "clear", "close", "complex", "copy", "delete",
    "imag", "len", "make", "max", "min", "new", "panic", "print",
    "println", "real", "recover",
];

const BASH_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do",
    "done", "case", "esac", "in", "select", "function", "time", "coproc",
];

// ── the configs ──────────────────────────────────────────────────

fn lang_config(lang: Lang) -> &'static LangConfig {
    use TokenKind::*;

    macro_rules! cfg {
        (
            opaque: [$($o:expr),* $(,)?];
            func: [$($f:expr),* $(,)?];
            type_def: [$($t:expr),* $(,)?];
            call: [$($c:expr),* $(,)?];
            ident: [$($i:expr),* $(,)?];
            property: [$($p:expr),* $(,)?];
            type_leaf: [$($tl:expr),* $(,)?];
            special: [$($s:expr),* $(,)?];
            keywords: $k:expr; types: $ty:expr; builtins: $b:expr;
        ) => {
            &LangConfig {
                opaque: &[$($o),*],
                func_def_kinds: &[$($f),*],
                type_def_kinds: &[$($t),*],
                call_kinds: &[$($c),*],
                ident_kinds: &[$($i),*],
                property_kinds: &[$($p),*],
                type_leaf_kinds: &[$($tl),*],
                special_kinds: &[$($s),*],
                keywords: $k,
                types: $ty,
                builtins: $b,
                constants: CONSTANTS,
            }
        };
    }

    match lang {
        Lang::Rust => cfg! {
            opaque: [
                ("line_comment", Comment), ("block_comment", Comment),
                ("string_literal", String), ("raw_string_literal", String),
                ("char_literal", String),
                ("integer_literal", Number), ("float_literal", Number),
                ("attribute_item", Decorator), ("inner_attribute_item", Decorator),
                ("lifetime", Lifetime),
            ];
            func: ["function_item", "function_signature_item", "scoped_identifier"];
            type_def: ["struct_item", "enum_item", "union_item", "trait_item", "type_item"];
            call: ["call_expression", "method_call_expression", "field_expression"];
            ident: ["identifier", "field_identifier", "shorthand_field_identifier"];
            property: ["field_identifier", "shorthand_field_identifier"];
            type_leaf: ["type_identifier", "primitive_type"];
            special: [];
            keywords: RS_KEYWORDS; types: RS_TYPES; builtins: RS_BUILTINS;
        },
        Lang::Python => cfg! {
            opaque: [
                ("comment", Comment),
                ("string", String), ("concatenated_string", String),
                ("integer", Number), ("float", Number),
                ("decorator", Decorator),
            ];
            func: ["function_definition"];
            type_def: ["class_definition"];
            call: ["call"];
            ident: ["identifier"];
            property: [];
            type_leaf: [];
            special: [];
            keywords: PY_KEYWORDS; types: &[]; builtins: PY_BUILTINS;
        },
        Lang::C => cfg! {
            opaque: [
                ("comment", Comment),
                ("string_literal", String), ("concatenated_string", String),
                ("char_literal", String),
                ("number_literal", Number),
                ("preproc_def", Decorator), ("preproc_include", Decorator),
                ("preproc_if", Decorator), ("preproc_ifdef", Decorator),
                ("preproc_else", Decorator), ("preproc_endif", Decorator),
                ("preproc_function_def", Decorator), ("preproc_call", Decorator),
            ];
            func: ["function_declarator", "preproc_function_def"];
            type_def: ["struct_specifier", "enum_specifier", "union_specifier", "type_definition"];
            call: ["call_expression"];
            ident: ["identifier", "field_identifier", "statement_identifier"];
            property: ["field_identifier"];
            type_leaf: ["type_identifier", "primitive_type", "sized_type_specifier"];
            special: [];
            keywords: C_KEYWORDS; types: C_TYPES; builtins: C_BUILTINS;
        },
        Lang::Cpp => cfg! {
            opaque: [
                ("comment", Comment),
                ("string_literal", String), ("concatenated_string", String),
                ("raw_string_literal", String), ("user_string_literal", String),
                ("char_literal", String),
                ("number_literal", Number),
                ("preproc_def", Decorator), ("preproc_include", Decorator),
                ("preproc_if", Decorator), ("preproc_ifdef", Decorator),
                ("preproc_else", Decorator), ("preproc_endif", Decorator),
                ("preproc_function_def", Decorator), ("preproc_call", Decorator),
            ];
            func: ["function_declarator", "preproc_function_def"];
            type_def: ["struct_specifier", "enum_specifier", "union_specifier",
                       "type_definition", "class_specifier"];
            call: ["call_expression"];
            ident: ["identifier", "field_identifier", "statement_identifier",
                    "namespace_identifier"];
            property: ["field_identifier"];
            type_leaf: ["type_identifier", "primitive_type", "sized_type_specifier"];
            special: [];
            keywords: CPP_KEYWORDS; types: C_TYPES; builtins: C_BUILTINS;
        },
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => cfg! {
            opaque: [
                ("comment", Comment),
                ("string", String), ("template_string", String),
                ("regex", String),
                ("number", Number),
                ("decorator", Decorator),
            ];
            func: ["function_expression", "function_declaration", "method_definition",
                   "generator_function", "generator_function_declaration",
                   "function_signature", "method_signature"];
            type_def: ["class_declaration", "class_expression", "interface_declaration",
                       "type_alias_declaration", "enum_declaration"];
            call: ["call_expression", "new_expression"];
            ident: ["identifier", "property_identifier", "shorthand_property_identifier",
                    "private_property_identifier"];
            property: ["property_identifier", "shorthand_property_identifier",
                       "private_property_identifier"];
            type_leaf: ["type_identifier", "predefined_type"];
            special: [];
            keywords: JS_KEYWORDS; types: &[]; builtins: JS_BUILTINS;
        },
        Lang::Html => cfg! {
            opaque: [
                ("comment", Comment),
                ("quoted_attribute_value", String), ("attribute_value", String),
                ("doctype", Decorator),
                ("entity", Number),
            ];
            func: [];
            type_def: [];
            call: [];
            ident: [];
            property: [];
            type_leaf: [];
            special: [("tag_name", Keyword), ("attribute_name", Builtin)];
            keywords: &[]; types: &[]; builtins: &[];
        },
        Lang::Css => cfg! {
            opaque: [
                ("comment", Comment),
                ("string_value", String),
                ("color_value", Number),
                ("integer_value", Number), ("float_value", Number),
                ("class_name", Builtin), ("id_name", Builtin),
                ("important", Keyword),
            ];
            func: [];
            type_def: [];
            call: [];
            ident: [];
            property: [];
            type_leaf: [];
            special: [
                ("tag_name", Keyword), ("property_name", Function),
                ("unit", Number), ("at_keyword", Decorator),
                ("feature_name", Builtin),
            ];
            keywords: &[]; types: &[]; builtins: &[];
        },
        Lang::Toml => cfg! {
            opaque: [
                ("comment", Comment),
                ("string", String), ("literal_string", String),
                ("multiline_string", String), ("multiline_literal_string", String),
                ("integer", Number), ("float", Number),
                ("boolean", Keyword),
            ];
            func: [];
            type_def: [];
            call: [];
            ident: [];
            property: [];
            type_leaf: [];
            special: [("bare_key", Builtin), ("quoted_key", Builtin)];
            keywords: &[]; types: &[]; builtins: &[];
        },
        Lang::Json => cfg! {
            opaque: [
                ("string", String),
                ("number", Number),
            ];
            func: [];
            type_def: [];
            call: [];
            ident: [];
            property: [];
            type_leaf: [];
            special: [];
            keywords: &[]; types: &[]; builtins: &[];
        },
        Lang::Bash => cfg! {
            opaque: [
                ("comment", Comment),
                ("string", String), ("raw_string", String), ("ansi_c_string", String),
                ("heredoc_start", String), ("heredoc_body", String),
                ("number", Number),
                ("command_name", Function),
            ];
            func: ["function_definition"];
            type_def: [];
            call: [];
            ident: ["word"];
            property: [];
            type_leaf: [];
            special: [
                ("variable_name", Builtin),
                ("test_operator", Operator), ("file_descriptor", Builtin),
            ];
            keywords: BASH_KEYWORDS; types: &[]; builtins: &[];
        },
        Lang::Go => cfg! {
            opaque: [
                ("comment", Comment),
                ("interpreted_string_literal", String), ("raw_string_literal", String),
                ("rune_literal", String),
                ("int_literal", Number), ("float_literal", Number),
                ("imaginary_literal", Number),
            ];
            func: ["function_declaration", "method_declaration"];
            type_def: ["type_spec", "struct_type", "interface_type"];
            call: ["call_expression", "selector_expression"];
            ident: ["identifier", "field_identifier", "package_identifier"];
            property: ["field_identifier"];
            type_leaf: ["type_identifier"];
            special: [];
            keywords: GO_KEYWORDS; types: GO_TYPES; builtins: GO_BUILTINS;
        },
        Lang::Markdown => cfg! {
            opaque: [
                ("fenced_code_block", String), ("indented_code_block", String),
                ("atx_heading", Function), ("setext_heading", Function),
                ("thematic_break", Operator),
                ("html_block", Comment), ("html_comment", Comment),
            ];
            func: [];
            type_def: [];
            call: [];
            ident: [];
            property: [];
            type_leaf: [];
            special: [("info_string", Builtin), ("link_label", Builtin)];
            keywords: &[]; types: &[]; builtins: &[];
        },
        Lang::Conf | Lang::Plain => cfg! {
            opaque: [];
            func: [];
            type_def: [];
            call: [];
            ident: [];
            property: [];
            type_leaf: [];
            special: [];
            keywords: &[]; types: &[]; builtins: &[];
        },
    }
}

// ── hand-rolled conf tokenizer (fallback) ────────────────────────

/// The old per-line token kinds used by the conf tokenizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfKind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
    Operator,
}

fn token_conf_kind(k: ConfKind) -> TokenKind {
    match k {
        ConfKind::Plain => TokenKind::Plain,
        ConfKind::Keyword => TokenKind::Keyword,
        ConfKind::String => TokenKind::String,
        ConfKind::Comment => TokenKind::Comment,
        ConfKind::Number => TokenKind::Number,
        ConfKind::Operator => TokenKind::Operator,
    }
}

#[derive(Debug, Clone)]
struct ConfToken {
    kind: ConfKind,
    text: String,
}

/// Common boolean / null keywords in config files.
const CONF_KW: &[&str] = &["true", "false", "null", "on", "off", "yes", "no"];

/// Tokenise a line from a `.conf` file (kept from ked's original
/// hand-rolled highlighter).
fn tokenize_conf(line: &str) -> Vec<ConfToken> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<ConfToken> = Vec::new();
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
            tokens.push(ConfToken {
                kind: ConfKind::String,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── `#`: comment or hex color? ──────────────────────
        if ch == '#' {
            let preceded_by_space =
                i == 0 || chars[i.saturating_sub(1)].is_whitespace();
            let could_be_color =
                preceded_by_space && i + 1 < n && chars[i + 1].is_ascii_hexdigit();
            if could_be_color {
                let start = i;
                i += 1; // skip #
                let hex_start = i;
                while i < n && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let hex_len = i - hex_start;
                if (hex_len == 3 || hex_len == 4 || hex_len == 6 || hex_len == 8)
                    && (i == n
                        || chars[i].is_whitespace()
                        || chars[i] == '"'
                        || chars[i] == '\'')
                {
                    tokens.push(ConfToken {
                        kind: ConfKind::Number,
                        text: chars[start..i].iter().collect(),
                    });
                    continue;
                }
                i = hex_start; // backtrack to after #
            }
            tokens.push(ConfToken {
                kind: ConfKind::Comment,
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
                && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_')
            {
                i += 1;
            }
            tokens.push(ConfToken {
                kind: ConfKind::Number,
                text: chars[start..i].iter().collect(),
            });
            continue;
        }

        // ── punctuation / operators used in conf bindings ──
        if matches!(ch, '+' | '-' | '=' | ':' | ',' | '~' | '>' | '<') {
            tokens.push(ConfToken {
                kind: ConfKind::Operator,
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
                ConfKind::Keyword
            } else {
                ConfKind::Plain
            };
            tokens.push(ConfToken { kind, text: word });
            continue;
        }

        // ── anything else: whitespace, etc. ──
        let start = i;
        i += 1;
        tokens.push(ConfToken {
            kind: ConfKind::Plain,
            text: chars[start..i].iter().collect(),
        });
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(lang: Lang, source: &str) -> Vec<(TokenKind, String)> {
        Highlighter::parse(lang, source, 0)
            .lines
            .into_iter()
            .flatten()
            .map(|t| (t.kind, t.text))
            .collect()
    }

    fn kinds(lang: Lang, source: &str) -> Vec<TokenKind> {
        tokens(lang, source).into_iter().map(|(k, _)| k).collect()
    }

    #[test]
    fn detection_extensions() {
        assert_eq!(detect_lang(Some("a.rs"), None), Lang::Rust);
        assert_eq!(detect_lang(Some("a.py"), None), Lang::Python);
        assert_eq!(detect_lang(Some("a.c"), None), Lang::C);
        assert_eq!(detect_lang(Some("a.hpp"), None), Lang::Cpp);
        assert_eq!(detect_lang(Some("a.js"), None), Lang::JavaScript);
        assert_eq!(detect_lang(Some("a.tsx"), None), Lang::Tsx);
        assert_eq!(detect_lang(Some("a.html"), None), Lang::Html);
        assert_eq!(detect_lang(Some("a.css"), None), Lang::Css);
        assert_eq!(detect_lang(Some("a.toml"), None), Lang::Toml);
        assert_eq!(detect_lang(Some("a.json"), None), Lang::Json);
        assert_eq!(detect_lang(Some("a.sh"), None), Lang::Bash);
        assert_eq!(detect_lang(Some("a.go"), None), Lang::Go);
        assert_eq!(detect_lang(Some("a.md"), None), Lang::Markdown);
        assert_eq!(detect_lang(Some("a.conf"), None), Lang::Conf);
        assert_eq!(detect_lang(Some("a.xyz"), None), Lang::Plain);
    }

    #[test]
    fn detection_shebang_beats_extension() {
        assert_eq!(detect_lang(Some("script.txt"), Some("#!/usr/bin/env python3")), Lang::Python);
        assert_eq!(detect_lang(Some("run.py"), Some("#!/bin/bash")), Lang::Bash);
        assert_eq!(detect_lang(Some("tool.sh"), Some("#!/usr/bin/env node")), Lang::JavaScript);
    }

    #[test]
    fn detection_known_filenames() {
        assert_eq!(detect_lang(Some(".gitignore"), None), Lang::Conf);
        assert_eq!(detect_lang(Some("Makefile"), None), Lang::Plain);
    }

    #[test]
    fn rust_tokens() {
        let k = kinds(Lang::Rust, "fn main() { let x: String = String::new(); // hi\n}");
        assert!(k.contains(&TokenKind::Keyword));
        assert!(k.contains(&TokenKind::Function)); // main
        assert!(k.contains(&TokenKind::Type)); // String
        assert!(k.contains(&TokenKind::Comment)); // // hi
        let t = tokens(Lang::Rust, "fn main() { let x: String = String::new(); // hi\n}");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Function && s == "main"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Type && s == "String"));
    }

    #[test]
    fn rust_lifetimes_and_attributes() {
        let t = tokens(Lang::Rust, "#[derive(Debug)]\nstruct Foo<'a> { x: &'a str }\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Decorator && s.contains("derive")));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Lifetime && s == "'a"));
    }

    #[test]
    fn python_tokens() {
        let t = tokens(Lang::Python, "def foo(x):\n    # c\n    return f\"hi {x}\"\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Function && s == "foo"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Keyword && s == "def"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Comment && s == "# c"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::FStringPrefix && s == "f"));
    }

    #[test]
    fn c_tokens() {
        let t = tokens(Lang::C, "#include <stdio.h>\nint main(void) { /* b */ printf(\"hi\"); return 0; }\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Decorator && s.contains("include")));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Comment && s == "/* b */"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Builtin && s == "printf"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Number && s == "0"));
    }

    #[test]
    fn js_tokens() {
        let t = tokens(Lang::JavaScript, "function foo() { return `a${b}` + /re/; }\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Function && s == "foo"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::String && s == "`a${b}`"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::String && s == "/re/"));
    }

    #[test]
    fn html_tokens() {
        let t = tokens(Lang::Html, "<body class=\"x\">hi</body>\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Keyword && s == "body"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Builtin && s == "class"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::String && s == "\"x\""));
    }

    #[test]
    fn go_tokens() {
        let t = tokens(Lang::Go, "package main\nfunc main() { fmt.Println(\"x\") }\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Keyword && s == "package"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Function && s == "main"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Function && s == "Println"));
    }

    #[test]
    fn bash_tokens() {
        let t = tokens(Lang::Bash, "echo \"hello $name\"\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Function && s == "echo"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::String && s == "\"hello $name\""));
    }

    #[test]
    fn markdown_tokens() {
        let t = tokens(Lang::Markdown, "# Title\n\n```rust\nlet x = 1;\n```\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Function && s == "# Title"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::String && s.contains("```")));
    }

    #[test]
    fn multiline_comment_stays_comment() {
        // The whole point of tree-sitter: state across lines.
        let t = tokens(Lang::C, "/* start\nstill comment */\nint x;\n");
        let comment_lines: Vec<&(TokenKind, String)> =
            t.iter().filter(|(k, s)| *k == TokenKind::Comment).collect();
        assert_eq!(comment_lines.len(), 2);
        assert!(comment_lines[0].1 == "/* start");
        assert!(comment_lines[1].1 == "still comment */");
    }

    #[test]
    fn conf_fallback_still_works() {
        let t = tokens(Lang::Conf, "# comment\nkey = \"value\"\nnum = 42\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Comment && s == "# comment"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::String && s == "\"value\""));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Number && s == "42"));
    }

    #[test]
    fn tokens_cover_the_whole_source() {
        // The editor relies on the invariant that a line's tokens
        // concatenate back to the line (search/selection highlighting).
        let src = "fn main() {\n    let s = \"hi\\tthere\";\n    // c\n}\n";
        for lang in [Lang::Rust, Lang::C, Lang::Go, Lang::Json, Lang::Markdown] {
            let h = Highlighter::parse(lang, src, 0);
            for (i, line) in src.lines().enumerate() {
                let rebuilt: String = h.lines[i].iter().map(|t| t.text.as_str()).collect();
                assert_eq!(rebuilt, line, "lang {lang:?} line {i} diverges");
            }
        }
    }

    #[test]
    fn constants_and_properties() {
        let t = tokens(Lang::Rust, "struct Foo { x: i32 }\nfn f() { let b = true; }\n");
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Property && s == "x"));
        assert!(t.iter().any(|(k, s)| *k == TokenKind::Constant && s == "true"));

        let j = tokens(Lang::JavaScript, "const o = { a: 1 };\nlet z = null;\n");
        assert!(j.iter().any(|(k, s)| *k == TokenKind::Property && s == "a"));
        assert!(j.iter().any(|(k, s)| *k == TokenKind::Constant && s == "null"));

        let g = tokens(Lang::Go, "type P struct { X int }\n");
        assert!(g.iter().any(|(k, s)| *k == TokenKind::Property && s == "X"));
    }
}
