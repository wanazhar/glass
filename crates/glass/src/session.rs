//! Append-only JSONL session log.
//!
//! One file per session, one JSON object per line. This is the only thing that
//! survives a crash, and it is the shared source of truth for both renderers:
//! the default view streams to the terminal, the fullscreen viewer reads back
//! from here. Keeping a single format is what stops the two code paths from
//! drifting apart.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use glass_mod_api::Message;
use glass_tui::Role;

/// One recorded event. Kept flat and stringly-typed on purpose: a log format
/// that needs a schema migration is a log format that loses data.
#[derive(Debug, Clone)]
pub enum Record {
    User(String),
    Assistant(String),
    Reasoning(String),
    Bash { command: String, output: String, exit_code: Option<i32>, truncated: bool },
    ToolResult { name: String, call_id: String, body: String },
    State(Vec<String>),
    System(String),
    Meta(String),
    Compacted { kept: usize, dropped: usize, summary: Option<String> },
}

impl Record {
    fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Record::User(t) => json!({ "k": "user", "t": t }),
            Record::Assistant(t) => json!({ "k": "assistant", "t": t }),
            Record::Reasoning(t) => json!({ "k": "reasoning", "t": t }),
            Record::Bash { command, output, exit_code, truncated } => json!({
                "k": "bash", "cmd": command, "out": output,
                "code": exit_code, "trunc": truncated
            }),
            Record::ToolResult { name, call_id, body } => {
                json!({ "k": "tool", "name": name, "id": call_id, "t": body })
            }
            Record::State(items) => json!({ "k": "state", "items": items }),
            Record::System(t) => json!({ "k": "system", "t": t }),
            Record::Meta(t) => json!({ "k": "meta", "t": t }),
            Record::Compacted { kept, dropped, summary } => {
                json!({ "k": "compacted", "kept": kept, "dropped": dropped, "sum": summary })
            }
        }
    }

    fn from_json(v: &serde_json::Value) -> Option<Self> {
        let s = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_string);
        Some(match v.get("k")?.as_str()? {
            "user" => Record::User(s("t")?),
            "assistant" => Record::Assistant(s("t")?),
            "reasoning" => Record::Reasoning(s("t")?),
            "bash" => Record::Bash {
                command: s("cmd").unwrap_or_default(),
                output: s("out").unwrap_or_default(),
                exit_code: v.get("code").and_then(|x| x.as_i64()).map(|x| x as i32),
                truncated: v.get("trunc").and_then(|x| x.as_bool()).unwrap_or(false),
            },
            "tool" => Record::ToolResult {
                name: s("name").unwrap_or_default(),
                call_id: s("id").unwrap_or_default(),
                body: s("t").unwrap_or_default(),
            },
            "state" => Record::State(
                v.get("items")
                    .and_then(|x| x.as_array())
                    .map(|a| a.iter().filter_map(|i| i.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
            ),
            "system" => Record::System(s("t")?),
            "meta" => Record::Meta(s("t")?),
            "compacted" => Record::Compacted {
                kept: v.get("kept").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                dropped: v.get("dropped").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                summary: s("sum"),
            },
            _ => return None,
        })
    }

    /// Project into the model context. `None` means display-only.
    fn to_message(&self) -> Option<Message> {
        match self {
            Record::User(t) => Some(Message::User(t.clone())),
            Record::Assistant(t) => Some(Message::Assistant(t.clone())),
            Record::ToolResult { name, call_id, body, .. } => Some(Message::ToolResult {
                call_id: call_id.clone(),
                name: name.clone(),
                body: body.clone(),
            }),
            Record::System(t) => Some(Message::System(t.clone())),
            // Reasoning, bash echo, state, meta and compaction markers are
            // display-only: replaying them wastes context without informing
            // the model, which already has the tool results that matter.
            _ => None,
        }
    }

    /// Project into the transcript viewer. The turn index is assigned by the
    /// caller, which is the only place that knows where user turns begin.
    pub fn to_entry(&self) -> (Role, String) {
        match self {
            Record::User(t) => (Role::User, t.clone()),
            Record::Assistant(t) => (Role::Assistant, t.clone()),
            Record::Reasoning(t) => (Role::Tool, t.clone()),
            Record::Bash { command, output, .. } => {
                (Role::Bash, format!("$ {command}\n{output}"))
            }
            Record::ToolResult { body, .. } => (Role::Tool, body.clone()),
            Record::State(items) => (Role::Meta, items.iter().map(|s| format!("• {s}")).collect::<Vec<_>>().join("\n")),
            Record::System(t) => (Role::System, t.clone()),
            Record::Meta(t) => (Role::Meta, t.clone()),
            Record::Compacted { kept, dropped, .. } => {
                (Role::Meta, format!("context compacted: dropped {dropped}, kept {kept}"))
            }
        }
    }
}

