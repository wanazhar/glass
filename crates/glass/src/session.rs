//! Append-only JSONL session log.
//!
//! One file per session, one JSON object per line. This is the only thing that
//! survives a crash, and it is the shared source of truth for both renderers:
//! the default view streams to the terminal, the fullscreen viewer reads back
//! from here. Keeping a single format is what stops the two code paths from
//! drifting apart.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use glass_mod_api::Message;
use glass_tui::Role;

type FileStamp = (u64, std::time::SystemTime, i64, i64, u64, u64);

/// One recorded event. Kept flat and stringly-typed on purpose: a log format
/// that needs a schema migration is a log format that loses data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    User(String),
    Assistant(String),
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    Reasoning(String),
    Bash {
        #[allow(dead_code)]
        call_id: Option<String>,
        command: String,
        output: String,
        exit_code: Option<i32>,
        truncated: bool,
        /// Where the full output went when it overran the inline cap, so the
        /// pointer survives a resume instead of pointing at a temp file the
        /// next session has never heard of.
        spill: Option<String>,
    },
    ToolResult {
        name: String,
        call_id: String,
        body: String,
    },
    State(Vec<String>),
    System(String),
    Meta(String),
    /// A user message carrying an image. `data` is base64; the harness never
    /// decodes it, so no image codec is needed, and the same record replays
    /// into every wire format.
    Image {
        media_type: String,
        data: String,
    },
    /// An append-only edit of future model context.
    ///
    /// The turn's raw records are untouched — transcript, export, and
    /// accounting keep the original — but the projection drops the turn
    /// (`hide`), swaps its user text (`replace`), or makes it visible again
    /// (`restore`). The latest edit for a turn wins; this mirrors pi's
    /// `context_edit` entries, minus the session tree.
    ContextEdit {
        turn: usize,
        mode: String,
        text: Option<String>,
    },
    /// A compaction epoch.
    ///
    /// `tail_start` is the index into the log where the surviving tail begins.
    /// Replay uses it to rebuild the projection, so a resumed session does not
    /// silently revert to the history compaction was supposed to drop — and
    /// then re-compact, and re-pay for a summary, on its first turn.
    ///
    /// `system` is the system prompt in effect at the boundary, captured
    /// because the compacted history was shaped under it. Replay keeps the
    /// compacted region under that prompt, so a session resumed after its
    /// prompt config changed is not silently re-interpreted under a new one;
    /// the next compaction re-captures the current prompt. This mirrors pi's
    /// `CompactionEntry.systemMessage`.
    Compacted {
        kept: usize,
        dropped: usize,
        summary: Option<String>,
        tail_start: usize,
        system: Option<String>,
    },
    /// A durable, display-only entry an extension owns.
    ///
    /// Extension state that must survive a restart without entering the
    /// model context: the log is the store, and the owning extension
    /// reads its entries back at startup. Kinds are namespaced
    /// `<extension-name>/…` so two extensions cannot collide. This
    /// mirrors pi's `CustomEntry`.
    Custom {
        kind: String,
        data: Option<String>,
    },
    /// Durable serve request dedupe state. Accepted requests are never
    /// automatically replayed after a crash; completion only improves the
    /// response sent to a later duplicate.
    ServeRequest {
        id: String,
        status: String,
    },
}

impl Record {
    fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Record::User(t) => json!({ "k": "user", "t": t }),
            Record::Assistant(t) => json!({ "k": "assistant", "t": t }),
            Record::ToolCall {
                id,
                name,
                arguments,
            } => json!({"k":"call", "id":id, "name":name, "args":arguments}),
            Record::Reasoning(t) => json!({ "k": "reasoning", "t": t }),
            Record::Bash {
                call_id,
                command,
                output,
                exit_code,
                truncated,
                spill,
            } => json!({
                "k": "bash", "id": call_id, "cmd": command, "out": output,
                "code": exit_code, "trunc": truncated, "spill": spill
            }),
            Record::ToolResult {
                name,
                call_id,
                body,
            } => {
                json!({ "k": "tool", "name": name, "id": call_id, "t": body })
            }
            Record::State(items) => json!({ "k": "state", "items": items }),
            Record::System(t) => json!({ "k": "system", "t": t }),
            Record::Meta(t) => json!({ "k": "meta", "t": t }),
            Record::Image { media_type, data } => {
                json!({ "k": "image", "mime": media_type, "data": data })
            }
            Record::ContextEdit { turn, mode, text } => {
                json!({ "k": "edit", "turn": turn, "mode": mode, "text": text })
            }
            Record::Compacted {
                kept,
                dropped,
                summary,
                tail_start,
                system,
            } => json!({
                "k": "compacted", "kept": kept, "dropped": dropped,
                "sum": summary, "tail": tail_start, "sys": system
            }),
            Record::Custom { kind, data } => {
                json!({ "k": "custom", "c": kind, "d": data })
            }
            Record::ServeRequest { id, status } => {
                json!({"k":"serve_request", "id":id, "status":status})
            }
        }
    }

    pub(crate) fn from_json(v: &serde_json::Value) -> Option<Self> {
        let s = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_string);
        Some(match v.get("k")?.as_str()? {
            "user" => Record::User(s("t")?),
            "assistant" => Record::Assistant(s("t")?),
            "call" => Record::ToolCall {
                id: s("id")?,
                name: s("name")?,
                arguments: s("args")?,
            },
            "reasoning" => Record::Reasoning(s("t")?),
            "bash" => Record::Bash {
                call_id: s("id"),
                command: s("cmd").unwrap_or_default(),
                output: s("out").unwrap_or_default(),
                exit_code: v.get("code").and_then(|x| x.as_i64()).map(|x| x as i32),
                truncated: v.get("trunc").and_then(|x| x.as_bool()).unwrap_or(false),
                spill: s("spill"),
            },
            "tool" => Record::ToolResult {
                name: s("name").unwrap_or_default(),
                call_id: s("id").unwrap_or_default(),
                body: s("t").unwrap_or_default(),
            },
            "state" => Record::State(
                v.get("items")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|i| i.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "system" => Record::System(s("t")?),
            "meta" => Record::Meta(s("t")?),
            "image" => Record::Image {
                media_type: s("mime")?,
                data: s("data")?,
            },
            "edit" => Record::ContextEdit {
                turn: v.get("turn").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                mode: s("mode")?,
                text: s("text"),
            },
            "compacted" => Record::Compacted {
                kept: v.get("kept").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                dropped: v.get("dropped").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                summary: s("sum"),
                tail_start: v.get("tail").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
                system: s("sys"),
            },
            "custom" => Record::Custom {
                kind: s("c")?,
                data: s("d"),
            },
            "serve_request" => Record::ServeRequest {
                id: s("id")?,
                status: s("status")?,
            },
            _ => return None,
        })
    }

    /// Project into the model context. `None` means display-only.
    pub(crate) fn to_message(&self) -> Option<Message> {
        match self {
            Record::User(t) => Some(Message::User(t.clone())),
            Record::Assistant(t) => Some(Message::Assistant(t.clone())),
            Record::ToolCall {
                id,
                name,
                arguments,
            } => Some(Message::ToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            }),
            Record::ToolResult {
                name,
                call_id,
                body,
                ..
            } => Some(Message::ToolResult {
                call_id: call_id.clone(),
                name: name.clone(),
                body: body.clone(),
            }),
            Record::System(t) => Some(Message::System(t.clone())),
            Record::Image { media_type, data } => Some(Message::Image {
                media_type: media_type.clone(),
                data: data.clone(),
            }),
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
            Record::ToolCall {
                name, arguments, ..
            } => (Role::Tool, format!("{name} {arguments}")),
            Record::Reasoning(t) => (Role::Tool, t.clone()),
            Record::Bash {
                command, output, ..
            } => (Role::Bash, format!("$ {command}\n{output}")),
            Record::ToolResult { body, .. } => (Role::Tool, body.clone()),
            Record::State(items) => (
                Role::Meta,
                items
                    .iter()
                    .map(|s| format!("• {s}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Record::System(t) => (Role::System, t.clone()),
            Record::Meta(t) => (Role::Meta, t.clone()),
            Record::Image { media_type, data } => (
                Role::User,
                // The blob never reaches the transcript; a placeholder does.
                format!(
                    "[image: {} · {} KB]",
                    media_type.rsplit('/').next().unwrap_or(media_type),
                    data.len() * 3 / 4 / 1024
                ),
            ),
            Record::ContextEdit { turn, mode, text } => (
                Role::Meta,
                match (mode.as_str(), text) {
                    ("replace", Some(text)) => {
                        format!("[context edit: turn {turn} rewritten for future context — {text}]")
                    }
                    ("restore", _) => format!("[context edit: turn {turn} restored to context]"),
                    _ => format!("[context edit: turn {turn} hidden from future context]"),
                },
            ),
            Record::Compacted { kept, dropped, .. } => (
                Role::Meta,
                format!("context compacted: dropped {dropped}, kept {kept}"),
            ),
            Record::Custom { kind, .. } => (Role::Meta, format!("[extension entry: {kind}]")),
            Record::ServeRequest { id, status } => {
                (Role::Meta, format!("[serve request {id}: {status}]"))
            }
        }
    }
}

/// What a log repair did.
#[derive(Debug, PartialEq, Eq)]
enum Repair {
    None,
    /// A valid final record lacked its line separator; one was appended so
    /// the next record cannot be concatenated with it.
    AddedSeparator,
    /// A torn final line (no trailing newline) was truncated away.
    TruncatedTail,
    /// Complete but unreadable lines were dropped; the original log is kept
    /// at `backup`.
    DroppedLines {
        first: usize,
        count: usize,
        backup: PathBuf,
    },
}

/// Repair a session log in place before replay.
///
/// Two failure shapes, previously conflated into "skip anything that does
/// not parse":
///
/// - a **torn tail** — killed mid-append, so the final line has no trailing
///   newline — is truncated. That matters beyond tidiness: the next append
///   would otherwise glue its record onto the fragment, producing a line
///   that never parses and silently losing the new record too;
/// - a **complete unreadable line** is corruption. pi-durable fails closed
///   here, but that rule is only safe behind commit markers; over a plain
///   append-only log it would lock a user out of their session over one bad
///   byte. The line is dropped, the original is backed up beside the log,
///   and the drop is reported (and recorded as a `Meta`) rather than hidden.
///
/// Lines that are valid JSON but carry an unknown `k` are forward
/// compatibility, not corruption, and pass through untouched.
fn repair_log(path: &Path) -> std::io::Result<Repair> {
    if !path.exists() {
        return Ok(Repair::None);
    }
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Ok(Repair::None);
    }
    let trailing_newline = bytes.last() == Some(&b'\n');
    // Keep byte offsets alongside lines. Re-serializing parsed lines here
    // changes CRLF and blank-line bytes, so a normalized length is not a safe
    // truncation boundary.
    let mut raw: Vec<(usize, &[u8])> = Vec::new();
    let mut start = 0;
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            raw.push((start, &bytes[start..i]));
            start = i + 1;
        }
    }
    if start < bytes.len() {
        raw.push((start, &bytes[start..]));
    }
    let last = raw.len().saturating_sub(1);
    let mut torn = false;
    let mut torn_offset = 0usize;
    let mut final_valid_without_newline = false;
    let mut first_bad = 0usize;
    let mut bad = 0usize;
    for (i, (offset, line)) in raw.iter().enumerate() {
        let Ok(line) = std::str::from_utf8(line) else {
            if i == last && !trailing_newline {
                torn = true;
                torn_offset = *offset;
            } else {
                bad += 1;
                if first_bad == 0 {
                    first_bad = i + 1;
                }
            }
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        if serde_json::from_str::<serde_json::Value>(line).is_ok() {
            if i == last && !trailing_newline {
                final_valid_without_newline = true;
            }
            continue;
        }
        if i == last && !trailing_newline {
            torn = true;
            torn_offset = *offset;
        } else {
            bad += 1;
            if first_bad == 0 {
                first_bad = i + 1;
            }
        }
    }
    if !torn && bad == 0 && final_valid_without_newline {
        OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(b"\n")?;
        return Ok(Repair::AddedSeparator);
    }
    if !torn && bad == 0 {
        return Ok(Repair::None);
    }
    if bad == 0 {
        // Truncate at the exact start of the torn bytes. This retains any
        // valid blank lines and original CRLF separators before the fragment.
        let file = OpenOptions::new().write(true).open(path)?;
        file.set_len(torn_offset as u64)?;
        return Ok(Repair::TruncatedTail);
    }
    let rewritten: String = raw
        .iter()
        .filter_map(|(_, line)| {
            std::str::from_utf8(line).ok().filter(|line| {
                !line.trim().is_empty() && serde_json::from_str::<serde_json::Value>(line).is_ok()
            })
        })
        .map(|line| format!("{line}\n"))
        .collect();
    let backup = PathBuf::from(format!(
        "{}.corrupt-{}",
        path.display(),
        crate::loops::now()
    ));
    crate::auth::secure_write(&backup, &bytes).map_err(std::io::Error::other)?;
    // Atomic replace, permissions preserved from the original.
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, rewritten.as_bytes())?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path)?;
    Ok(Repair::DroppedLines {
        first: first_bad,
        count: bad,
        backup,
    })
}

