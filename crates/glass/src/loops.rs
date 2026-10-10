//! `/loop`: prompts that fire on an interval while the session is idle.
//!
//! Claude's `/loop` and Codex's automations turn a session from interactive
//! into progressing-while-you-are-away. This version keeps the whole feature
//! in one sidecar next to the session (`<id>.loops.json`), fires only when
//! the session is idle — never mid-turn, never stacked — and skips missed
//! intervals rather than replaying them. With `serve`, a detached session
//! keeps running its loops; that is the autonomy story, no daemon required.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth;
use crate::session::Session;

/// One scheduled prompt.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub prompt: String,
    /// Interval in seconds (>= 1).
    pub every_s: u64,
    /// Unix seconds when this should next fire.
    pub next_at: u64,
    #[serde(default)]
    pub runs: u64,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Loops {
    pub entries: Vec<Entry>,
    /// Bounded dispatch receipts; these do not claim task completion.
    pub history: Vec<Dispatch>,
}

fn path(s: &Session) -> PathBuf {
    s.path.with_extension("loops.json")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Dispatch {
    pub at: u64,
    pub prompt: String,
    pub missed_intervals: u64,
    pub status: String,
}

fn load_checked(s: &Session) -> Result<Loops, String> {
    let path = path(s);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Loops::default()),
        Err(error) => return Err(format!("automation schedule: {error}")),
        Ok(_) => {},
    }
    let bytes = crate::requests::bounded(&path, 1024 * 1024)?;
    let loops: Loops = serde_json::from_slice(&bytes).map_err(|e| format!("automation schedule: {e}"))?;
    if loops.entries.len() > 16 || loops.history.len() > 64
        || loops.entries.iter().any(|e| e.every_s == 0 || e.prompt.is_empty() || e.prompt.len() > 8000)
        || loops.history.iter().any(|e| e.prompt.len() > 8000) {
        return Err("Automation schedule exceeds limits or contains invalid entries".into());
    }
    Ok(loops)
}

#[cfg(test)]
pub fn load(s: &Session) -> Loops {
    load_checked(s).expect("valid test schedule")
}

