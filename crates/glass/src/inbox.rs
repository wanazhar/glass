//! `gc send`: messages between glass-core sessions on this machine.
//!
//! Claude has cross-session messaging; two glass-core sessions had no way to
//! talk even when both were running next to each other. The mechanism here is
//! deliberately the dumbest one that works: a line-per-message file per
//! session under `<data>/inbox/<id>.jsonl` (0600). A writer appends; a running
//! session reads new lines when it is idle and turns each into a normal turn,
//! with the sender named in a `System` note so the model knows where it came
//! from. No broker, no protocol, nothing to keep alive.

use std::io::Write;
use std::path::PathBuf;

use crate::config;
use crate::session::Session;

/// Inbox files live here; one per session id.
pub fn inbox_dir() -> PathBuf {
    config::data_dir().join("inbox")
}

fn path(id: &str) -> PathBuf {
    inbox_dir().join(format!("{id}.jsonl"))
}

/// Session ids are filenames here, so they are checked, not trusted.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Append one message. Refuses oversized text: an inbox that can carry a
/// megabyte is a prompt-injection delivery mechanism, not a mailbox.
pub fn send(id: &str, text: &str, from: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("`{id}` is not a session id"));
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("the message is empty".into());
    }
    if text.len() > 32_768 {
        return Err("the message exceeds 32 KiB".into());
    }
    std::fs::create_dir_all(inbox_dir()).map_err(|e| e.to_string())?;
    let path = path(id);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    let line = serde_json::json!({
        "from": from,
        "text": text,
        "at": crate::loops::now(),
    });
    writeln!(file, "{line}").map_err(|e| e.to_string())?;
    Ok(())
}

/// The reader side: one new message per call, at most.
pub struct Mailbox {
    path: PathBuf,
    seen: usize,
}

impl Mailbox {
    pub fn open(id: &str) -> Self {
        Self {
            path: path(id),
            seen: 0,
        }
    }

    /// The next unread message as `(from, text)`.
    ///
    /// Only complete, parseable lines are consumed. A writer mid-append (no
    /// trailing newline yet) is not delivered this tick and is not skipped:
    /// advancing past a partial line would lose the message once the writer
    /// finishes, which is exactly the race a mailbox must not have.
    pub fn next(&mut self) -> Option<(String, String)> {
        let data = std::fs::read_to_string(&self.path).ok()?;
        let lines: Vec<&str> = data.lines().filter(|l| !l.trim().is_empty()).collect();
        if self.seen >= lines.len() {
            return None;
        }
        let parsed = serde_json::from_str::<serde_json::Value>(lines[self.seen])
            .ok()
            .and_then(|value| {
                let from = value
                    .get("from")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                value
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|t| (from, t.to_string()))
            });
        match parsed {
            Some(message) => {
                self.seen += 1;
                Some(message)
            }
            None => {
                if self.seen == lines.len() - 1 && !data.ends_with('\n') {
                    None // possibly mid-append: wait
                } else {
                    self.seen += 1; // complete but malformed: skip it
                    None
                }
            }
        }
    }
}

/// `gc send …` entry point.
pub fn command(cfg: &config::Config, args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("usage: glass-core send <session-id|--latest> <message>".into());
    }
    let target = &args[0];
    let text = args[1..].join(" ");
    let id = if target == "--latest" {
        crate::polish::sessions(&cfg.workspace)
            .into_iter()
            .next()
            .map(|s| s.id)
            .ok_or("no session found for this workspace")?
    } else {
        target.clone()
    };
    send(&id, &text, "cli")?;
    crate::cli_println!("sent to {id}");
    Ok(())
}

/// `/send <id> <text>` from inside a session.
pub fn from_session(session: &Session, args: &str) -> Result<String, String> {
    let (id, text) = args
        .trim()
        .split_once(char::is_whitespace)
        .ok_or("usage: /send <session-id> <message>")?;
    // `--latest` needs the workspace, which the caller does not hand us; the
    // command line has it, so the in-session form takes an explicit id.
    send(id, text, &session.id)?;
    Ok(format!("sent to {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_one_at_a_time_reads() {
        let dir = std::env::temp_dir().join(format!(
            "gc-inbox-{}-{}",
            std::process::id(),
            crate::loops::now()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // Point the mailbox at a scratch dir by writing through the module's
        // own path helper after overriding the data dir is not possible here
        // (config is process-wide), so exercise the file directly.
        let file = dir.join("session-1.jsonl");
        let mut mailbox = Mailbox {
            path: file.clone(),
            seen: 0,
        };
        assert_eq!(mailbox.next(), None, "missing file is simply empty");

        let write = |from: &str, text: &str| {
            let line = serde_json::json!({"from": from, "text": text, "at": 0});
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file)
                .unwrap();
            writeln!(f, "{line}").unwrap();
        };
        write("a", "first");
        let got = mailbox.next();
        assert_eq!(got, Some(("a".into(), "first".into())));
        assert_eq!(mailbox.next(), None, "not delivered twice");
        write("b", "second");
        write("b", "third");
        assert_eq!(mailbox.next(), Some(("b".into(), "second".into())));
        assert_eq!(mailbox.next(), Some(("b".into(), "third".into())));
        assert_eq!(mailbox.next(), None);

        // A writer mid-append is not delivered until its line is complete.
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
        write!(f, "{{\"from\":\"c\"").unwrap();
        assert_eq!(mailbox.next(), None, "partial line waits");
        writeln!(f, ",\"text\":\"fourth\",\"at\":0}}").unwrap();
        assert_eq!(mailbox.next(), Some(("c".into(), "fourth".into())));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversize_empty_and_bad_targets_are_refused() {
        assert!(send("", "x", "cli").is_err());
        assert!(send("../escape", "x", "cli").is_err());
        assert!(send("ok-id", "   ", "cli").is_err());
        assert!(send("ok-id", &"x".repeat(40_000), "cli").is_err());
    }

    #[test]
    fn valid_ids_follow_the_filename_rules() {
        assert!(valid_id("abc-123_DEF"));
        assert!(!valid_id("has/slash"));
        assert!(!valid_id("has space"));
        assert!(!valid_id(""));
        assert!(!valid_id(&"x".repeat(200)));
    }
}
