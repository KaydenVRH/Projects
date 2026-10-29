//! Remembered things: notes, reminders, journal entries.
//!
//! Everything lives as **plain markdown files with TOML frontmatter**
//! under `~/.local/share/ked/memories` (or `$XDG_DATA_HOME`).  One file
//! per entry keeps them editable in ked, greppable, and git-friendly:
//!
//! ```text
//! +++
//! created = "2026-10-26T09:14"
//! due = "2026-10-27T09:00"
//! repeat = "daily"
//! tags = ["health"]
//! +++
//! Call the dentist.
//! ```
//!
//! The same code powers the in-editor reminder check and the
//! `ked --remind` LaunchAgent.  A `fired` timestamp is written to disk
//! whenever a reminder is delivered, so the editor and the background
//! agent never notify twice for the same occurrence.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{
    Datelike, Duration, Local, Months, NaiveDate, NaiveDateTime, NaiveTime,
    TimeZone, Weekday,
};
use serde::{Deserialize, Serialize};

use crate::config::MemoryConfig;

pub const DT_FMT: &str = "%Y-%m-%dT%H:%M";

/// Current local wall-clock time (no timezone — reminders are "9am"
/// local, like a paper diary).
pub fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

pub fn fmt_dt(dt: NaiveDateTime) -> String {
    dt.format(DT_FMT).to_string()
}

pub fn parse_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.trim(), DT_FMT)
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M:%S").ok())
        .or_else(|| {
            NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(9, 0, 0))
        })
}

/// Number of days in a month (handles leap years).
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(31)
}

/// The dates shown in a month grid, row-major, seven per week.
/// `week_start` is 0 for Monday or 6 for Sunday.
pub fn month_grid(year: i32, month: u32, week_start: u32) -> Vec<NaiveDate> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let dim = days_in_month(year, month) as usize;
    let ws = week_start % 7;
    let offset = (first.weekday().num_days_from_monday() + 7 - ws) % 7;
    let start = first - Duration::days(offset as i64);
    let weeks = (offset as usize + dim).div_ceil(7);
    (0..weeks * 7)
        .map(|i| start + Duration::days(i as i64))
        .collect()
}

/// Where the memory files live (`cfg.dir`, or the XDG default).
pub fn dir_for(cfg: &MemoryConfig) -> PathBuf {
    if !cfg.dir.is_empty() {
        return expand_tilde(&cfg.dir);
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("ked").join("memories");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("ked")
        .join("memories")
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(p)
}

// ── repeats ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Daily,
    Weekly,
    Monthly,
    Hours(i64),
    Days(i64),
}

impl Repeat {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "daily" | "day" => Some(Repeat::Daily),
            "weekly" | "week" => Some(Repeat::Weekly),
            "monthly" | "month" => Some(Repeat::Monthly),
            _ => {
                let rest = s.strip_prefix("every:")?;
                let (num, unit) = rest.split_at(rest.find(|c: char| !c.is_ascii_digit())?);
                let n: i64 = num.parse().ok()?;
                if n <= 0 {
                    return None;
                }
                match unit {
                    "h" | "hr" | "hrs" => Some(Repeat::Hours(n)),
                    "d" | "day" | "days" => Some(Repeat::Days(n)),
                    "w" | "week" | "weeks" => Some(Repeat::Days(n * 7)),
                    _ => None,
                }
            }
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Repeat::Daily => "daily".into(),
            Repeat::Weekly => "weekly".into(),
            Repeat::Monthly => "monthly".into(),
            Repeat::Hours(n) => format!("every:{n}h"),
            Repeat::Days(n) => format!("every:{n}d"),
        }
    }

    fn advance(&self, dt: NaiveDateTime) -> NaiveDateTime {
        match self {
            Repeat::Daily => dt + Duration::days(1),
            Repeat::Weekly => dt + Duration::days(7),
            Repeat::Monthly => dt
                .checked_add_months(Months::new(1))
                .unwrap_or(dt + Duration::days(30)),
            Repeat::Hours(n) => dt + Duration::hours(*n),
            Repeat::Days(n) => dt + Duration::days(*n),
        }
    }

    fn advance_owned(&self, dt: NaiveDateTime) -> NaiveDateTime {
        self.advance(dt)
    }

    /// First occurrence strictly after `after`.
    pub fn next_after(&self, due: NaiveDateTime, after: NaiveDateTime) -> NaiveDateTime {
        let mut d = due;
        // Bounded: at most a million steps even for a stale hourly item.
        for _ in 0..1_000_000 {
            d = self.advance_owned(d);
            if d > after {
                break;
            }
        }
        d
    }
}

