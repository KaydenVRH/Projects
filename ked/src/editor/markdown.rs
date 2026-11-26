//! Markdown preview rendering and concealed-syntax column maths.

use super::*;

/// Render a markdown document into styled chunks (one per block),
/// using the tree-sitter-md block grammar plus the inline grammar for
/// emphasis/strong/code/links.
pub(super) fn render_markdown(lines: &[String], theme: &Theme) -> Vec<MdChunk> {
    let source = lines.join("\n");
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(&source, None) else {
        return Vec::new();
    };
    let mut chunks = Vec::new();
    md_blocks(tree.root_node(), &source, theme, &mut chunks, 0);
    chunks
}

/// Walk the children of a block-level node, collecting chunks.
pub(super) fn md_blocks(node: tree_sitter::Node, source: &str, theme: &Theme, chunks: &mut Vec<MdChunk>, list_depth: usize) {
    for i in 0..node.child_count() {
        md_block(node.child(i as u32).unwrap(), source, theme, chunks, list_depth);
    }
}

/// Render one block-level markdown node into a chunk.
pub(super) fn md_block(node: tree_sitter::Node, source: &str, theme: &Theme, chunks: &mut Vec<MdChunk>, list_depth: usize) {
    let dim = Style::new().fg(theme.comment.fg.unwrap_or(theme.fg));
    let fg = Style::new().fg(theme.fg);

    match node.kind() {
        "section" | "document" => {
            md_blocks(node, source, theme, chunks, list_depth);
        }
        "list" => {
            for i in 0..node.child_count() {
                let c = node.child(i as u32).unwrap();
                if c.kind() == "list_item" {
                    md_list_item(c, source, theme, chunks, list_depth);
                } else if c.kind() == "list" {
                    md_block(c, source, theme, chunks, list_depth + 1);
                }
            }
        }
        "atx_heading" | "setext_heading" => {
            let level = md_heading_level(node);
            let base = match level {
                1 => Style::new().fg(theme.keyword.fg.unwrap_or(theme.fg)).add_modifier(Modifier::BOLD),
                2 => Style::new().fg(theme.rstype.fg.unwrap_or(theme.fg)).add_modifier(Modifier::BOLD),
                3 => Style::new().fg(theme.builtin.fg.unwrap_or(theme.fg)).add_modifier(Modifier::BOLD),
                _ => fg.add_modifier(Modifier::BOLD),
            };
            let text = md_heading_text(node, source).unwrap_or("").to_string();
            let mut lines = vec![Line::from("")];
            lines.push(Line::from(md_inline_spans(&text, theme, base)));
            if level == 1 {
                let w = text.chars().count();
                lines.push(Line::from(Span::styled("─".repeat(w), Style::new().fg(theme.border))));
            }
            lines.push(Line::from(""));
            chunks.push(md_chunk(node, lines));
        }
        "paragraph" => {
            // paragraph children are `inline` nodes
            let mut spans = Vec::new();
            for i in 0..node.child_count() {
                let c = node.child(i as u32).unwrap();
                if c.kind() == "inline" {
                    let t = c.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    spans.extend(md_inline_spans(&t, theme, fg));
                }
            }
            if !spans.is_empty() {
                chunks.push(md_chunk(node, vec![Line::from(spans)]));
            }
        }
        "fenced_code_block" | "indented_code_block" => {
            let mut info = String::new();
            let mut content: Vec<&str> = Vec::new();
            for i in 0..node.child_count() {
                let c = node.child(i as u32).unwrap();
                match c.kind() {
                    "info_string" => {
                        info = c.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    }
                    "code_fence_content" => {
                        let t = c.utf8_text(source.as_bytes()).unwrap_or("");
                        content = t.lines().collect();
                    }
                    _ => {}
                }
            }
            let mut lines = vec![Line::from(vec![
                Span::styled("```".to_string(), dim),
                Span::styled(info, Style::new().fg(theme.builtin.fg.unwrap_or(theme.fg))),
            ])];
            for l in content {
                lines.push(Line::from(Span::styled(l.to_string(), fg)));
            }
            lines.push(Line::from(Span::styled("```".to_string(), dim)));
            chunks.push(md_chunk(node, lines));
        }
        "block_quote" => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("");
            let lines = text
                .lines()
                .map(|l| {
                    let l = l.trim_start_matches('>').trim_start();
                    Line::from(vec![
                        Span::styled("│ ".to_string(), dim),
                        Span::styled(l.to_string(), fg),
                    ])
                })
                .collect();
            chunks.push(md_chunk(node, lines));
        }
        "thematic_break" => {
            chunks.push(md_chunk(node, vec![Line::from(Span::styled(
                "─".repeat(40),
                dim,
            ))]));
        }
        "html_block" => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("");
            let lines = text
                .lines()
                .map(|l| Line::from(Span::styled(l.to_string(), dim)))
                .collect();
            chunks.push(md_chunk(node, lines));
        }
        _ => {
            // tables etc.: render the raw text
            let text = node.utf8_text(source.as_bytes()).unwrap_or("");
            if !text.trim().is_empty() && node.child_count() > 0 {
                let lines = text
                    .lines()
                    .map(|l| Line::from(Span::styled(l.to_string(), fg)))
                    .collect();
                chunks.push(md_chunk(node, lines));
            }
        }
    }
}