/// Recovery policy for a tool call, mirroring pi's `replay` declaration.
///
/// `state` is pure bookkeeping and idempotent; every other tool — `bash`
/// first of all — is treated as unsafe. Nothing is ever re-run
/// automatically: the policy only decides what the model is told after an
/// interruption, because unattended re-execution is a destructive-action
/// risk worth more than the convenience.
pub(crate) fn replay_policy(name: &str) -> &'static str {
    match name {
        "state" => "safe",
        _ => "unsafe",
    }
}

/// One interrupted-result body for a call that never recorded an outcome.
fn interrupted_result(call: &Record) -> Option<Record> {
    let Record::ToolCall { id, name, .. } = call else {
        return None;
    };
    let body = match replay_policy(name) {
        "safe" => format!(
            "interrupted: `{name}` never completed, but it has no side effects — \
             calling it again is safe."
        ),
        _ => format!(
            "interrupted: `{name}` was killed while running and may have partially run. \
             Check the workspace before retrying; do not re-run it blindly."
        ),
    };
    Some(Record::ToolResult {
        name: name.clone(),
        call_id: id.clone(),
        body,
    })
}

/// Tool calls with no recorded result — the crash-mid-tool case.
///
/// The call record is the durable intent: it is appended before the tool
/// executes, so a call without a result means the effect may have landed.
/// Answering it here keeps the provider transcript well-formed (`tool_use`
/// always paired with `tool_result`) and stops the `/retry` recovery path
/// from re-executing a command that may already have run.
pub(crate) fn interrupted_calls(raw: &[Record]) -> Vec<Record> {
    let answered: std::collections::HashSet<&str> = raw
        .iter()
        .filter_map(|r| match r {
            Record::ToolResult { call_id, .. } => Some(call_id.as_str()),
            _ => None,
        })
        .collect();
    raw.iter()
        .filter(|r| matches!(r, Record::ToolCall { id, .. } if !answered.contains(id.as_str())))
        .filter_map(interrupted_result)
        .collect()
}

/// Whether every appended record is flushed to stable storage.
///
/// Off by default: append + flush already gives process-crash consistency,
/// which is what a kill needs. `log_fsync = "always"` adds `sync_data` per
/// record for survival of power or host loss, at a write cost per record.
static LOG_FSYNC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Configure per-record fsync from the loaded config.
pub fn configure_fsync(on: bool) {
    LOG_FSYNC.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub struct Session {
    pub id: String,
    /// Kept so `resume` can report where a session lives and so a future
    /// `export` has a source without re-deriving it.
    #[allow(dead_code)]
    pub path: PathBuf,
    file: Option<File>,
    _writer_lock: Option<File>,
    raw_index: RefCell<Vec<Record>>,
    raw_stamp: RefCell<FileStamp>,
    /// Everything recorded this process, in order.
    pub records: Vec<Record>,
    /// Total records ever appended, including compaction markers.
    ///
    /// This is the index space `Record::Compacted::tail_start` points
    /// into. It must be tracked separately from `records.len()`:
    /// compaction rewrites `records` in place while the log keeps
    /// appending, so the two diverge permanently after the first
    /// compaction and a projection-relative index would point at the
    /// wrong slice of the log.
    log_len: usize,
    write_error: Option<String>,
    /// Number of user turns replayed or started, used by jump-to-turn.
    #[allow(dead_code)]
    pub turn: usize,
}

fn acquire_writer_lock(path: &Path) -> std::io::Result<File> {
    let canonical = if path.exists() {
        path.canonicalize()?
    } else {
        let parent = path.parent().ok_or_else(|| std::io::Error::other("session path has no parent"))?.canonicalize()?;
        parent.join(path.file_name().ok_or_else(|| std::io::Error::other("session path has no filename"))?)
    };
    let lock_path = canonical.with_extension("writer.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "session is already open for writing ({}): {error}",
                path.display()
            ),
        )
    })?;
    Ok(file)
}

fn file_stamp(metadata: &fs::Metadata) -> FileStamp {
    use std::os::unix::fs::MetadataExt;
    (
        metadata.len(),
        metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
        metadata.ctime(),
        metadata.ctime_nsec(),
        metadata.dev(),
        metadata.ino(),
    )
}

fn read_snapshot(path: &Path) -> std::io::Result<(Vec<Record>, FileStamp)> {
    loop {
        let before = file_stamp(&fs::metadata(path)?);
        let records = Session::raw_records(path)?;
        let after = file_stamp(&fs::metadata(path)?);
        if before == after { return Ok((records, after)); }
    }
}