// ── one entry ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Memory {
    pub created: NaiveDateTime,
    pub due: Option<NaiveDateTime>,
    pub repeat: Option<Repeat>,
    pub tags: Vec<String>,
    pub context: Option<String>,
    pub priority: u8,
    pub done: bool,
    pub fired: Option<NaiveDateTime>,
    pub review: Option<NaiveDateTime>,
    pub interval: f64,
    pub ease: f64,
    pub body: String,
    pub path: PathBuf,
}

impl Memory {
    /// Compact one-line summary for lists.
    pub fn summary(&self, max: usize) -> String {
        let s = self
            .body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if s.chars().count() > max {
            let cut: String = s.chars().take(max.saturating_sub(1)).collect();
            format!("{cut}…")
        } else {
            s
        }
    }

    /// Does this entry belong on `date`?  Reminders count on their due
    /// date; repeating reminders also count on projected occurrences.
    pub fn occurs_on(&self, date: NaiveDate) -> bool {
        let Some(due) = self.due else { return false };
        if due.date() == date {
            return true;
        }
        if due.date() > date {
            return false;
        }
        let Some(rep) = self.repeat else { return false };
        let Some(start) = date.pred_opt().and_then(|d| d.and_hms_opt(23, 59, 59)) else {
            return false;
        };
        rep.next_after(due, start).date() == date
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Frontmatter {
    #[serde(default)]
    created: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repeat: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    priority: u8,
    #[serde(default, skip_serializing_if = "is_false")]
    done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fired: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    review: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    interval: f64,
    #[serde(default = "default_ease")]
    ease: f64,
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}
fn is_zero_f64(n: &f64) -> bool {
    *n == 0.0
}
fn default_ease() -> f64 {
    2.5
}

/// Parse a memory file.  A file without frontmatter is treated as a
/// plain note (so hand-created markdown still works).
pub fn load_file(path: &Path) -> Option<Memory> {
    let text = fs::read_to_string(path).ok()?;
    let (meta, body) = split_frontmatter(&text);
    let fm: Frontmatter = toml::from_str(&meta).unwrap_or_default();
    let mtime = fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| Local.timestamp_opt(d.as_secs() as i64, 0).single())
        .map(|dt| dt.naive_local())
        .unwrap_or_else(now);
    let created = parse_dt(&fm.created)
        .or_else(|| parse_dt(&fm.due.clone().unwrap_or_default()))
        .unwrap_or(mtime);
    Some(Memory {
        created,
        due: fm.due.as_deref().and_then(parse_dt),
        repeat: fm.repeat.as_deref().and_then(Repeat::parse),
        tags: fm.tags,
        context: fm.context,
        priority: fm.priority,
        done: fm.done,
        fired: fm.fired.as_deref().and_then(parse_dt),
        review: fm.review.as_deref().and_then(parse_dt),
        interval: fm.interval,
        ease: if fm.ease == 0.0 { 2.5 } else { fm.ease },
        body: body.trim().to_string(),
        path: path.to_path_buf(),
    })
}

fn split_frontmatter(text: &str) -> (String, String) {
    let text = text.replace("\r\n", "\n");
    if let Some(rest) = text.strip_prefix("+++\n") {
        if let Some(end) = rest.find("\n+++") {
            let meta = rest[..end].to_string();
            let body = rest[end + 4..].trim_start_matches('\n').to_string();
            return (meta, body);
        }
    }
    (String::new(), text)
}

pub fn save_file(m: &Memory) -> io::Result<()> {
    let fm = Frontmatter {
        created: fmt_dt(m.created),
        due: m.due.map(fmt_dt),
        repeat: m.repeat.map(|r| r.as_str()),
        tags: m.tags.clone(),
        context: m.context.clone(),
        priority: m.priority,
        done: m.done,
        fired: m.fired.map(fmt_dt),
        review: m.review.map(fmt_dt),
        interval: m.interval,
        ease: m.ease,
    };
    let meta = toml::to_string(&fm).unwrap_or_default();
    let content = format!("+++\n{meta}+++\n\n{}\n", m.body.trim_end());
    if let Some(parent) = m.path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Atomic replace so the background agent never reads a half file.
    let tmp = m.path.with_extension("md.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, &m.path)
}

fn slugify(body: &str) -> String {
    let mut out = String::new();
    for c in body.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c.is_whitespace() || c == '-' || c == '_' {
            if !out.ends_with('-') {
                out.push('-');
            }
        }
        if out.len() >= 40 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

fn unique_path(dir: &Path, created: NaiveDateTime, body: &str) -> PathBuf {
    let stamp = created.format("%Y%m%d-%H%M%S").to_string();
    let slug = slugify(body);
    let stem = if slug.is_empty() {
        stamp
    } else {
        format!("{stamp}-{slug}")
    };
    let mut p = dir.join(format!("{stem}.md"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{stem}-{n}.md"));
        n += 1;
    }
    p
}

// ── quick-add syntax ─────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct Draft {
    pub body: String,
    pub due: Option<NaiveDateTime>,
    pub repeat: Option<Repeat>,
    pub tags: Vec<String>,
    pub context: Option<String>,
    pub priority: u8,
}

/// Parse a one-line capture such as
/// `call mom @tomorrow 9am #family !2 @here`.
pub fn parse_draft(input: &str, now: NaiveDateTime, current_file: Option<&str>) -> Draft {
    let mut d = Draft::default();
    let mut date: Option<(NaiveDate, bool)> = None; // (date, came from weekday)
    let mut time: Option<NaiveTime> = None;
    let mut relative: Option<Duration> = None;
    let mut words: Vec<&str> = Vec::new();

    for tok in input.split_whitespace() {
        if let Some(tag) = tok.strip_prefix('#') {
            if !tag.is_empty() {
                d.tags.push(tag.trim_start_matches('#').to_string());
            }
            continue;
        }
        if let Some(p) = tok.strip_prefix('!') {
            if let Ok(n) = p.parse::<u8>() {
                d.priority = n.min(3);
                continue;
            }
        }
        let Some(raw) = tok.strip_prefix('@') else {
            // A bare time is accepted once a date is known, so the
            // natural `@tomorrow 9am` (and `@2026-11-02 14:30`) works.
            if time.is_none() && date.is_some() {
                if let Some(t) = parse_time_token(tok) {
                    time = Some(t);
                    continue;
                }
            }
            words.push(tok);
            continue;
        };
        let low = raw.to_ascii_lowercase();
        if low == "here" {
            d.context = current_file.map(|s| s.to_string());
            continue;
        }
        if let Some(rep) = Repeat::parse(&low) {
            d.repeat = Some(rep);
            continue;
        }
        if let Some(rest) = low.strip_prefix('+') {
            if let Some(dur) = parse_relative(rest) {
                relative = Some(dur);
                continue;
            }
        }
        if let Some((date_val, weekday)) = parse_date_token(&low, now) {
            date = Some((date_val, weekday));
            continue;
        }
        if let Some(t) = parse_time_token(&low) {
            time = Some(t);
            continue;
        }
        // Not a recognised @token — keep it in the body.
        words.push(tok);
    }

    d.body = words.join(" ");

    // Resolve the due date/time.
    if let Some(dur) = relative {
        d.due = Some(now + dur);
    } else if date.is_some() || time.is_some() || d.repeat.is_some() {
        d.due = resolve_due(now, date, time, d.repeat);
    }
    d
}

fn resolve_due(
    now: NaiveDateTime,
    date: Option<(NaiveDate, bool)>,
    time: Option<NaiveTime>,
    repeat: Option<Repeat>,
) -> Option<NaiveDateTime> {
    let today = now.date();
    match (date, time) {
        (Some((d, weekday)), Some(t)) => {
            let mut dt = d.and_time(t);
            if dt <= now {
                if weekday {
                    dt += Duration::days(7);
                } else if d == today {
                    // "today 9am" already past: keep it (it is overdue).
                }
            }
            Some(dt)
        }
        (Some((d, _)), None) => {
            if d == today {
                Some(now + Duration::minutes(1))
            } else {
                d.and_hms_opt(9, 0, 0)
            }
        }
        (None, Some(t)) => {
            let mut dt = today.and_time(t);
            if dt <= now {
                dt += Duration::days(1);
            }
            Some(dt)
        }
        (None, None) => {
            // Recurrence with no explicit time: next period, 09:00.
            let rep = repeat?;
            let base = now;
            let next = rep.next_after(base, base);
            Some(next.date().and_hms_opt(9, 0, 0).unwrap_or(next))
        }
    }
}

fn parse_relative(s: &str) -> Option<Duration> {
    let split = s.find(|c: char| !c.is_ascii_digit())?;
    let (num, unit) = s.split_at(split);
    let n: i64 = num.parse().ok()?;
    if n <= 0 {
        return None;
    }
    match unit {
        "m" | "min" | "mins" => Some(Duration::minutes(n)),
        "h" | "hr" | "hrs" => Some(Duration::hours(n)),
        "d" | "day" | "days" => Some(Duration::days(n)),
        "w" | "week" | "weeks" => Some(Duration::days(n * 7)),
        _ => None,
    }
}

fn parse_date_token(s: &str, now: NaiveDateTime) -> Option<(NaiveDate, bool)> {
    let today = now.date();
    match s {
        "today" => return Some((today, false)),
        "tomorrow" | "tmrw" => return Some((today + Duration::days(1), false)),
        "yesterday" => return Some((today - Duration::days(1), false)),
        _ => {}
    }
    if let Some(wd) = parse_weekday(s) {
        let delta = (wd.num_days_from_monday() as i64
            - today.weekday().num_days_from_monday() as i64
            + 7)
            % 7;
        return Some((today + Duration::days(delta), true));
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .map(|d| (d, false))
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "mon" | "monday" => Weekday::Mon,
        "tue" | "tues" | "tuesday" => Weekday::Tue,
        "wed" | "wednesday" => Weekday::Wed,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thu,
        "fri" | "friday" => Weekday::Fri,
        "sat" | "saturday" => Weekday::Sat,
        "sun" | "sunday" => Weekday::Sun,
        _ => return None,
    })
}

fn parse_time_token(s: &str) -> Option<NaiveTime> {
    let s = s.trim();
    if let Some((h, m)) = s.split_once(':') {
        let h: u32 = h.parse().ok()?;
        let m: u32 = m.parse().ok()?;
        return NaiveTime::from_hms_opt(h, m, 0);
    }
    let (num, pm) = if let Some(n) = s.strip_suffix("am") {
        (n, false)
    } else if let Some(n) = s.strip_suffix("pm") {
        (n, true)
    } else {
        return None;
    };
    let mut h: u32 = num.parse().ok()?;
    if pm && h < 12 {
        h += 12;
    }
    if !pm && h == 12 {
        h = 0;
    }
    NaiveTime::from_hms_opt(h, 0, 0)
}

// ── store ────────────────────────────────────────────────────────

pub struct MemoryStore {
    pub dir: PathBuf,
    pub entries: Vec<Memory>,
    pub error: Option<String>,
}

impl MemoryStore {
    pub fn load(dir: PathBuf) -> Self {
        let mut s = MemoryStore { dir, entries: Vec::new(), error: None };
        s.reload();
        s
    }

    pub fn reload(&mut self) {
        self.entries.clear();
        self.error = None;
        if let Err(e) = fs::create_dir_all(&self.dir) {
            self.error = Some(format!("can't create {}: {e}", self.dir.display()));
            return;
        }
        let rd = match fs::read_dir(&self.dir) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(format!("can't read {}: {e}", self.dir.display()));
                return;
            }
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(m) = load_file(&p) {
                self.entries.push(m);
            }
        }
        self.entries.sort_by(|a, b| b.created.cmp(&a.created));
    }

