//! The shared chat plane where the person talks to all their marbles at once.
//!
//! `everyone` is a product-level conversation, not an engine one: a message is
//! mirrored into the timeline, and each chosen marble answers in an engine
//! session scoped to the thread. Replies carry the speaker's name.

use crate::state::{HandoffEntry, ProductState, TranscriptEntry};

/// The fixed conversation id the shared plane keeps in the document.
pub const EVERYONE_CONVERSATION: &str = "everyone";

/// Speaker ids used in mirrored entries.
pub const USER_SPEAKER: &str = "user";
pub const DEFAULT_SPEAKER: &str = "marble";

pub(crate) fn reserve(
    state: &mut ProductState,
    group: &str,
    source: &str,
    speaker: &str,
    name: &str,
    thread: Option<&str>,
) {
    state.everyone_pending.insert(source.into(), speaker.into());
    state
        .everyone_orders
        .entry(group.into())
        .or_default()
        .push_back(source.into());
    state.push_transcript(TranscriptEntry {
        conversation_id: EVERYONE_CONVERSATION.into(),
        task_id: source.into(),
        kind: "pending".into(),
        speaker: speaker.into(),
        speaker_name: name.into(),
        thread_id: thread.map(str::to_string),
        at: crate::worker::now_millis(),
        ..Default::default()
    });
}

pub(crate) fn rekey(state: &mut ProductState, source: &str, target: &str) {
    if let Some(speaker) = state.everyone_pending.remove(source) {
        state.everyone_pending.insert(target.into(), speaker);
    }
    for order in state.everyone_orders.values_mut() {
        for key in order {
            if key == source {
                *key = target.into();
            }
        }
    }
    for entry in &mut state.transcript {
        if entry.conversation_id == EVERYONE_CONVERSATION
            && entry.kind == "pending"
            && entry.task_id == source
        {
            entry.task_id = target.strip_prefix("task:").unwrap_or(target).into();
        }
    }
}

const SESSION_PREFIX: &str = "everyone-session-";

pub(crate) fn session_id(speaker: &str, thread: Option<&str>) -> String {
    format!(
        "{SESSION_PREFIX}{}{speaker}",
        thread.map_or_else(|| "lobby__".into(), |id| format!("{id}__"))
    )
}

pub(crate) fn is_session(id: &str) -> bool {
    id.starts_with(SESSION_PREFIX)
}

pub(crate) fn is_group_session(id: &str) -> bool {
    id.strip_prefix(SESSION_PREFIX)
        .is_some_and(|id| id.starts_with("group-thread-"))
}

pub(crate) fn session_in_thread(id: &str, thread: &str) -> bool {
    id.strip_prefix(SESSION_PREFIX)
        .and_then(|id| id.split_once("__"))
        .is_some_and(|(scope, _)| scope == thread)
}

pub(crate) fn ensure_session(state: &mut ProductState, id: &str) {
    if !state.conversations.iter().any(|record| record.id == id) {
        let record = state.conversation_mut(id);
        record.needs_engine_session = true;
        record.lane = crate::state::ConversationRecord::UNASSIGNED_LANE;
    }
}

/// Who should answer one Everyone message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EveryoneTarget {
    /// These marble ids were named — each one answers.
    Named(Vec<String>),
    /// No known mention or `@everyone` — the whole roster answers.
    All,
}

/// Parse `@name` mentions against the roster, whole-word and case-insensitive.
///
/// `@everyone` names the whole roster; a named marble that is not on the
/// roster is treated as ordinary text rather than a failed mention.
pub fn plan(state: &ProductState, text: &str) -> EveryoneTarget {
    let lowered = text.to_lowercase();
    let default = state.prism.default_marble();
    let roster = std::iter::once(&default).chain(
        state
            .prism
            .marbles
            .iter()
            .filter(|agent| agent.in_everyone()),
    );
    let mut mentions: Vec<_> = roster
        .filter_map(|marble| {
            let at = mention_position(&lowered, &marble.name.to_lowercase());
            let at = if marble.id == DEFAULT_SPEAKER {
                at.into_iter()
                    .chain(mention_position(&lowered, "marble"))
                    .min()
            } else {
                at
            };
            at.map(|at| (at, marble.id.clone()))
        })
        .collect();
    mentions.sort_by_key(|(at, _)| *at);
    let mut named: Vec<String> = mentions.into_iter().map(|(_, id)| id).collect();
    if mentioned(&lowered, "everyone") {
        for id in std::iter::once(DEFAULT_SPEAKER.to_string()).chain(
            state
                .prism
                .marbles
                .iter()
                .filter(|agent| agent.in_everyone())
                .map(|m| m.id.clone()),
        ) {
            if !named.contains(&id) {
                named.push(id);
            }
        }
    }
    if named.is_empty() {
        EveryoneTarget::All
    } else {
        EveryoneTarget::Named(named)
    }
}