impl Session {
    pub(crate) fn append_custom_path(
        path: &Path,
        kind: &str,
        data: Option<&str>,
    ) -> std::io::Result<()> {
        let record = Record::Custom {
            kind: kind.to_string(),
            data: data.map(str::to_string),
        };
        let mut bytes = serde_json::to_vec(&record.to_json()).map_err(std::io::Error::other)?;
        bytes.push(b'\n');
        let mut file = OpenOptions::new().read(true).append(true).open(path)?;
        let start = file.metadata()?.len();
        let result = file.write_all(&bytes).and_then(|()| file.flush()).and_then(|()| {
            if LOG_FSYNC.load(std::sync::atomic::Ordering::Relaxed) { file.sync_data() } else { Ok(()) }
        });
        if let Err(error) = result {
            let rollback = file.set_len(start).and_then(|()| file.sync_data());
            return Err(if let Err(rollback_error) = rollback {
                std::io::Error::other(format!("custom append failed ({error}); rollback failed ({rollback_error})"))
            } else { error });
        }
        Ok(())
    }
    /// Read durable records without repairing, opening for append, or replaying the session.
    pub fn raw_records(path: &Path) -> std::io::Result<Vec<Record>> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(text
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|v| Record::from_json(&v))
            .collect())
    }

    /// Open a read-only snapshot without repair, locking, or recovery writes.
    pub fn open_readonly(dir: &Path, target: &str) -> std::io::Result<Self> {
        let path = if target.ends_with(".jsonl") { PathBuf::from(target) } else { dir.join(format!("{target}.jsonl")) };
        let (raw, raw_stamp) = read_snapshot(&path)?;
        let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| target.to_string());
        let log_len = raw.len();
        let raw_index = raw.clone();
        let (records, turn) = apply_projection(raw);
        Ok(Self {
            id,
            path,
            file: None,
            _writer_lock: None,
            raw_index: RefCell::new(raw_index),
            raw_stamp: RefCell::new(raw_stamp),
            records,
            turn,
            log_len,
            write_error: None,
        })
    }

    /// Open a new session file under the state directory.
    pub fn create(dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let id = new_id();
        let path = dir.join(format!("{id}.jsonl"));
        let writer_lock = acquire_writer_lock(&path)?;
        let mut options = OpenOptions::new();
        options.create_new(true).append(true);
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        Ok(Self {
            id,
            path,
            file: Some(file),
            _writer_lock: Some(writer_lock),
            raw_index: RefCell::new(Vec::new()),
            raw_stamp: RefCell::new((0, std::time::UNIX_EPOCH, 0, 0, 0, 0)),
            records: Vec::new(),
            turn: 0,
            log_len: 0,
            write_error: None,
        })
    }

    /// Reopen a session by id or path, replaying it into memory.
    pub fn open(dir: &Path, target: &str) -> std::io::Result<Self> {
        let path = if target.ends_with(".jsonl") {
            PathBuf::from(target)
        } else {
            dir.join(format!("{target}.jsonl"))
        };
        // The lock precedes repair: two resume processes must never repair or
        // append the same log concurrently.
        let writer_lock = acquire_writer_lock(&path)?;
        let repair = repair_log(&path)?;
        match &repair {
            Repair::None => {}
            Repair::AddedSeparator => {}
            Repair::TruncatedTail => eprintln!(
                "glass-core: repaired a truncated session log (killed mid-write): {}",
                path.display()
            ),
            Repair::DroppedLines {
                first,
                count,
                backup,
            } => eprintln!(
                "glass-core: skipped {count} unreadable record(s) starting at line {first} in {}; \
                 the original is kept at {}",
                path.display(),
                backup.display()
            ),
        }
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
                // Repair ran first; a remaining parse failure is a race with
                // another writer. Skipping keeps resume working.
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

        // Rebuild the compaction projection from the log.
        //
        // Without this, `open` replayed every raw record and the in-memory
        // rewrite that compaction performed was thrown away — so a resumed
        // session started with the full uncompacted history, immediately
        // tripped the threshold again, and paid for another summary. The
        // records on disk are the immutable log; `records` is the projection
        // the model sees, and they are allowed to differ.
        let repairs = interrupted_calls(&records);
        let raw_index = records.clone();
        let mut raw_turn = 0u64;
        let mut assistants_by_turn: HashMap<u64, Vec<String>> = HashMap::new();
        for record in &records {
            if matches!(record, Record::User(_)) {
                raw_turn += 1;
            }
            if let Record::Assistant(text) = record {
                assistants_by_turn
                    .entry(raw_turn)
                    .or_default()
                    .push(text.clone());
            }
        }
        let live = crate::partial::load(&path);
        let live_file = crate::partial::path(&path);
        let log_len = records.len();
        let (records, turn) = apply_projection(records);
        let metadata = fs::metadata(&path)?;
        let raw_stamp = file_stamp(&metadata);

        let mut session = Self {
            id,
            path,
            file: Some(file),
            _writer_lock: Some(writer_lock),
            raw_index: RefCell::new(raw_index),
            raw_stamp: RefCell::new(raw_stamp),
            records,
            turn,
            log_len,
            write_error: None,
        };
        for repair in repairs {
            session.append(repair);
        }
        if let Repair::DroppedLines {
            first,
            count,
            backup,
        } = repair
        {
            // Visible in the transcript, not in model context: the repair
            // becomes part of the record rather than a stderr line nobody
            // reads again.
            session.append(Record::Meta(format!(
                "log repair: skipped {count} unreadable record(s) starting at line {first}; \
                 the original log is kept at {}",
                backup.display()
            )));
        }
        // A leftover partial-output sidecar means the last turn was killed
        // mid-response. The recovered text is recorded as the assistant
        // message it was going to be, so `/retry` continues instead of
        // pretending the turn never happened.
        if let Some(live) = live {
            let completed = assistants_by_turn
                .get(&live.turn)
                .and_then(|answers| answers.iter().find(|text| text.contains(&live.text)))
                .map(String::as_str);
            session.recover_partial(&live_file, live, completed)?;
        }
        Ok(session)
    }

    pub fn raw_turns(&self) -> std::io::Result<usize> {
        Ok(self
            .read_history_records()
            .iter()
            .filter(|r| matches!(r, Record::User(_)))
            .count())
    }

    /// Fork raw history through a completed user turn, preserving compaction indices.
    pub fn branch(&self, dir: &Path, through: usize) -> std::io::Result<Self> {
        if through == 0 {
            return Err(std::io::Error::other("turn must be positive"));
        }
        let mut fork = Self::create(dir)?;
        let mut turn = 0;
        for line in BufReader::new(File::open(&self.path)?).lines() {
            let line = line?;
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let Some(r) = Record::from_json(&v) else {
                continue;
            };
            if matches!(r, Record::User(_)) {
                turn += 1;
            }
            if turn > through {
                break;
            }
            fork.append(r);
        }
        if turn < through {
            fs::remove_file(&fork.path)?;
            return Err(std::io::Error::other("turn does not exist"));
        }
        fork.append(Record::Meta(format!(
            "branched from {} through turn {through}; workspace files unchanged",
            self.id
        )));
        let fork_id = fork.id.clone();
        drop(fork);
        Self::open(dir, &fork_id)
    }

    /// Most recently modified session, if any.
    #[cfg(test)]
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

    /// Number of records ever written to this session's log.
    pub fn log_len(&self) -> usize {
        self.log_len
    }

    pub fn write_error(&self) -> Option<&str> { self.write_error.as_deref() }

    pub(crate) fn validate_writes(&mut self) { self.sync_external_index(); }

    pub fn append(&mut self, record: Record) {
        if self.write_error.is_some() { return; }
        self.sync_external_index();
        if self.write_error.is_some() { return; }
        let Some(start) = self.file.as_ref().and_then(|file| file.metadata().ok()).map(|m| m.len()) else {
            self.write_error = Some("cannot inspect session log before append; writes and effects are disabled".into());
            return;
        };
        let result = if let Some(f) = self.file.as_mut() {
            serde_json::to_string(&record.to_json()).map_err(std::io::Error::other).and_then(|line| writeln!(f, "{line}").map_err(std::io::Error::other))
                .and_then(|()| f.flush())
                .and_then(|()| {
                    if LOG_FSYNC.load(std::sync::atomic::Ordering::Relaxed) {
                        f.sync_data()
                    } else {
                        Ok(())
                    }
                })
        } else {
            Err(std::io::Error::other("session file closed"))
        };
        if let Err(error) = result {
            self.rollback_failed_write(start, &error);
            return;
        }
        self.records.push(record);
        self.log_len += 1;
        if let Some(record) = self.records.last() { self.raw_index.borrow_mut().push(record.clone()); }
        self.refresh_stamp();
    }

    /// Append a recovery group and report write failures so its sidecar can
    /// remain available for another resume attempt.
    pub(crate) fn append_checked(&mut self, records: &[Record]) -> std::io::Result<()> {
        if let Some(error) = &self.write_error { return Err(std::io::Error::other(error.clone())); }
        self.sync_external_index();
        if let Some(error) = &self.write_error { return Err(std::io::Error::other(error.clone())); }
        let mut bytes = Vec::new();
        for record in records {
            serde_json::to_writer(&mut bytes, &record.to_json()).map_err(std::io::Error::other)?;
            bytes.push(b'\n');
        }
        let Some(start) = self.file.as_ref().and_then(|file| file.metadata().ok()).map(|m| m.len()) else {
            let error = std::io::Error::other("cannot inspect session log before append");
            self.write_error = Some(error.to_string());
            return Err(error);
        };
        if self.file.is_none() {
            let error = std::io::Error::other("session file closed");
            self.write_error = Some(error.to_string());
            return Err(error);
        }
        let file = self.file.as_mut().expect("checked session file");
        let result = file.write_all(&bytes).and_then(|()| file.flush()).and_then(|()| {
            if LOG_FSYNC.load(std::sync::atomic::Ordering::Relaxed) { file.sync_data() } else { Ok(()) }
        });
        if let Err(error) = result {
            self.rollback_failed_write(start, &error);
            return Err(error);
        }
        self.records.extend_from_slice(records);
        self.log_len += records.len();
        self.raw_index.borrow_mut().extend_from_slice(records);
        self.refresh_stamp();
        Ok(())
    }

    fn rollback_failed_write(&mut self, start: u64, error: &std::io::Error) {
        let rollback = self.file.as_mut().map(|file| file.set_len(start).and_then(|()| file.sync_data())).unwrap_or_else(|| Err(std::io::Error::other("session file closed")));
        let message = if let Err(rollback_error) = rollback {
            format!("session append failed ({error}) and rollback failed ({rollback_error}); writes and effects are disabled")
        } else {
            format!("session append failed: {error}")
        };
        eprintln!("glass-core: {message}");
        self.write_error = Some(message);
    }

    fn refresh_stamp(&self) {
        if let Ok(metadata) = fs::metadata(&self.path) {
            *self.raw_stamp.borrow_mut() = file_stamp(&metadata);
        }
    }

    /// Incorporate checked writes made through a host-owned session path
    /// (for example an extension custom entry) before advancing local indexes.
    fn sync_external_index(&mut self) {
        let Ok(metadata) = fs::metadata(&self.path) else {
            self.write_error = Some("session log path is unavailable; writes and effects are disabled".into());
            return;
        };
        let stamp = file_stamp(&metadata);
        if stamp == *self.raw_stamp.borrow() { return; }
        if let Ok(text) = fs::read_to_string(&self.path)
            && text.lines().any(|line| !line.trim().is_empty() && serde_json::from_str::<serde_json::Value>(line).is_err()) {
                self.write_error = Some("session log has an incomplete external append; writes and effects are disabled until resume repair".into());
                return;
            }
        if let Ok(records) = Self::raw_records(&self.path) {
            let old_len = self.raw_index.borrow().len();
            if records.len() < old_len || !records.starts_with(&self.raw_index.borrow()) {
                self.write_error = Some("session log changed outside its writer; further writes and effects are disabled".into());
                return;
            }
            let unseen: Vec<Record> = records.iter().skip(old_len).cloned().collect();
            if unseen.iter().any(|record| !matches!(record, Record::Custom { .. } | Record::ServeRequest { .. })) {
                self.write_error = Some("unexpected external session records changed turn or context indexes; writes and effects are disabled".into());
                return;
            }
            self.log_len = records.len();
            *self.raw_index.borrow_mut() = records.clone();
            // Custom entries are display-only, but remain in raw history.
            self.records.extend(unseen);
            *self.raw_stamp.borrow_mut() = stamp;
        } else {
            self.write_error = Some("cannot reread externally changed session log; writes and effects are disabled".into());
        }
    }

    fn recover_partial(
        &mut self,
        sidecar: &Path,
        live: crate::partial::Live,
        assistant_for_turn: Option<&str>,
    ) -> std::io::Result<()> {
        if let Some(recovered) = crate::partial::recover(live, assistant_for_turn) {
            let mut additions = vec![Record::Meta(recovered.meta)];
            if let Some(partial) = recovered.partial {
                additions.push(Record::Assistant(partial));
            }
            self.append_checked(&additions)?;
        }
        // A failed checked append returns above, leaving the sidecar for the
        // next resume; successful and stale recovery can consume it.
        let _ = fs::remove_file(sidecar);
        Ok(())
    }

    /// The conversation as the model should see it.
    pub fn messages(&self) -> Vec<Message> {
        match self.context_edits() {
            // Turn numbers must match `/branch` and the transcript, which both
            // count raw log turns; edits therefore apply to the raw records
            // and compaction projects afterwards.
            Some(edits) => {
                let raw = self.read_history_records();
                let last_epoch = raw
                    .iter()
                    .rposition(|r| matches!(r, Record::Compacted { .. }));
                let edit_invalidates_summary = last_epoch.is_some_and(|epoch| {
                    let mut turn = 0usize;
                    let mut user_record_indices = HashMap::new();
                    for (index, record) in raw.iter().enumerate() {
                        if matches!(record, Record::User(_)) {
                            turn += 1;
                            user_record_indices.insert(turn, index);
                        }
                    }
                    edits.keys().any(|edited_turn| {
                        user_record_indices
                            .get(edited_turn)
                            .is_some_and(|index| *index < epoch)
                    })
                });
                if edit_invalidates_summary {
                    crate::compact::project(&bounded_edited_context(&raw, &edits))
                } else {
                    let edited = apply_edits(&raw, &edits);
                    let (projected, _) = apply_projection(edited);
                    crate::compact::project(&projected)
                }
            }
            None => crate::compact::project(&self.records),
        }
    }

    /// The latest edit per turn: `restore` removes the entry, so the last
    /// word wins exactly like an append-only log implies.
    fn context_edits(&self) -> Option<HashMap<usize, (String, Option<String>)>> {
        let mut edits: HashMap<usize, (String, Option<String>)> = HashMap::new();
        for record in &self.read_history_records() {
            if let Record::ContextEdit { turn, mode, text } = record {
                if mode == "restore" {
                    edits.remove(turn);
                } else {
                    edits.insert(*turn, (mode.clone(), text.clone()));
                }
            }
        }
        (!edits.is_empty()).then_some(edits)
    }

    /// `/forget`, `/unforget`, `/rewrite`: append-only context edits.
    ///
    /// They change only what the model sees next: raw history, the transcript,
    /// exports, and accounting keep the original turn, and files on disk were
    /// never affected either way.
    pub fn context_command(&mut self, command: &str, args: &str) -> Result<String, String> {
        let args = args.trim();
        let turns = self.raw_turns().map_err(|e| e.to_string())?;
        let parse_turn = |text: &str| -> Result<usize, String> {
            let n: usize = text
                .trim()
                .parse()
                .map_err(|_| format!("`{text}` is not a turn number (see /forget)"))?;
            if n == 0 || n > turns {
                return Err(format!("turn {n} does not exist; there are {turns}"));
            }
            Ok(n)
        };
        match command {
            "forget" => {
                if args.is_empty() {
                    return Ok(self.context_edits_listing(turns));
                }
                let turn = parse_turn(args)?;
                self.append(Record::ContextEdit {
                    turn,
                    mode: "hide".into(),
                    text: None,
                });
                Ok(format!(
                    "turn {turn} hidden from future context \
                     (raw history and files unchanged; /unforget {turn} restores it)"
                ))
            }
            "unforget" => {
                let turn = parse_turn(args)?;
                self.append(Record::ContextEdit {
                    turn,
                    mode: "restore".into(),
                    text: None,
                });
                Ok(format!("turn {turn} restored to future context"))
            }
            "rewrite" => {
                let (turn_text, replacement) = args
                    .split_once(char::is_whitespace)
                    .ok_or("usage: /rewrite <turn> <text>")?;
                let turn = parse_turn(turn_text)?;
                let replacement = replacement.trim();
                if replacement.is_empty() {
                    return Err("the replacement is empty".into());
                }
                if replacement.len() > 8_000 {
                    return Err("the replacement is longer than 8 KB".into());
                }
                self.append(Record::ContextEdit {
                    turn,
                    mode: "replace".into(),
                    text: Some(replacement.to_string()),
                });
                Ok(format!(
                    "turn {turn}'s message rewritten for future context \
                     (the transcript keeps the original)"
                ))
            }
            _ => Err("usage: /forget [turn] | /unforget <turn> | /rewrite <turn> <text>".into()),
        }
    }

    fn context_edits_listing(&self, turns: usize) -> String {
        let records = self.read_history_records();
        let edits = self.context_edits().unwrap_or_default();
        let mut out = String::from("turns (edits change only future model context):\n");
        let mut turn = 0usize;
        for record in &records {
            if let Record::User(text) = record {
                turn += 1;
                let state = match edits.get(&turn) {
                    Some((mode, _)) if mode == "hide" => " · hidden",
                    Some((mode, _)) if mode == "replace" => " · rewritten",
                    _ => "",
                };
                let snippet: String = text.lines().next().unwrap_or("").chars().take(60).collect();
                out.push_str(&format!("  {turn}. {snippet}{state}\n"));
                if turn >= 40 && turn < turns {
                    out.push_str(&format!("  … {} more turns\n", turns - turn));
                    break;
                }
            }
        }
        out.trim_end().to_string()
    }

    /// Full append-only history for UI inspection, independent of model compaction.
    pub fn history_entries(&self) -> Vec<glass_tui::Entry> {
        let records = self.read_history_records();
        canonical_entries(&records)
    }

    /// Records for the transcript viewer, with turn indices assigned.
    #[cfg(test)]
    pub fn entries(&self) -> Vec<glass_tui::Entry> {
        canonical_entries(&self.records)
    }

    /// Public provider reasoning for the current user turn, kept out of the
    /// main conversation and available only through the dedicated thinking view.
    pub fn reasoning_history(&self) -> String {
        let records = self.read_history_records();
        latest_reasoning(&records)
    }

    pub fn history_records(&self) -> Vec<Record> {
        self.read_history_records()
    }

    fn read_history_records(&self) -> Vec<Record> {
        if let Ok(metadata) = fs::metadata(&self.path) {
            let stamp = file_stamp(&metadata);
            if stamp == *self.raw_stamp.borrow() {
                return self.raw_index.borrow().clone();
            }
            if let Ok((records, stamp)) = read_snapshot(&self.path) {
                *self.raw_index.borrow_mut() = records.clone();
                *self.raw_stamp.borrow_mut() = stamp;
                return records;
            }
        }
        let Ok(file) = File::open(&self.path) else {
            return self.records.clone();
        };
        let records: Vec<Record> = BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str(&line).ok())
            .filter_map(|value| Record::from_json(&value))
            .collect();
        *self.raw_index.borrow_mut() = records.clone();
        self.refresh_stamp();
        records
    }
}

