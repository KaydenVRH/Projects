use super::*;
use super::pty::key_to_bytes;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn prompt_edits_at_the_cursor() {
    let mut buf = String::new();
    let mut cur = 0;
    for c in "hi x".chars() {
        assert!(edit_prompt(&mut buf, &mut cur, &key(KeyCode::Char(c))));
    }
    assert_eq!(buf, "hi x");
    assert_eq!(cur, 4);
    // Left over the x, then delete the space.
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Left));
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Backspace));
    assert_eq!(buf, "hix");
    assert_eq!(cur, 2);
    // Home, then insert at the front.
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Home));
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Char('>')));
    assert_eq!(buf, ">hix");
    assert_eq!(cur, 1);
    // Delete removes forward.
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Delete));
    assert_eq!(buf, ">ix");
}

#[test]
fn prompt_word_delete_and_clear() {
    let mut buf = "git commit -m".to_string();
    let mut cur = buf.len();
    edit_prompt(&mut buf, &mut cur, &ctrl('w'));
    assert_eq!(buf, "git commit ");
    assert_eq!(cur, 11);
    edit_prompt(&mut buf, &mut cur, &ctrl('u'));
    assert_eq!(buf, "");
    assert_eq!(cur, 0);
}

#[test]
fn prompt_unicode_moves_by_char() {
    let mut buf = "héllo".to_string();
    let mut cur = buf.len();
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Left));
    assert_eq!(cur, 5); // before 'o', not mid-'é'
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Home));
    assert_eq!(cur, 0);
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Right));
    assert_eq!(cur, 1); // after 'h'
    edit_prompt(&mut buf, &mut cur, &key(KeyCode::Right));
    assert_eq!(cur, 3); // after the two-byte 'é'
}

#[test]
fn prompt_view_scrolls_to_keep_cursor_visible() {
    assert_eq!(prompt_view("hello", 5, 10), ("hello".to_string(), 5));
    let (text, x) = prompt_view("abcdefghijklmnop", 16, 10);
    assert_eq!(text, "hijklmnop");
    assert_eq!(x, 9);
    // Spaces are preserved as cells.
    let (text, x) = prompt_view("ab cd", 5, 10);
    assert_eq!(text, "ab cd");
    assert_eq!(x, 5);
}

#[test]
fn elide_left_keeps_the_filename_visible() {
    assert_eq!(elide_left("short.rs", 20), "short.rs");
    let s = "/Users/kayden/programs/projects/ked/src/editor.rs";
    let out = elide_left(s, 20);
    assert_eq!(col_width(&out), 20);
    assert!(out.starts_with('…'));
    assert!(out.ends_with("src/editor.rs"));
    assert_eq!(elide_left("abc", 0), "abc");
    assert_eq!(elide_left("abc", 1), "…");
}

#[test]
fn col_width_counts_cells_not_bytes() {
    assert_eq!(col_width("abc"), 3);
    assert_eq!(col_width("héllo"), 5); // 2-byte char, 1 cell
    assert_eq!(col_width("日本語"), 6); // 3 wide chars
    assert_eq!(col_width("🐍"), 2); // emoji is 2 cells
    assert_eq!(col_width("a\tb"), 9); // tab advances to col 8
    assert_eq!(col_width("e\u{301}"), 1); // combining mark is 0
}

#[test]
fn byte_at_col_finds_char_boundaries() {
    let s = "héllo";
    assert_eq!(byte_at_col(s, 1), 1); // after 'h'
    assert_eq!(byte_at_col(s, 2), 3); // after 'é' (1 byte + 2 bytes)
    assert_eq!(byte_at_col(s, 99), s.len());
}

#[test]
fn visible_spans_clips_and_expands_tabs() {
    let raw = "ab\tcd";
    let gutter = Span::styled("1│".to_string(), Style::new());

    // Window: columns [0, 4) → "ab  " (tab clipped at the edge).
    let spans = vec![
        gutter.clone(),
        Span::styled(raw.to_string(), Style::new()),
    ];
    let out = visible_spans(raw, spans, 0, 4);
    let text: String = out.iter().skip(1).map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "ab  ");

    // Window: columns [8, 12) → "cd" (tab is left of the window).
    let spans = vec![gutter, Span::styled(raw.to_string(), Style::new())];
    let out = visible_spans(raw, spans, byte_at_col(raw, 8), 4);
    let text: String = out.iter().skip(1).map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "cd");
}