/// Whether `@word` appears in `text` bounded by non-word characters, so `@Ada`
/// matches but `radar` and `@adaptive` do not.
fn mentioned(text: &str, word: &str) -> bool {
    mention_position(text, word).is_some()
}

pub(crate) fn mention_position(text: &str, word: &str) -> Option<usize> {
    let needle = format!("@{word}");
    let mut rest = text;
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let bounded = after
            .chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && next != '-' && next != '_');
        if bounded {
            return Some(text.len() - rest.len() + at);
        }
        rest = after;
    }
    None
}

pub(crate) fn addressing(state: &ProductState, text: &str, owner: &str) -> String {
    let ids = match plan(state, text) {
        EveryoneTarget::Named(ids) => ids,
        EveryoneTarget::All => std::iter::once(DEFAULT_SPEAKER.to_string())
            .chain(state.prism.marbles.iter().map(|marble| marble.id.clone()))
            .collect(),
    };
    let name = |id: &str| {
        if id == DEFAULT_SPEAKER {
            state.prism.default_marble().name
        } else {
            state
                .prism
                .get(id)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| id.to_string())
        }
    };
    let names = ids
        .iter()
        .enumerate()
        .map(|(i, id)| format!("{}{}", name(id), if i == 0 { " (lead)" } else { "" }))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "This message is addressed to: {names}. You are {}.",
        name(owner)
    )
}

/// The conversation record the shared plane needs in the document.
pub fn ensure_conversation(state: &mut ProductState) {
    let conversation = state.conversation_mut(EVERYONE_CONVERSATION);
    if conversation.title.is_empty() {
        conversation.title = "Everyone".into();
    }
    conversation.updated_at = crate::worker::now_millis();
}

/// Append one entry to the shared timeline, stamped with who said it.
pub fn mirror(
    state: &mut ProductState,
    speaker: &str,
    speaker_name: &str,
    kind: &str,
    text: impl Into<String>,
    task_id: &str,
    handoff: Option<HandoffEntry>,
) {
    ensure_conversation(state);
    let failure = if kind == "failure" {
        state
            .transcript
            .iter()
            .rev()
            .find(|entry| entry.task_id == task_id && entry.kind == "failure")
            .and_then(|entry| entry.failure.clone())
            .or_else(|| {
                Some(crate::failure::FailureInfo::of(
                    "turn interrupted before settlement",
                ))
            })
    } else {
        None
    };
    state.push_transcript(TranscriptEntry {
        conversation_id: EVERYONE_CONVERSATION.to_string(),
        kind: kind.to_string(),
        text: text.into(),
        task_id: task_id.to_string(),
        at: crate::worker::now_millis(),
        speaker: speaker.to_string(),
        speaker_name: speaker_name.to_string(),
        handoff,
        failure,
        ..Default::default()
    });
}

/// Take the pending-mirror marker for a just-accepted turn, moving it from the
/// placeholder key (`queued-<request>` or `marble:<id>`) to the real task id.
/// Returns whether the turn belongs to the shared plane.
pub fn claim(state: &mut ProductState, placeholder: &str, task_id: &str) -> bool {
    if let Some(speaker) = state.everyone_pending.remove(placeholder) {
        let entry_id = state.everyone_orders.iter().find_map(|(entry_id, order)| {
            order
                .iter()
                .any(|key| key == placeholder)
                .then(|| entry_id.clone())
        });
        for order in state.everyone_orders.values_mut() {
            for key in order.iter_mut() {
                if key == placeholder {
                    *key = format!("task:{task_id}");
                }
            }
        }
        state
            .everyone_pending
            .insert(format!("task:{task_id}"), speaker);
        for entry in &mut state.transcript {
            if entry.conversation_id == EVERYONE_CONVERSATION
                && entry.kind == "pending"
                && entry.task_id == placeholder
            {
                entry.task_id = task_id.into();
            }
        }
        // Stamp the person's line with the task now answering it, so
        // deleting that turn's chat takes the line it triggered too.
        if let Some(entry) = state.transcript.iter_mut().find(|entry| {
            entry.conversation_id == EVERYONE_CONVERSATION
                && entry.kind == "user"
                && ((!entry.task_id.is_empty()
                    && (entry.task_id == placeholder || entry.task_id == task_id))
                    || (entry.task_id.is_empty() && entry_id.as_ref() == Some(&entry.id)))
        }) {
            entry.task_id = task_id.to_string();
        }
        true
    } else {
        false
    }
}

