//! The shared chat plane where the person talks to all their marbles at once.
//!
//! `everyone` is a product-level conversation, not an engine one: a message is
//! mirrored into the timeline, the marbles it names (or the default assistant
//! when it names nobody) answer in their own engine conversations, and each
//! reply is mirrored back labelled with who said it.

use crate::state::{HandoffEntry, ProductState, TranscriptEntry};

/// The fixed conversation id the shared plane keeps in the document.
pub const EVERYONE_CONVERSATION: &str = "everyone";

/// Speaker ids used in mirrored entries.
pub const USER_SPEAKER: &str = "user";
pub const DEFAULT_SPEAKER: &str = "marble";

/// Who should answer one Everyone message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EveryoneTarget {
    /// Nobody was named: the default assistant answers.
    Default,
    /// These marble ids were named — each one answers.
    Named(Vec<String>),
    /// `@everyone` — the whole roster answers.
    All,
}

/// Parse `@name` mentions against the roster, whole-word and case-insensitive.
///
/// `@everyone` names the whole roster; a named marble that is not on the
/// roster is treated as ordinary text rather than a failed mention, so
/// `@someone` the person mistyped does not silently become a dispatch.
pub fn plan(state: &ProductState, text: &str) -> EveryoneTarget {
    let lowered = text.to_lowercase();
    let default = state.prism.default_marble();
    let roster = std::iter::once(&default).chain(state.prism.marbles.iter());
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
        for id in std::iter::once(DEFAULT_SPEAKER.to_string())
            .chain(state.prism.marbles.iter().map(|m| m.id.clone()))
        {
            if !named.contains(&id) {
                named.push(id);
            }
        }
    }
    if named.is_empty() {
        EveryoneTarget::Default
    } else {
        EveryoneTarget::Named(named)
    }
}

/// Whether `@word` appears in `text` bounded by non-word characters, so `@Ada`
/// matches but `radar` and `@adaptive` do not.
fn mentioned(text: &str, word: &str) -> bool {
    mention_position(text, word).is_some()
}

fn mention_position(text: &str, word: &str) -> Option<usize> {
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
        _ => vec![owner.to_string()],
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
    state.push_transcript(TranscriptEntry {
        conversation_id: EVERYONE_CONVERSATION.to_string(),
        kind: kind.to_string(),
        text: text.into(),
        task_id: task_id.to_string(),
        at: crate::worker::now_millis(),
        speaker: speaker.to_string(),
        speaker_name: speaker_name.to_string(),
        handoff,
    });
}

/// Take the pending-mirror marker for a just-accepted turn, moving it from the
/// placeholder key (`queued-<request>` or `marble:<id>`) to the real task id.
/// Returns whether the turn belongs to the shared plane.
pub fn claim(state: &mut ProductState, placeholder: &str, task_id: &str) -> bool {
    if let Some(speaker) = state.everyone_pending.remove(placeholder) {
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
        // Stamp the person's line with the task now answering it, so
        // deleting that turn's chat takes the line it triggered too.
        if let Some(entry) = state.transcript.iter_mut().rev().find(|entry| {
            entry.conversation_id == EVERYONE_CONVERSATION
                && entry.kind == "user"
                && entry.task_id.is_empty()
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
        "completed" => ("assistant", reply.to_string()),
        _ => ("system", format!("{name} could not finish: {}", status)),
    };
    if !text.trim().is_empty() {
        mirror(state, &speaker, &name, kind, text, task_id, None);
    }
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
        assert_eq!(plan(&state, "hi there"), EveryoneTarget::Default);
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
        assert_eq!(plan(&state, "@adaptive thing"), EveryoneTarget::Default);
        assert_eq!(plan(&state, "asked by radar"), EveryoneTarget::Default);
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
        assert_eq!(plan(&state, ""), EveryoneTarget::Default);
        assert_eq!(plan(&state, "hello"), EveryoneTarget::Default);
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
}
