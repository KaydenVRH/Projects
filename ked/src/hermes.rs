//! Hermes chat inside ked.
//!
//! One `hermes chat --format stream-json --oneshot` process per turn. Each turn
//! streams JSON lines — text deltas, tool calls, tool results, and a final
//! `result` carrying the session id — which are folded into a transcript.
//! `--resume` keeps the conversation, so the model keeps its context across
//! turns while ked keeps ownership of the screen.
//!
//! Deliberately not a nested terminal: the transcript is a normal scrollable
//! pane, keys behave like ked, and no full-screen TUI fights this one for the
//! cursor. The cost is that interactive tool approvals have to go through
//! hermes' own config (ked runs the turn non-interactively).

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

/// Who said it — drives colour and prefix when the transcript is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Tool,
    ToolResult,
    Error,
    Note,
}

#[derive(Debug, Clone)]
pub struct Msg {
    pub role: Role,
    pub text: String,
}

impl Role {
    fn tag(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
            Role::ToolResult => "tool_result",
            Role::Error => "error",
            Role::Note => "note",
        }
    }

    fn from_tag(tag: &str) -> Role {
        match tag {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            "tool_result" => Role::ToolResult,
            "error" => Role::Error,
            _ => Role::Note,
        }
    }
}

/// One thing that arrived from the hermes process.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Text(String),
    Tool { name: String, detail: String },
    ToolDone { ok: bool, ms: Option<u64>, preview: String },
    Result { session: Option<String>, tokens: Option<(u64, u64)> },
    Stderr(String),
    Done,
}

/// Parse one line of `--format stream-json` output.
///
/// Unknown and non-JSON lines return `None`: hermes prints the occasional bare
/// notice on stdout, and a new event type must never break a turn.
pub fn parse_line(line: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    match v.get("type")?.as_str()? {
        "text" => Some(Event::Text(v.get("text")?.as_str()?.to_string())),
        "tool_use" => {
            let name = v
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("tool")
                .to_string();
            Some(Event::Tool {
                detail: tool_detail(v.get("input")),
                name,
            })
        }
        "tool_result" => {
            let ok = !v
                .get("is_error")
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            Some(Event::ToolDone {
                ok,
                ms: v.get("duration_ms").and_then(|d| d.as_u64()),
                preview: tool_preview(v.get("output")),
            })
        }
        "result" => {
            let session = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let tokens = v.get("tokens").and_then(|t| {
                Some((
                    t.get("input")?.as_u64()?,
                    t.get("output")?.as_u64()?,
                ))
            });
            Some(Event::Result { session, tokens })
        }
        _ => None,
    }
}