/// Whether a settling turn was answering an Everyone message, and who spoke.
pub fn settle_mirror(state: &mut ProductState, task_id: &str) -> Option<String> {
    state.everyone_pending.remove(&format!("task:{task_id}"))
}

/// Mirror one settled turn's outcome into the shared timeline.
pub fn mirror_settlement(state: &mut ProductState, task_id: &str, reply: &str, status: &str) {
    let Some(speaker) = settle_mirror(state, task_id) else {
        return;
    };
    let name = if speaker == DEFAULT_SPEAKER {
        "Marble".to_string()
    } else {
        state
            .prism
            .get(&speaker)
            .map(|marble| marble.name.clone())
            .unwrap_or_else(|| speaker.clone())
    };
    let (kind, text) = match status {
        "failed" => ("failure", crate::failure::MESSAGE.to_string()),
        "completed" => ("assistant", reply.to_string()),
        _ => ("system", format!("{name} could not finish: {}", status)),
    };
    if !text.trim().is_empty() {
        mirror(state, &speaker, &name, kind, text, task_id, None);
    }
    let reasoning: Vec<_> = state
        .transcript
        .iter()
        .filter(|entry| {
            entry.task_id == task_id
                && entry.kind == "reasoning"
                && entry.conversation_id != EVERYONE_CONVERSATION
                && !entry.text.trim().is_empty()
        })
        .cloned()
        .collect();
    for mut entry in reasoning {
        entry.id = format!("{}-everyone", entry.id);
        entry.conversation_id = EVERYONE_CONVERSATION.into();
        entry.speaker = speaker.clone();
        entry.speaker_name = name.clone();
        let id = entry.id.clone();
        state.push_transcript(entry);
        let reasoning_index = state
            .transcript
            .iter()
            .position(|entry| entry.id == id)
            .unwrap();
        if let Some(reply_index) = state.transcript.iter().position(|entry| {
            entry.conversation_id == EVERYONE_CONVERSATION
                && entry.task_id == task_id
                && matches!(
                    entry.kind.as_str(),
                    "pending" | "assistant" | "failure" | "system"
                )
        }) && reasoning_index > reply_index
        {
            let entry = state.transcript.remove(reasoning_index);
            state.transcript.insert(reply_index, entry);
        }
    }
    state.flush_everyone();
}

/// One plain sentence for a handoff entry, as it appears in chat.
pub fn handoff_line(entry: &HandoffEntry) -> String {
    match entry.status.as_str() {
        "asked" => format!(
            "{} asked {} to {}.",
            entry.from_name, entry.to_name, entry.instruction
        ),
        "finished" => format!("{} finished: {}", entry.to_name, entry.summary),
        "refused" => format!("{} couldn't take it: {}.", entry.to_name, entry.summary),
        _ => format!("{} couldn't finish: {}.", entry.to_name, entry.summary),
    }
}

