use crate::{ProductState, state::TranscriptEntry};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default)]
pub struct ThreadEvents(pub Option<crate::EventBus>);

impl PartialEq for ThreadEvents {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: String,
    pub reply_count: usize,
    pub last_activity_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub conversation_id: String,
    pub parent_entry_id: String,
    pub parent_preview: String,
    pub parent_sender: String,
    pub reply_count: usize,
    pub last_activity_at: u64,
    pub created_at: u64,
    pub participants: Vec<String>,
}

impl Thread {
    fn summary(&self) -> ThreadSummary {
        ThreadSummary {
            id: self.id.clone(),
            reply_count: self.reply_count,
            last_activity_at: self.last_activity_at,
        }
    }
}

impl ProductState {
    pub fn ensure_entry_ids(&mut self) {
        let mut used: std::collections::HashSet<String> = self
            .transcript
            .iter()
            .filter(|e| !e.id.is_empty())
            .map(|e| e.id.clone())
            .collect();
        for entry in &mut self.transcript {
            if !entry.id.is_empty() {
                continue;
            }
            let source = serde_json::to_vec(entry).unwrap_or_default();
            let hash = source.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
            let mut occurrence = 0;
            loop {
                let id = format!("entry-{hash:016x}-{occurrence}");
                if used.insert(id.clone()) {
                    entry.id = id;
                    break;
                }
                occurrence += 1;
            }
        }
    }