#[test]
fn visible_spans_drops_control_chars() {
    let raw = "a\x08b";
    let gutter = Span::styled("1│".to_string(), Style::new());
    let spans = vec![gutter, Span::styled(raw.to_string(), Style::new())];
    let out = visible_spans(raw, spans, 0, 8);
    let text: String = out.iter().skip(1).map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "ab");
}

#[test]
fn undo_redo_roundtrip_restores_cursor() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();

    // One insert session: type "hello".
    ed.mode = Mode::Insert;
    ed.insert_saved = false;
    for ch in "hello".chars() {
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    ed.mode = Mode::Normal;
    assert_eq!(ed.lines[0], "hello");
    assert_eq!(ed.cx, 5);

    // u — undo the whole session at once.
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
    assert!(ed.lines[0].is_empty());
    assert_eq!(ed.cx, 0);

    // Ctrl+R — redo it back (cursor restored too).
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(ed.lines[0], "hello");
    assert_eq!(ed.cx, 5);

    // Undo again — redo stack must still round-trip.
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
    assert!(ed.lines[0].is_empty());
}

#[test]
fn ctrl_chars_do_not_insert_in_insert_mode() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();

    ed.mode = Mode::Insert;
    ed.insert_saved = false;
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert!(ed.lines[0].is_empty());

    // Ctrl+C exits insert mode back to normal.
    let keep = ed.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(keep);
    assert_eq!(ed.mode, Mode::Normal);
}

#[test]
fn bracketed_paste_inserts_literally() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();

    // Insert mode: multi-line paste types at the cursor.
    ed.mode = Mode::Insert;
    ed.insert_saved = false;
    ed.paste_from_terminal("foo\nbar");
    assert_eq!(ed.lines, vec!["foo".to_string(), "bar".to_string()]);
    assert_eq!(ed.cx, 3);
    assert_eq!(ed.cy, 1);

    // Normal mode: paste after the cursor like `p` — never as
    // commands.
    ed.mode = Mode::Normal;
    ed.paste_from_terminal("i not a command\ndd neither");
    assert!(ed.lines.contains(&"i not a command".to_string()));
    assert!(ed.lines.contains(&"dd neither".to_string()));

    // Windows line endings are normalised.
    ed.mode = Mode::Insert;
    ed.insert_saved = false;
    ed.cy = 0;
    ed.cx = 0;
    ed.paste_from_terminal("a\r\nb");
    // Inserting at col 0 splits the line: "a" stays, the rest is
    // carried onto the next line after the pasted "b".
    assert_eq!(ed.lines[0], "a");
    assert_eq!(ed.lines[1], "bfoo");
}

#[test]
fn paste_after_multiline_goes_below_cursor() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    ed.lines = vec!["one".to_string(), "two".to_string()];
    ed.cy = 0;
    ed.cx = 0;
    ed.paste_after("x\ny");
    assert_eq!(ed.lines, vec!["one", "x", "y", "two"]);
    assert_eq!(ed.cy, 2);
}

#[test]
fn base64_roundtrip() {
    assert_eq!(base64_encode(b"ked"), "a2Vk");
    assert_eq!(base64_encode(b"ab"), "YWI=");
    assert_eq!(base64_encode(b"abc"), "YWJj");
    assert_eq!(base64_encode(b"hello world"), "aGVsbG8gd29ybGQ=");
}