fn save(s: &Session, loops: &Loops) -> Result<(), String> {
    auth::secure_write(
        &path(s),
        &serde_json::to_vec_pretty(loops).map_err(|e| e.to_string())?,
    )
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse `30m`, `2h`, `45s`, or bare seconds. Zero is refused: a loop that
/// fires continuously is a bug wearing a feature's clothes.
pub fn parse_every(text: &str) -> Option<u64> {
    let t = text.trim().to_ascii_lowercase();
    let (digits, multiplier) = if let Some(v) = t.strip_suffix('s') {
        (v, 1)
    } else if let Some(v) = t.strip_suffix('m') {
        (v, 60)
    } else if let Some(v) = t.strip_suffix('h') {
        (v, 3600)
    } else {
        (t.as_str(), 1)
    };
    let n: u64 = digits.trim().parse().ok()?;
    if n == 0 { None } else { n.checked_mul(multiplier) }
}

/// The first due prompt, advanced and persisted. `None` when nothing is due.
///
/// Missed intervals skip: a session resumed after a day fires each loop once,
/// not 2,880 times.
pub fn due(s: &Session) -> Option<String> {
    // A damaged or unwritable schedule must never execute an unacknowledged run.
    due_checked(s).ok().flatten()
}

pub fn due_checked(s: &Session) -> Result<Option<String>, String> {
    let mut loops = load_checked(s)?;
    let at = now();
    let Some(idx) = loops.entries.iter().position(|e| e.next_at <= at) else { return Ok(None); };
    let entry = &mut loops.entries[idx];
    let missed_intervals = at.saturating_sub(entry.next_at) / entry.every_s;
    entry.runs = entry.runs.checked_add(1).ok_or("Automation run counter overflow")?;
    entry.next_at = at.checked_add(entry.every_s).ok_or("Automation interval exceeds supported date range")?;
    let prompt = entry.prompt.clone();
    loops.history.push(Dispatch { at, prompt: prompt.clone(), missed_intervals,
        status: "dispatched; completion is recorded in the conversation".into() });
    if loops.history.len() > 64 { loops.history.remove(0); }
    save(s, &loops)?;
    Ok(Some(prompt))
}

fn human(seconds: u64) -> String {
    match seconds {
        s if s >= 3600 => format!("{}h{:02}m", s / 3600, (s % 3600) / 60),
        s if s >= 60 => format!("{}m{:02}s", s / 60, s % 60),
        s => format!("{s}s"),
    }
}

/// `/loop` command handling. `args` is everything after the command word.
pub fn command(s: &Session, args: &str) -> Result<String, String> {
    let args = args.trim();
    if args == "history" {
        let loops = load_checked(s)?;
        return Ok(serde_json::to_string_pretty(&loops.history).map_err(|e| e.to_string())?);
    }
    if args.is_empty() || args == "list" {
        let loops = load_checked(s)?;
        if loops.entries.is_empty() {
            return Ok(
                "no loops. Add one with `/loop 30m check the build`, remove with \
                 `/loop rm <n>`, clear with `/loop clear`."
                    .into(),
            );
        }
        let now = now();
        let mut out = String::from("loops (fire when the session is idle):\n");
        for (i, e) in loops.entries.iter().enumerate() {
            let wait = e.next_at.saturating_sub(now);
            out.push_str(&format!(
                "  {}. every {} · ran {}× · next in {} — {}\n",
                i + 1,
                human(e.every_s),
                e.runs,
                human(wait),
                e.prompt
            ));
        }
        return Ok(out.trim_end().to_string());
    }
    if let Some(rest) = args.strip_prefix("rm ") {
        let n: usize = rest
            .trim()
            .parse()
            .map_err(|_| "usage: /loop rm <n> (see /loop list)".to_string())?;
        let mut loops = load_checked(s)?;
        if n == 0 || n > loops.entries.len() {
            return Err(format!("no loop {n}; there are {}", loops.entries.len()));
        }
        let removed = loops.entries.remove(n - 1);
        save(s, &loops)?;
        return Ok(format!("loop removed: {}", removed.prompt));
    }
    if args == "clear" {
        save(s, &Loops::default())?;
        return Ok("all loops cleared".into());
    }
    let (interval, prompt) = args
        .split_once(char::is_whitespace)
        .ok_or("usage: /loop <interval> <prompt>  (intervals: 90s, 30m, 2h)")?;
    let every = parse_every(interval)
        .ok_or_else(|| format!("interval `{interval}` is not a positive time (90s, 30m, 2h)"))?;
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("the loop needs a prompt".into());
    }
    // A long instruction is easy to paste by accident; loops multiply it.
    if prompt.len() > 8_000 {
        return Err("the loop prompt is longer than 8 KB".into());
    }
    let mut loops = load_checked(s)?;
    if loops.entries.len() >= 16 {
        return Err("16 loops is the cap; remove one first".into());
    }
    loops.entries.push(Entry {
        prompt: prompt.to_string(),
        every_s: every,
        next_at: now().checked_add(every).ok_or("Automation interval exceeds supported date range")?,
        runs: 0,
    });
    save(s, &loops)?;
    Ok(format!(
        "loop added: every {} — {prompt} (fires when the session is idle)",
        human(every)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_schedule_is_preserved_and_never_dispatched() {
        let root = std::env::temp_dir().join(format!("gc-loops-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let s = Session::create(&root).unwrap();
        std::fs::write(path(&s), b"{broken").unwrap();
        assert!(due_checked(&s).is_err());
        assert!(command(&s, "1s hello").is_err());
        assert_eq!(std::fs::read(path(&s)).unwrap(), b"{broken");
        let _ = std::fs::remove_dir_all(root);
    }


    #[test]
    fn intervals_parse_with_suffixes_and_bare_seconds() {
        assert_eq!(parse_every("90"), Some(90));
        assert_eq!(parse_every("45s"), Some(45));
        assert_eq!(parse_every("30m"), Some(1800));
        assert_eq!(parse_every("2h"), Some(7200));
        assert_eq!(parse_every(" 10M "), Some(600));
        assert_eq!(parse_every("0"), None, "a zero interval is refused");
        assert_eq!(parse_every("0m"), None);
        assert_eq!(parse_every("soon"), None);
        assert_eq!(parse_every("-5m"), None);
        assert_eq!(parse_every("18446744073709551615h"), None);
    }

    #[test]
    fn due_fires_once_and_skips_missed_intervals() {
        let root = std::env::temp_dir().join(format!("gc-loops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut s = Session::create(&root).unwrap();
        s.append(crate::session::Record::User("x".into()));
        let mut loops = Loops::default();
        loops.entries.push(Entry {
            prompt: "check the build".into(),
            every_s: 60,
            next_at: now().saturating_sub(600), // ten intervals overdue
            runs: 0,
        });
        save(&s, &loops).unwrap();

        assert_eq!(due(&s).as_deref(), Some("check the build"));
        assert_eq!(due(&s), None, "not due again immediately");
        let after = load(&s);
        assert_eq!(after.entries[0].runs, 1);
        assert_eq!(after.history.len(), 1);
        assert!(after.history[0].missed_intervals >= 10);
        assert!(
            after.entries[0].next_at >= now() + 59,
            "missed intervals skip forward instead of stacking"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn command_adds_lists_and_removes() {
        let root = std::env::temp_dir().join(format!("gc-loops-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let s = Session::create(&root).unwrap();
        assert!(command(&s, "").unwrap().contains("no loops"));
        let out = command(&s, "30m run the tests").unwrap();
        assert!(out.contains("every 30m"), "{out}");
        let listing = command(&s, "").unwrap();
        assert!(listing.contains("run the tests"), "{listing}");
        assert!(command(&s, "nonsense").is_err(), "interval is validated");
        assert!(command(&s, "5m").is_err(), "prompt is required");
        let removed = command(&s, "rm 1").unwrap();
        assert!(removed.contains("loop removed"), "{removed}");
        assert!(command(&s, "").unwrap().contains("no loops"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