/// Render a list item: bullet + inline content, then nested lists.
pub(super) fn md_list_item(node: tree_sitter::Node, source: &str, theme: &Theme, chunks: &mut Vec<MdChunk>, list_depth: usize) {
    let fg = Style::new().fg(theme.fg);
    let indent = "  ".repeat(list_depth);
    let mut spans = vec![Span::styled(
        format!("{indent}• "),
        Style::new().fg(theme.builtin.fg.unwrap_or(theme.fg)),
    )];
    for i in 0..node.child_count() {
        let c = node.child(i as u32).unwrap();
        if c.kind() == "paragraph" {
            for j in 0..c.child_count() {
                let gc = c.child(j as u32).unwrap();
                if gc.kind() == "inline" {
                    let t = gc.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    spans.extend(md_inline_spans(&t, theme, fg));
                }
            }
        }
    }
    chunks.push(md_chunk(node, vec![Line::from(spans)]));
    for i in 0..node.child_count() {
        let c = node.child(i as u32).unwrap();
        if c.kind() == "list" {
            md_block(c, source, theme, chunks, list_depth + 1);
        }
    }
}

pub(super) fn md_chunk(node: tree_sitter::Node, lines: Vec<Line<'static>>) -> MdChunk {
    MdChunk {
        src_start: node.start_position().row,
        lines,
    }
}

pub(super) fn md_heading_level(node: tree_sitter::Node) -> usize {
    for i in 0..node.child_count() {
        let k = node.child(i as u32).unwrap().kind();
        if let Some(rest) = k.strip_prefix("atx_h") {
            if let Some(n) = rest.chars().next().and_then(|c| c.to_digit(10)) {
                return n as usize;
            }
        }
        if k == "setext_h1_underline" {
            return 1;
        }
        if k == "setext_h2_underline" {
            return 2;
        }
    }
    6
}

/// The heading's inline text (atx: `heading_content` field; setext:
/// the paragraph child).
pub(super) fn md_heading_text<'a>(node: tree_sitter::Node<'a>, source: &'a str) -> Option<&'a str> {
    if let Some(n) = node.child_by_field_name("heading_content") {
        return n.utf8_text(source.as_bytes()).ok();
    }
    for i in 0..node.child_count() {
        let c = node.child(i as u32).unwrap();
        if c.kind() == "inline" || c.kind() == "paragraph" {
            return c.utf8_text(source.as_bytes()).ok();
        }
    }
    None
}