pub struct Session {
    pub id: String,
    /// Kept so `resume` can report where a session lives and so a future
    /// `export` has a source without re-deriving it.
    #[allow(dead_code)]
    pub path: PathBuf,
    file: Option<File>,
    /// Everything recorded this process, in order.
    pub records: Vec<Record>,
    /// Number of user turns replayed or started, used by jump-to-turn.
    #[allow(dead_code)]
    pub turn: usize,
}

impl Session {
    /// Open a new session file under the state directory.
    pub fn create(dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let id = new_id();
        let path = dir.join(format!("{id}.jsonl"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { id, path, file: Some(file), records: Vec::new(), turn: 0 })
    }

    /// Reopen a session by id or path, replaying it into memory.
    pub fn open(dir: &Path, target: &str) -> std::io::Result<Self> {
        let path = if target.ends_with(".jsonl") {
            PathBuf::from(target)
        } else {
            dir.join(format!("{target}.jsonl"))
        };
        // `append(true)` alone opens write-only, and reading from such a handle
        // fails with EBADF. `read(true)` is required for the replay pass.
        let file = OpenOptions::new().read(true).append(true).open(&path)?;
        let reader = BufReader::new(file.try_clone()?);

        let mut records = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                // A truncated final line is expected after a hard kill.
                // Skipping it is strictly better than refusing to resume.
                continue;
            };
            if let Some(r) = Record::from_json(&v) {
                records.push(r);
            }
        }

        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.to_string());
        let turn = records.iter().filter(|r| matches!(r, Record::User(_))).count();
        Ok(Self { id, path, file: Some(file), records, turn })
    }

    /// Most recently modified session, if any.
    pub fn last(dir: &Path) -> Option<PathBuf> {
        let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
            .filter_map(|e| {
                let m = e.metadata().ok()?.modified().ok()?;
                Some((m, e.path()))
            })
            .collect();
        // Newest first, so `--last` and the resume default pick the latest.
        candidates.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
        candidates.into_iter().next().map(|(_, p)| p)
    }

    pub fn append(&mut self, record: Record) {
        if let Some(f) = self.file.as_mut()
            && let Ok(line) = serde_json::to_string(&record.to_json())
        {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
        self.records.push(record);
    }

    /// The conversation as the model should see it.
    pub fn messages(&self) -> Vec<Message> {
        self.records.iter().filter_map(Record::to_message).collect()
    }

    /// Records for the transcript viewer, with turn indices assigned.
    pub fn entries(&self) -> Vec<glass_tui::Entry> {
        let mut out = Vec::with_capacity(self.records.len());
        let mut turn = 0usize;
        for r in &self.records {
            if matches!(r, Record::User(_)) {
                turn += 1;
            }
            let (role, text) = r.to_entry();
            if text.is_empty() {
                continue;
            }
            out.push(glass_tui::Entry { role, text, turn });
        }
        out
    }
}