#[test]
fn diff_lines_marks_added_and_changed() {
    use LineChange::*;
    let o = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // Appended line.
    let d = diff_lines(&o(&["a", "b"]), &o(&["a", "b", "c"]));
    assert_eq!(d, vec![None, None, Added]);

    // Middle line changed in place.
    let d = diff_lines(&o(&["a", "b", "c"]), &o(&["a", "B", "c"]));
    assert_eq!(d, vec![None, Changed, None]);

    // Line inserted in the middle.
    let d = diff_lines(&o(&["a", "c"]), &o(&["a", "b", "c"]));
    assert_eq!(d, vec![None, Added, None]);

    // Deletion leaves nothing to mark.
    let d = diff_lines(&o(&["a", "b", "c"]), &o(&["a", "c"]));
    assert_eq!(d, vec![None, None]);

    // Two replaced by three: paired in order as two Changed
    // lines plus one Added.
    let d = diff_lines(&o(&["a", "x", "y", "d"]), &o(&["a", "p", "q", "r", "d"]));
    assert_eq!(d, vec![None, Changed, Changed, Added, None]);

    // Identical files.
    let d = diff_lines(&o(&["a", "b"]), &o(&["a", "b"]));
    assert_eq!(d, vec![None, None]);
}

#[test]
fn editing_clears_change_markers() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    ed.line_changes = Some(vec![LineChange::Added]);
    ed.save_undo();
    assert!(ed.line_changes.is_none());
}

#[test]
fn markdown_preview_renders_blocks() {
    let lines: Vec<String> = vec![
        "# Title".into(),
        "".into(),
        "Some *em* and **bold** text.".into(),
        "".into(),
        "- item one".into(),
        "  - nested".into(),
        "".into(),
        "```rust".into(),
        "let x = 1;".into(),
        "```".into(),
        "".into(),
        "> quoted".into(),
    ];
    let theme = crate::theme::ThemeKind::Default.theme();
    let chunks = render_markdown(&lines, &theme);
    let all: Vec<String> = chunks
        .iter()
        .flat_map(|c| c.lines.iter())
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect();
    let joined = all.join("\n");
    assert!(joined.contains("Title"), "heading text missing: {joined}");
    assert!(joined.contains("Some em and bold text."));
    assert!(joined.contains("• item one"));
    assert!(joined.contains("• nested"));
    assert!(joined.contains("let x = 1;"));
    assert!(joined.contains("│ quoted"));
    // scroll mapping: the chunk containing source line 2 exists
    assert!(chunks.iter().any(|c| c.src_start <= 2));
}

#[test]
fn markdown_inline_styles() {
    let theme = crate::theme::ThemeKind::Default.theme();
    let spans = md_inline_spans("a *i* **b** `c`", &theme, Style::new().fg(theme.fg));
    let kinds: Vec<bool> = spans.iter().map(|s| {
        s.style.add_modifier.contains(Modifier::ITALIC)
            || s.style.add_modifier.contains(Modifier::BOLD)
            || s.style == theme.string
    }).collect();
    assert!(kinds.iter().any(|b| *b), "no styled spans: {spans:?}");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "a i b c");
}

#[test]
fn markdown_conceal_segments() {
    use crate::highlight::{build_md_segs, MdSegKind};
    let src = "# My Title\nplain **bold** and *em* and `code`\n- a list\n> quoted\n";
    let segs = build_md_segs(src);

    // Line 0: "# My Title" → marker concealed, text styled.
    let l0 = segs[0].iter().map(|s| s.display.as_str()).collect::<String>();
    assert_eq!(l0, "My Title");
    assert!(segs[0].iter().any(|s| s.kind == MdSegKind::Heading(1) && s.display == "My Title"));

    // Line 1: delimiters concealed, content styled.
    let l1 = segs[1].iter().map(|s| s.display.as_str()).collect::<String>();
    assert_eq!(l1, "plain bold and em and code");
    assert!(segs[1].iter().any(|s| s.kind == MdSegKind::Strong && s.display == "bold"));
    assert!(segs[1].iter().any(|s| s.kind == MdSegKind::Emph && s.display == "em"));
    assert!(segs[1].iter().any(|s| s.kind == MdSegKind::Code && s.display == "code"));

    // Line 2: list marker replaced with a bullet (same width).
    let l2 = segs[2].iter().map(|s| s.display.as_str()).collect::<String>();
    assert_eq!(l2, "\u{2022} a list");

    // Line 3: quote marker replaced.
    let l3 = segs[3].iter().map(|s| s.display.as_str()).collect::<String>();
    assert_eq!(l3, "\u{2502} quoted");
}