/// A one-line summary of a tool's input — the command, the path, or whatever
/// string the tool was given.
fn tool_detail(input: Option<&serde_json::Value>) -> String {
    let Some(obj) = input else {
        return String::new();
    };
    for key in ["command", "path", "file_path", "query", "pattern", "url"] {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            return truncate(&one_line(s), 70);
        }
    }
    match obj {
        serde_json::Value::String(s) => truncate(&one_line(s), 70),
        serde_json::Value::Object(map) => {
            for v in map.values() {
                if let Some(s) = v.as_str() {
                    return truncate(&one_line(s), 70);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// The useful part of a tool result.
///
/// Tool output arrives JSON-encoded twice over — a `{"output": "...",
/// "exit_code": 0}` document inside a JSON string — so unwrap until something
/// printable is left, then keep the first non-blank line.
fn tool_preview(raw: Option<&serde_json::Value>) -> String {
    let text = match raw {
        Some(serde_json::Value::String(s)) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(inner) => inner
                .get("output")
                .and_then(|o| o.as_str())
                .map(|s| s.to_string())
                .or_else(|| inner.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| s.clone()),
            Err(_) => s.clone(),
        },
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    truncate(first.trim(), 70)
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A chat session: the transcript, the input line, and at most one live turn.
pub struct Chat {
    pub messages: Vec<Msg>,
    pub input: String,
    pub running: bool,
    pub session: Option<String>,
    /// Lines scrolled up from the bottom of the transcript.
    pub scroll: u16,
    pub status: String,
    pub cwd: PathBuf,
    pub turns: u32,
    pub tokens: (u64, u64),
    command: String,
    model: String,
    child: Option<Child>,
    rx: Option<Receiver<Event>>,
    stderr: Vec<String>,
    /// When the current turn started, for the elapsed timer in the title.
    turn_started: Option<std::time::Instant>,
}

impl Chat {
    pub fn new(cwd: PathBuf, command: String, model: String) -> Self {
        Chat {
            messages: Vec::new(),
            input: String::new(),
            running: false,
            session: None,
            scroll: 0,
            status: "ready".into(),
            cwd,
            turns: 0,
            tokens: (0, 0),
            command,
            model,
            child: None,
            rx: None,
            stderr: Vec::new(),
            turn_started: None,
        }
    }

    /// Start a turn. The prompt goes in as `-q`, so nothing is shell-interpreted
    /// and quotes, `$(…)` and backticks survive verbatim.
    pub fn send(&mut self, prompt: String) -> Result<(), String> {
        if self.running {
            return Err("still answering — Esc cancels".into());
        }
        self.messages.push(Msg {
            role: Role::User,
            text: prompt.clone(),
        });
        self.scroll = 0;
        self.status = "thinking…".into();
        self.turns += 1;

        let child = match self.spawn(&prompt) {
            Ok(c) => c,
            Err(e) => {
                // The pane owns the error: a flash message would be drawn
                // behind the popup and never seen.
                self.push_error(e.clone());
                return Err(e);
            }
        };
        self.turn_started = Some(std::time::Instant::now());
        self.child = Some(child);

        let (tx, rx) = mpsc::channel();
        let stdout = self.child.as_mut().and_then(|c| c.stdout.take());
        if let Some(out) = stdout {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(out).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Some(ev) = parse_line(&line) {
                        if tx.send(ev).is_err() {
                            return;
                        }
                    }
                }
                let _ = tx.send(Event::Done);
            });
        }
        let stderr = self.child.as_mut().and_then(|c| c.stderr.take());
        if let Some(err) = stderr {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(err).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(Event::Stderr(line)).is_err() {
                        return;
                    }
                }
            });
        }
        drop(tx);
        self.rx = Some(rx);
        self.running = true;
        Ok(())
    }

    fn spawn(&self, prompt: &str) -> Result<Child, String> {
        let mut first_error = String::new();
        // `hermes` is usually on PATH, but ked can be launched from somewhere
        // that has no user bin dir — fall back to the install location.
        let mut candidates = vec![self.command.clone()];
        if !self.command.contains('/') {
            if let Some(home) = std::env::var_os("HOME") {
                let mut p = PathBuf::from(home);
                p.push(".hermes/hermes-agent/.hermes/bin/hermes");
                candidates.push(p.to_string_lossy().into_owned());
            }
        }
        for cmd in candidates {
            let mut c = Command::new(&cmd);
            c.arg("chat")
                .arg("--format")
                .arg("stream-json")
                .arg("--oneshot");
            if let Some(sid) = self.session.as_deref() {
                c.arg("--resume").arg(sid);
            }
            if !self.model.is_empty() {
                c.arg("-m").arg(&self.model);
            }
            c.arg("-q").arg(prompt);
            c.current_dir(&self.cwd);
            c.stdin(Stdio::null());
            c.stdout(Stdio::piped());
            c.stderr(Stdio::piped());
            match c.spawn() {
                Ok(child) => return Ok(child),
                Err(e) => {
                    if first_error.is_empty() {
                        first_error = format!("cannot run {cmd}: {e}");
                    }
                }
            }
        }
        Err(first_error)
    }

    /// Fold everything that arrived since the last frame. Called every tick.
    pub fn poll(&mut self) {
        let Some(rx) = self.rx.take() else { return };
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(Event::Done) => {
                    finished = true;
                    break;
                }
                Ok(ev) => self.apply(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.running = false;
            if let Some(mut child) = self.child.take() {
                let _ = child.wait();
            }
            // A turn that died before answering is the only time stderr is
            // worth showing — otherwise it is full of harmless warnings.
            let answered = self
                .messages
                .iter()
                .rev()
                .take_while(|m| m.role != Role::User)
                .any(|m| m.role == Role::Assistant);
            if !answered {
                let tail = self
                    .stderr
                    .iter()
                    .filter(|l| !l.trim().is_empty())
                    .rev()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !tail.trim().is_empty() {
                    self.messages.push(Msg {
                        role: Role::Error,
                        text: truncate(&one_line(&tail), 200),
                    });
                    self.status = "error".into();
                }
            }
            self.stderr.clear();
            if self.status != "error" {
                self.status = "ready".into();
            }
            self.turn_started = None;
            // Cancelled turns and dead ones both land here; keep the record.
            save_state(self.session.as_deref(), &self.messages);
        } else {
            self.rx = Some(rx);
        }
    }

    fn apply(&mut self, ev: Event) {
        match ev {
            Event::Text(t) => {
                self.status = "answering…".into();
                // Text arrives as deltas — append to the open answer instead of
                // starting a new paragraph for every token.
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.text.push_str(&t);
                        return;
                    }
                }
                self.messages.push(Msg {
                    role: Role::Assistant,
                    text: t,
                });
            }
            Event::Tool { name, detail } => {
                self.status = format!("{name}…");
                self.messages.push(Msg {
                    role: Role::Tool,
                    text: format!("{name}  {detail}").trim_end().to_string(),
                });
            }
            Event::ToolDone { ok, ms, preview } => {
                let mut text = if ok { "ok" } else { "failed" }.to_string();
                if let Some(ms) = ms {
                    text.push_str(&format!(" · {ms} ms"));
                }
                if !preview.is_empty() {
                    text.push_str(&format!(" · {preview}"));
                }
                self.messages.push(Msg {
                    role: Role::ToolResult,
                    text,
                });
                self.status = "thinking…".into();
            }
            Event::Result { session, tokens } => {
                if let Some(s) = session {
                    self.session = Some(s);
                }
                if let Some((i, o)) = tokens {
                    self.tokens = (i, o);
                }
                self.status = "ready".into();
                // The turn is done: persist the conversation for the next ked.
                save_state(self.session.as_deref(), &self.messages);
            }
            Event::Stderr(l) => {
                if !l.trim().is_empty() {
                    self.stderr.push(l);
                }
            }
            Event::Done => {}
        }
    }

    /// Kill the running turn. The session survives, so the next message still
    /// has the context that was built before the cancel.
    pub fn cancel(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
        self.rx = None;
        self.running = false;
        self.stderr.clear();
        self.status = "cancelled".into();
        self.messages.push(Msg {
            role: Role::Note,
            text: "(cancelled)".into(),
        });
        save_state(self.session.as_deref(), &self.messages);
    }

    /// Drop the conversation and start a fresh hermes session.
    pub fn new_session(&mut self) {
        if self.running {
            self.cancel();
        }
        self.messages.clear();
        self.session = None;
        self.turns = 0;
        self.tokens = (0, 0);
        self.scroll = 0;
        self.status = "ready".into();
        // Forget it on disk too, or the next ked would resume what was cleared.
        save_state(None, &[]);
    }

    /// A short session tag for the pane title.
    pub fn session_tag(&self) -> String {
        match self.session.as_deref() {
            Some(s) => s.rsplit('-').next().unwrap_or(s).chars().take(6).collect(),
            None => "new".into(),
        }
    }

    /// Add a ked-side line (a local action, not something hermes said).
    pub fn push_note(&mut self, text: impl Into<String>) {
        self.messages.push(Msg {
            role: Role::Note,
            text: text.into(),
        });
    }

    /// Add a local failure to the transcript, where it can be seen.
    pub fn push_error(&mut self, text: impl Into<String>) {
        self.messages.push(Msg {
            role: Role::Error,
            text: text.into(),
        });
        self.status = "error".into();
    }

    /// Seconds the running turn has been going, for the title.
    pub fn elapsed_secs(&self) -> u64 {
        self.turn_started
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// The most recent answer, for "copy" and "insert into buffer".
    pub fn last_answer(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
            .map(|m| m.text.trim())
            .filter(|t| !t.is_empty())
    }

    /// Adopt the conversation from the last ked run, if it is recent enough.
    ///
    /// The transcript is restored along with the session id, so what the pane
    /// shows and what hermes remembers do not drift apart.
    pub fn resume_previous(&mut self) -> bool {
        let Some(saved) = load_state() else {
            return false;
        };
        let age = now_secs().saturating_sub(saved.updated);
        if age > RESUME_WINDOW_SECS {
            return false;
        }
        if saved.messages.is_empty() && saved.session.is_none() {
            return false;
        }
        self.session = saved.session;
        self.messages = saved.messages;
        if !self.messages.is_empty() {
            self.messages.push(Msg {
                role: Role::Note,
                text: format!("— resumed from {} ago —", human_age(age)),
            });
        }
        true
    }
}