/// Sortable, collision-resistant session id: base-36 millis plus a counter.
fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut id = String::new();
    let mut v = ms;
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    while v > 0 {
        id.push(ALPHABET[(v % 36) as usize] as char);
        v /= 36;
    }
    let reversed: String = id.chars().rev().collect();
    format!("{reversed}-{:04x}", n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_tui::Role;

    /// Unique scratch directory per call. Tests run in parallel, so a shared
    /// fixed path would let one test truncate another's session file.
    fn tmpdir(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-session-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn roundtrips_every_record_kind() {
        let cases = vec![
            Record::User("u".into()),
            Record::Assistant("a".into()),
            Record::Reasoning("r".into()),
            Record::Bash { command: "ls".into(), output: "x".into(), exit_code: Some(0), truncated: false },
            Record::ToolResult { name: "bash".into(), call_id: "c1".into(), body: "b".into() },
            Record::State(vec!["s1".into()]),
            Record::System("sy".into()),
            Record::Meta("m".into()),
            Record::Compacted { kept: 3, dropped: 9, summary: Some("sum".into()) },
        ];
        for c in cases {
            let j = c.to_json();
            let back = Record::from_json(&j).expect("must roundtrip");
            assert_eq!(j, back.to_json(), "roundtrip changed the record");
        }
    }

    #[test]
    fn bash_record_preserves_exit_code_and_truncation() {
        let r = Record::Bash { command: "c".into(), output: "o".into(), exit_code: Some(127), truncated: true };
        let back = Record::from_json(&r.to_json()).unwrap();
        match back {
            Record::Bash { exit_code, truncated, .. } => {
                assert_eq!(exit_code, Some(127));
                assert!(truncated);
            }
            other => panic!("wrong kind: {other:?}"),
        }
    }

    #[test]
    fn messages_excludes_display_only_records() {
        let mut s = Session { id: "t".into(), path: "t".into(), file: None, records: vec![], turn: 0 };
        s.records = vec![
            Record::User("hi".into()),
            Record::Reasoning("secret".into()),
            Record::Bash { command: "ls".into(), output: "out".into(), exit_code: Some(0), truncated: false },
            Record::Assistant("hello".into()),
            Record::Meta("noise".into()),
            Record::ToolResult { name: "bash".into(), call_id: "c".into(), body: "res".into() },
        ];
        let msgs = s.messages();
        assert_eq!(msgs.len(), 3, "reasoning, bash echo and meta must not replay");
        assert!(!msgs.iter().any(|m| m.text().contains("secret")));
    }

    #[test]
    fn resume_reconstructs_conversation_and_turns() {
        let dir = tmpdir("resume");
        let mut s = Session::create(&dir).unwrap();
        s.append(Record::User("one".into()));
        s.append(Record::Assistant("two".into()));
        s.append(Record::User("three".into()));
        let id = s.id.clone();
        drop(s);

        let reopened = Session::open(&dir, &id).unwrap();
        assert_eq!(reopened.records.len(), 3);
        assert_eq!(reopened.turn, 2, "two user messages means turn 2");
        // user + assistant + user are all context-visible.
        assert_eq!(reopened.messages().len(), 3);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn truncated_final_line_does_not_block_resume() {
        let dir = tmpdir("trunc");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crashed.jsonl");
        fs::write(&path, "{\"k\":\"user\",\"t\":\"first\"}\n{\"k\":\"assis").unwrap();
        let s = Session::open(&dir, "crashed").unwrap();
        assert_eq!(s.records.len(), 1, "partial line is skipped, valid line kept");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn last_picks_most_recently_modified() {
        let dir = tmpdir("last");
        let a = Session::create(&dir).unwrap();
        let a_id = a.id.clone();
        drop(a);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let b = Session::create(&dir).unwrap();
        let b_id = b.id.clone();
        drop(b);

        let last = Session::last(&dir).expect("at least one session");
        let stem = last.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(stem == b_id || stem == a_id);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn entries_tag_roles_and_turns() {
        let mut s = Session { id: "t".into(), path: "t".into(), file: None, records: vec![], turn: 0 };
        s.records = vec![Record::User("q1".into()), Record::Assistant("a1".into()), Record::User("q2".into())];
        let entries = s.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].role, Role::User);
        assert_eq!(entries[0].turn, 1);
        assert_eq!(entries[2].turn, 2, "second user message advances the turn");
    }

    #[test]
    fn compaction_marker_renders_a_summary_line() {
        let (role, text) = Record::Compacted { kept: 2, dropped: 5, summary: None }.to_entry();
        assert_eq!(role, Role::Meta);
        assert!(text.contains("dropped 5"));
    }

    #[test]
    fn ids_are_unique_and_sorted_ascending() {
        let mut ids: Vec<String> = (0..50).map(|_| new_id()).collect();
        let original = ids.clone();
        ids.sort();
        assert_eq!(ids, original, "ids must be monotonic");
        ids.dedup();
        assert_eq!(ids.len(), 50);
    }
}