#[test]
fn markdown_display_col_skips_concealed() {
    use crate::highlight::build_md_segs;
    let src = "# Title\nplain **bold** tail\n";
    let segs = build_md_segs(src);

    // Raw byte 2 is after "# " (2 raw chars) — display col 0.
    assert_eq!(md_segs_col(&segs[0], 2), 0);
    // End of raw line (7) → display col 5 ("Title").
    assert_eq!(md_segs_col(&segs[0], 7), 5);

    // Line 1 (starts at raw byte 8): "plain **bold** tail".
    // The "**" markers are concealed, so the display column does
    // not advance across them.
    assert_eq!(md_segs_col(&segs[1], 14), 6); // after "plain "
    assert_eq!(md_segs_col(&segs[1], 16), 6); // inside leading "**"
    assert_eq!(md_segs_col(&segs[1], 22), 10); // after "**bold**"
    assert_eq!(md_segs_col(&segs[1], 23), 11); // the space after
}

#[test]
fn shell_key_encoding() {
    let k = |code: KeyCode, mods: KeyModifiers| KeyEvent::new(code, mods);
    assert_eq!(
        key_to_bytes(k(KeyCode::Char('c'), KeyModifiers::CONTROL), false),
        vec![3]
    );
    assert_eq!(
        key_to_bytes(k(KeyCode::Char('x'), KeyModifiers::ALT), false),
        vec![0x1b, b'x']
    );
    assert_eq!(
        key_to_bytes(k(KeyCode::Left, KeyModifiers::NONE), false),
        b"\x1b[D".to_vec()
    );
    assert_eq!(
        key_to_bytes(k(KeyCode::Left, KeyModifiers::NONE), true),
        b"\x1bOD".to_vec()
    );
    assert_eq!(
        key_to_bytes(k(KeyCode::Up, KeyModifiers::CONTROL), false),
        b"\x1b[1;5A".to_vec()
    );
    assert_eq!(
        key_to_bytes(k(KeyCode::Backspace, KeyModifiers::NONE), false),
        vec![0x7f]
    );
    assert_eq!(
        key_to_bytes(k(KeyCode::F(5), KeyModifiers::NONE), false),
        b"\x1b[15~".to_vec()
    );
    assert_eq!(
        key_to_bytes(k(KeyCode::BackTab, KeyModifiers::NONE), false),
        b"\x1b[Z".to_vec()
    );
    assert!(key_to_bytes(
        k(KeyCode::Char('c'), KeyModifiers::SUPER),
        false
    )
    .is_empty());
}

#[test]
fn theme_selector_filters_fuzzily_and_applies() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    assert_eq!(ed.state, State::Theme);
    // Unfiltered: the canonical list, in ThemeKind::all() order.
    assert_eq!(ed.theme_matches(), ThemeKind::all().to_vec());

    for ch in "tokyo".chars() {
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert_eq!(ed.theme_query, "tokyo");
    let m = ed.theme_matches();
    assert!(m.contains(&ThemeKind::TokyoNight), "{m:?}");
    assert!(m.contains(&ThemeKind::TokyoNightDay), "{m:?}");

    // Enter applies whatever is highlighted in the filtered list.
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(ed.state, State::Normal);
    assert_eq!(ed.theme, m[0]);
}

#[test]
fn theme_selector_esc_clears_the_filter_before_closing() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    for ch in "gruv".chars() {
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert!(ed.theme_matches().contains(&ThemeKind::Gruvbox));

    // First Esc: filter cleared, selector still open.
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(ed.state, State::Theme);
    assert!(ed.theme_query.is_empty());
    assert_eq!(ed.theme_matches().len(), ThemeKind::all().len());

    // Second Esc: closed, theme unchanged.
    let before = ed.theme;
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(ed.state, State::Normal);
    assert_eq!(ed.theme, before);
}

#[test]
fn theme_selector_rejects_non_matches() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    for ch in "zzzq".chars() {
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert!(ed.theme_matches().is_empty());
    // Enter on an empty result set must not panic or change the theme.
    let before = ed.theme;
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(ed.theme, before);
    assert_eq!(ed.state, State::Theme);
}