/// A remembered session id is only worth resuming while it is still the same
/// working day; older context is usually stale and expensive.
const RESUME_WINDOW_SECS: u64 = 12 * 3600;

impl Drop for Chat {
    fn drop(&mut self) {
        // A turn still running when ked exits would keep writing into a closed
        // pipe; kill it rather than leave it behind.
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn human_age(secs: u64) -> String {
    match secs {
        0..=90 => "a moment".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h", s / 3600),
    }
}

/// `$XDG_STATE_HOME/ked/hermes.json`, or `~/.local/state/ked/hermes.json`.
fn state_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
        })?;
    Some(base.join("ked").join("hermes.json"))
}

/// What survives between ked runs: the session id and the transcript, so the
/// pane and the model stay in step.
struct Saved {
    session: Option<String>,
    messages: Vec<Msg>,
    updated: u64,
}

/// Older messages are dropped from the saved transcript: it is a convenience,
/// not an archive, and a huge file would slow the first draw.
const MAX_SAVED_MSGS: usize = 200;

fn load_state() -> Option<Saved> {
    load_state_at(&state_path()?)
}

fn load_state_at(path: &std::path::Path) -> Option<Saved> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let session = v
        .get("session")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let updated = v.get("updated").and_then(|u| u.as_u64()).unwrap_or(0);
    let messages = v
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    Some(Msg {
                        role: Role::from_tag(m.get("role")?.as_str()?),
                        text: m.get("text")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Saved {
        session,
        messages,
        updated,
    })
}

