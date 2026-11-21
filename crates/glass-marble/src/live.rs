//! Durable journal for the turns that are still running.
//!
//! The product document is written when a turn is admitted and again when it
//! settles, so a crash in between loses exactly the part a person is watching:
//! the reply being written. Core keeps its own durable session journal, and that
//! is what makes a conversation resumable; it is not a copy of the partial reply
//! Marble is streaming to a surface. This module keeps one small record per
//! in-flight turn — who is running it, for which conversation, and the text so
//! far — and folds whatever it finds back into the transcript on the next start.
//!
//! The record is also the intent: a turn that never wrote a word still leaves a
//! visible interrupted task instead of vanishing. That is the effect sandwich
//! from pi-durable, kept deliberately small — one record per task, replayed
//! once, never re-executed.
//!
//! Writes are paced: a token stream is not a reason to touch the disk thousands
//! of times a second, and the last progress write before a crash is worth more
//! than every one of them.

use crate::state::{ProductState, TranscriptEntry};
use crate::store::{self, StateStore};
use crate::worker::now_millis;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How often a running turn rewrites its partial reply. A crash can lose at
/// most this much of the text being written.
pub const PARTIAL_INTERVAL: Duration = Duration::from_millis(250);
/// Longest identifier accepted as a journal file name.
const ID_LIMIT: usize = 96;

/// What one recovered journal entry restored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveRecovery {
    /// Turns that were still running when the previous process stopped.
    pub turns: usize,
    /// Recovered turns that had already written some reply text.
    pub partials: usize,
    /// Journal entries that could not be read and were kept aside.
    pub unreadable: usize,
}

impl LiveRecovery {
    pub fn is_empty(&self) -> bool {
        self.turns == 0 && self.unreadable == 0
    }
}

/// One in-flight turn as it exists on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveRecord {
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    conversation_id: String,
    #[serde(default)]
    task_id: String,
    /// Who is running the turn: the host, or the marble that owns the engine.
    #[serde(default)]
    owner: String,
    /// What the person asked for, so an interrupted turn explains itself.
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    reasoning: String,
    #[serde(default)]
    at: u64,
}

/// Directory holding one journal entry per in-flight turn.
pub fn live_dir(data_dir: &Path) -> PathBuf {
    store::live_dir(data_dir)
}

/// The live journal for one host.
#[derive(Debug, Clone)]
pub struct LiveStore {
    dir: PathBuf,
}

impl LiveStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Journal beside the durable product document.
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(live_dir(data_dir))
    }

    /// Same journal, for a host built straight from a state store.
    pub fn for_store(store: &StateStore) -> Self {
        match store.data_dir() {
            Some(data_dir) => Self::in_data_dir(data_dir),
            None => store
                .path()
                .parent()
                .map(Self::in_data_dir)
                .unwrap_or_else(|| Self::new("live")),
        }
    }

    /// Start journaling one turn. `None` means the journal is unavailable, and
    /// the turn still runs — a turn is never refused because a sidecar failed.
    pub fn begin(
        &self,
        prompt: &str,
        owner: &str,
        conversation_id: &str,
        request_id: &str,
    ) -> Option<LiveTurn> {
        let stem: String = request_id
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
            .take(ID_LIMIT)
            .collect();
        if stem.is_empty() {
            return None;
        }
        if let Err(error) = fs::create_dir_all(&self.dir) {
            tracing::warn!(%error, dir = %self.dir.display(), "cannot open the live turn journal");
            return None;
        }
        let record = LiveRecord {
            request_id: request_id.to_string(),
            conversation_id: conversation_id.to_string(),
            owner: owner.to_string(),
            prompt: prompt.to_string(),
            at: now_millis(),
            ..LiveRecord::default()
        };
        let mut turn = LiveTurn {
            path: self.dir.join(format!("{stem}.json")),
            record,
            written: Instant::now(),
            finished: false,
        };
        turn.flush();
        Some(turn)
    }

    /// Remove journal entries that belong to one conversation. Deleting a
    /// conversation clears its in-flight record too; a turn that is still
    /// running cannot be deleted, so anything left for it is finished.
    pub fn remove_conversation(&self, conversation_id: &str) -> usize {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        let mut removed = 0;
        for path in entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        {
            let belongs = fs::read_to_string(&path)
                .ok()
                .and_then(|encoded| serde_json::from_str::<LiveRecord>(&encoded).ok())
                .is_some_and(|record| record.conversation_id == conversation_id);
            if belongs && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// Fold every journal entry left by a previous process into the document.
    ///
    /// A recovered turn is marked interrupted, its partial reply is appended to
    /// the conversation it belongs to, and the entry is removed. Recovering is
    /// the only thing that ever reads these files, so a completed turn can never
    /// replay into a second transcript entry.
    pub fn recover(&self, state: &mut ProductState) -> LiveRecovery {
        let mut outcome = LiveRecovery::default();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return outcome;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let record = match fs::read_to_string(&path)
                .ok()
                .and_then(|encoded| serde_json::from_str::<LiveRecord>(&encoded).ok())
            {
                Some(record) => record,
                None => {
                    let quarantined = path.with_extension(format!("corrupt-{}", now_millis()));
                    let _ = fs::rename(&path, &quarantined);
                    outcome.unreadable += 1;
                    tracing::warn!(
                        path = %path.display(),
                        kept = %quarantined.display(),
                        "a live turn entry was unreadable and was kept aside"
                    );
                    continue;
                }
            };
            apply(&record, state);
            outcome.turns += 1;
            if !record.text.trim().is_empty() {
                outcome.partials += 1;
            }
            let _ = fs::remove_file(&path);
        }
        if outcome.turns > 0 || outcome.unreadable > 0 {
            state.bump();
        }
        outcome
    }
}