#[test]
fn theme_selector_draws_the_filtered_rows() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();

    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    term.draw(|f| ed.render(f)).unwrap();
    let unfiltered: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    // overlay_block upper-cases the title
    assert!(unfiltered.contains("THEME SELECTOR"), "{unfiltered}");
    assert!(unfiltered.contains("type to filter themes"));

    for ch in "vice".chars() {
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    term.draw(|f| ed.render(f)).unwrap();
    let filtered: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(filtered.contains("vice-city"), "filtered screen: {filtered}");
    assert!(!filtered.contains("gruvbox"), "unrelated theme still drawn");
}

#[test]
#[ignore]
fn dump_hermes_pane() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    let mut term = Terminal::new(TestBackend::new(100, 28)).unwrap();
    let _ = ed.handle_key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    ed.hermes.messages.push(crate::hermes::Msg {
        role: Role::User,
        text: "why does the bar flicker when I scroll".into(),
    });
    ed.hermes.messages.push(crate::hermes::Msg {
        role: Role::Assistant,
        text: "Because tmux repaints after each write, and ratatui emits the\ncursor move as its own write.".into(),
    });
    ed.hermes.input = "here is a question\nwith a second line\nand a third".into();
    ed.hermes.running = true;
    ed.hermes.status = "answering…".into();
    ed.hermes.scroll = 3;
    term.draw(|f| ed.render(f)).unwrap();
    let buf = term.backend().buffer();
    for y in 0..buf.area.height {
        let row: String = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect();
        println!("|{row}|");
    }
}

// ── Hermes chat pane ───────────────────────────────────────────

#[test]
fn hermes_pane_opens_on_its_key_and_draws_the_transcript() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();

    let _ = ed.handle_key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(ed.state, State::Hermes, "Ctrl+Shift+H opens the pane");

    ed.hermes.messages.push(crate::hermes::Msg {
        role: Role::User,
        text: "why does the bar flicker".into(),
    });
    ed.hermes.messages.push(crate::hermes::Msg {
        role: Role::Assistant,
        text: "because tmux repaints per write".into(),
    });

    term.draw(|f| ed.render(f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(screen.contains("Hermes"), "{screen}");
    assert!(screen.contains("why does the bar flicker"), "{screen}");
    assert!(screen.contains("tmux repaints per write"), "{screen}");
}

#[test]
fn hermes_message_line_takes_typing_and_escape_leaves() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    let _ = ed.handle_key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));

    for ch in "hello".chars() {
        let _ = ed.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert_eq!(ed.hermes.input, "hello");
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(ed.hermes.input, "hell");

    // Esc closes the pane when nothing is running.
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(ed.state, State::Normal);
}

#[test]
fn hermes_ctrl_o_attaches_the_open_file() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    ed.filename = Some("/tmp/notes.md".into());
    let _ = ed.handle_key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert_eq!(ed.hermes.input, "/tmp/notes.md ");
}

#[test]
fn hermes_quotes_a_visual_selection() {
    let cfg = crate::config::Config::default();
    let mut ed = Editor::new(None, &cfg).unwrap();
    ed.lines = vec![
        "fn main() {".to_string(),
        "    body();".to_string(),
        "}".to_string(),
    ];
    ed.cy = 0;
    ed.cx = 0;
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = ed.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = ed.handle_key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(ed.state, State::Hermes);
    assert!(
        ed.hermes.input.starts_with("```"),
        "selection should be quoted: {:?}",
        ed.hermes.input
    );
    assert!(
        ed.hermes.input.contains("body();"),
        "selection missing: {:?}",
        ed.hermes.input
    );
}

#[test]
fn wrap_text_keeps_blank_lines_and_breaks_long_words() {
    let wrapped = wrap_text("one two\n\nthree", 20);
    assert_eq!(wrapped, vec!["one two", "", "three"]);

    // A word wider than the pane must be split, not overflow.
    let long = "x".repeat(50);
    let wrapped = wrap_text(&long, 10);
    assert!(wrapped.len() >= 5, "{wrapped:?}");
    assert!(wrapped.iter().all(|l| l.chars().count() <= 10));
}