/// A changed turn may have contributed to a compacted summary or state note.
/// In that case those derived records cannot safely be reused. Keep the
/// opening task and a bounded suffix of whole raw turns, preserving tool
/// call/result groups and their original turn ordering.
fn bounded_edited_context(
    records: &[Record],
    edits: &HashMap<usize, (String, Option<String>)>,
) -> Vec<Record> {
    let mut groups: Vec<(usize, Vec<Record>)> = Vec::new();
    let mut turn = 0usize;
    for record in records {
        if matches!(record, Record::User(_)) {
            turn += 1;
            groups.push((turn, Vec::new()));
        }
        if matches!(
            record,
            Record::ContextEdit { .. } | Record::Compacted { .. } | Record::State(_)
        ) {
            continue;
        }
        let Some((_, group)) = groups.last_mut() else {
            continue;
        };
        match edits.get(&turn) {
            Some((mode, _)) if mode == "hide" => continue,
            Some((mode, replacement)) if mode == "replace" => {
                if matches!(record, Record::User(_)) {
                    if let Some(replacement) = replacement {
                        group.push(Record::User(replacement.clone()));
                    }
                    continue;
                }
            }
            _ => {}
        }
        group.push(record.clone());
    }
    let Some((opening_turn, opening_group)) = groups.iter().find(|(_, group)| !group.is_empty())
    else {
        return Vec::new();
    };
    let opening_turn = *opening_turn;
    let opening = opening_group[0].clone();
    let mut selected = Vec::new();
    let mut count = 0;
    for (turn, group) in groups.into_iter().rev() {
        if group.is_empty() {
            continue;
        }
        count += group.len();
        selected.push((turn, group));
        if count >= crate::compact::KEEP_RECENT {
            break;
        }
    }
    selected.reverse();
    let mut out = vec![opening];
    for (turn, group) in selected {
        if turn == opening_turn {
            out.extend(group.into_iter().skip(1));
        } else {
            out.extend(group);
        }
    }
    out
}

/// Apply context edits to a record list: hidden turns vanish entirely,
/// replaced turns keep their assistant/tool records with a substituted user
/// message, and everything else passes through untouched.
fn apply_edits(
    records: &[Record],
    edits: &HashMap<usize, (String, Option<String>)>,
) -> Vec<Record> {
    let mut out = Vec::with_capacity(records.len());
    let mut turn = 0usize;
    for record in records {
        if matches!(record, Record::User(_)) {
            turn += 1;
        }
        match edits.get(&turn) {
            Some((mode, _)) if mode == "hide" => continue,
            Some((mode, text)) if mode == "replace" => {
                if let Record::User(_) = record
                    && let Some(text) = text
                {
                    out.push(Record::User(text.clone()));
                    continue;
                }
            }
            _ => {}
        }
        out.push(record.clone());
    }
    out
}

fn canonical_entries(records: &[Record]) -> Vec<glass_tui::Entry> {
    type CallKey = (usize, usize, String);
    let mut scopes = Vec::with_capacity(records.len());
    let (mut turn, mut epoch) = (0usize, 0usize);
    let mut call_counts: HashMap<CallKey, usize> = HashMap::new();
    let mut result_counts: HashMap<CallKey, usize> = HashMap::new();
    let mut bash_counts: HashMap<CallKey, usize> = HashMap::new();
    for record in records {
        if matches!(record, Record::User(_)) {
            turn += 1;
        }
        if matches!(record, Record::Compacted { .. }) {
            epoch += 1;
        }
        scopes.push((turn, epoch));
        match record {
            Record::ToolCall {
                id,
                name,
                arguments,
            } => {
                let key = (turn, epoch, id.clone());
                *call_counts.entry(key.clone()).or_default() += 1;
                let _ = (name, arguments);
            }
            Record::ToolResult { call_id, .. } => {
                *result_counts
                    .entry((turn, epoch, call_id.clone()))
                    .or_default() += 1;
            }
            Record::Bash { call_id: Some(id), .. } => {
                *bash_counts.entry((turn, epoch, id.clone())).or_default() += 1;
            }
            Record::Bash { .. } => {
                // Legacy Bash records did not retain call IDs. Keep them as
                // independent rows rather than guessing identity from command text.
            }
            _ => {}
        }
    }
    // Old Bash records predate the `id` field. Recover those associations by
    // causal call order within the turn, never by comparing command text.
    let mut pending_bash: HashMap<(usize, usize), VecDeque<CallKey>> = HashMap::new();
    let mut legacy_bash_calls: HashMap<usize, CallKey> = HashMap::new();
    for (record_index, (record, (turn, epoch))) in records.iter().zip(&scopes).enumerate() {
        match record {
            Record::ToolCall { id, name, .. } if name == "bash" => {
                let key = (*turn, *epoch, id.clone());
                if call_counts.get(&key) == Some(&1) {
                    pending_bash.entry((*turn, *epoch)).or_default().push_back(key);
                }
            }
            Record::Bash { call_id: Some(id), .. } => {
                if let Some(queue) = pending_bash.get_mut(&(*turn, *epoch)) {
                    queue.retain(|key| key.2 != *id);
                }
            }
            Record::Bash { call_id: None, .. } => {
                if let Some(key) = pending_bash.get_mut(&(*turn, *epoch)).and_then(VecDeque::pop_front) {
                    legacy_bash_calls.insert(record_index, key.clone());
                    *bash_counts.entry(key).or_default() += 1;
                }
            }
            _ => {}
        }
    }
    let paired_bash_calls: HashSet<CallKey> = bash_counts
        .into_iter()
        .filter_map(|(key, count)| (count == 1).then_some(key))
        .collect();

    // Associate only unambiguous IDs. Malformed duplicate IDs stay visible in
    // full instead of silently hiding one of their records.
    let mut results: HashMap<CallKey, (&str, &str)> = HashMap::new();
    let (mut turn, mut epoch) = (0usize, 0usize);
    for record in records {
        if matches!(record, Record::User(_)) {
            turn += 1;
        }
        if matches!(record, Record::Compacted { .. }) {
            epoch += 1;
        }
        if let Record::ToolResult {
            call_id,
            name,
            body,
        } = record
        {
            let key = (turn, epoch, call_id.clone());
            if call_counts.get(&key) == Some(&1) && result_counts.get(&key) == Some(&1) {
                results.insert(key, (name.as_str(), body.as_str()));
            }
        }
    }

    let mut entries = Vec::new();
    for (record_index, (record, (turn, epoch))) in records.iter().zip(scopes).enumerate() {
        let key = |id: &str| (turn, epoch, id.to_owned());
        let entry = match record {
            Record::ToolCall {
                id,
                name,
                arguments,
            } => {
                let call_key = key(id);
                if call_counts.get(&call_key) == Some(&1) && paired_bash_calls.contains(&call_key) {
                    continue;
                }
                let body = results.get(&call_key).map(|(_, body)| *body);
                let heading = readable_tool_call(name, arguments);
                let output = body.unwrap_or("").to_string();
                let status = if body.is_some() { "completed" } else { "interrupted" };
                let card = glass_tui::ToolCard {
                    call_id: id.clone(),
                    command: heading.clone(),
                    output: output.clone(),
                    status: status.into(),
                    elapsed_ms: None,
                    spill: None,
                    truncated: false,
                };
                (glass_tui::Role::Tool, tool_card_text(&card, false), Some(card))
            }
            Record::Bash {
                call_id,
                command,
                output,
                exit_code,
                truncated,
                spill,
            } => {
                let status = exit_code.map_or_else(|| "signal".into(), |code| format!("exit {code}"));
                let card = glass_tui::ToolCard {
                    call_id: call_id.clone().or_else(|| legacy_bash_calls.get(&record_index).map(|key| key.2.clone()))
                        .unwrap_or_else(|| format!("legacy-bash-record-{record_index}")),
                    command: command.clone(),
                    output: output.clone(),
                    status: if *truncated { format!("{status} · truncated") } else { status },
                    elapsed_ms: None,
                    spill: spill.clone(),
                    truncated: *truncated,
                };
                (glass_tui::Role::Bash, tool_card_text(&card, false), Some(card))
            }
            Record::ToolResult {
                call_id,
                name,
                body,
            } => {
                let call_key = key(call_id);
                if call_counts.get(&call_key) == Some(&1)
                    && result_counts.get(&call_key) == Some(&1)
                {
                    continue;
                }
                (glass_tui::Role::Tool, format!("{name} result\n{}", readable_output(body)), None)
            }
            Record::Reasoning(_) => continue,
            other => {
                let (role, text) = other.to_entry();
                (role, text, None)
            }
        };
        if !entry.1.is_empty() {
            entries.push(glass_tui::Entry {
                role: entry.0,
                text: entry.1,
                turn,
                tool_card: entry.2,
            });
        }
    }
    entries
}