    /// Create and persist an entry from quick-add text.
    pub fn add(
        &mut self,
        input: &str,
        now: NaiveDateTime,
        current_file: Option<&str>,
    ) -> Result<PathBuf, String> {
        let draft = parse_draft(input, now, current_file);
        if draft.body.trim().is_empty() {
            return Err("nothing to remember".to_string());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = unique_path(&self.dir, now, &draft.body);
        let m = Memory {
            created: now,
            due: draft.due,
            repeat: draft.repeat,
            tags: draft.tags,
            context: draft.context,
            priority: draft.priority,
            done: false,
            fired: None,
            review: None,
            interval: 0.0,
            ease: 2.5,
            body: draft.body,
            path: path.clone(),
        };
        save_file(&m).map_err(|e| e.to_string())?;
        self.entries.insert(0, m);
        Ok(path)
    }

    /// Create (or return) today's journal note.
    pub fn today_journal(&mut self, now: NaiveDateTime) -> Result<PathBuf, String> {
        let date = now.date();
        if let Some(m) = self.entries.iter().find(|m| {
            m.tags.iter().any(|t| t.eq_ignore_ascii_case("journal"))
                && m.created.date() == date
        }) {
            return Ok(m.path.clone());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = unique_path(&self.dir, now, &format!("journal-{date}"));
        let m = Memory {
            created: now,
            due: None,
            repeat: None,
            tags: vec!["journal".to_string()],
            context: None,
            priority: 0,
            done: false,
            fired: None,
            review: None,
            interval: 0.0,
            ease: 2.5,
            body: format!("# {date}\n\n"),
            path: path.clone(),
        };
        save_file(&m).map_err(|e| e.to_string())?;
        self.entries.insert(0, m);
        Ok(path)
    }

    /// Mark done (one-shot) or advance to the next occurrence (repeat).
    pub fn mark_done(&mut self, idx: usize, now: NaiveDateTime) -> Result<(), String> {
        let m = self
            .entries
            .get_mut(idx)
            .ok_or_else(|| "no such memory".to_string())?;
        if let Some(rep) = m.repeat {
            let base = m.due.unwrap_or(now);
            m.due = Some(rep.next_after(base, now));
            m.fired = None;
            m.done = false;
        } else {
            m.done = true;
        }
        save_file(m).map_err(|e| e.to_string())
    }

    /// Push a reminder's due time forward by `dur`.
    pub fn snooze(&mut self, idx: usize, now: NaiveDateTime, dur: Duration) -> Result<(), String> {
        let m = self
            .entries
            .get_mut(idx)
            .ok_or_else(|| "no such memory".to_string())?;
        let base = m.due.unwrap_or(now).max(now);
        m.due = Some(base + dur);
        m.fired = None;
        save_file(m).map_err(|e| e.to_string())
    }

    /// Reminders whose due time has passed and that haven't been
    /// delivered yet.  `fired` is written to disk before returning, and
    /// re-read first so a concurrently-running agent can't double-fire.
    pub fn take_due(&mut self, now: NaiveDateTime) -> Vec<DueAlert> {
        let mut alerts = Vec::new();
        for m in self.entries.iter_mut() {
            if m.done {
                continue;
            }
            let Some(due) = m.due else { continue };
            if due > now {
                continue;
            }
            // The LaunchAgent may have delivered this one already.
            let on_disk = load_file(&m.path).and_then(|f| f.fired);
            let effective = on_disk.or(m.fired);
            if matches!(effective, Some(f) if f >= due) {
                m.fired = effective;
                continue;
            }
            m.fired = Some(now);
            if save_file(m).is_ok() {
                alerts.push(DueAlert {
                    body: m.summary(200),
                    due,
                    priority: m.priority,
                });
            }
        }
        alerts
    }

    pub fn due_count(&self, now: NaiveDateTime) -> usize {
        self.entries
            .iter()
            .filter(|m| !m.done && m.due.map(|d| d <= now).unwrap_or(false))
            .count()
    }

    pub fn next_due(&self, now: NaiveDateTime) -> Option<NaiveDateTime> {
        self.entries
            .iter()
            .filter(|m| !m.done)
            .filter_map(|m| m.due)
            .filter(|d| *d > now)
            .min()
    }

    /// Entries for one calendar day: reminders due (or recurring) that
    /// day, plus notes created that day.  Sorted by time of day.
    pub fn entries_on(&self, date: NaiveDate) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                if m.done {
                    return false;
                }
                if m.due.is_some() {
                    m.occurs_on(date)
                } else {
                    m.created.date() == date
                }
            })
            .map(|(i, _)| i)
            .collect();
        v.sort_by_key(|i| {
            let m = &self.entries[*i];
            match m.due {
                Some(d) => (0u8, d.time()),
                None => (1u8, m.created.time()),
            }
        });
        v
    }

    /// (reminders, notes) on a day, for the calendar dots.
    pub fn counts_on(&self, date: NaiveDate) -> (usize, usize) {
        let mut reminders = 0;
        let mut notes = 0;
        for m in &self.entries {
            if m.done {
                continue;
            }
            if m.due.is_some() {
                if m.occurs_on(date) {
                    reminders += 1;
                }
            } else if m.created.date() == date {
                notes += 1;
            }
        }
        (reminders, notes)
    }

    /// Grouped indices for the dashboard.
    pub fn sections(&self, now: NaiveDateTime) -> Sections {
        let mut overdue = Vec::new();
        let mut today = Vec::new();
        let mut upcoming = Vec::new();
        let mut recent = Vec::new();
        let week = now + Duration::days(7);
        for (i, m) in self.entries.iter().enumerate() {
            if m.done {
                continue;
            }
            match m.due {
                None => recent.push(i),
                Some(d) if d <= now => overdue.push(i),
                Some(d) if d.date() == now.date() => today.push(i),
                Some(d) if d <= week => upcoming.push(i),
                Some(_) => {}
            }
        }
        let by_due = |a: &usize, b: &usize| {
            self.entries[*a]
                .due
                .cmp(&self.entries[*b].due)
        };
        overdue.sort_by(by_due);
        today.sort_by(by_due);
        upcoming.sort_by(by_due);
        recent.sort_by(|a, b| self.entries[*b].created.cmp(&self.entries[*a].created));
        recent.truncate(20);
        Sections { overdue, today, upcoming, recent }
    }
}