/// Restore one journal entry into the product document.
fn apply(record: &LiveRecord, state: &mut ProductState) {
    let now = now_millis();
    let conversation_id = record.conversation_id.trim();
    // A journal entry outlives its turn only when the process died mid-flight.
    // If the task already settled, the entry is a leftover from a finish() that
    // raced the stop: the turn's real outcome is in the document, and replaying
    // it would both rewrite the status and append a bogus "stopped mid-turn"
    // note beside the reply that did arrive.
    let already_settled = !record.task_id.is_empty()
        && state
            .tasks
            .iter()
            .find(|task| task.id == record.task_id)
            .is_some_and(|task| crate::state::settled(&task.status));
    if already_settled {
        return;
    }
    if !conversation_id.is_empty() {
        let conversation = state.conversation_mut(conversation_id);
        conversation.updated_at = now;
    }
    if !record.task_id.is_empty() {
        // Only a turn that still believes it is running becomes interrupted;
        // anything already settled keeps the outcome it recorded.
        state.set_task_status(&record.task_id, "interrupted");
        state.routines.note_settled(&record.task_id, "interrupted");
    }
    if conversation_id.is_empty() {
        return;
    }
    if !record.text.trim().is_empty() {
        state.push_transcript(TranscriptEntry {
            conversation_id: conversation_id.to_string(),
            kind: "assistant".into(),
            text: record.text.clone(),
            task_id: record.task_id.clone(),
            at: now.saturating_sub(1),
            ..Default::default()
        });
    }
    if !record.reasoning.trim().is_empty() {
        state.push_transcript(TranscriptEntry {
            conversation_id: conversation_id.to_string(),
            kind: "reasoning".into(),
            text: record.reasoning.clone(),
            task_id: record.task_id.clone(),
            at: now.saturating_sub(1),
            ..Default::default()
        });
    }
    let who = match record.owner.trim() {
        "" | "agent" => "Marble".to_string(),
        owner => owner.to_string(),
    };
    let recovered = if record.text.trim().is_empty() {
        "It had not written a reply yet."
    } else {
        "The partial reply above was recovered from the live journal."
    };
    state.push_transcript(TranscriptEntry {
        conversation_id: conversation_id.to_string(),
        kind: "system".into(),
        text: format!(
            "{who} stopped mid-turn when the host restarted. {recovered} Send the message again or \
             use retry to continue."
        ),
        task_id: record.task_id.clone(),
        at: now,
        ..Default::default()
    });
}

/// One turn's journal entry, held open for the life of the turn.
#[derive(Debug)]
pub struct LiveTurn {
    path: PathBuf,
    record: LiveRecord,
    written: Instant,
    finished: bool,
}

impl LiveTurn {
    /// Record the durable task the engine issued for this turn.
    pub fn note_task(&mut self, conversation_id: &str, task_id: &str) {
        self.record.conversation_id = conversation_id.to_string();
        self.record.task_id = task_id.to_string();
        self.flush();
    }

    /// Record how far the reply has got. Writes are paced, so this is cheap
    /// enough to call for every streamed delta.
    pub fn progress(&mut self, text: &str, reasoning: &str) {
        self.record.text = text.to_string();
        self.record.reasoning = reasoning.to_string();
        if self.written.elapsed() >= PARTIAL_INTERVAL {
            self.flush();
        }
    }

    /// Write the current state regardless of the pace.
    fn flush(&mut self) {
        if self.finished {
            return;
        }
        let encoded = match serde_json::to_vec(&self.record) {
            Ok(encoded) => encoded,
            Err(error) => {
                tracing::warn!(%error, "cannot encode a live turn entry");
                return;
            }
        };
        if let Err(error) = store::write_durably(&self.path, &encoded) {
            tracing::warn!(%error, path = %self.path.display(), "cannot write a live turn entry");
            return;
        }
        self.written = Instant::now();
    }