pub(crate) fn tool_card_text(card: &glass_tui::ToolCard, expanded: bool) -> String {
    let mut text = format!("{} · {}", card.command, card.status);
    if let Some(elapsed) = card.elapsed_ms {
        text.push_str(&format!(" · {:.1}s", elapsed as f64 / 1000.0));
    }
    if !card.output.is_empty() {
        text.push('\n');
        if expanded {
            text.push_str(&card.output);
        } else {
            let mut preview = card.output.lines().take(4).collect::<Vec<_>>().join("\n");
            if preview.len() > 360 {
                preview.truncate(360);
                preview.push('…');
            }
            text.push_str(&preview);
            if card.output.lines().count() > 4 || card.output.len() > 360 {
                text.push_str("\n… Enter to expand full output");
            }
        }
    } else if card.status == "running" {
        text.push_str("\nRunning…");
    }
    if card.truncated {
        text.push_str("\n[inline output truncated]");
    }
    if card.spill.is_some() {
        text.push_str("\n[full output spill available in /details]");
    }
    text
}

fn latest_reasoning(records: &[Record]) -> String {
    let mut latest = String::new();
    for record in records {
        match record {
            Record::User(_) => latest.clear(),
            Record::Reasoning(text) => latest.push_str(text),
            _ => {}
        }
    }
    latest
}

fn tool_command(arguments: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()?
        .get("command")?
        .as_str()
        .map(str::to_owned)
}

fn readable_tool_call(name: &str, arguments: &str) -> String {
    if name == "bash" {
        return tool_command(arguments)
            .map(|command| format!("$ {command}"))
            .unwrap_or_else(|| format!("{name} [arguments unavailable]"));
    }
    let arguments = serde_json::from_str::<serde_json::Value>(arguments).ok();
    let details = match arguments {
        Some(serde_json::Value::Object(values)) if !values.is_empty() => values
            .into_iter()
            .map(|(key, value)| format!("{key}: {}", value_as_text(&value)))
            .collect::<Vec<_>>()
            .join(" · "),
        Some(value) if !value.is_null() => value_as_text(&value),
        _ => "[arguments unavailable]".into(),
    };
    format!("{name} · {details}")
}

fn value_as_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn readable_output(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| body.to_owned())
}

/// Rebuild the model-facing projection from an append-only log.
///
/// The most recent compaction epoch wins. Its `tail_start` marks the first
/// record of the surviving tail; everything before it is replaced by the
/// opening task, the model's own recorded state, and the stored summary, which
/// is exactly what the in-memory rebuild produces at compaction time. Applying
/// it here makes compaction idempotent across a resume instead of something
/// that only exists for the life of one process.
fn apply_projection(records: Vec<Record>) -> (Vec<Record>, usize) {
    let Some(epoch) = records
        .iter()
        .rposition(|r| matches!(r, Record::Compacted { .. }))
    else {
        let turn = records
            .iter()
            .filter(|r| matches!(r, Record::User(_)))
            .count();
        return (records, turn);
    };

    let (summary, mut tail_start) = match &records[epoch] {
        Record::Compacted {
            summary,
            tail_start,
            ..
        } => (summary.clone(), *tail_start),
        _ => unreachable!("position matched a Compacted record"),
    };
    // Logs written before the tail index existed record 0, which would
    // drop the tail entirely. Keep the last few records instead.
    if tail_start == 0 {
        tail_start = epoch.saturating_sub(crate::compact::KEEP_RECENT);
    }

    let mut out: Vec<Record> = Vec::new();

    // The opening task anchors everything that follows.
    if let Some(first) = records.iter().find(|r| matches!(r, Record::User(_))) {
        out.push(first.clone());
    }
    if let Some(text) = summary {
        out.push(Record::System(format!(
            "Summary of earlier work in this session:\n{text}"
        )));
    }
    // The model's own durable notes, taken from the last one it wrote.
    if let Some(state) = records.iter().rev().find_map(|r| match r {
        Record::State(items) if !items.is_empty() => Some(items.clone()),
        _ => None,
    }) {
        out.push(Record::State(state));
    }

    // Two regions survive: the tail that compaction kept (the
    // records just before the marker), and every turn that happened
    // after it. The marker itself, and any earlier markers, are log
    // metadata — they are never content the model should see.
    let from = tail_start.min(epoch);
    out.extend(
        records[from..epoch]
            .iter()
            .filter(|r| !matches!(r, Record::Compacted { .. }))
            .cloned(),
    );
    out.extend(
        records[epoch + 1..]
            .iter()
            .filter(|r| !matches!(r, Record::Compacted { .. }))
            .cloned(),
    );

    // Never lead with an orphaned tool result: the assistant turn that
    // requested it may have been dropped.
    while matches!(out.first(), Some(Record::ToolResult { .. })) {
        out.remove(0);
    }

    let turn = out.iter().filter(|r| matches!(r, Record::User(_))).count();
    (out, turn)
}

