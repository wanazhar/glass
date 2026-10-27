//! Throttled partial-output sidecar: what a killed turn was saying.
//!
//! pi-durable commits the in-flight response every `partialIntervalMs`
//! (default 100 ms) and awaits the final flush before the outcome commit, so
//! a crash loses at most that window. The equivalent here is a small
//! `<session>.live.json` sidecar written at most every `live_flush_ms`
//! (default 250): the assistant text tail, which tool was running, and the
//! retry state. It is deleted whenever the turn ends normally — success,
//! failure, or cancel — which is what makes a leftover file mean
//! interruption, with one heuristic for the final-flush race: if the last
//! recorded assistant text already contains the live text, the flush merely
//! arrived late and the file is stale.
//!
//! Tool *output* is not committed here because the harness does not stream
//! it — `bash` collects output and the tool result records what completed —
//! so there is nothing in flight to preserve mid-tool.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Which tool was running when the process died.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolLive {
    pub call_id: String,
    pub name: String,
}

/// The sidecar contents.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Live {
    /// User-turn counter at turn start, for the recovery message.
    pub turn: u64,
    /// Retry attempt in flight, if any (the committed retry clock).
    pub attempt: u32,
    /// When the current backoff ends, if a retry is waiting.
    pub retry_until: Option<u64>,
    /// Assistant text streamed so far (tail-capped).
    pub text: String,
    pub tool: Option<ToolLive>,
    pub started_at: u64,
    pub at: u64,
}

const TEXT_CAP: usize = 65_536;
const RECOVERED_CAP: usize = 8_192;
const TOOL_FIELD_CAP: usize = 4_096;
const LIVE_SNAPSHOT_CAP: usize = 512 * 1024;