pub struct Sections {
    pub overdue: Vec<usize>,
    pub today: Vec<usize>,
    pub upcoming: Vec<usize>,
    pub recent: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct DueAlert {
    pub body: String,
    pub due: NaiveDateTime,
    pub priority: u8,
}

// ── notifications ────────────────────────────────────────────────

/// Play a notification/sound for a due reminder, without blocking the
/// UI.  Returns true if at least one alert was dispatched.
pub fn notify(body: &str, cfg: &MemoryConfig) -> bool {
    if !cfg.notify {
        return false;
    }
    let title = "ked reminder";
    #[cfg(target_os = "macos")]
    {
        let mut script = format!(
            "display notification {} with title {}",
            applescript_str(body),
            applescript_str(title)
        );
        if cfg.sound {
            script.push_str(" sound name \"Glass\"");
        }
        Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn()
            .ok();
        // Optional custom sound (in addition to the notification).
        if cfg.sound {
            if let Some(file) = default_sound_file() {
                let _ = Command::new("afplay").arg(file).spawn();
            }
        }
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("notify-send").arg(title).arg(body).spawn().ok();
        if cfg.sound {
            for player in ["paplay", "aplay"] {
                if Command::new(player)
                    .arg("/usr/share/sounds/freedesktop/stereo/complete.oga")
                    .spawn()
                    .is_ok()
                {
                    break;
                }
            }
        }
        return true;
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (body, cfg);
        false
    }
}

#[cfg(target_os = "macos")]
fn default_sound_file() -> Option<&'static str> {
    Some("/System/Library/Sounds/Glass.aiff")
}