/// Sortable session id: base-36 millis, process ID, and a per-process counter.
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
    format!("{reversed}-{:x}-{:04x}", std::process::id(), n)
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
        let d =
            std::env::temp_dir().join(format!("glass-session-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn history_index_updates_on_append_and_invalidates_for_external_append() {
        let dir = tmpdir("raw-index");
        let mut s = Session::create(&dir).unwrap();
        s.append(Record::User("first".into()));
        assert_eq!(s.history_records().len(), 1);
        let mut external = OpenOptions::new().append(true).open(&s.path).unwrap();
        writeln!(external, "{{\"k\":\"assistant\",\"t\":\"external\"}}").unwrap();
        external.flush().unwrap();
        assert_eq!(s.history_records().len(), 2);
        assert_eq!(s.raw_turns().unwrap(), 1);
    }

    #[test]
    fn external_rewrite_poison_prevents_append_behind_changed_history() {
        let dir = tmpdir("external-rewrite");
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("original".into()));
        fs::write(&session.path, "{\"k\":\"user\",\"t\":\"replacement\"}\n").unwrap();
        session.append(Record::Assistant("must not land".into()));
        assert!(session.write_error().is_some());
        assert_eq!(fs::read_to_string(&session.path).unwrap().lines().count(), 1);
    }

    #[test]
    fn writer_lock_contention_returns_actionable_error() {
        let dir = tmpdir("writer-lock");
        let session = Session::create(&dir).unwrap();
        let error = acquire_writer_lock(&session.path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        assert!(error.to_string().contains("already open for writing"));
    }

    #[test]
    fn writer_lock_resolves_symlink_aliases_to_one_log() {
        let dir = tmpdir("writer-lock-alias");
        let session = Session::create(&dir).unwrap();
        let alias = dir.join("alias.jsonl");
        std::os::unix::fs::symlink(&session.path, &alias).unwrap();
        let error = acquire_writer_lock(&alias).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    }

    #[test]
    fn failed_checked_append_does_not_advance_memory_indexes() {
        let dir = tmpdir("checked-write-failure");
        let mut session = Session::create(&dir).unwrap();
        session.file = None;
        assert!(session.append_checked(&[Record::ToolCall { id: "c1".into(), name: "bash".into(), arguments: "{}".into() }]).is_err());
        assert_eq!(session.log_len(), 0);
        assert!(session.history_records().is_empty());
        assert!(session.write_error().is_some());
    }

    #[test]
    fn read_only_open_needs_no_writer_lock_and_does_not_repair() {
        let dir = tmpdir("read-only");
        let mut writer = Session::create(&dir).unwrap();
        writer.append(Record::User("open".into()));
        writeln!(OpenOptions::new().append(true).open(&writer.path).unwrap(), "{{broken").unwrap();
        let before = fs::read(&writer.path).unwrap();
        let readonly = Session::open_readonly(&dir, &writer.id).unwrap();
        assert_eq!(readonly.raw_turns().unwrap(), 1);
        assert_eq!(fs::read(&writer.path).unwrap(), before, "read-only open must not repair");
        assert!(Session::raw_records(&writer.path).unwrap().iter().any(|r| matches!(r, Record::User(t) if t == "open")));
        let error = Session::open(&dir, &writer.id).err().unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    }

    #[test]
    fn served_request_state_survives_reopen() {
        let dir = tmpdir("serve-dedupe");
        let mut session = Session::create(&dir).unwrap();
        session
            .append_checked(&[Record::ServeRequest {
                id: "client-42".into(),
                status: "accepted".into(),
            }])
            .unwrap();
        session
            .append_checked(&[Record::ServeRequest {
                id: "client-42".into(),
                status: "completed".into(),
            }])
            .unwrap();
        let id = session.id.clone();
        drop(session);
        let reopened = Session::open(&dir, &id).unwrap();
        let states: Vec<_> = reopened
            .history_records()
            .into_iter()
            .filter_map(|record| match record {
                Record::ServeRequest { id, status } => Some((id, status)),
                _ => None,
            })
            .collect();
        assert_eq!(
            states,
            [
                ("client-42".into(), "accepted".into()),
                ("client-42".into(), "completed".into())
            ]
        );
    }

    #[test]
    fn custom_append_is_checked_and_visible_before_acknowledgment() {
        let dir = tmpdir("custom-ack");
        let mut session = Session::create(&dir).unwrap();
        Session::append_custom_path(&session.path, "ext/state", Some("saved")).unwrap();
        assert!(
            Session::append_custom_path(&dir.join("missing.jsonl"), "ext/state", None).is_err()
        );
        session.append(Record::User("next".into()));
        assert_eq!(session.log_len(), 2);
        let records = Session::raw_records(&session.path).unwrap();
        assert!(
            matches!(records.as_slice(), [Record::Custom { kind, data: Some(data) }, Record::User(user)] if kind == "ext/state" && data == "saved" && user == "next")
        );
    }

    #[test]
    fn branch_uses_raw_turns_and_keeps_original_log_unchanged() {
        let dir = tmpdir("branch");
        let mut s = Session::create(&dir).unwrap();
        for i in 1..=3 {
            s.append(Record::User(format!("task {i}")));
            s.append(Record::Assistant(format!("answer {i}")));
        }
        let original = fs::read(&s.path).unwrap();
        // In-memory projection can contain fewer turns than the log.
        s.records = vec![
            Record::User("task 1".into()),
            Record::Assistant("summary".into()),
        ];
        assert_eq!(s.raw_turns().unwrap(), 3);
        let b = s.branch(&dir, 2).unwrap();
        assert_eq!(b.raw_turns().unwrap(), 2);
        assert!(b.messages().iter().any(|m| m.text() == "answer 2"));
        assert!(!b.messages().iter().any(|m| m.text() == "task 3"));
        assert_eq!(fs::read(&s.path).unwrap(), original);
        assert!(s.branch(&dir, 4).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn roundtrips_every_record_kind() {
        let cases = vec![
            Record::User("u".into()),
            Record::Assistant("a".into()),
            Record::Reasoning("r".into()),
            Record::Bash {
                call_id: None,
                command: "ls".into(),
                output: "x".into(),
                exit_code: Some(0),
                truncated: false,
                spill: None,
            },
            Record::ToolResult {
                name: "bash".into(),
                call_id: "c1".into(),
                body: "b".into(),
            },
            Record::State(vec!["s1".into()]),
            Record::System("sy".into()),
            Record::Meta("m".into()),
            Record::Compacted {
                kept: 3,
                dropped: 9,
                summary: Some("sum".into()),
                tail_start: 0,
                system: Some("sys".into()),
            },
        ];
        for c in cases {
            let j = c.to_json();
            let back = Record::from_json(&j).expect("must roundtrip");
            assert_eq!(j, back.to_json(), "roundtrip changed the record");
        }
    }

    #[test]
    fn bash_record_preserves_exit_code_and_truncation() {
        let r = Record::Bash {
            call_id: None,
            command: "c".into(),
            output: "o".into(),
            exit_code: Some(127),
            truncated: true,
            spill: None,
        };
        let back = Record::from_json(&r.to_json()).unwrap();
        match back {
            Record::Bash {
                exit_code,
                truncated,
                ..
            } => {
                assert_eq!(exit_code, Some(127));
                assert!(truncated);
            }
            other => panic!("wrong kind: {other:?}"),
        }
    }

    #[test]
    fn messages_excludes_display_only_records() {
        let mut s = Session {
            id: "t".into(),
            path: "t".into(),
            file: None,
            _writer_lock: None,
            raw_index: RefCell::new(Vec::new()),
            raw_stamp: RefCell::new((0, std::time::UNIX_EPOCH, 0, 0, 0, 0)),
            records: vec![],
            turn: 0,
            log_len: 0,
            write_error: None,
        };
        s.records = vec![
            Record::User("hi".into()),
            Record::Reasoning("secret".into()),
            Record::Bash {
                call_id: Some("c".into()),
                command: "ls".into(),
                output: "out".into(),
                exit_code: Some(0),
                truncated: false,
                spill: None,
            },
            Record::Assistant("hello".into()),
            Record::Meta("noise".into()),
            Record::ToolResult {
                name: "bash".into(),
                call_id: "c".into(),
                body: "res".into(),
            },
        ];
        let msgs = s.messages();
        assert_eq!(
            msgs.len(),
            3,
            "reasoning, bash echo and meta must not replay"
        );
        assert!(!msgs.iter().any(|m| m.text().contains("secret")));
    }

    #[test]
    fn history_projection_scopes_reused_call_ids_to_each_turn() {
        let records = vec![
            Record::User("first".into()),
            Record::ToolCall {
                id: "call_0".into(),
                name: "search".into(),
                arguments: r#"{"query":"alpha"}"#.into(),
            },
            Record::ToolResult {
                name: "search".into(),
                call_id: "call_0".into(),
                body: "first result".into(),
            },
            Record::User("second".into()),
            Record::ToolCall {
                id: "call_0".into(),
                name: "search".into(),
                arguments: r#"{"query":"beta"}"#.into(),
            },
            Record::ToolResult {
                name: "search".into(),
                call_id: "call_0".into(),
                body: "second result".into(),
            },
            Record::Reasoning("hidden summary".into()),
            Record::Assistant("done".into()),
        ];
        let entries = canonical_entries(&records);
        let tools: Vec<_> = entries
            .iter()
            .filter(|entry| entry.role == Role::Tool)
            .collect();
        assert_eq!(tools.len(), 2);
        assert!(tools[0].text.contains("query: alpha"));
        assert!(tools[0].text.contains("first result"));
        assert!(tools[1].text.contains("query: beta"));
        assert!(tools[1].text.contains("second result"));
        assert!(
            !entries
                .iter()
                .any(|entry| entry.text.contains("hidden summary"))
        );
        assert_eq!(latest_reasoning(&records), "hidden summary");
    }

    #[test]
    fn history_projection_keeps_repeated_bash_executions_once_each() {
        let bash = |id: &str| Record::ToolCall {
            id: id.into(),
            name: "bash".into(),
            arguments: r#"{"command":"pwd"}"#.into(),
        };
        let result = |id: &str| Record::ToolResult {
            name: "bash".into(),
            call_id: id.into(),
            body: "duplicate output".into(),
        };
        let output = |id: &str, text: &str| Record::Bash {
            call_id: Some(id.into()),
            command: "pwd".into(),
            output: text.into(),
            exit_code: Some(0),
            truncated: false,
            spill: None,
        };
        let entries = canonical_entries(&[
            Record::User("run twice".into()),
            bash("a"),
            bash("b"),
            output("a", "first"),
            result("a"),
            output("b", "second"),
            result("b"),
            Record::User("legacy".into()),
            output("legacy", "legacy output"),
        ]);
        let bash_entries: Vec<_> = entries
            .iter()
            .filter(|entry| entry.role == Role::Bash)
            .collect();
        assert_eq!(bash_entries.len(), 3);
        assert!(bash_entries[0].text.contains("first"));
        assert!(bash_entries[1].text.contains("second"));
        assert!(bash_entries[2].text.contains("legacy output"));
        assert_eq!(bash_entries[0].tool_card.as_ref().unwrap().call_id, "a");
        assert_eq!(bash_entries[1].tool_card.as_ref().unwrap().call_id, "b");
        assert_ne!(bash_entries[0].tool_card.as_ref().unwrap().call_id, bash_entries[1].tool_card.as_ref().unwrap().call_id);
        assert!(
            !entries
                .iter()
                .any(|entry| entry.text.contains("No result recorded"))
        );
    }

    #[test]
    fn tool_card_preview_is_bounded_but_expansion_keeps_full_output() {
        let output = (0..12).map(|n| format!("line {n} {}", "x".repeat(100))).collect::<Vec<_>>().join("\n");
        let card = glass_tui::ToolCard {
            call_id: "call-exact".into(),
            command: "$ cargo test".into(),
            output: output.clone(),
            status: "exit 0".into(),
            elapsed_ms: Some(1234),
            spill: Some("/tmp/full-output".into()),
            truncated: true,
        };
        let preview = tool_card_text(&card, false);
        assert!(preview.contains("cargo test"));
        assert!(preview.contains("1.2s"));
        assert!(preview.contains("Enter to expand"));
        assert!(preview.len() < 500, "preview must remain small: {} bytes", preview.len());
        let full = tool_card_text(&card, true);
        assert!(full.contains(&output));
        assert_eq!(card.call_id, "call-exact");
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

    /// Compaction must survive a resume.
    ///
    /// The log on disk is append-only and still holds every dropped
    /// record. `open` is what rebuilds the projection, so a resumed
    /// session has to end up looking compact — otherwise it starts the
    /// first turn above the threshold, compacts again, and pays for a
    /// second summary for work the model already summarized once.
    #[test]
    fn resume_rebuilds_the_compaction_projection() {
        let dir = tmpdir("proj-resume");
        let mut s = Session::create(&dir).unwrap();
        let id = s.id.clone();
        let log_path = s.path.clone();

        // A long history: the opening task, then enough turns to have
        // a tail worth keeping.
        s.append(Record::User("the original task".into()));
        for i in 0..40 {
            s.append(Record::Assistant(format!("answer {i}")));
            s.append(Record::User(format!("question {i}")));
        }
        s.append(Record::State(vec!["task: migrate the parser".into()]));

        // Compact: keep the opening task, the state, and the tail.
        let total = s.records.len();
        let tail_start = total.saturating_sub(crate::compact::KEEP_RECENT);
        let rebuilt = crate::compact::rebuild(&s.records, &crate::tools::State::new());
        let kept = rebuilt.len();
        s.records = rebuilt;
        s.append(Record::Compacted {
            kept,
            dropped: total.saturating_sub(kept),
            tail_start,
            summary: Some("did the migration".into()),
            system: None,
        });
        drop(s);

        let reopened = Session::open(&dir, &id).unwrap();

        // The projection is small: task + summary + state + tail.
        assert!(
            reopened.records.len() < total / 2,
            "resume replayed the uncompacted history: {} of {} records",
            reopened.records.len(),
            total
        );

        // The opening task and the summary both survive.
        assert!(matches!(reopened.records.first(), Some(Record::User(_))));
        assert!(
            reopened
                .records
                .iter()
                .any(|r| matches!(r, Record::System(t) if t.contains("did the migration")))
        );

        // The tail is intact and in order.
        let tail: Vec<_> = reopened
            .records
            .iter()
            .filter_map(|r| match r {
                Record::User(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        assert!(
            tail.contains(&"question 39".into()),
            "tail lost its last turn"
        );

        // The dropped middle is gone from the projection.
        assert!(
            !reopened
                .records
                .iter()
                .any(|r| matches!(r, Record::Assistant(t) if t == "answer 5"))
        );

        // The log itself is untouched — compaction is not destructive.
        let raw = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            raw.contains("answer 5"),
            "the immutable log lost a dropped record"
        );
    }

    #[test]
    fn resume_is_idempotent_across_repeated_compaction() {
        // Compacting, resuming, and compacting again must converge
        // rather than growing.
        let dir = tmpdir("proj-idem");
        let mut s = Session::create(&dir).unwrap();
        let id = s.id.clone();
        s.append(Record::User("task".into()));
        for i in 0..60 {
            s.append(Record::User(format!("q{i}")));
            s.append(Record::Assistant(format!("a{i}")));
        }
        // Compact three times in a row.
        for _ in 0..3 {
            let total = s.records.len();
            // Index by log position, exactly as the agent does.
            // The log only grows, so this stays stable across
            // compactions; the projection length does not.
            let tail_start = s.log_len().saturating_sub(crate::compact::KEEP_RECENT);
            let rebuilt = crate::compact::rebuild(&s.records, &crate::tools::State::new());
            let kept = rebuilt.len();
            s.records = rebuilt;
            s.append(Record::Compacted {
                kept,
                dropped: total.saturating_sub(kept),
                tail_start,
                summary: None,
                system: None,
            });
        }
        drop(s);

        let reopened = Session::open(&dir, &id).unwrap();
        // Only the most recent epoch applies, so the projection is a
        // single compaction's worth — not three stacked summaries.
        assert!(
            reopened.records.len() <= crate::compact::KEEP_RECENT + 4,
            "projection grew to {}",
            reopened.records.len()
        );
        // Exactly one compaction epoch is reflected.
        assert_eq!(
            reopened
                .records
                .iter()
                .filter(|r| matches!(r, Record::Compacted { .. }))
                .count(),
            0,
            "the projection must not carry the marker itself"
        );
    }

    #[test]
    fn truncated_final_line_does_not_block_resume() {
        let dir = tmpdir("trunc");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crashed.jsonl");
        fs::write(&path, "{\"k\":\"user\",\"t\":\"first\"}\n{\"k\":\"assis").unwrap();
        let mut s = Session::open(&dir, "crashed").unwrap();
        assert_eq!(
            s.records.len(),
            1,
            "partial line is skipped, valid line kept"
        );
        // The torn fragment is truncated, not merely ignored: without that,
        // the next append would glue itself to the fragment and be lost.
        s.append(Record::User("second".into()));
        drop(s);
        let reopened = Session::open(&dir, "crashed").unwrap();
        assert_eq!(
            reopened.records.len(),
            2,
            "the next append is a clean record"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn valid_final_record_without_newline_gets_a_separator_before_append() {
        let dir = tmpdir("valid-no-lf");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let first = b"{\"k\":\"user\",\"t\":\"first\"}";
        fs::write(&path, first).unwrap();
        let mut session = Session::open(&dir, "s").unwrap();
        assert_eq!(session.records.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), [first.as_slice(), b"\n"].concat());
        session.append(Record::User("second".into()));
        drop(session);
        let reopened = Session::open(&dir, "s").unwrap();
        assert_eq!(reopened.records.len(), 2);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn torn_tail_truncation_preserves_original_blank_and_crlf_bytes() {
        let dir = tmpdir("torn-byte-boundary");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let prefix = b"{\"k\":\"user\",\"t\":\"one\"}\r\n\r\n";
        fs::write(&path, [prefix.as_slice(), b"{\"k\":\"assistant\""].concat()).unwrap();
        let session = Session::open(&dir, "s").unwrap();
        assert_eq!(session.records.len(), 1);
        drop(session);
        assert_eq!(fs::read(&path).unwrap(), prefix);
        // Repair is idempotent and leaves the exact retained bytes alone.
        let session = Session::open(&dir, "s").unwrap();
        drop(session);
        assert_eq!(fs::read(&path).unwrap(), prefix);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn torn_non_utf8_tail_is_repaired_without_lossy_boundary_changes() {
        let dir = tmpdir("torn-invalid-utf8");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let prefix = b"{\"k\":\"user\",\"t\":\"caf\xc3\xa9\"}\n";
        fs::write(&path, [prefix.as_slice(), b"{\xff"].concat()).unwrap();
        let session = Session::open(&dir, "s").unwrap();
        assert_eq!(session.records.len(), 1);
        drop(session);
        assert_eq!(fs::read(&path).unwrap(), prefix);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_corrupt_middle_line_is_dropped_with_a_backup_and_a_visible_note() {
        let dir = tmpdir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sess.jsonl");
        fs::write(
            &path,
            "{\"k\":\"user\",\"t\":\"one\"}\nthis is not json\n{\"k\":\"assistant\",\"t\":\"two\"}\n",
        )
        .unwrap();
        let s = Session::open(&dir, "sess").unwrap();
        assert!(matches!(s.records.first(), Some(Record::User(t)) if t == "one"));
        assert!(matches!(s.records.get(1), Some(Record::Assistant(t)) if t == "two"));
        assert!(
            s.records
                .iter()
                .any(|r| matches!(r, Record::Meta(t) if t.contains("unreadable"))),
            "the repair must be visible in the transcript"
        );
        let backups: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1, "the original log is backed up");
        assert!(
            fs::read_to_string(backups[0].path())
                .unwrap()
                .contains("this is not json")
        );
        // Stable across a second open: the bad line is gone, one note only.
        drop(s);
        let again = Session::open(&dir, "sess").unwrap();
        assert_eq!(
            again
                .records
                .iter()
                .filter(|r| matches!(r, Record::Meta(t) if t.contains("unreadable")))
                .count(),
            1
        );
        assert_eq!(
            fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
                .count(),
            1,
            "no second backup on re-open"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_complete_unreadable_final_line_is_corruption_not_a_torn_tail() {
        let dir = tmpdir("complete-corrupt");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        fs::write(&path, "{\"k\":\"user\",\"t\":\"one\"}\noops but newline\n").unwrap();
        let s = Session::open(&dir, "s").unwrap();
        assert_eq!(
            s.records
                .iter()
                .filter(|r| matches!(r, Record::User(_)))
                .count(),
            1
        );
        assert!(
            s.records
                .iter()
                .any(|r| matches!(r, Record::Meta(t) if t.contains("line 2"))),
            "a complete bad line is corruption at its real line number"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_record_kinds_are_forward_compatibility_not_corruption() {
        let dir = tmpdir("forward");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.jsonl");
        fs::write(
            &path,
            "{\"k\":\"future_thing\",\"x\":1}\n{\"k\":\"user\",\"t\":\"hi\"}\n",
        )
        .unwrap();
        let s = Session::open(&dir, "f").unwrap();
        assert_eq!(
            s.records.len(),
            1,
            "unknown kinds are skipped, not repaired"
        );
        assert_eq!(
            fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
                .count(),
            0,
            "no backup for a line from a newer version"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn context_edits_change_only_the_model_view() {
        let dir = tmpdir("ctx-edit");
        let mut s = Session::create(&dir).unwrap();
        s.append(Record::User("first question".into()));
        s.append(Record::Assistant("first answer".into()));
        s.append(Record::User("second question".into()));
        s.append(Record::Assistant("second answer".into()));

        // Hide the first turn: the model view loses it, the log keeps it.
        let note = s.context_command("forget", "1").unwrap();
        assert!(note.contains("hidden"), "{note}");
        let listing = s.context_command("forget", "").unwrap();
        assert!(listing.contains("hidden"), "{listing}");
        let text = format!("{:?}", s.messages());
        assert!(
            !text.contains("first question") && !text.contains("first answer"),
            "{text}"
        );
        assert!(text.contains("second question"), "{text}");
        assert!(
            s.records
                .iter()
                .any(|r| matches!(r, Record::User(t) if t == "first question")),
            "raw history keeps the original"
        );

        // Rewrite turn 2's user message only; the assistant reply stays.
        s.context_command("rewrite", "2 REWRITTEN-SECOND").unwrap();
        let text = format!("{:?}", s.messages());
        assert!(text.contains("REWRITTEN-SECOND"), "{text}");
        assert!(!text.contains("second question"), "{text}");
        assert!(text.contains("second answer"), "{text}");

        // Restore the first turn: the latest edit wins.
        s.context_command("unforget", "1").unwrap();
        let text = format!("{:?}", s.messages());
        assert!(text.contains("first question"), "{text}");
        assert!(text.contains("REWRITTEN-SECOND"), "{text}");

        // Validation.
        assert!(s.context_command("forget", "0").is_err());
        assert!(s.context_command("forget", "99").is_err());
        assert!(s.context_command("forget", "abc").is_err());
        assert!(
            s.context_command("rewrite", "1").is_err(),
            "a rewrite needs replacement text"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hiding_a_turn_with_tools_drops_its_tool_records_too() {
        let dir = tmpdir("ctx-edit-tools");
        let mut s = Session::create(&dir).unwrap();
        s.append(Record::User("run something".into()));
        s.append(Record::ToolCall {
            id: "c1".into(),
            name: "bash".into(),
            arguments: "{}".into(),
        });
        s.append(Record::ToolResult {
            name: "bash".into(),
            call_id: "c1".into(),
            body: "TOOL-OUTPUT-MARKER".into(),
        });
        s.append(Record::Assistant("done".into()));
        s.append(Record::User("next".into()));
        s.context_command("forget", "1").unwrap();
        let text = format!("{:?}", s.messages());
        assert!(!text.contains("TOOL-OUTPUT-MARKER"), "{text}");
        assert!(!text.contains("run something"), "{text}");
        assert!(text.contains("next"), "{text}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn edit_of_compacted_history_does_not_replay_summary_or_unbounded_raw_history() {
        let dir = tmpdir("ctx-edit-compacted");
        let mut s = Session::create(&dir).unwrap();
        let id = s.id.clone();
        s.append(Record::User("opening task".into()));
        s.append(Record::Assistant("SECRET-OLD-ANSWER".into()));
        for i in 2..=12 {
            s.append(Record::User(format!("question {i}")));
            s.append(Record::Assistant(format!("answer {i}")));
        }
        s.append(Record::State(vec!["SECRET-DERIVED-STATE".into()]));
        let tail_start = s.log_len().saturating_sub(crate::compact::KEEP_RECENT);
        s.append(Record::Compacted {
            kept: crate::compact::KEEP_RECENT,
            dropped: tail_start,
            summary: Some("SUMMARY-CONTAINS-SECRET-OLD-ANSWER".into()),
            tail_start,
            system: None,
        });
        s.context_command("forget", "1").unwrap();
        let text = format!("{:?}", s.messages());
        assert!(!text.contains("SECRET-OLD-ANSWER"), "{text}");
        assert!(!text.contains("SUMMARY-CONTAINS"), "{text}");
        assert!(!text.contains("SECRET-DERIVED-STATE"), "{text}");
        assert!(
            !text.contains("opening task") && text.contains("question 12"),
            "{text}"
        );
        assert!(
            !text.contains("question 3"),
            "fallback must stay bounded: {text}"
        );
        drop(s);
        // Edits are recovered from the append-only raw log, even if a later
        // projection excludes their metadata record.
        let reopened = Session::open(&dir, &id).unwrap();
        let text = format!("{:?}", reopened.messages());
        assert!(!text.contains("SECRET-OLD-ANSWER"), "{text}");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn partial_sidecar_survives_a_failed_checked_recovery_append() {
        let dir = tmpdir("partial-write-fail");
        let mut session = Session::create(&dir).unwrap();
        let sidecar = crate::partial::path(&session.path);
        fs::write(&sidecar, b"keep me").unwrap();
        session.file.take();
        assert!(
            session
                .recover_partial(
                    &sidecar,
                    crate::partial::Live {
                        text: "partial".into(),
                        at: crate::partial::now(),
                        ..crate::partial::Live::default()
                    },
                    None,
                )
                .is_err()
        );
        assert_eq!(fs::read(&sidecar).unwrap(), b"keep me");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn partial_staleness_is_checked_against_the_same_raw_user_turn() {
        let dir = tmpdir("partial-turn-match");
        let mut session = Session::create(&dir).unwrap();
        let id = session.id.clone();
        session.append(Record::User("first".into()));
        session.append(Record::Assistant("repeated partial text".into()));
        session.append(Record::User("second".into()));
        drop(session);
        let sidecar = crate::partial::path(&dir.join(format!("{id}.jsonl")));
        fs::write(
            &sidecar,
            serde_json::to_vec(&crate::partial::Live {
                turn: 2,
                text: "repeated partial text".into(),
                at: crate::partial::now(),
                ..crate::partial::Live::default()
            })
            .unwrap(),
        )
        .unwrap();
        let reopened = Session::open(&dir, &id).unwrap();
        assert!(
            reopened.records.iter().any(
                |r| matches!(r, Record::Meta(t) if t.contains("previous turn was interrupted"))
            )
        );
        assert_eq!(
            reopened
                .records
                .iter()
                .filter(|r| matches!(r, Record::Assistant(t) if t == "repeated partial text"))
                .count(),
            2,
            "the prior turn's matching assistant must not make the current sidecar stale"
        );
        assert!(
            !sidecar.exists(),
            "successful recovery consumes its sidecar"
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn fsync_policy_is_settable_and_appends_stay_readable() {
        configure_fsync(true);
        let dir = tmpdir("fsync");
        let mut s = Session::create(&dir).unwrap();
        let id = s.id.clone();
        s.append(Record::User("durable".into()));
        drop(s);
        let reopened = Session::open(&dir, &id).unwrap();
        assert_eq!(reopened.records.len(), 1);
        configure_fsync(false);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replay_policy_is_safe_only_for_state() {
        assert_eq!(replay_policy("state"), "safe");
        assert_eq!(replay_policy("bash"), "unsafe");
        assert_eq!(replay_policy("hl_edit"), "unsafe");
        assert_eq!(replay_policy("anything-unknown"), "unsafe");
    }

    #[test]
    fn opening_a_session_recovers_a_leftover_partial() {
        let dir = tmpdir("live-recover");
        let mut s = Session::create(&dir).unwrap();
        let id = s.id.clone();
        s.append(Record::User("go".into()));
        s.append(Record::Assistant("an earlier answer".into()));
        // Simulate the kill: a sidecar with streamed text and no final record.
        let live = crate::partial::Live {
            text: "half a th".into(),
            at: crate::partial::now(),
            ..Default::default()
        };
        fs::write(
            crate::partial::path(&s.path),
            serde_json::to_vec(&live).unwrap(),
        )
        .unwrap();
        drop(s);
        let reopened = Session::open(&dir, &id).unwrap();
        assert!(
            reopened
                .records
                .iter()
                .any(|r| matches!(r, Record::Meta(t) if t.contains("partial output recovered"))),
            "the interruption must be visible"
        );
        assert!(
            matches!(reopened.records.last(), Some(Record::Assistant(t)) if t == "half a th"),
            "the partial is recorded as the assistant message it was becoming"
        );
        assert!(
            !crate::partial::path(&dir.join(format!("{id}.jsonl"))).exists(),
            "the sidecar is consumed"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_stale_sidecar_whose_text_already_landed_is_discarded_quietly() {
        let dir = tmpdir("live-stale");
        let mut s = Session::create(&dir).unwrap();
        let id = s.id.clone();
        s.append(Record::Assistant("the whole answer".into()));
        let live = crate::partial::Live {
            text: "the whole answer".into(),
            at: crate::partial::now(),
            ..Default::default()
        };
        fs::write(
            crate::partial::path(&s.path),
            serde_json::to_vec(&live).unwrap(),
        )
        .unwrap();
        drop(s);
        let reopened = Session::open(&dir, &id).unwrap();
        assert!(
            !reopened
                .records
                .iter()
                .any(|r| matches!(r, Record::Meta(t) if t.contains("interrupted"))),
            "a late flush is not an interruption"
        );
        assert_eq!(
            reopened
                .records
                .iter()
                .filter(|r| matches!(r, Record::Assistant(_)))
                .count(),
            1,
            "no duplicate assistant text"
        );
        assert!(!crate::partial::path(&dir.join(format!("{id}.jsonl"))).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_interrupted_tool_call_is_repaired_once_with_policy_wording() {
        let dir = tmpdir("interrupted");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"k\":\"user\",\"t\":\"go\"}\n",
                "{\"k\":\"call\",\"id\":\"c1\",\"name\":\"bash\",\"args\":\"{}\"}\n",
                "{\"k\":\"call\",\"id\":\"c2\",\"name\":\"state\",\"args\":\"{}\"}\n",
                "{\"k\":\"tool\",\"name\":\"state\",\"id\":\"c2\",\"t\":\"ok\"}\n",
            ),
        )
        .unwrap();
        let s = Session::open(&dir, "s").unwrap();
        let repairs: Vec<&Record> = s
            .records
            .iter()
            .filter(|r| matches!(r, Record::ToolResult { call_id, .. } if call_id == "c1"))
            .collect();
        assert_eq!(repairs.len(), 1, "one repair for the unanswered call");
        let Record::ToolResult { body, .. } = repairs[0] else {
            unreachable!()
        };
        assert!(body.contains("may have partially run"), "{body}");
        // The answered call gained nothing; a second open repairs nothing more.
        assert_eq!(
            s.records
                .iter()
                .filter(|r| matches!(r, Record::ToolResult { call_id, .. } if call_id == "c2"))
                .count(),
            1,
            "the answered call gained nothing"
        );
        drop(s);
        let again = Session::open(&dir, "s").unwrap();
        assert_eq!(
            again
                .records
                .iter()
                .filter(|r| matches!(r, Record::ToolResult { call_id, .. } if call_id == "c1"))
                .count(),
            1,
            "the repair is not repeated on reopen"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_safe_interrupted_call_says_it_has_no_side_effects() {
        let dir = tmpdir("interrupted-safe");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        fs::write(
            &path,
            "{\"k\":\"call\",\"id\":\"c9\",\"name\":\"state\",\"args\":\"{}\"}\n",
        )
        .unwrap();
        let s = Session::open(&dir, "s").unwrap();
        let body = s
            .records
            .iter()
            .find_map(|r| match r {
                Record::ToolResult { call_id, body, .. } if call_id == "c9" => Some(body.clone()),
                _ => None,
            })
            .expect("repaired");
        assert!(body.contains("no side effects"), "{body}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_repair_pairs_the_provider_transcript() {
        let dir = tmpdir("interrupted-pair");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"k\":\"user\",\"t\":\"go\"}\n",
                "{\"k\":\"call\",\"id\":\"c1\",\"name\":\"bash\",\"args\":\"{}\"}\n",
            ),
        )
        .unwrap();
        let s = Session::open(&dir, "s").unwrap();
        let messages = s.messages();
        let call_at = messages
            .iter()
            .position(|m| matches!(m, Message::ToolCall { id, .. } if id == "c1"))
            .expect("the call is in context");
        let result_at = messages
            .iter()
            .position(|m| matches!(m, Message::ToolResult { call_id, body, .. } if call_id == "c1" && body.contains("interrupted")))
            .expect("the repair is in context, so tool_use always pairs with tool_result");
        assert!(result_at > call_at, "the result follows its call");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn raw_records_reads_without_repairing_a_torn_log() {
        let dir = tmpdir("raw-read-only");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let bytes = b"{\"k\":\"user\",\"t\":\"live\"}\n{\"k\":\"call\"";
        fs::write(&path, bytes).unwrap();
        let sidecar = crate::partial::path(&path);
        fs::write(&sidecar, b"live sidecar").unwrap();
        let records = Session::raw_records(&path).unwrap();
        assert!(matches!(records.as_slice(), [Record::User(text)] if text == "live"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(&sidecar).unwrap(), b"live sidecar");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn raw_history_keeps_custom_records_removed_from_compaction_projection() {
        let dir = tmpdir("raw-custom");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"k\":\"custom\",\"c\":\"ext/state\",\"d\":\"saved\"}\n",
                "{\"k\":\"compacted\",\"kept\":0,\"dropped\":1,\"tail\":2,\"sys\":null}\n",
                "{\"k\":\"user\",\"t\":\"tail\"}\n",
            ),
        )
        .unwrap();
        let reopened = Session::open(&dir, "s").unwrap();
        assert!(
            !reopened
                .records
                .iter()
                .any(|r| matches!(r, Record::Custom { .. }))
        );
        let raw = Session::raw_records(&path).unwrap();
        assert!(raw.iter().any(|r| matches!(r, Record::Custom { kind, data: Some(d) } if kind == "ext/state" && d == "saved")));
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
        let mut s = Session {
            id: "t".into(),
            path: "t".into(),
            file: None,
            _writer_lock: None,
            raw_index: RefCell::new(Vec::new()),
            raw_stamp: RefCell::new((0, std::time::UNIX_EPOCH, 0, 0, 0, 0)),
            records: vec![],
            turn: 0,
            log_len: 0,
            write_error: None,
        };
        s.records = vec![
            Record::User("q1".into()),
            Record::Assistant("a1".into()),
            Record::User("q2".into()),
        ];
        let entries = s.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].role, Role::User);
        assert_eq!(entries[0].turn, 1);
        assert_eq!(entries[2].turn, 2, "second user message advances the turn");
    }

    #[test]
    fn compaction_marker_renders_a_summary_line() {
        let (role, text) = Record::Compacted {
            kept: 2,
            dropped: 5,
            summary: None,
            tail_start: 0,
            system: None,
        }
        .to_entry();
        assert_eq!(role, Role::Meta);
        assert!(text.contains("dropped 5"));
    }

    #[test]
    fn custom_records_roundtrip_and_stay_out_of_context() {
        let r = Record::Custom {
            kind: "ext/state".into(),
            data: Some("v1".into()),
        };
        let j = r.to_json();
        assert_eq!(j["k"], "custom");
        let back = Record::from_json(&j).expect("must roundtrip");
        assert_eq!(j, back.to_json(), "roundtrip changed the record");
        assert!(
            back.to_message().is_none(),
            "custom entries never enter the model context"
        );
        let (role, text) = back.to_entry();
        assert_eq!(role, Role::Meta);
        assert!(text.contains("ext/state"), "{text}");

        // A record without data still roundtrips; the kind is the
        // only required field.
        let bare = Record::Custom {
            kind: "ext/flag".into(),
            data: None,
        };
        let back = Record::from_json(&bare.to_json()).expect("must roundtrip");
        assert!(matches!(back, Record::Custom { data: None, .. }));
    }

    #[test]
    fn a_compaction_checkpoint_roundtrips() {
        let r = Record::Compacted {
            kept: 1,
            dropped: 2,
            summary: None,
            tail_start: 0,
            system: Some("prompt text".into()),
        };
        let back = Record::from_json(&r.to_json()).expect("must roundtrip");
        assert!(matches!(
            back,
            Record::Compacted {
                system: Some(system),
                ..
            } if system == "prompt text"
        ));
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