/// Record a handoff into the asker's own transcript and the shared timeline.
pub fn record_handoff(
    state: &mut ProductState,
    asker_conversation: &str,
    task_id: &str,
    entry: &HandoffEntry,
) {
    let text = handoff_line(entry);
    if !asker_conversation.is_empty() && asker_conversation != EVERYONE_CONVERSATION {
        state.push_transcript(TranscriptEntry {
            conversation_id: asker_conversation.to_string(),
            kind: "handoff".to_string(),
            text: text.clone(),
            task_id: task_id.to_string(),
            at: crate::worker::now_millis(),
            speaker: entry.from.clone(),
            speaker_name: entry.from_name.clone(),
            handoff: Some(entry.clone()),
            ..Default::default()
        });
    }
    mirror(
        state,
        &entry.from,
        &entry.from_name,
        "handoff",
        text,
        task_id,
        Some(entry.clone()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prism::PrismState;

    fn roster() -> ProductState {
        let mut state = ProductState::default();
        let mut prism = PrismState::default();
        prism.add("Ada", "writer", "star", 40, "", "").unwrap();
        prism.add("Ben", "checker", "hex", 210, "", "").unwrap();
        state.prism = prism;
        state
    }

    #[test]
    fn mentions_route_to_the_named_marbles() {
        let state = roster();
        assert_eq!(plan(&state, "hi there"), EveryoneTarget::All);
        assert_eq!(
            plan(&state, "@everyone ship it"),
            EveryoneTarget::Named(vec![
                "marble".into(),
                state.prism.marbles[0].id.clone(),
                state.prism.marbles[1].id.clone()
            ])
        );
        match plan(&state, "@ada please draft this") {
            EveryoneTarget::Named(ids) => assert_eq!(ids.len(), 1),
            other => panic!("expected a named target: {other:?}"),
        }
        match plan(&state, "@ADA and @ben check this") {
            EveryoneTarget::Named(ids) => assert_eq!(ids.len(), 2),
            other => panic!("expected two named targets: {other:?}"),
        }
        // A mistyped or partial name is plain text, not a mention.
        assert_eq!(plan(&state, "@adaptive thing"), EveryoneTarget::All);
        assert_eq!(plan(&state, "asked by radar"), EveryoneTarget::All);
    }

    #[test]
    fn default_and_named_mentions_keep_the_explicit_leader() {
        let state = roster();
        let ada = state.prism.marbles[0].id.clone();
        let ben = state.prism.marbles[1].id.clone();
        assert_eq!(
            plan(&state, "@Ada hi"),
            EveryoneTarget::Named(vec![ada.clone()])
        );
        assert_eq!(
            plan(&state, "@Marble take the lead @everyone"),
            EveryoneTarget::Named(vec!["marble".into(), ada.clone(), ben.clone()])
        );
        assert_eq!(
            plan(&state, "@mArBlE and @Ada help"),
            EveryoneTarget::Named(vec!["marble".into(), ada.clone()])
        );
        assert_eq!(
            plan(&state, "@Ben @everyone @Ada @Ben"),
            EveryoneTarget::Named(vec![ben, ada.clone(), "marble".into()])
        );
        assert_eq!(plan(&state, ""), EveryoneTarget::All);
        assert_eq!(plan(&state, "hello"), EveryoneTarget::All);
        assert!(
            addressing(&state, "@Marble @everyone", &ada)
                .starts_with("This message is addressed to: Marble (lead), Ada, Ben. You are Ada.")
        );
    }

    #[test]
    fn everyone_replies_are_mirrored_in_mention_order_not_completion_order() {
        let mut state = roster();
        state.everyone_orders.insert(
            "message".into(),
            ["marble:lead".into(), "marble:next".into()].into(),
        );
        state
            .everyone_pending
            .insert("marble:lead".into(), "marble".into());
        state
            .everyone_pending
            .insert("marble:next".into(), state.prism.marbles[0].id.clone());
        claim(&mut state, "marble:lead", "first");
        claim(&mut state, "marble:next", "second");
        mirror_settlement(&mut state, "second", "Second answer", "completed");
        assert!(state.transcript.is_empty());
        mirror_settlement(&mut state, "first", "Lead answer", "completed");
        assert_eq!(
            state
                .transcript
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            ["Lead answer", "Second answer"]
        );
        assert!(state.everyone_orders.is_empty());
        assert!(state.everyone_ready.is_empty());
    }

    #[test]
    fn cancelled_lead_releases_buffered_replies() {
        for status in ["cancelled", "completed"] {
            let mut state = roster();
            state.everyone_orders.insert(
                "message".into(),
                ["task:lead".into(), "task:next".into(), "task:last".into()].into(),
            );
            state
                .everyone_pending
                .insert("task:lead".into(), "marble".into());
            state
                .everyone_pending
                .insert("task:next".into(), state.prism.marbles[0].id.clone());
            state
                .everyone_pending
                .insert("task:last".into(), state.prism.marbles[1].id.clone());
            mirror_settlement(&mut state, "last", "Last answer", "completed");
            mirror_settlement(&mut state, "next", "Next answer", "completed");
            assert!(state.transcript.is_empty());
            mirror_settlement(&mut state, "lead", "", status);
            let answers: Vec<_> = state
                .transcript
                .iter()
                .filter(|e| e.kind == "assistant")
                .map(|e| e.text.as_str())
                .collect();
            assert_eq!(answers, ["Next answer", "Last answer"], "{status}");
            if status == "cancelled" {
                assert_eq!(state.transcript[0].kind, "system");
            }
            assert!(state.everyone_pending.is_empty());
            assert!(state.everyone_orders.is_empty());
            assert!(state.everyone_ready.is_empty());
        }
    }

    #[test]
    fn mirrored_entries_carry_the_speaker() {
        let mut state = roster();
        let marble = state.prism.marbles[0].id.clone();
        mirror(&mut state, &marble, "Ada", "assistant", "on it", "t1", None);
        let entry = state.transcript.last().unwrap();
        assert_eq!(entry.conversation_id, EVERYONE_CONVERSATION);
        assert_eq!(entry.speaker, marble);
        assert_eq!(entry.speaker_name, "Ada");
        assert_eq!(state.conversations[0].id, EVERYONE_CONVERSATION);
    }

    #[test]
    fn a_settled_everyone_turn_mirrors_once() {
        let mut state = roster();
        let marble = state.prism.marbles[0].id.clone();
        state
            .everyone_pending
            .insert(format!("marble:{marble}"), marble.clone());
        assert!(claim(&mut state, &format!("marble:{marble}"), "task-1"));
        mirror_settlement(&mut state, "task-1", "done it", "completed");
        assert_eq!(state.transcript.len(), 1);
        assert_eq!(state.transcript[0].speaker_name, "Ada");
        assert!(
            settle_mirror(&mut state, "task-1").is_none(),
            "the mirror is consumed"
        );
    }
}

#[cfg(test)]
mod claim_tests {
    use super::*;

    #[test]
    fn a_claimed_turn_stamps_the_persons_line() {
        let mut state = ProductState::default();
        mirror(&mut state, USER_SPEAKER, "You", "user", "@ben hi", "", None);
        state.everyone_orders.insert(
            state.transcript[0].id.clone(),
            ["marble:m-ben".into(), "marble:m-ada".into()].into(),
        );
        state
            .everyone_pending
            .insert("marble:m-ben".into(), "m-ben".into());
        assert!(claim(&mut state, "marble:m-ben", "task-9"));
        let line = state
            .transcript
            .iter()
            .find(|entry| entry.kind == "user")
            .unwrap();
        assert_eq!(line.task_id, "task-9");
        // A second marble answering the same line does not re-stamp it.
        state
            .everyone_pending
            .insert("marble:m-ada".into(), "m-ada".into());
        assert!(claim(&mut state, "marble:m-ada", "task-10"));
        let line = state
            .transcript
            .iter()
            .find(|entry| entry.kind == "user")
            .unwrap();
        assert_eq!(line.task_id, "task-9");
    }

    #[test]
    fn claim_does_not_stamp_unrelated_messages_when_the_source_is_missing() {
        let mut state = ProductState::default();
        mirror(
            &mut state,
            USER_SPEAKER,
            "You",
            "user",
            "unrelated",
            "",
            None,
        );
        state
            .everyone_orders
            .insert("removed-entry".into(), ["queued-1".into()].into());
        state
            .everyone_pending
            .insert("queued-1".into(), DEFAULT_SPEAKER.into());
        assert!(claim(&mut state, "queued-1", "task-1"));
        assert!(state.transcript[0].task_id.is_empty());
        assert!(!claim(&mut state, "queued-1", "task-1"));
    }

    #[test]
    fn claim_matches_an_existing_task_link_without_a_source_entry_id() {
        for linked in ["queued-1", "task-1"] {
            let mut state = ProductState::default();
            mirror(
                &mut state,
                USER_SPEAKER,
                "You",
                "user",
                "same text",
                linked,
                None,
            );
            mirror(
                &mut state,
                USER_SPEAKER,
                "You",
                "user",
                "same text",
                "",
                None,
            );
            state
                .everyone_pending
                .insert("queued-1".into(), DEFAULT_SPEAKER.into());
            assert!(claim(&mut state, "queued-1", "task-1"));
            assert_eq!(state.transcript[0].task_id, "task-1");
            assert!(state.transcript[1].task_id.is_empty());
        }
    }
}