#[cfg(target_os = "macos")]
fn applescript_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Is `now` inside the configured quiet window (`"22:00-08:00"`)?
pub fn in_quiet_hours(cfg: &MemoryConfig, now: NaiveDateTime) -> bool {
    let q = cfg.quiet_hours.trim();
    let Some((from, to)) = q.split_once('-') else {
        return false;
    };
    let parse = |s: &str| -> Option<NaiveTime> {
        let (h, m) = s.trim().split_once(':')?;
        NaiveTime::from_hms_opt(h.trim().parse().ok()?, m.trim().parse().ok()?, 0)
    };
    let (Some(from), Some(to)) = (parse(from), parse(to)) else {
        return false;
    };
    let t = now.time();
    if from <= to {
        t >= from && t < to
    } else {
        t >= from || t < to
    }
}

/// One-shot check for the LaunchAgent / `ked --remind`.
pub fn run_reminder_pass(cfg: &MemoryConfig) {
    let now = now();
    if in_quiet_hours(cfg, now) || !cfg.notify {
        return;
    }
    let mut store = MemoryStore::load(dir_for(cfg));
    for alert in store.take_due(now) {
        notify(&alert.body, cfg);
    }
}

// ── LaunchAgent (macOS) ──────────────────────────────────────────

#[cfg(target_os = "macos")]
const AGENT_LABEL: &str = "dev.ked.reminders";