/// Style one piece of inline markdown: emphasis (italic), strong
/// (bold), code spans (string colour), links (underlined accent).
pub(super) fn md_inline_spans(text: &str, theme: &Theme, base: Style) -> Vec<Span<'static>> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_md::INLINE_LANGUAGE.into())
        .is_err()
    {
        return vec![Span::styled(text.to_string(), base)];
    }
    let Some(tree) = parser.parse(text, None) else {
        return vec![Span::styled(text.to_string(), base)];
    };
    let root = tree.root_node();
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut last = 0;
    for i in 0..root.child_count() {
        let c = root.child(i as u32).unwrap();
        let start = c.start_byte();
        if start > last {
            out.push(Span::styled(text[last..start].to_string(), base));
        }
        md_inline_child(c, text, theme, base, &mut out);
        last = c.end_byte();
    }
    if last < text.len() {
        out.push(Span::styled(text[last..].to_string(), base));
    }
    if out.is_empty() {
        out.push(Span::styled(text.to_string(), base));
    }
    out
}

pub(super) fn md_inline_child(node: tree_sitter::Node, text: &str, theme: &Theme, base: Style, out: &mut Vec<Span<'static>>) {
    match node.kind() {
        "emphasis" => {
            let inner = md_delimited_inner(node, text);
            out.push(Span::styled(inner, base.add_modifier(Modifier::ITALIC)));
        }
        "strong_emphasis" => {
            let inner = md_delimited_inner(node, text);
            out.push(Span::styled(inner, base.add_modifier(Modifier::BOLD)));
        }
        "code_span" => {
            let inner = md_delimited_inner(node, text);
            out.push(Span::styled(inner, theme.string));
        }
        "inline_link" | "image" => {
            // show the link/alt text, not the URL
            let mut shown = false;
            for i in 0..node.child_count() {
                let c = node.child(i as u32).unwrap();
                if c.kind() == "link_text" {
                    let lt = c.utf8_text(text.as_bytes()).unwrap_or("").to_string();
                    out.push(Span::styled(
                        lt,
                        Style::new()
                            .fg(theme.builtin.fg.unwrap_or(theme.fg))
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                    shown = true;
                }
            }
            if !shown {
                let t = node.utf8_text(text.as_bytes()).unwrap_or("");
                out.push(Span::styled(t.to_string(), base));
            }
        }
        _ => {
            let t = node.utf8_text(text.as_bytes()).unwrap_or("");
            if node.child_count() > 0 {
                for i in 0..node.child_count() {
                    md_inline_child(node.child(i as u32).unwrap(), text, theme, base, out);
                }
            } else if !t.is_empty() {
                out.push(Span::styled(t.to_string(), base));
            }
        }
    }
}

/// The text of a delimiter-wrapped node (emphasis, code spans) with
/// the delimiter characters stripped from both ends.
pub(super) fn md_delimited_inner(node: tree_sitter::Node, text: &str) -> String {
    let raw = node.utf8_text(text.as_bytes()).unwrap_or("");
    match raw.chars().next() {
        Some(d) if !d.is_alphanumeric() && !d.is_whitespace() => {
            raw.trim_start_matches(d).trim_end_matches(d).to_string()
        }
        _ => raw.to_string(),
    }
}

/// Map markdown display segments to styled spans for one line.
pub(super) fn md_seg_spans(segs: &[highlight::MdSeg], theme: &Theme) -> Vec<Span<'static>> {
    let fg = Style::new().fg(theme.fg);
    let dim = Style::new().fg(theme.comment.fg.unwrap_or(theme.fg));
    segs.iter()
        .map(|s| {
            let style = match s.kind {
                highlight::MdSegKind::Plain => fg,
                highlight::MdSegKind::Heading(1) => {
                    fg.fg(theme.keyword.fg.unwrap_or(theme.fg)).add_modifier(Modifier::BOLD)
                }
                highlight::MdSegKind::Heading(2) => {
                    fg.fg(theme.rstype.fg.unwrap_or(theme.fg)).add_modifier(Modifier::BOLD)
                }
                highlight::MdSegKind::Heading(3) => {
                    fg.fg(theme.builtin.fg.unwrap_or(theme.fg)).add_modifier(Modifier::BOLD)
                }
                highlight::MdSegKind::Heading(_) => fg.add_modifier(Modifier::BOLD),
                highlight::MdSegKind::Strong => fg.add_modifier(Modifier::BOLD),
                highlight::MdSegKind::Emph => fg.add_modifier(Modifier::ITALIC),
                highlight::MdSegKind::Code => theme.string,
                highlight::MdSegKind::Link => fg
                    .fg(theme.builtin.fg.unwrap_or(theme.fg))
                    .add_modifier(Modifier::UNDERLINED),
                highlight::MdSegKind::Bullet => fg.fg(theme.builtin.fg.unwrap_or(theme.fg)),
                highlight::MdSegKind::QuoteMark => dim,
                highlight::MdSegKind::Fence => dim,
                highlight::MdSegKind::FenceInfo => fg.fg(theme.builtin.fg.unwrap_or(theme.fg)),
            };
            Span::styled(s.display.clone(), style)
        })
        .collect()
}

/// Effective display segments for the cursor's line: width-changing
/// concealed segments (hidden markers) that contain the cursor are
/// revealed, so the cursor is always visibly on a character and
/// moves one cell per keypress.
pub(super) fn md_cursor_segs(segs: &[highlight::MdSeg], cx: usize) -> Vec<highlight::MdSeg> {
    segs.iter()
        .map(|s| {
            let mut s = s.clone();
            if s.display != s.raw && s.start <= cx && cx < s.end {
                s.display = s.raw.clone();
            }
            s
        })
        .collect()
}

/// Display column (in terminal cells) of a raw byte offset on a
/// concealed markdown line: concealed characters contribute nothing.
pub(super) fn md_segs_col(segs: &[highlight::MdSeg], byte: usize) -> usize {    let mut col = 0usize;
    for s in segs {
        if s.end <= byte {
            col += col_width(&s.display);
        } else if s.start < byte {
            // Cursor inside the segment: visible segments contribute
            // their prefix width; concealed/replaced ones don't.
            if s.display == s.raw {
                let cut = byte - s.start;
                col += col_width(&s.display[..cut.min(s.display.len())]);
            }
        }
    }
    col
}

/// Inverse of [`md_segs_col`]: the raw byte offset whose display
/// column is at least `target` (used for horizontal scrolling on
/// concealed markdown lines).
pub(super) fn md_segs_byte_at_col(segs: &[highlight::MdSeg], target: usize) -> usize {
    let mut w = 0usize;
    for s in segs {
        let dw = col_width(&s.display);
        if w + dw > target {
            if s.display == s.raw {
                let mut c = w;
                for (i, ch) in s.display.char_indices() {
                    if c >= target {
                        return s.start + i;
                    }
                    c += char_width(ch);
                }
                return s.end;
            }
            // Concealed/replaced segment — land at its raw start.
            return s.start;
        }
        w += dw;
    }
    segs.last().map(|s| s.end).unwrap_or(0)
}

/// Display byte offset within the concatenated display text of a
/// line, for a raw byte offset (used to map selection edges onto
/// concealed markdown display spans).
pub(super) fn md_segs_display_byte(segs: &[highlight::MdSeg], byte: usize) -> usize {    let mut n = 0usize;
    for s in segs {
        if s.end <= byte {
            n += s.display.len();
        } else if s.start < byte {
            if s.display.len() == s.end - s.start {
                let cut = byte - s.start;
                n += s.display[..cut.min(s.display.len())].len();
            }
        }
    }
    n
}
