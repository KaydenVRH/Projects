//! `:` command execution, saving, and running the current file (Ctrl+E).

use super::*;

impl Editor {
    // ═══════════════════════════════════════════════════════════════
    //  Command execution
    // ═══════════════════════════════════════════════════════════════

    /// Parse and execute a command string (without the leading `:`).
    ///
    /// Returns `false` if the command should quit the editor.
    pub(super) fn exec_cmd(&mut self, cmd: &str) -> bool {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        if parts.is_empty() {
            return true;
        }
        let cmd_name = parts[0];

        match cmd_name {
            // :q — close current buffer (quit if last)
            "q" => {
                if self.modified {
                    self.flash = Some("No write since last change (add ! to override)".to_string());
                    return true;
                }
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() {
                    return false;
                }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
            // :q! — force close (quit if last)
            "q!" => {
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() {
                    return false;
                }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
            // :w [file] — save (optionally to a new name)
            "w" => {
                let fname = if parts.len() > 1 {
                    let name = parts[1..].join(" ");
                    self.filename = Some(name.clone());
                    name
                } else {
                    match self.filename.clone() {
                        Some(f) => f,
                        None => {
                            self.flash = Some("No filename.  Use :w <name>".to_string());
                            return true;
                        }
                    }
                };
                match self.save_to_disk(&fname) {
                    Ok(_) => {
                        self.modified = false;
                        self.flash = Some(format!("'{}' written", fname));
                    }
                    Err(e) => {
                        self.flash = Some(format!("Error: {e}"));
                    }
                }
            }
            // :wq — save and close buffer (quit if last)
            "wq" => {
                let fname = self.filename.clone().unwrap_or_default();
                if !fname.is_empty() && self.save_to_disk(&fname).is_err() {
                    self.flash = Some("Error saving".to_string());
                    return true;
                }
                if fname.is_empty() {
                    self.flash = Some("No filename".to_string());
                    return true;
                }
                self.modified = false;
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() {
                    return false;
                }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
            // :bn — next buffer
            "bn" | "bnext" => {
                if self.buffers.len() > 1 {
                    let next = (self.current + 1) % self.buffers.len();
                    self.switch_buffer(next);
                } else {
                    self.flash = Some("Only one buffer open".to_string());
                }
            }
            // :bp — previous buffer
            "bp" | "bprev" => {
                if self.buffers.len() > 1 {
                    let prev = if self.current == 0 {
                        self.buffers.len() - 1
                    } else {
                        self.current - 1
                    };
                    self.switch_buffer(prev);
                } else {
                    self.flash = Some("Only one buffer open".to_string());
                }
            }
            // :3 — hidden easter egg: cycle colour fx mode
            "3" => {
                self.fx_mode = (self.fx_mode + 1) % 4;
                self.fx_start = Instant::now();
                let names = ["off", "gentle", "breathing", "warm"];
                self.flash = Some(format!("fx: {}", names[self.fx_mode as usize]));
            }
            // :sys — system dashboard
            "sys" => {
                self.state = State::SysInfo;
                // Kick off a fresh stat collection so the dashboard
                // doesn't show zeros on first open.
                if self.sys_task.is_none() {
                    self.sys_task = Some(std::thread::spawn(collect_sys_stats));
                }
            }
            // :keys — show the effective (possibly customised) keymap
            "keys" | "keybinds" | "keymaps" => {
                self.run_output = self.keymap.describe();
                // Show the top of the list (usize::MAX clamps in render).
                self.run_scroll = usize::MAX;
                self.state = State::Run;
            }
            // :mem [text] — remember something (or open the dashboard)
            "mem" | "remember" => {
                if parts.len() > 1 {
                    let text = parts[1..].join(" ");
                    let now = memory::now();
                    let current = self.filename.clone();
                    match self.memory.add(&text, now, current.as_deref()) {
                        Ok(_) => self.flash = Some("Remembered".to_string()),
                        Err(e) => self.flash = Some(format!("memory: {e}")),
                    }
                } else {
                    self.open_memories();
                }
            }
            // :memories — reminders / notes dashboard
            "memories" | "notes" | "reminders" => self.open_memories(),
            // :shell — pop-up terminal
            "sh" | "shell" | "term" => self.open_shell(),
            // :cal — month planner / calendar
            "cal" | "planner" | "calendar" => self.open_planner(),
            // :today — open (or create) today's journal entry
            "today" | "journal" => {
                let now = memory::now();
                match self.memory.today_journal(now) {
                    Ok(path) => self.open_path(&path),
                    Err(e) => self.flash = Some(format!("journal: {e}")),
                }
            }
            // :e <file> — open file
            "e" if parts.len() > 1 => {
                let path = parts[1..].join(" ");
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                        if content.ends_with('\n') { lines.push(String::new()); }
                        if lines.is_empty() { lines.push(String::new()); }
                        let mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                        self.sync_to_buffer();
                        self.buffers.push(Buffer::new(lines, Some(path.clone()), mtime));
                        self.current = self.buffers.len() - 1;
                        self.sync_from_buffer(self.current);
                        self.flash = Some(format!("Opened {path}"));
                    }
                    Err(e) => {
                        self.flash = Some(format!("Can't open {path}: {e}"));
                    }
                }
            }
            // :hermes [new] — open the chat pane (new = fresh session)
            "hermes" => {
                if parts.len() > 1 && parts[1] == "new" {
                    self.hermes.new_session();
                }
                self.open_hermes();
            }
            // :theme <name> — switch theme
            "theme" if parts.len() > 1 => {
                let name = parts[1].to_lowercase();
                if let Some(tk) = ThemeKind::from_str(&name) {
                    self.theme = tk;
                    self.flash = Some(format!("Theme: {}", tk.name()));
                } else {
                    self.flash = Some(format!("Unknown theme: {name}"));
                }
            }
            // :img — paste the clipboard image into the buffer
            "img" | "image" => {
                if let Err(e) = self.paste_clipboard_image() {
                    self.flash = Some(format!("img: {e}"));
                }
            }
            // :wq! — save and force close
            "wq!" => {
                let fname = self.filename.clone().unwrap_or_default();
                if !fname.is_empty() && self.save_to_disk(&fname).is_err() {
                    self.flash = Some("Error saving".to_string());
                    return true;
                }
                self.modified = false;
                self.sync_to_buffer();
                self.buffers.remove(self.current);
                if self.buffers.is_empty() { return false; }
                self.current = min(self.current, self.buffers.len().saturating_sub(1));
                self.sync_from_buffer(self.current);
            }
            // :<number> — jump to line
            cmd_name if cmd_name.parse::<usize>().is_ok() => {
                let num: usize = cmd_name.parse().unwrap();
                let line = num.saturating_sub(1);
                self.cy = line.min(self.lines.len().saturating_sub(1));
                self.cx = 0;
                self.left = 0;
                let page = (self.terminal_height() as usize).saturating_sub(2).max(1);
                self.top = self.cy.saturating_sub(page / 2);
                self.clamp_scroll();
            }
            // Unknown command.
            _ => {
                self.flash = Some(format!("Unknown command: {cmd_name}"));
            }
        }
        true
    }

    // ═══════════════════════════════════════════════════════════════
    //  File operations
    // ═══════════════════════════════════════════════════════════════

    /// Save the current buffer to `path`.
    pub(super) fn save_to_disk(&mut self, path: &str) -> Result<()> {
        let content = self.lines.join("\n");
        fs::write(path, &content).with_context(|| format!("can't write {path}"))?;
        self.last_mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
        Ok(())
    }

    /// Run a file and capture its stdout + stderr.
    /// Python files (`.py`) run via `python3`, C files (`.c`/`.h`)
    /// compile with `cc` then run the resulting binary.
    pub(super) fn run_file(&self, path: &str) -> String {
        let ext = path.rsplit('.').next().unwrap_or("");
        match ext {
            "c" | "h" => self.run_c(path),
            "rs" => self.run_rust(path),
            "py" => run_python(path),
            "go" => run_go(path),
            _ => format!("Don't know how to run .{ext} files.  Use .py, .rs, .c, .h, or .go."),
        }
    }

    /// Compile and run a C source file.
    pub(super) fn run_c(&self, path: &str) -> String {
        let bin = "/tmp/ked_c_bin";

        // Remove any stale binary so a failed compile can't leave an
        // old executable behind that "succeeds" on the next run.
        let _ = fs::remove_file(bin);

        // Compile
        let comp = ProcCmd::new("cc")
            .args(["-o", bin, "-Wall", "-Wextra", path])
            .output();
        let comp_out = match comp {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => return format!("Error running cc: {e}"),
        };

        // Check if compilation succeeded (binary exists).
        if !std::path::Path::new(bin).exists() {
            let mut result = String::from("── Compilation failed ──\n");
            result.push_str(&comp_out);
            return result;
        }

        // Run
        let run = ProcCmd::new(bin).output();
        let run_out = match run {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => format!("Error running binary: {e}"),
        };

        let mut result = String::new();
        if !comp_out.is_empty() {
            result.push_str("── Compilation ──\n");
            result.push_str(&comp_out);
            result.push('\n');
        }
        result.push_str("── Output ──\n");
        result.push_str(&run_out);
        result
    }

    /// Compile and run a Rust source file.
    pub(super) fn run_rust(&self, path: &str) -> String {
        let bin = "/tmp/ked_rust_bin";

        // Remove any stale binary so a failed compile can't leave an
        // old executable behind that "succeeds" on the next run.
        let _ = fs::remove_file(bin);

        let comp = ProcCmd::new("rustup")
            .args(["run", "stable", "rustc", "-o", bin, path])
            .output();
        let comp_out = match comp {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => return format!("Error running rustc: {e}"),
        };

        if !std::path::Path::new(bin).exists() {
            let mut result = String::from("── Compilation failed ──\n");
            result.push_str(&comp_out);
            return result;
        }

        let run = ProcCmd::new(bin).output();
        let run_out = match run {
            Ok(out) => {
                let mut s = String::new();
                if !out.stdout.is_empty() {
                    s.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !s.is_empty() { s.push('\n'); }
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                s
            }
            Err(e) => format!("Error running binary: {e}"),
        };

        let mut result = String::new();
        if !comp_out.is_empty() {
            result.push_str("── Compilation ──\n");
            result.push_str(&comp_out);
            result.push('\n');
        }
        result.push_str("── Output ──\n");
        result.push_str(&run_out);
        result
    }
}

pub(super) fn run_python(path: &str) -> String {
    let output = ProcCmd::new("python3")
        .arg(path)
        .output();
    match output {
        Ok(out) => {
            let mut result = String::new();
            if !out.stdout.is_empty() {
                result.push_str(&String::from_utf8_lossy(&out.stdout));
            }
            if !out.stderr.is_empty() {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&String::from_utf8_lossy(&out.stderr));
            }
            if result.is_empty() {
                result = "(no output)".to_string();
            }
            result
        }
        Err(e) => format!("Error running python3: {e}"),
    }
}

pub(super) fn run_go(path: &str) -> String {
    let output = ProcCmd::new("go").args(["run", path]).output();
    match output {
        Ok(out) => {
            let mut result = String::new();
            if !out.stdout.is_empty() {
                result.push_str(&String::from_utf8_lossy(&out.stdout));
            }
            if !out.stderr.is_empty() {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&String::from_utf8_lossy(&out.stderr));
            }
            if result.is_empty() {
                result = "(no output)".to_string();
            }
            result
        }
        Err(e) => format!("Error running go: {e}"),
    }
}