/// Remember the conversation for the next ked run. Failure is silent: a chat
/// that cannot write a state file still works.
fn save_state(session: Option<&str>, messages: &[Msg]) {
    if let Some(path) = state_path() {
        save_state_at(&path, session, messages);
    }
}

fn save_state_at(path: &std::path::Path, session: Option<&str>, messages: &[Msg]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let skip = messages.len().saturating_sub(MAX_SAVED_MSGS);
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .skip(skip)
        .map(|m| serde_json::json!({ "role": m.role.tag(), "text": m.text }))
        .collect();
    let body = serde_json::json!({
        "session": session,
        "updated": now_secs(),
        "messages": msgs,
    });
    let _ = std::fs::write(path, body.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chat() -> Chat {
        Chat::new(PathBuf::from("/tmp"), "hermes".into(), String::new())
    }

    #[test]
    fn parses_text_and_result() {
        let ev = parse_line(r#"{"type":"text","text":"hello"}"#).unwrap();
        assert_eq!(ev, Event::Text("hello".into()));

        let ev = parse_line(
            r#"{"type":"result","session_id":"local-abc123","tokens":{"input":10,"output":2}}"#,
        )
        .unwrap();
        assert_eq!(
            ev,
            Event::Result {
                session: Some("local-abc123".into()),
                tokens: Some((10, 2))
            }
        );
    }

    #[test]
    fn parses_tool_use_and_result() {
        let ev = parse_line(
            r#"{"type":"tool_use","name":"terminal","input":{"command":"echo hi"}}"#,
        )
        .unwrap();
        assert_eq!(
            ev,
            Event::Tool {
                name: "terminal".into(),
                detail: "echo hi".into()
            }
        );

        // Output is a JSON document encoded inside a JSON string.
        let ev = parse_line(
            r#"{"type":"tool_result","name":"terminal","output":"{\"output\": \"hi\", \"exit_code\": 0}","duration_ms":139,"is_error":false}"#,
        )
        .unwrap();
        assert_eq!(
            ev,
            Event::ToolDone {
                ok: true,
                ms: Some(139),
                preview: "hi".into()
            }
        );
    }

    #[test]
    fn ignores_unknown_and_noise_lines() {
        assert!(parse_line(r#"{"type":"system","subtype":"init"}"#).is_none());
        assert!(parse_line("Session: local-abc").is_none());
        assert!(parse_line("").is_none());
    }

    #[test]
    fn streamed_text_folds_into_one_answer() {
        let mut c = chat();
        c.messages.push(Msg {
            role: Role::User,
            text: "hi".into(),
        });
        c.apply(Event::Text("\n\nI ran the".into()));
        c.apply(Event::Text(" command.".into()));
        let answers: Vec<_> = c
            .messages
            .iter()
            .filter(|m| m.role == Role::Assistant)
            .collect();
        assert_eq!(answers.len(), 1, "deltas must not each start a message");
        assert_eq!(answers[0].text, "\n\nI ran the command.");
    }

    #[test]
    fn tool_result_becomes_one_line() {
        let mut c = chat();
        c.apply(Event::ToolDone {
            ok: false,
            ms: None,
            preview: "permission denied".into(),
        });
        assert_eq!(c.messages.last().unwrap().role, Role::ToolResult);
        assert_eq!(c.messages.last().unwrap().text, "failed · permission denied");
    }

    #[test]
    fn a_new_session_forgets_context() {
        let mut c = chat();
        c.session = Some("local-1".into());
        c.messages.push(Msg {
            role: Role::User,
            text: "hi".into(),
        });
        c.new_session();
        assert!(c.messages.is_empty());
        assert!(c.session.is_none());
        assert_eq!(c.session_tag(), "new");
    }

    #[test]
    fn truncation_keeps_it_short() {
        let long = "x".repeat(200);
        assert_eq!(truncate(&long, 10).chars().count(), 10);
        assert!(truncate(&long, 10).ends_with('…'));
    }

    #[test]
    fn session_state_round_trips_with_the_transcript() {
        let dir = std::env::temp_dir().join(format!("ked-hermes-{}", std::process::id()));
        let path = dir.join("hermes.json");
        let _ = std::fs::remove_file(&path);

        let msgs = vec![
            Msg {
                role: Role::User,
                text: "hi".into(),
            },
            Msg {
                role: Role::Assistant,
                text: "hello".into(),
            },
        ];
        save_state_at(&path, Some("local-abc"), &msgs);
        let saved = load_state_at(&path).expect("state file written");
        assert_eq!(saved.session.as_deref(), Some("local-abc"));
        assert_eq!(saved.messages.len(), 2);
        assert_eq!(saved.messages[1].role, Role::Assistant);
        assert_eq!(saved.messages[1].text, "hello");
        assert!(now_secs().saturating_sub(saved.updated) < 5, "timestamp is now");

        // An old stamp is loadable but must be past the resume window.
        let stale = serde_json::json!({ "session": "local-old", "updated": 1 });
        std::fs::write(&path, stale.to_string()).unwrap();
        let saved = load_state_at(&path).unwrap();
        assert_eq!(saved.session.as_deref(), Some("local-old"));
        assert!(now_secs().saturating_sub(saved.updated) > RESUME_WINDOW_SECS);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn last_answer_skips_notes_and_tool_output() {
        let mut c = chat();
        assert!(c.last_answer().is_none());
        c.messages.push(Msg {
            role: Role::Assistant,
            text: "the answer".into(),
        });
        c.messages.push(Msg {
            role: Role::ToolResult,
            text: "ok".into(),
        });
        assert_eq!(c.last_answer(), Some("the answer"));
    }

    /// End-to-end against the real `hermes` binary: one turn, then a follow-up
    /// that only answers correctly if the session was resumed. Costs a model
    /// call, so it is ignored by default:
    ///   cargo test -- --ignored live_turn
    #[test]
    #[ignore]
    fn live_turn() {
        use std::time::{Duration, Instant};

        fn run(c: &mut Chat, prompt: &str) {
            c.send(prompt.to_string()).unwrap();
            let start = Instant::now();
            while c.running && start.elapsed() < Duration::from_secs(180) {
                c.poll();
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(!c.running, "turn did not finish");
        }

        fn answer(c: &Chat) -> String {
            c.messages
                .iter()
                .rev()
                .find(|m| m.role == Role::Assistant)
                .map(|m| m.text.clone())
                .unwrap_or_default()
        }

        let mut c = Chat::new(std::env::temp_dir(), "hermes".into(), String::new());
        // The turn saves its session id; put the real state file back afterwards
        // so a test run never hijacks the next ked session.
        let state = state_path();
        let before = state.as_ref().and_then(|p| std::fs::read_to_string(p).ok());

        run(&mut c, "Reply with exactly: pong");
        assert!(answer(&c).to_lowercase().contains("pong"), "{:?}", answer(&c));
        assert!(c.session.is_some(), "no session id captured");

        let sid = c.session.clone();
        run(&mut c, "Which word did you just reply with?");
        assert_eq!(c.session, sid, "session changed mid-conversation");
        assert!(
            answer(&c).to_lowercase().contains("pong"),
            "context lost: {:?}",
            answer(&c)
        );

        if let Some(path) = state {
            match before {
                Some(text) => {
                    let _ = std::fs::write(&path, text);
                }
                None => {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
}
