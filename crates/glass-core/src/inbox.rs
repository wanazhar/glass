//! `gc send`: messages between glass-core sessions on this machine.
//!
//! Claude has cross-session messaging; two glass-core sessions had no way to
//! talk even when both were running next to each other. The mechanism here is
//! deliberately the dumbest one that works: a line-per-message file per
//! session under `<data>/inbox/<id>.jsonl` (0600). A writer appends; a running
//! session reads new lines when it is idle and turns each into a normal turn,
//! with the sender named in a `System` note so the model knows where it came
//! from. No broker, no protocol, nothing to keep alive.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
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
    if from.is_empty() || from.len() > 128 || from.chars().any(char::is_control) {
        return Err("Sender must be 1–128 bytes without control characters".into());
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
        .read(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Inbox must be a regular file".into());
    }
    fs2::FileExt::try_lock_exclusive(&file).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
        return Err("Inbox reached 16 MiB; archive it after reviewing delivery receipts".into());
    }
    let line = serde_json::json!({
        "from": from,
        "text": text,
        "at": crate::loops::now(),
    });
    let start = file.metadata().map_err(|e| e.to_string())?.len();
    if let Err(error) = writeln!(file, "{line}").and_then(|()| file.sync_data()) {
        file.set_len(start).map_err(|e| e.to_string())?;
        return Err(error.to_string());
    }
    Ok(())
}

/// The reader side: one new message per call, at most.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Cursor {
    offset: usize,
    device: u64,
    inode: u64,
    boundary: String,
}
struct Pending {
    next: usize,
    cursor: Cursor,
    message: Option<(String, String)>,
}
fn boundary(file: &mut std::fs::File, offset: usize) -> Result<String, String> {
    let start = offset.saturating_sub(256);
    file.seek(SeekFrom::Start(start as u64))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![0; offset - start];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(crate::workspace::hash(&bytes))
}
pub struct Mailbox {
    path: PathBuf,
    seen: usize,
    cursor: Option<Cursor>,
}

impl Mailbox {
    pub fn open(session: &Session) -> Self {
        let data = session
            .history_records()
            .iter()
            .rev()
            .find_map(|r| match r {
                crate::session::Record::Custom {
                    kind,
                    data: Some(data),
                } if kind == "inbox/delivery" => Some(data.clone()),
                _ => None,
            });
        let cursor = data
            .as_ref()
            .and_then(|s| serde_json::from_str::<Cursor>(s).ok());
        let seen = cursor
            .as_ref()
            .map(|c| c.offset)
            .or_else(|| data.as_ref().and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        Self {
            path: path(&session.id),
            seen,
            cursor,
        }
    }

    fn peek(&self) -> Result<Option<Pending>, String> {
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("Inbox must be a regular file".into());
        }
        if meta.len() < self.seen as u64 {
            return Err(
                "Inbox was truncated after delivery; restore it before receiving more messages"
                    .into(),
            );
        }
        let mut file = file;
        if let Some(cursor) = &self.cursor
            && (cursor.device != meta.dev()
                || cursor.inode != meta.ino()
                || cursor.boundary != boundary(&mut file, self.seen)?)
        {
            return Err("Inbox identity or delivered boundary changed; restore the original mailbox before continuing".into());
        }
        file.seek(SeekFrom::Start(self.seen as u64))
            .map_err(|e| e.to_string())?;
        let mut line = Vec::new();
        BufReader::new((&mut file).take(262145))
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if line.len() > 262144 {
            return Err("Inbox record exceeds 256 KiB; inspect it before continuing".into());
        }
        if line.last() != Some(&b'\n') {
            return Ok(None);
        }
        let next = self
            .seen
            .checked_add(line.len())
            .ok_or("Inbox cursor overflow")?;
        let message = serde_json::from_slice::<serde_json::Value>(&line)
            .ok()
            .and_then(|v| {
                let text = v["text"].as_str()?.to_owned();
                if text.trim().is_empty() || text.len() > 32768 {
                    return None;
                }
                let from = v["from"].as_str().unwrap_or("unknown");
                Some((
                    glass_tui::markdown::sanitize(from)
                        .chars()
                        .take(128)
                        .collect(),
                    text,
                ))
            });
        let cursor = Cursor {
            offset: next,
            device: meta.dev(),
            inode: meta.ino(),
            boundary: boundary(&mut file, next)?,
        };
        Ok(Some(Pending {
            next,
            cursor,
            message,
        }))
    }

    /// Acceptance and the user prompt are one durable transaction. Provider
    /// execution may fail afterward, but a restart never repeats this prompt.
    pub fn deliver(&mut self, session: &mut Session) -> Result<Option<(String, String)>, String> {
        let Some(Pending {
            next,
            cursor,
            message,
        }) = self.peek()?
        else {
            return Ok(None);
        };
        let mut records = vec![crate::session::Record::Custom {
            kind: "inbox/delivery".into(),
            data: Some(serde_json::to_string(&cursor).map_err(|e| e.to_string())?),
        }];
        let message = message.map(|(from, text)| {
            let prompt = format!("[Background follow-up from {from}]\n{text}");
            records.push(crate::session::Record::User(prompt.clone()));
            (from, prompt)
        });
        session
            .append_checked(&records)
            .map_err(|e| e.to_string())?;
        self.seen = next;
        self.cursor = Some(cursor);
        Ok(message)
    }

    #[cfg(test)]
    fn next(&mut self) -> Option<(String, String)> {
        let Pending {
            next,
            cursor,
            message,
        } = self.peek().ok().flatten()?;
        self.seen = next;
        self.cursor = Some(cursor);
        message
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
    fn delivered_message_and_cursor_survive_restart_without_replay() {
        let root = std::env::temp_dir().join(format!("gc-inbox-restart-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut session = Session::create(&root).unwrap();
        let id = session.id.clone();
        let mailbox_path = root.join("mailbox.jsonl");
        std::fs::write(&mailbox_path, "{\"from\":\"human\",\"text\":\"hello\"}\n").unwrap();
        let mut mailbox = Mailbox {
            path: mailbox_path.clone(),
            seen: 0,
            cursor: None,
        };
        assert!(mailbox.deliver(&mut session).unwrap().is_some());
        drop(session);
        let mut session = Session::open(&root, &id).unwrap();
        let mut mailbox = Mailbox::open(&session);
        mailbox.path = mailbox_path;
        assert!(mailbox.deliver(&mut session).unwrap().is_none());
        std::fs::write(&mailbox.path, "changed mailbox boundary".repeat(10)).unwrap();
        assert!(mailbox.deliver(&mut session).is_err());
        assert_eq!(
            session
                .history_records()
                .iter()
                .filter(|r| matches!(r, crate::session::Record::User(_)))
                .count(),
            1
        );
        drop(session);
        let _ = std::fs::remove_dir_all(root);
    }

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
            cursor: None,
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
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
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