    pub fn entry_sender(&self, entry: &TranscriptEntry) -> String {
        if !entry.speaker_name.is_empty() {
            return entry.speaker_name.clone();
        }
        if entry.kind == "user" {
            return "You".into();
        }
        self.tasks
            .iter()
            .find(|t| t.id == entry.task_id)
            .and_then(|t| self.prism.get(&t.marble_id))
            .or_else(|| self.prism.owner_of(&entry.conversation_id))
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "Marble".into())
    }

    pub fn create_thread(&mut self, conversation: &str, parent: &str) -> Option<Thread> {
        self.ensure_entry_ids();
        if let Some(thread) = self
            .threads
            .iter()
            .find(|t| t.parent_entry_id == parent && t.conversation_id == conversation)
        {
            return Some(thread.clone());
        }
        let entry = self
            .transcript
            .iter()
            .find(|e| e.id == parent && e.conversation_id == conversation)?;
        let now = crate::worker::now_millis();
        let thread = Thread {
            id: format!("thread-{}", crate::next_request_id()),
            conversation_id: conversation.into(),
            parent_entry_id: parent.into(),
            parent_preview: entry.text.chars().take(240).collect(),
            parent_sender: self.entry_sender(entry),
            created_at: now,
            last_activity_at: now,
            ..Default::default()
        };
        self.threads.push(thread.clone());
        self.refresh_thread(&thread.id);
        self.threads.iter().find(|t| t.id == thread.id).cloned()
    }

    pub fn bind_thread(&mut self, task: &str, thread: Option<&str>) {
        if let Some(id) = thread {
            self.task_threads.insert(task.into(), id.into());
            if let Some(events) = &self.thread_events.0 {
                events.bind_thread(task, id);
            }
        }
    }

    pub fn thread_entries(&self, thread: &Thread) -> Vec<TranscriptEntry> {
        let mut entries: Vec<_> = self
            .transcript
            .iter()
            .filter(|e| e.id == thread.parent_entry_id)
            .cloned()
            .collect();
        entries.extend(
            self.transcript
                .iter()
                .filter(|entry| {
                    entry.id != thread.parent_entry_id && self.thread_entry_visible(thread, entry)
                })
                .cloned(),
        );
        entries
    }

    fn thread_entry_visible(&self, thread: &Thread, entry: &TranscriptEntry) -> bool {
        if entry.id == thread.parent_entry_id {
            return true;
        }
        if entry.thread_id.as_deref() != Some(&thread.id) {
            return false;
        }
        if thread.conversation_id != crate::everyone::EVERYONE_CONVERSATION
            || entry.conversation_id == thread.conversation_id
        {
            return true;
        }
        if entry.kind == "user" {
            return false;
        }
        !self.transcript.iter().any(|mirror| {
            mirror.conversation_id == thread.conversation_id
                && mirror.thread_id == entry.thread_id
                && mirror.task_id == entry.task_id
                && mirror.kind == entry.kind
        })
    }

    fn search_entry_visible(&self, entry: &TranscriptEntry, thread: Option<&str>) -> bool {
        if let Some(id) = thread.or(entry.thread_id.as_deref()) {
            return self
                .threads
                .iter()
                .find(|t| t.id == id)
                .is_some_and(|thread| self.thread_entry_visible(thread, entry));
        }
        entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION
            || entry.task_id.is_empty()
            || !self.transcript.iter().any(|mirror| {
                mirror.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                    && mirror.thread_id.is_none()
                    && mirror.task_id == entry.task_id
                    && mirror.kind == entry.kind
            })
    }

    pub fn refresh_thread(&mut self, id: &str) {
        let Some(index) = self.threads.iter().position(|t| t.id == id) else {
            return;
        };
        let entries = self.thread_entries(&self.threads[index]);
        let count = entries
            .iter()
            .skip(1)
            .filter(|e| matches!(e.kind.as_str(), "user" | "assistant" | "failure"))
            .count();
        let mut participants = Vec::new();
        for entry in &entries {
            let sender = self.entry_sender(entry);
            if !participants.contains(&sender) {
                participants.push(sender);
            }
        }
        let thread = &mut self.threads[index];
        thread.reply_count = count;
        thread.participants = participants;
        thread.last_activity_at = entries
            .iter()
            .map(|e| e.at)
            .max()
            .unwrap_or(0)
            .max(thread.created_at)
            .max(thread.last_activity_at);
        let summary = thread.summary();
        let thread = thread.clone();
        if let Some(parent) = self
            .transcript
            .iter_mut()
            .find(|e| e.id == thread.parent_entry_id)
        {
            parent.thread = Some(summary);
        }
        if let Some(events) = &self.thread_events.0 {
            events.publish(
                &thread.conversation_id,
                "",
                "thread",
                json!({"thread":thread}),
            );
        }
    }

    pub fn thread_preamble(&self, id: &str) -> Option<String> {
        let thread = self.threads.iter().find(|t| t.id == id)?;
        let parent = self
            .transcript
            .iter()
            .find(|e| e.id == thread.parent_entry_id)?;
        Some(format!(
            "This is a thread reply. Parent message from {}:\n[Thread parent]\n{}\n[/Thread parent]",
            thread.parent_sender, parent.text
        ))
    }

    pub fn search_messages(&self, query: &str, thread: Option<&str>, limit: usize) -> Vec<Value> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let needle = query.to_lowercase();
        self.transcript.iter().rev().filter(|e| matches!(e.kind.as_str(), "user" | "assistant" | "failure") && self.search_entry_visible(e, thread))
            .filter_map(|entry| {
                let mut folded = String::new();
                let mut positions = Vec::new();
                for (at, ch) in entry.text.char_indices() { for c in ch.to_lowercase() { folded.push(c); positions.extend(std::iter::repeat_n((at, at + ch.len_utf8()), c.len_utf8())); } }
                let first = folded.find(&needle)?;
                let start_match = positions[first].0;
                let end_match = positions[first + needle.len() - 1].1;
                let start = entry.text[..start_match].char_indices().rev().nth(80).map(|(i, _)| i).unwrap_or(0);
                let end = entry.text[end_match..].char_indices().nth(80).map(|(i, _)| end_match + i).unwrap_or(entry.text.len());
                let snippet = &entry.text[start..end];
                let ranges: Vec<_> = folded.match_indices(&needle).filter_map(|(at, _)| {
                    let (lo, hi) = (positions[at].0, positions[at + needle.len() - 1].1);
                    (lo >= start && hi <= end).then_some([lo.saturating_sub(start), hi - lo])
                }).collect();
                Some(json!({"entry_id":entry.id,"conversation_id":entry.conversation_id,"thread_id":thread.or(entry.thread_id.as_deref()),"sender":self.entry_sender(entry),"created_at":entry.at,"snippet":snippet,"match_ranges":ranges}))
            }).take(limit.clamp(1, 1000)).collect()
    }

    pub(crate) fn prune_threads(&mut self) {
        self.threads
            .retain(|t| self.transcript.iter().any(|e| e.id == t.parent_entry_id));
        self.task_threads.retain(|task, id| {
            let keep = self.tasks.iter().any(|t| &t.id == task)
                && self.threads.iter().any(|t| &t.id == id);
            if !keep && let Some(events) = &self.thread_events.0 {
                events.unbind_thread(task);
            }
            keep
        });
        let ids: Vec<_> = self.threads.iter().map(|t| t.id.clone()).collect();
        for id in ids {
            self.refresh_thread(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_identical_entries_receive_distinct_reproducible_ids() {
        let mut old = ProductState {
            transcript: vec![
                TranscriptEntry {
                    conversation_id: "chat".into(),
                    kind: "user".into(),
                    text: "hello".into(),
                    at: 1,
                    ..Default::default()
                };
                2
            ],
            ..Default::default()
        };
        let root =
            std::env::temp_dir().join(format!("marble-legacy-entry-ids-{}", std::process::id()));
        let store = crate::StateStore::in_data_dir(&root);
        store.save(&old).unwrap();
        assert_eq!(
            store.load().unwrap().transcript,
            store.load().unwrap().transcript
        );
        let mut again = old.clone();
        old.ensure_entry_ids();
        again.ensure_entry_ids();
        assert_eq!(old.transcript, again.transcript);
        assert_ne!(old.transcript[0].id, old.transcript[1].id);
        let encoded = serde_json::to_string(&old).unwrap();
        let mut restored: ProductState = serde_json::from_str(&encoded).unwrap();
        restored.ensure_entry_ids();
        assert_eq!(old.transcript, restored.transcript);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn thread_records_are_idempotent_durable_and_track_only_message_replies() {
        let mut state = ProductState::default();
        let events = crate::EventBus::default();
        state.thread_events.0 = Some(events.clone());
        state.push_transcript(TranscriptEntry {
            conversation_id: "chat".into(),
            kind: "assistant".into(),
            text: "Parent".into(),
            at: 1,
            ..Default::default()
        });
        let parent = state.transcript[0].id.clone();
        assert!(state.create_thread("wrong", &parent).is_none());
        let thread = state.create_thread("chat", &parent).unwrap();
        assert_eq!(thread.participants, ["Marble"]);
        assert_eq!(state.create_thread("chat", &parent).unwrap(), thread);
        state.bind_thread("task", Some(&thread.id));
        for kind in ["user", "tool", "reasoning", "assistant", "failure"] {
            state.push_transcript(TranscriptEntry {
                conversation_id: "chat".into(),
                task_id: "task".into(),
                kind: kind.into(),
                text: "reply".into(),
                at: 2,
                ..Default::default()
            });
        }
        assert_eq!(state.threads[0].reply_count, 3);
        assert_eq!(state.transcript[0].thread.as_ref().unwrap().reply_count, 3);
        let page = state.thread_entries(&state.threads[0]);
        assert_eq!(page[0].id, parent);
        assert_eq!(page.len(), 6);
        state.threads[0].created_at = 1;
        state.threads[0].last_activity_at = 2;
        state.prune_threads();
        assert_eq!(state.threads[0].last_activity_at, 2);
        assert!(events.since(0).iter().all(|e| e.kind == "thread"));
        let restored: ProductState = serde_json::from_value(state.encode()).unwrap();
        assert_eq!(restored.threads, state.threads);
        assert_eq!(restored.task_threads, state.task_threads);
    }

    #[test]
    fn search_prefers_main_mirrors_but_keeps_scoped_parents_and_distinct_messages() {
        let mut state = ProductState::default();
        for kind in ["user", "assistant", "failure"] {
            for conversation in ["private", "everyone"] {
                state.push_transcript(TranscriptEntry {
                    conversation_id: conversation.into(),
                    kind: kind.into(),
                    task_id: format!("task-{kind}"),
                    text: "needle".into(),
                    ..Default::default()
                });
            }
        }
        let parent = state.transcript[0].id.clone();
        let thread = state.create_thread("private", &parent).unwrap();
        let hits = state.search_messages("needle", None, 30);
        assert_eq!(hits.len(), 3);
        assert!(hits.iter().all(|h| h["conversation_id"] == "everyone"));
        let scoped = state.search_messages("needle", Some(&thread.id), 30);
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0]["entry_id"], parent);
        assert_eq!(state.thread_entries(&thread).len(), 1);
        for _ in 0..2 {
            state.push_transcript(TranscriptEntry {
                conversation_id: "private".into(),
                kind: "user".into(),
                text: "separate".into(),
                ..Default::default()
            });
        }
        assert_eq!(state.search_messages("separate", None, 30).len(), 2);
        state.push_transcript(TranscriptEntry {
            conversation_id: "private".into(),
            kind: "assistant".into(),
            text: "orphan".into(),
            thread_id: Some("deleted".into()),
            ..Default::default()
        });
        assert!(state.search_messages("orphan", None, 30).is_empty());
        assert!(
            state
                .search_messages("orphan", Some("deleted"), 30)
                .is_empty()
        );
    }

    #[test]
    fn search_uses_utf8_ranges_and_original_unicode_boundaries() {
        let mut state = ProductState::default();
        state.push_transcript(TranscriptEntry {
            conversation_id: "chat".into(),
            kind: "user".into(),
            text: "parent".into(),
            ..Default::default()
        });
        let parent = state.transcript[0].id.clone();
        let thread = state.create_thread("chat", &parent).unwrap();
        for kind in ["user", "assistant", "failure", "reasoning", "tool"] {
            state.push_transcript(TranscriptEntry {
                conversation_id: "chat".into(),
                kind: kind.into(),
                text: format!("{}😀 CAFÉ {}", "α".repeat(150), "ω".repeat(150)),
                thread_id: Some(thread.id.clone()),
                ..Default::default()
            });
        }
        assert!(state.search_messages("", None, 30).is_empty());
        assert!(state.search_messages("absent", None, 30).is_empty());
        assert!(state.search_messages("café", Some("other"), 30).is_empty());
        let hits = state.search_messages("café", Some(&thread.id), 30);
        assert_eq!(hits.len(), 3);
        for hit in hits {
            let snippet = hit["snippet"].as_str().unwrap();
            let range = &hit["match_ranges"][0];
            let start = range[0].as_u64().unwrap() as usize;
            let length = range[1].as_u64().unwrap() as usize;
            assert_eq!(&snippet[start..start + length], "CAFÉ");
            assert_eq!(length, 5);
            assert!(start > snippet[..start].chars().count());
            assert!(snippet.chars().count() < 200);
        }
        assert_eq!(state.search_messages("café", None, 1).len(), 1);
        state.push_transcript(TranscriptEntry {
            kind: "user".into(),
            text: "İ".into(),
            ..Default::default()
        });
        let hit = &state.search_messages("i\u{307}", None, 1)[0];
        assert_eq!(hit["match_ranges"], json!([[0, 2]]));
    }
}
