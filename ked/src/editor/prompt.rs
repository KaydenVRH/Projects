//! Single-line prompt editing, shared by every text field.

use super::*;

/// Delete the word before the cursor (Ctrl+W).
pub(super) fn delete_word_before(buf: &mut String, cursor: &mut usize) {
    let mut start = *cursor;
    while start > 0 {
        let p = prev_boundary(buf, start);
        let c = buf[p..start].chars().next().unwrap_or(' ');
        if c.is_whitespace() {
            start = p;
        } else {
            break;
        }
    }
    while start > 0 {
        let p = prev_boundary(buf, start);
        let c = buf[p..start].chars().next().unwrap_or(' ');
        if c.is_whitespace() {
            break;
        }
        start = p;
    }
    buf.replace_range(start..*cursor, "");
    *cursor = start;
}

/// Apply a key to a single-line prompt.  Returns true when handled.
pub(super) fn edit_prompt(buf: &mut String, cursor: &mut usize, key: &KeyEvent) -> bool {
    *cursor = (*cursor).min(buf.len());
    while *cursor > 0 && !buf.is_char_boundary(*cursor) {
        *cursor -= 1;
    }
    match key.code {
        KeyCode::Char('a') if key.modifiers == KeyModifiers::CONTROL => *cursor = 0,
        KeyCode::Char('e') if key.modifiers == KeyModifiers::CONTROL => *cursor = buf.len(),
        KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
            buf.clear();
            *cursor = 0;
        }
        KeyCode::Char('w') if key.modifiers == KeyModifiers::CONTROL => {
            delete_word_before(buf, cursor);
        }
        KeyCode::Char('h') if key.modifiers == KeyModifiers::CONTROL => {
            *cursor = prev_boundary(buf, *cursor);
        }
        KeyCode::Left => *cursor = prev_boundary(buf, *cursor),
        KeyCode::Right => *cursor = next_boundary(buf, *cursor),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = buf.len(),
        KeyCode::Backspace => {
            if *cursor > 0 {
                let p = prev_boundary(buf, *cursor);
                buf.replace_range(p..*cursor, "");
                *cursor = p;
            }
        }
        KeyCode::Delete => {
            if *cursor < buf.len() {
                let n = next_boundary(buf, *cursor);
                buf.replace_range(*cursor..n, "");
            }
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            buf.insert(*cursor, c);
            *cursor += c.len_utf8();
        }
        _ => return false,
    }
    true
}

/// Visible slice of a prompt line, scrolled so the cursor stays on
/// screen.  Returns (visible text, cursor cell offset within it).
pub(super) fn prompt_view(buf: &str, cursor: usize, width: u16) -> (String, u16) {
    let width = width.max(1) as usize;
    let cursor = cursor.min(buf.len());
    let cursor_cell = col_width(&buf[..cursor]);
    let start_cell = cursor_cell.saturating_sub(width - 1);
    let mut out = String::new();
    let mut cell = 0usize;
    for c in buf.chars() {
        let w = if c == '\t' {
            (8 - cell % 8).max(1)
        } else if c.is_control() {
            0
        } else {
            char_width(c)
        };
        if cell + w <= start_cell {
            cell += w;
            continue;
        }
        if cell < start_cell {
            out.push(' ');
            cell = start_cell;
            continue;
        }
        if cell >= start_cell + width {
            break;
        }
        out.push(c);
        cell += w;
    }
    let x = cursor_cell.saturating_sub(start_cell).min(width - 1) as u16;
    (out, x)
}