/// `<session>.jsonl` → `<session>.live.json`.
pub fn path(session_path: &Path) -> PathBuf {
    session_path.with_extension("live.json")
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Keep the last `cap` characters, on a char boundary.
fn tail_chars(text: &str, cap: usize) -> String {
    let count = text.chars().count();
    if count <= cap {
        return text.to_string();
    }
    text.chars().skip(count - cap).collect()
}

fn prefix_bytes(text: &str, cap: usize) -> String {
    let mut end = text.len().min(cap);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Read the sidecar, discarding an unreadable one (it is best-effort by
/// definition: a torn write here loses a partial, never the log).
pub fn load(session_path: &Path) -> Option<Live> {
    let path = path(session_path);
    let data = crate::requests::bounded(&path, LIVE_SNAPSHOT_CAP).ok()?;
    match serde_json::from_slice::<Live>(&data) {
        Ok(live) => Some(live),
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// What a leftover sidecar tells a resuming session.
pub struct Recovered {
    pub meta: String,
    pub partial: Option<String>,
}

/// Interpret a leftover sidecar, returning what to record — or `None` for a
/// stale file. The caller removes it only after the recovery records are
/// durably appended.
pub fn recover(live: Live, assistant_for_turn: Option<&str>) -> Option<Recovered> {
    if live.text.is_empty() && live.tool.is_none() && live.attempt == 0 {
        return None; // nothing had happened yet
    }
    // The final flush can lose the race with process death after the full
    // message was already recorded; that file is stale, not a partial.
    if !live.text.is_empty()
        && assistant_for_turn.is_some_and(|text| text.contains(live.text.as_str()))
    {
        return None;
    }
    let ago = now().saturating_sub(live.at);
    let mut meta = String::from("previous turn was interrupted");
    if live.attempt > 0 {
        meta.push_str(&format!(" while retrying (attempt {})", live.attempt));
    }
    if let Some(tool) = &live.tool {
        meta.push_str(&format!(" with `{}` running", tool.name));
    }
    meta.push_str(&format!(" {ago}s ago;"));
    if live.text.is_empty() {
        meta.push_str(" no partial response had been streamed");
    } else {
        meta.push_str(&format!(
            " {} chars of partial output recovered",
            live.text.chars().count()
        ));
    }
    let partial = (!live.text.is_empty()).then(|| tail_chars(&live.text, RECOVERED_CAP));
    Some(Recovered { meta, partial })
}

/// The write side, owned by an `Agent` for the duration of a turn.
pub struct Writer {
    path: PathBuf,
    interval: Duration,
    last_write: Instant,
    live: Live,
    dirty: bool,
}

impl Writer {
    /// `interval_ms == 0` disables the sidecar entirely.
    pub fn new(session_path: &Path, interval_ms: u64) -> Option<Self> {
        if interval_ms == 0 {
            return None;
        }
        Some(Self {
            path: path(session_path),
            // Below 10ms this is disk churn for no benefit.
            interval: Duration::from_millis(interval_ms.max(10)),
            last_write: Instant::now() - Duration::from_secs(3_600),
            live: Live::default(),
            dirty: false,
        })
    }

    pub fn begin_turn(&mut self, turn: u64) {
        self.live = Live {
            turn,
            started_at: now(),
            at: now(),
            ..Live::default()
        };
        self.dirty = true;
        self.flush(true);
    }

    /// Append streamed assistant text, keeping only the tail.
    pub fn text(&mut self, delta: &str) {
        self.live.text.push_str(delta);
        if self.live.text.len() > TEXT_CAP {
            let overflow = self.live.text.len() - TEXT_CAP;
            let cut = self
                .live
                .text
                .char_indices()
                .find(|(i, _)| *i >= overflow)
                .map(|(i, _)| i)
                .unwrap_or(self.live.text.len());
            self.live.text.drain(..cut);
        }
        self.dirty = true;
        self.flush(false);
    }

    pub fn tool(&mut self, call_id: &str, name: &str) {
        self.live.tool = Some(ToolLive {
            call_id: prefix_bytes(call_id, TOOL_FIELD_CAP),
            name: prefix_bytes(name, TOOL_FIELD_CAP),
        });
        self.dirty = true;
        // A tool starting is a phase boundary: everything the model said in
        // this round is final, and the tool may run for minutes. Flushing
        // only on the throttle would leave the last delta unwritten for
        // exactly as long as the tool takes, which is when a kill is most
        // likely and the partial most valuable.
        self.flush(true);
    }

    /// Record a retry in flight: attempt number and the backoff about to
    /// elapse. The deadline is committed with it so a restart does not reset
    /// the clock (the "committed retry clock").
    pub fn retry(&mut self, attempt: u32, delay_s: u64) {
        self.live.attempt = attempt;
        self.live.retry_until = Some(now() + delay_s);
        self.dirty = true;
        self.flush(true);
    }

    /// A tool finished; whatever comes next is not "with X running".
    pub fn tool_done(&mut self) {
        if self.live.tool.is_some() {
            self.live.tool = None;
            self.dirty = true;
            self.flush(false);
        }
    }

    /// Write when something changed and the interval has elapsed.
    pub fn flush(&mut self, force: bool) {
        if !self.dirty || (!force && self.last_write.elapsed() < self.interval) {
            return;
        }
        self.live.at = now();
        if let Ok(data) = serde_json::to_vec(&self.live) {
            if data.len() > LIVE_SNAPSHOT_CAP {
                return;
            }
            // Keep the previous good snapshot intact until the new one is
            // complete. The sidecar contains private prompt text, so create
            // the staging file with owner-only permissions as well.
            let tmp = self
                .path
                .with_extension(format!("live.{}.tmp", std::process::id()));
            let result = (|| -> std::io::Result<()> {
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&tmp)?;
                file.write_all(&data)?;
                file.flush()?;
                file.sync_all()?;
                std::fs::rename(&tmp, &self.path)?;
                Ok(())
            })();
            if result.is_ok() {
                self.last_write = Instant::now();
                self.dirty = false;
            } else {
                let _ = std::fs::remove_file(&tmp);
                // Retain dirty so a later tick or phase boundary retries.
            }
        }
    }

    /// The turn is over, however it ended: the final records on disk are the
    /// truth, so the sidecar goes away.
    pub fn finish(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gc-partial-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_interval_throttles_writes_and_force_bypasses_it() {
        let dir = scratch("throttle");
        let session = dir.join("s.jsonl");
        let mut w = Writer::new(&session, 60_000).expect("enabled");
        w.begin_turn(1);
        assert!(path(&session).exists(), "begin_turn writes immediately");
        // Inside the window, text is held back; force writes it.
        w.text("hello");
        let held = load(&session).unwrap();
        assert!(held.text.is_empty(), "throttled: {:?}", held.text);
        w.flush(true);
        assert_eq!(load(&session).unwrap().text, "hello");
        // finish removes the sidecar entirely.
        w.finish();
        assert!(!path(&session).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_atomic_snapshot_keeps_previous_snapshot_and_stays_dirty() {
        let dir = scratch("atomic-failure");
        let session = dir.join("s.jsonl");
        let mut w = Writer::new(&session, 10).unwrap();
        w.begin_turn(1);
        w.text("previous");
        w.flush(true);
        let old = std::fs::read(path(&session)).unwrap();
        // A directory at the staging path makes the write fail while the
        // previous committed snapshot remains available for recovery.
        let tmp = path(&session).with_extension(format!("live.{}.tmp", std::process::id()));
        std::fs::create_dir(&tmp).unwrap();
        w.text(" retry");
        w.flush(true);
        assert!(w.dirty);
        assert_eq!(std::fs::read(path(&session)).unwrap(), old);
        std::fs::remove_dir(&tmp).unwrap();
        w.flush(true);
        assert_eq!(load(&session).unwrap().text, "previous retry");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_disabled_writer_is_none() {
        let dir = scratch("disabled");
        assert!(Writer::new(&dir.join("s.jsonl"), 0).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn streamed_text_keeps_only_the_tail() {
        let dir = scratch("tail");
        let session = dir.join("s.jsonl");
        let mut w = Writer::new(&session, 10).unwrap();
        w.begin_turn(1);
        w.text(&"x".repeat(TEXT_CAP + 1_000));
        w.flush(true);
        let live = load(&session).unwrap();
        assert!(live.text.len() <= TEXT_CAP);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn streamed_tool_identifiers_are_bounded_at_utf8_boundaries() {
        let dir = scratch("tool-field-bound");
        let session = dir.join("s.jsonl");
        let mut writer = Writer::new(&session, 10).unwrap();
        writer.begin_turn(1);
        writer.tool(&"é".repeat(TOOL_FIELD_CAP), &"工".repeat(TOOL_FIELD_CAP));
        writer.flush(true);

        let loaded = load(&session).expect("bounded sidecar should load");
        let tool = loaded.tool.expect("tool phase should be present");
        assert!(tool.call_id.len() <= TOOL_FIELD_CAP);
        assert!(tool.name.len() <= TOOL_FIELD_CAP);
        assert!(std::fs::metadata(path(&session)).unwrap().len() <= LIVE_SNAPSHOT_CAP as u64);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn oversized_sidecar_is_rejected_before_json_parsing() {
        let dir = scratch("oversized");
        let session = dir.join("s.jsonl");
        let sidecar = path(&session);
        let content = vec![b' '; LIVE_SNAPSHOT_CAP + 1];
        std::fs::write(&sidecar, &content).unwrap();

        assert!(load(&session).is_none());
        assert_eq!(
            std::fs::metadata(&sidecar).unwrap().len(),
            content.len() as u64
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recovery_reports_a_partial_without_consuming_the_sidecar() {
        let dir = scratch("recover");
        let session = dir.join("s.jsonl");
        let live = Live {
            text: "half a".into(),
            at: now(),
            tool: Some(ToolLive {
                call_id: "c1".into(),
                name: "bash".into(),
            }),
            ..Live::default()
        };
        std::fs::write(path(&session), serde_json::to_vec(&live).unwrap()).unwrap();
        let recovered = recover(live, Some("an earlier answer")).expect("recovered");
        assert!(
            recovered.meta.contains("with `bash` running"),
            "{}",
            recovered.meta
        );
        assert_eq!(recovered.partial.as_deref(), Some("half a"));
        assert!(
            path(&session).exists(),
            "the caller owns deletion after checked append"
        );

        // A final flush that lost the race is stale, not a partial.
        let live = Live {
            text: "the whole answer".into(),
            at: now(),
            ..Live::default()
        };
        std::fs::write(path(&session), b"{}").unwrap();
        assert!(recover(live, Some("prefix the whole answer suffix")).is_none());

        // Nothing had happened: nothing to report.
        assert!(recover(Live::default(), None).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unreadable_sidecar_is_discarded_not_an_error() {
        let dir = scratch("junk");
        let session = dir.join("s.jsonl");
        std::fs::write(path(&session), b"not json").unwrap();
        assert!(load(&session).is_none());
        assert!(!path(&session).exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