    /// The turn settled, so there is nothing left to recover. Idempotent.
    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(%error, path = %self.path.display(), "cannot clear a live turn entry")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TaskRecord;

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("glass-marble-live-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_finished_turn_leaves_nothing_to_recover() {
        let dir = dir("finished");
        let journal = LiveStore::in_data_dir(&dir);
        let mut turn = journal.begin("tidy this", "Marble", "", "req-1").unwrap();
        turn.note_task("conv-1", "task-1");
        turn.progress("half a thought", "");
        turn.finish();

        let mut state = ProductState::default();
        let outcome = journal.recover(&mut state);
        assert!(outcome.is_empty(), "{outcome:?}");
        assert!(state.transcript.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_conversation_clears_only_its_own_journal() {
        let dir = dir("remove");
        let journal = LiveStore::in_data_dir(&dir);
        let mut first = journal
            .begin("gone work", "Marble", "gone", "req-1")
            .unwrap();
        first.note_task("gone", "task-1");
        let mut second = journal
            .begin("kept work", "Marble", "kept", "req-2")
            .unwrap();
        second.note_task("kept", "task-2");

        assert_eq!(journal.remove_conversation("gone"), 1);
        assert_eq!(journal.remove_conversation("gone"), 0);

        // The kept turn still recovers; the removed one is gone for good.
        let mut state = ProductState::default();
        state.conversation_mut("kept");
        let outcome = journal.recover(&mut state);
        assert_eq!(outcome.turns, 1);
        assert!(
            state
                .transcript
                .iter()
                .all(|entry| entry.conversation_id != "gone")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crashed_turn_recovers_its_partial_reply_once() {
        let dir = dir("partial");
        let journal = LiveStore::in_data_dir(&dir);
        let mut state = ProductState::default();
        state.tasks.push(TaskRecord {
            id: "task-1".into(),
            conversation_id: "conv-1".into(),
            status: "running".into(),
            owner: "agent".into(),
            prompt: "tidy this".into(),
            at: 1,
            routine_id: String::new(),
        });
        {
            let mut turn = journal
                .begin("tidy this", "Scout", "conv-1", "req-1")
                .unwrap();
            turn.note_task("conv-1", "task-1");
            // Progress writes are paced, so let the pace elapse: this is the
            // text a crash a moment later would keep.
            std::thread::sleep(PARTIAL_INTERVAL + Duration::from_millis(50));
            turn.progress("the workspace has ", "");
            // Dropped, not finished: the process is assumed killed here.
        }

        let outcome = journal.recover(&mut state);
        assert_eq!(outcome.turns, 1);
        assert_eq!(outcome.partials, 1);
        assert_eq!(state.tasks[0].status, "interrupted");
        let assistant = state
            .transcript
            .iter()
            .find(|entry| entry.kind == "assistant")
            .expect("the partial reply is kept");
        assert_eq!(assistant.text, "the workspace has ");
        let note = state
            .transcript
            .iter()
            .find(|entry| entry.kind == "system")
            .expect("the interruption is explained");
        assert!(note.text.contains("Scout"), "{}", note.text);
        assert!(note.text.contains("restarted"), "{}", note.text);
        assert!(state.conversations.iter().any(|c| c.id == "conv-1"));

        let second = journal.recover(&mut state);
        assert!(second.is_empty(), "a journal entry is replayed once");
        assert_eq!(
            state
                .transcript
                .iter()
                .filter(|entry| entry.kind == "assistant")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stale_entry_for_a_settled_turn_rewrites_nothing() {
        // finish() removes the journal file, but a stop that lands between the
        // settled state write and the removal leaves a stale entry. Recovery
        // must not flip the real outcome back to interrupted nor append a
        // "stopped mid-turn" note beside the reply that did arrive.
        let dir = dir("stale-settled");
        let journal = LiveStore::in_data_dir(&dir);
        let mut state = ProductState::default();
        state.tasks.push(TaskRecord {
            id: "task-1".into(),
            conversation_id: "conv-1".into(),
            status: "completed".into(),
            owner: "agent".into(),
            prompt: "tidy this".into(),
            at: 1,
            routine_id: String::new(),
        });
        {
            let mut turn = journal
                .begin("tidy this", "Marble", "conv-1", "req-1")
                .unwrap();
            turn.note_task("conv-1", "task-1");
            std::thread::sleep(PARTIAL_INTERVAL + Duration::from_millis(50));
            turn.progress("all done", "");
            // Dropped without finish: the file survives as if stale.
        }

        let outcome = journal.recover(&mut state);
        assert_eq!(outcome.turns, 1, "the entry is still folded in once");
        assert_eq!(state.tasks[0].status, "completed");
        assert!(state.transcript.is_empty(), "no replay, no note");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_entry_is_kept_aside_instead_of_dropped() {
        let dir = dir("corrupt");
        let journal = LiveStore::in_data_dir(&dir);
        fs::create_dir_all(live_dir(&dir)).unwrap();
        fs::write(live_dir(&dir).join("broken.json"), b"{ not json").unwrap();
        let mut state = ProductState::default();
        let outcome = journal.recover(&mut state);
        assert_eq!(outcome.unreadable, 1);
        assert_eq!(outcome.turns, 0);
        let kept = fs::read_dir(live_dir(&dir))
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-"));
        assert!(kept, "the unreadable entry is kept for inspection");
        let _ = fs::remove_dir_all(&dir);
    }
}
