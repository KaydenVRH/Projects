//! Terminal-cell helpers: display widths, clipping, wrapping, line diffs.

use super::*;

/// Word-wrap `text` to `width` columns, keeping blank lines.
///
/// The chat transcript has no `Wrap` widget because wrapped lines need their
/// own prefix and style per message, and a word longer than the pane still has
/// to be broken or it would run off the edge.
pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out: Vec<String> = Vec::new();
    let push_long = |word: &str, out: &mut Vec<String>| {
        let chars: Vec<char> = word.chars().collect();
        for chunk in chars.chunks(width) {
            out.push(chunk.iter().collect());
        }
    };
    for line in text.split('\n') {
        if line.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut cur = String::new();
        for word in line.split_whitespace() {
            let wlen = word.chars().count();
            if cur.is_empty() {
                if wlen > width {
                    push_long(word, &mut out);
                } else {
                    cur.push_str(word);
                }
            } else if cur.chars().count() + 1 + wlen <= width {
                cur.push(' ');
                cur.push_str(word);
            } else {
                out.push(std::mem::take(&mut cur));
                if wlen > width {
                    push_long(word, &mut out);
                } else {
                    cur.push_str(word);
                }
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Approximate terminal cell width of a character (wcwidth).
/// ASCII is 1 cell, combining marks are 0, East Asian wide/fullwidth
/// characters and common emoji are 2.  Good enough to keep the
/// cursor aligned with the text it edits.
pub fn char_width(c: char) -> usize {
    if c == '\0' {
        return 0;
    }
    if c.is_ascii() {
        return 1;
    }
    match c as u32 {
        // Combining diacritics — zero width.
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20FF | 0xFE20..=0xFE2F => 0,
        // East Asian wide & fullwidth, Hangul, emoji, CJK extensions.
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE10..=0xFE19
        | 0xFE30..=0xFE6F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6
        | 0x16FE0..=0x16FFF | 0x17000..=0x187F7 | 0x1B000..=0x1B2FF
        | 0x1F300..=0x1F64F | 0x1F680..=0x1F6FF | 0x1F900..=0x1F9FF
        | 0x20000..=0x2FFFD | 0x30000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// Number of terminal cells a string occupies: tabs advance to the
/// next multiple of 8, control characters are zero-width (they are
/// dropped by [`visible_spans`]).
pub(super) fn col_width(s: &str) -> usize {
    let mut w = 0usize;
    for c in s.chars() {
        if c == '\t' {
            w = (w / 8 + 1) * 8;
        } else if !c.is_control() {
            w += char_width(c);
        }
    }
    w
}


/// Keep the tail of a path so the filename stays visible when space is
/// tight: `…/src/editor.rs` rather than clipping the end.
pub(super) fn elide_left(s: &str, max: usize) -> String {
    if max == 0 || col_width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for c in s.chars().rev() {
        let cw = char_width(c);
        if w + cw > max.saturating_sub(1) {
            break;
        }
        out.insert(0, c);
        w += cw;
    }
    format!("…{out}")
}

pub(super) fn prev_boundary(s: &str, mut i: usize) -> usize {
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub(super) fn next_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    i += 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Smallest char-boundary byte offset in `s` whose visual column is
/// at least `col`.  Returns `s.len()` if `col` is past the end.
pub(super) fn byte_at_col(s: &str, col: usize) -> usize {
    let mut w = 0usize;
    for (b, c) in s.char_indices() {
        if w >= col {
            return b;
        }
        if c == '\t' {
            w = (w / 8 + 1) * 8;
        } else if !c.is_control() {
            w += char_width(c);
        }
    }
    s.len()
}

/// Clip the content spans of one rendered line to the visible
/// horizontal window (measured in terminal cells, not bytes).
///
/// `spans[0]` is the line-number gutter and is always kept; the rest
/// must concatenate to `raw`.  Tabs are expanded to spaces and stray
/// control characters are dropped so the terminal frame never
/// desynchronises.
pub(super) fn visible_spans(
    raw: &str,
    mut spans: Vec<Span<'static>>,
    left: usize,
    visible_cols: usize,
) -> Vec<Span<'static>> {
    if spans.is_empty() {
        return spans;
    }
    let gutter = spans.remove(0);
    let mut out = vec![gutter];

    let left = raw.floor_char_boundary(left.min(raw.len()));
    let left_col = col_width(&raw[..left]);
    let end_byte = byte_at_col(raw, left_col.saturating_add(visible_cols));
    if end_byte <= left {
        return out;
    }

    let mut byte_off = 0usize;
    let mut current: Option<(Style, String)> = None;

    for span in spans {
        let text = span.content.as_ref();
        let span_start = byte_off;
        let span_end = span_start + text.len();
        byte_off = span_end;

        let s = span_start.max(left);
        let e = span_end.min(end_byte);
        if s >= e {
            continue;
        }
        let slice = &text[s - span_start..e - span_start];
        let mut col = col_width(&raw[..s]);
        for c in slice.chars() {
            if col >= left_col + visible_cols {
                break;
            }
            if c == '\t' {
                let stop = (col / 8 + 1) * 8;
                let spaces = (stop - col).min(left_col + visible_cols - col);
                push_text(&mut out, &mut current, " ".repeat(spaces), span.style);
                col = stop;
            } else if c.is_control() {
                // Drop — control characters would corrupt the frame.
            } else {
                push_text(&mut out, &mut current, c.to_string(), span.style);
                col += char_width(c);
            }
        }
    }
    if let Some((style, text)) = current {
        out.push(Span::styled(text, style));
    }
    out
}

/// Append `text` to the current run in `current`, flushing to `out`
/// when the style changes (keeps adjacent same-styled spans merged).
pub(super) fn push_text(
    out: &mut Vec<Span<'static>>,
    current: &mut Option<(Style, String)>,
    text: String,
    style: Style,
) {
    if text.is_empty() {
        return;
    }
    match current.as_mut() {
        Some((cs, ct)) if *cs == style => ct.push_str(&text),
        Some((cs, ct)) => {
            out.push(Span::styled(std::mem::take(ct), *cs));
            *cs = style;
            ct.push_str(&text);
        }
        None => *current = Some((style, text)),
    }
}

/// Clip already-styled display spans to a cell window, keeping whole
/// characters.
pub(super) fn clip_spans_cells(
    spans: Vec<Span<'static>>,
    left_col: usize,
    visible_cols: usize,
) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut current: Option<(Style, String)> = None;
    let mut col = 0usize;
    'outer: for span in spans {
        for c in span.content.chars() {
            let w = char_width(c);
            if col >= left_col && col < left_col + visible_cols {
                push_text(&mut out, &mut current, c.to_string(), span.style);
            }
            col += w;
            if col >= left_col + visible_cols {
                break 'outer;
            }
        }
    }
    if let Some((style, text)) = current {
        out.push(Span::styled(text, style));
    }
    out
}

/// Cheap content fingerprint used to decide whether the highlighter
/// cache is stale.
pub(super) fn hash_lines(lines: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    lines.hash(&mut h);
    h.finish()
}

/// Mark which lines of `new` were added or changed relative to `old`.
///
/// Common prefix/suffix are trimmed first; the middle block is diffed
/// with an LCS when small enough (so edits-in-place read as Changed
/// rather than Added), otherwise with a cheaper count heuristic.
pub(super) fn diff_lines(old: &[String], new: &[String]) -> Vec<LineChange> {
    let mut out = vec![LineChange::None; new.len()];
    let mut p = 0;
    while p < old.len() && p < new.len() && old[p] == new[p] {
        p += 1;
    }
    let mut s = 0;
    while s + p < old.len() && s + p < new.len() && old[old.len() - 1 - s] == new[new.len() - 1 - s] {
        s += 1;
    }
    let o = &old[p..old.len() - s];
    let n = &new[p..new.len() - s];
    if o.is_empty() {
        for i in p..new.len() - s {
            out[i] = LineChange::Added;
        }
        return out;
    }
    if n.is_empty() {
        return out; // pure deletion — nothing left to mark
    }
    if o.len() * n.len() <= 100_000 {
        // LCS: identical lines stay unmarked, unmatched new lines are
        // Changed while unmatched old lines remain, then Added.
        let m = o.len();
        let k = n.len();
        let mut dp = vec![vec![0u16; k + 1]; m + 1];
        for i in 1..=m {
            for j in 1..=k {
                dp[i][j] = if o[i - 1] == n[j - 1] {
                    dp[i - 1][j - 1] + 1
                } else {
                    dp[i - 1][j].max(dp[i][j - 1])
                };
            }
        }
        let mut new_match = vec![None; k];
        let (mut i, mut j) = (m, k);
        while i > 0 && j > 0 {
            if o[i - 1] == n[j - 1] {
                new_match[j - 1] = Some(i - 1);
                i -= 1;
                j -= 1;
            } else if dp[i - 1][j] >= dp[i][j - 1] {
                i -= 1;
            } else {
                j -= 1;
            }
        }
        let mut remaining = m - dp[m][k] as usize;
        for j in 0..k {
            if new_match[j].is_some() {
                continue;
            }
            out[p + j] = if remaining > 0 {
                remaining -= 1;
                LineChange::Changed
            } else {
                LineChange::Added
            };
        }
    } else {
        // Too big for LCS: equal block lengths read as Changed,
        // otherwise as Added.
        let kind = if o.len() == n.len() { LineChange::Changed } else { LineChange::Added };
        for i in p..new.len() - s {
            out[i] = kind;
        }
    }
    out
}