#[cfg(target_os = "macos")]
fn agent_plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{AGENT_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Install a launchd job that runs `ked --remind` every minute, so
/// reminders fire even when the editor is closed.
pub fn install_launch_agent() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>--remind</string>
  </array>
  <key>StartInterval</key><integer>60</integer>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>/tmp/ked-reminders.out</string>
  <key>StandardErrorPath</key><string>/tmp/ked-reminders.err</string>
</dict>
</plist>
"#,
            label = AGENT_LABEL,
            exe = xml_escape(&exe),
        );
        let path = agent_plist_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&path, plist).map_err(|e| e.to_string())?;
        let uid = unsafe { libc::getuid() };
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{AGENT_LABEL}")])
            .output();
        let out = Command::new("launchctl")
            .args(["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "launchctl bootstrap failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("LaunchAgent installation is only supported on macOS; \
             run `ked --remind` from cron/systemd on other systems"
            .to_string())
    }
}

pub fn uninstall_launch_agent() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let uid = unsafe { libc::getuid() };
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{AGENT_LABEL}")])
            .output();
        let path = agent_plist_path();
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("LaunchAgent is only supported on macOS".to_string())
    }
}

// ── tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn parses_relative_words() {
        let now = at(2026, 10, 26, 9, 0); // a Monday
        let d = parse_draft("call mom @tomorrow 9am", now, None);
        assert_eq!(d.body, "call mom");
        assert_eq!(d.due, Some(at(2026, 10, 27, 9, 0)));

        let d = parse_draft("ping @+2h", now, None);
        assert_eq!(d.due, Some(at(2026, 10, 26, 11, 0)));

        let d = parse_draft("standup @weekly", now, None);
        assert_eq!(d.repeat, Some(Repeat::Weekly));
        assert!(d.due.is_some());

        let d = parse_draft("dentist @2026-11-02 14:30 #health !2", now, None);
        assert_eq!(d.due, Some(at(2026, 11, 2, 14, 30)));
        assert_eq!(d.tags, vec!["health"]);
        assert_eq!(d.priority, 2);
        assert_eq!(d.body, "dentist");
    }

    #[test]
    fn time_only_rolls_to_tomorrow_when_passed() {
        let now = at(2026, 10, 26, 18, 0);
        let d = parse_draft("water plants @8am", now, None);
        assert_eq!(d.due, Some(at(2026, 10, 27, 8, 0)));
    }

    #[test]
    fn weekday_rolls_forward() {
        // Monday 26 Oct 2026; @fri means this coming Friday.
        let now = at(2026, 10, 26, 9, 0);
        let d = parse_draft("review @fri 10am", now, None);
        assert_eq!(d.due, Some(at(2026, 10, 30, 10, 0)));
        // Same weekday but time already passed -> next week.
        let d = parse_draft("review @mon 8am", now, None);
        assert_eq!(d.due, Some(at(2026, 11, 2, 8, 0)));
    }

    #[test]
    fn repeat_advances_to_the_future() {
        let due = at(2026, 10, 20, 9, 0);
        let now = at(2026, 10, 26, 12, 0);
        assert_eq!(Repeat::Daily.next_after(due, now), at(2026, 10, 27, 9, 0));
        assert_eq!(Repeat::Weekly.next_after(due, now), at(2026, 10, 27, 9, 0));
        assert_eq!(
            Repeat::Hours(2).next_after(due, now),
            at(2026, 10, 26, 13, 0)
        );
    }

    #[test]
    fn frontmatter_round_trips() {
        let tmp = std::env::temp_dir().join(format!("ked-mem-test-{}.md", std::process::id()));
        let m = Memory {
            created: at(2026, 10, 26, 9, 0),
            due: Some(at(2026, 10, 27, 9, 0)),
            repeat: Some(Repeat::Daily),
            tags: vec!["health".into()],
            context: Some("/tmp/x.rs".into()),
            priority: 2,
            done: false,
            fired: None,
            review: None,
            interval: 0.0,
            ease: 2.5,
            body: "Call the dentist".into(),
            path: tmp.clone(),
        };
        save_file(&m).unwrap();
        let back = load_file(&tmp).unwrap();
        assert_eq!(back.body, "Call the dentist");
        assert_eq!(back.due, m.due);
        assert_eq!(back.repeat, Some(Repeat::Daily));
        assert_eq!(back.tags, vec!["health"]);
        assert_eq!(back.priority, 2);
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn quiet_hours_wrap_midnight() {
        let cfg = MemoryConfig {
            dir: String::new(),
            notify: true,
            sound: true,
            quiet_hours: "22:00-08:00".into(),
            week_start: "monday".into(),
        };
        assert!(in_quiet_hours(&cfg, at(2026, 10, 26, 23, 0)));
        assert!(in_quiet_hours(&cfg, at(2026, 10, 26, 7, 30)));
        assert!(!in_quiet_hours(&cfg, at(2026, 10, 26, 12, 0)));
    }

    #[test]
    fn month_lengths_and_grid() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 12), 31);

        // October 2026 starts on a Thursday.
        let mon = month_grid(2026, 10, 0);
        assert_eq!(mon[0], NaiveDate::from_ymd_opt(2026, 9, 28).unwrap());
        assert_eq!(mon[3], NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        assert_eq!(mon.len(), 35);

        let sun = month_grid(2026, 10, 6);
        assert_eq!(sun[0], NaiveDate::from_ymd_opt(2026, 9, 27).unwrap());
        assert_eq!(sun[4], NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
    }

    #[test]
    fn calendar_queries_and_repeats() {
        let tmp = std::env::temp_dir().join(format!("ked-mem-cal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let mut store = MemoryStore::load(tmp.clone());
        let now = at(2026, 10, 26, 9, 0);
        store.add("dentist @2026-10-27 9am", now, None).unwrap();
        store.add("note for today", now, None).unwrap();
        store
            .add("standup @2026-10-26 10am @weekly", now, None)
            .unwrap();

        let d26 = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let d27 = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let d02 = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let text = |s: &MemoryStore, i: usize| s.entries[i].summary(40);

        let on27: Vec<String> = store
            .entries_on(d27)
            .iter()
            .map(|i| text(&store, *i))
            .collect();
        assert!(on27.iter().any(|s| s.contains("dentist")));
        assert_eq!(store.counts_on(d27), (1, 0));

        let on26: Vec<String> = store
            .entries_on(d26)
            .iter()
            .map(|i| text(&store, *i))
            .collect();
        assert!(on26.iter().any(|s| s.contains("standup")));
        assert!(on26.iter().any(|s| s.contains("note for today")));

        // A weekly reminder is projected onto future dates.
        let on02: Vec<String> = store
            .entries_on(d02)
            .iter()
            .map(|i| text(&store, *i))
            .collect();
        assert!(on02.iter().any(|s| s.contains("standup")));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn due_count_and_next() {
        let tmp = std::env::temp_dir().join(format!("ked-mem-dir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let mut store = MemoryStore::load(tmp.clone());
        let now = at(2026, 10, 26, 12, 0);
        store.add("overdue @today", now, None).unwrap();
        store.add("soon @tomorrow 9am", now, None).unwrap();
        // @today resolves to now+1min, which is not yet due.
        assert_eq!(store.due_count(now), 0);
        assert_eq!(store.due_count(now + Duration::minutes(2)), 1);
        assert_eq!(store.next_due(now), Some(now + Duration::minutes(1)));
        let _ = fs::remove_dir_all(&tmp);
    }
}
