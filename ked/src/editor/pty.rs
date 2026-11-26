//! The pop-up shell (Ctrl+J): opening it, forwarding keys as terminal
//! byte sequences, and drawing the vt100 screen.

use super::*;

impl Editor {
    /// Open the pop-up shell (Ctrl+J or `:shell`).
    pub(super) fn open_shell(&mut self) {
        if self.shell.is_some() {
            self.state = State::Shell;
            return;
        }
        match ShellProcess::spawn() {
            Ok((shell, reader)) => {
                shell.force_raw_mode();
                self.shell = Some(shell);
                self.shell_reader = Some(reader);
                self.state = State::Shell;
                // Size the PTY to the popup right away so the first
                // prompt is already at the right width (the render
                // pass refines it to the exact inner rect).
                let rows = ((self.cache_h as f32 * 0.85) as u16)
                    .saturating_sub(2)
                    .max(1);
                let cols = ((self.cache_w as f32 * 0.92) as u16)
                    .saturating_sub(2)
                    .max(1);
                if let Some(ref mut s) = self.shell {
                    s.resize(rows, cols);
                }
            }
            Err(e) => {
                self.flash = Some(format!("shell failed: {e}"));
            }
        }
    }

    pub(super) fn handle_shell_state(&mut self, key: KeyEvent) -> bool {
        let Some(shell) = self.shell.as_ref() else {
            self.state = State::Normal;
            return true;
        };
        // Force raw mode on the slave side so the terminal driver
        // never echoes (even if the shell re-enabled it).
        shell.force_raw_mode();
        let bytes = key_to_bytes(key, shell.application_cursor());
        if !bytes.is_empty() {
            shell.write(&bytes);
        }
        true
    }

    // ── Hermes chat (Ctrl+Shift+H, `:hermes`) ──────────────────────

    pub(super) fn render_shell(&mut self, f: &mut Frame, area: Rect) {
        let theme = self.theme.theme();
        // The shell is the one oversized overlay: it's a real
        // terminal, so it keeps most of the screen — same frame style.
        let popup = modal_rect(area, 0.92, 0.85);

        f.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Shell — type exit to close ")
            .title_style(Style::new().fg(theme.fg).bg(theme.status_bg))
            .border_style(Style::new().fg(theme.border));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        if inner.height < 1 || inner.width < 1 {
            return;
        }

        // Resize the emulated screen and the kernel PTY to the popup.
        let (lines, cursor) = match self.shell.as_mut() {
            Some(shell) => {
                shell.resize(inner.height, inner.width);
                (
                    shell.styled_lines(theme.fg, theme.bg),
                    shell.cursor(),
                )
            }
            None => (Vec::new(), None),
        };

        f.render_widget(
            Paragraph::new(Text::from(lines)).style(Style::new().bg(theme.bg)),
            inner,
        );

        // Password prompts and full-screen apps can hide the cursor.
        if let Some((row, col)) = cursor {
            if row < inner.height && col < inner.width {
                let pos = Position::new(inner.x + col, inner.y + row);
                self.cursor_screen = Some((pos.x, pos.y));
                f.set_cursor_position(pos);
            }
        }
    }
}

/// Encode a key for the PTY application as the bytes a terminal would
/// send.  `app_cursor` selects SS3 (`ESC O A`) instead of CSI
/// (`ESC [ A`) for cursor keys, which full-screen apps request.
pub(super) fn key_to_bytes(key: KeyEvent, app_cursor: bool) -> Vec<u8> {
    let mods = key.modifiers;
    // Cmd/Super combinations belong to the editor, not the shell.
    if mods.contains(KeyModifiers::SUPER) {
        return Vec::new();
    }
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let shift = mods.contains(KeyModifiers::SHIFT);
    // xterm modifier parameter: 1 + shift + 2*alt + 4*ctrl.
    let modifier = 1 + (shift as u8) + 2 * (alt as u8) + 4 * (ctrl as u8);
    let mut out = Vec::new();

    match key.code {
        KeyCode::Char(c) => {
            if ctrl && c.is_ascii_alphabetic() {
                let b = (c.to_ascii_uppercase() as u8 - b'A') + 1;
                if alt {
                    out.push(0x1b);
                }
                out.push(b);
            } else if ctrl {
                match c {
                    ' ' | '@' => out.push(0x00),
                    '[' => out.push(0x1b),
                    '\\' => out.push(0x1c),
                    ']' => out.push(0x1d),
                    '^' => out.push(0x1e),
                    '_' => out.push(0x1f),
                    _ => {
                        if alt {
                            out.push(0x1b);
                        }
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    }
                }
            } else {
                if alt {
                    out.push(0x1b);
                }
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        KeyCode::Enter => out.push(b'\r'),
        KeyCode::Backspace => out.push(0x7f),
        KeyCode::Tab => out.push(b'\t'),
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Esc => out.push(0x1b),
        KeyCode::Left => csi_arrow(&mut out, b'D', modifier, app_cursor),
        KeyCode::Right => csi_arrow(&mut out, b'C', modifier, app_cursor),
        KeyCode::Up => csi_arrow(&mut out, b'A', modifier, app_cursor),
        KeyCode::Down => csi_arrow(&mut out, b'B', modifier, app_cursor),
        KeyCode::Home => csi_arrow(&mut out, b'H', modifier, app_cursor),
        KeyCode::End => csi_arrow(&mut out, b'F', modifier, app_cursor),
        KeyCode::Delete => csi_tilde(&mut out, 3, modifier),
        KeyCode::Insert => csi_tilde(&mut out, 2, modifier),
        KeyCode::PageUp => csi_tilde(&mut out, 5, modifier),
        KeyCode::PageDown => csi_tilde(&mut out, 6, modifier),
        KeyCode::F(n) => fkey_bytes(&mut out, n, modifier),
        _ => {}
    }
    out
}

pub(super) fn csi_arrow(out: &mut Vec<u8>, final_byte: u8, modifier: u8, app: bool) {
    if modifier > 1 {
        out.extend_from_slice(
            format!("\x1b[1;{modifier}{}", final_byte as char).as_bytes(),
        );
    } else if app {
        out.extend_from_slice(&[0x1b, b'O', final_byte]);
    } else {
        out.extend_from_slice(&[0x1b, b'[', final_byte]);
    }
}

pub(super) fn csi_tilde(out: &mut Vec<u8>, n: u8, modifier: u8) {
    if modifier > 1 {
        out.extend_from_slice(format!("\x1b[{n};{modifier}~").as_bytes());
    } else {
        out.extend_from_slice(format!("\x1b[{n}~").as_bytes());
    }
}

pub(super) fn fkey_bytes(out: &mut Vec<u8>, n: u8, modifier: u8) {
    // F1-F4 use SS3 (or CSI 1;<m> when modified); F5+ use CSI <n>~.
    let (code, ss3) = match n {
        1 => ("P", true),
        2 => ("Q", true),
        3 => ("R", true),
        4 => ("S", true),
        5 => ("15", false),
        6 => ("17", false),
        7 => ("18", false),
        8 => ("19", false),
        9 => ("20", false),
        10 => ("21", false),
        11 => ("23", false),
        12 => ("24", false),
        _ => return,
    };
    if modifier > 1 {
        if ss3 {
            out.extend_from_slice(format!("\x1b[1;{modifier}{code}").as_bytes());
        } else {
            out.extend_from_slice(format!("\x1b[{code};{modifier}~").as_bytes());
        }
    } else if ss3 {
        out.extend_from_slice(format!("\x1bO{code}").as_bytes());
    } else {
        out.extend_from_slice(format!("\x1b[{code}~").as_bytes());
    }
}
