//! Durable memory the assistant carries between conversations.
//!
//! Memory is deliberately boring and legible: a short list of statements the
//! person asked Marble to keep, plus suggestions Marble offers when it notices
//! something worth keeping. Nothing is remembered silently — an explicit
//! "remember this" command writes a fact, and a noticed fact waits as a
//! suggestion until the person saves or dismisses it.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Facts one instance keeps.
pub const MEMORY_LIMIT: usize = 200;
/// Suggestions waiting for a decision.
pub const SUGGESTION_LIMIT: usize = 8;
/// Longest fact Marble accepts from one command.
pub const MAX_FACT_CHARS: usize = 280;

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn next_id(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{:x}-{sequence:x}", now_millis())
}

/// Collapse whitespace so two phrasings of the same fact compare equal.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn bounded(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut short: String = text.chars().take(limit).collect();
    short.push('…');
    short
}

/// One remembered statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFact {
    pub id: String,
    pub text: String,
    /// `fact`, `preference`, or `role`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    /// How many turns have carried this fact.
    #[serde(default)]
    pub uses: u64,
    /// Written by an explicit command rather than accepted from a suggestion.
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub conversation_id: String,
    /// Which marble keeps this fact. Empty means shared with every marble;
    /// `"marble"` is the default assistant's own memory.
    #[serde(default)]
    pub owner: String,
}

/// Something Marble noticed and wants permission to keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySuggestion {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub conversation_id: String,
}

/// Durable memory document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryState {
    #[serde(default)]
    pub facts: Vec<MemoryFact>,
    #[serde(default)]
    pub suggestions: Vec<MemorySuggestion>,
}

impl MemoryState {
    /// Whether an equivalent fact is already kept.
    pub fn knows(&self, text: &str) -> bool {
        let candidate = normalize(text).to_ascii_lowercase();
        !candidate.is_empty()
            && self
                .facts
                .iter()
                .any(|fact| normalize(&fact.text).to_ascii_lowercase() == candidate)
    }

    /// Keep one statement, reusing an equivalent fact instead of duplicating it.
    pub fn remember(
        &mut self,
        text: &str,
        kind: &str,
        conversation_id: &str,
        explicit: bool,
        owner: &str,
    ) -> Result<MemoryFact, String> {
        let text = normalize(text);
        if text.is_empty() {
            return Err("there is nothing to remember".into());
        }
        if text.chars().count() > MAX_FACT_CHARS {
            return Err(format!(
                "a memory is at most {MAX_FACT_CHARS} characters; keep it to one statement"
            ));
        }
        let candidate = text.to_ascii_lowercase();
        if let Some(existing) = self.facts.iter_mut().find(|fact| {
            fact.owner == owner && normalize(&fact.text).to_ascii_lowercase() == candidate
        }) {
            existing.updated_at = now_millis();
            existing.explicit = existing.explicit || explicit;
            return Ok(existing.clone());
        }
        if self.facts.len() >= MEMORY_LIMIT {
            return Err(format!(
                "marble keeps at most {MEMORY_LIMIT} memories; forget one first"
            ));
        }
        let now = now_millis();
        let fact = MemoryFact {
            id: next_id("memory"),
            text,
            kind: normalize_kind(kind).to_string(),
            created_at: now,
            updated_at: now,
            uses: 0,
            explicit,
            conversation_id: conversation_id.to_string(),
            owner: owner.to_string(),
        };
        self.facts.push(fact.clone());
        self.suggestions
            .retain(|suggestion| normalize(&suggestion.text).to_ascii_lowercase() != candidate);
        Ok(fact)
    }

    /// Correct one fact in place, keeping its id and history.
    ///
    /// A remembered detail changes (a new timezone, a different editor), and
    /// forgetting then re-adding it would lose the fact's id and use count. The
    /// edit is explicit, so it is marked as such.
    pub fn update(&mut self, id: &str, text: &str, kind: &str) -> Result<MemoryFact, String> {
        let text = normalize(text);
        if text.is_empty() {
            return Err("there is nothing to remember".into());
        }
        if text.chars().count() > MAX_FACT_CHARS {
            return Err(format!(
                "a memory is at most {MAX_FACT_CHARS} characters; keep it to one statement"
            ));
        }
        let candidate = text.to_ascii_lowercase();
        let owner = &self
            .facts
            .iter()
            .find(|fact| fact.id == id)
            .ok_or_else(|| "unknown memory".to_string())?
            .owner;
        if self.facts.iter().any(|fact| {
            fact.id != id
                && &fact.owner == owner
                && normalize(&fact.text).to_ascii_lowercase() == candidate
        }) {
            return Err("an equivalent memory is already kept".into());
        }
        let fact = self
            .facts
            .iter_mut()
            .find(|fact| fact.id == id)
            .ok_or_else(|| "unknown memory".to_string())?;
        fact.text = text;
        if !kind.trim().is_empty() {
            fact.kind = normalize_kind(kind).to_string();
        }
        fact.updated_at = now_millis();
        fact.explicit = true;
        Ok(fact.clone())
    }

    /// Forget one fact by id.
    pub fn forget(&mut self, id: &str) -> bool {
        let before = self.facts.len();
        self.facts.retain(|fact| fact.id != id);
        self.facts.len() != before
    }

    /// Forget every fact whose text contains `query`.
    pub fn forget_matching(&mut self, query: &str) -> usize {
        let needle = normalize(query).to_ascii_lowercase();
        if needle.is_empty() {
            return 0;
        }
        let before = self.facts.len();
        self.facts
            .retain(|fact| !normalize(&fact.text).to_ascii_lowercase().contains(&needle));
        before - self.facts.len()
    }

    /// Actually write to memory on disk: called by the store after a change.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.suggestions.is_empty()
    }

    /// Offer one statement for confirmation. Returns `None` when it is already
    /// known, already suggested, or not worth keeping.
    pub fn suggest(
        &mut self,
        text: &str,
        kind: &str,
        reason: &str,
        conversation_id: &str,
    ) -> Option<MemorySuggestion> {
        let text = normalize(text);
        if text.is_empty() || text.chars().count() > MAX_FACT_CHARS {
            return None;
        }
        let candidate = text.to_ascii_lowercase();
        if self.knows(&text)
            || self
                .suggestions
                .iter()
                .any(|suggestion| normalize(&suggestion.text).to_ascii_lowercase() == candidate)
        {
            return None;
        }
        while self.suggestions.len() >= SUGGESTION_LIMIT {
            self.suggestions.remove(0);
        }
        let suggestion = MemorySuggestion {
            id: next_id("suggested"),
            text,
            kind: normalize_kind(kind).to_string(),
            reason: reason.to_string(),
            created_at: now_millis(),
            conversation_id: conversation_id.to_string(),
        };
        self.suggestions.push(suggestion.clone());
        Some(suggestion)
    }

    /// Save one suggestion as a fact. `owner` is the marble whose chat the
    /// suggestion came from, or `"marble"` for the default assistant.
    pub fn accept(&mut self, id: &str, owner: &str) -> Option<MemoryFact> {
        let suggestion = self
            .suggestions
            .iter()
            .find(|suggestion| suggestion.id == id)
            .cloned()?;
        self.suggestions.retain(|item| item.id != id);
        self.remember(
            &suggestion.text,
            &suggestion.kind,
            &suggestion.conversation_id,
            false,
            owner,
        )
        .ok()
    }

    /// Drop one suggestion without keeping it.
    pub fn dismiss(&mut self, id: &str) -> bool {
        let before = self.suggestions.len();
        self.suggestions.retain(|suggestion| suggestion.id != id);
        self.suggestions.len() != before
    }

    /// Context block prepended to a turn so the assistant actually knows what
    /// it was told to remember. One speaker sees the shared facts plus its own:
    /// `"marble"` for the default assistant, a marble id for a named one.
    pub fn context_block_for(&self, owner: &str) -> Option<String> {
        let facts: Vec<&MemoryFact> = self
            .facts
            .iter()
            .filter(|fact| fact.owner.is_empty() || fact.owner == owner)
            .collect();
        if facts.is_empty() {
            return None;
        }
        let mut lines = Vec::new();
        let mut budget = 2000usize;
        for fact in facts {
            let line = format!("- {}: {}", fact.kind, fact.text);
            if line.len() > budget {
                break;
            }
            budget -= line.len();
            lines.push(line);
        }
        if lines.is_empty() {
            return None;
        }
        Some(format!(
            "What the person asked you to remember about them (durable memory, keep using it):\n{}",
            lines.join("\n")
        ))
    }

    /// Count one use of every fact carried into a turn — the shared ones plus
    /// the speaker's own.
    pub fn note_use(&mut self, owner: &str) {
        for fact in self.facts.iter_mut() {
            if fact.owner.is_empty() || fact.owner == owner {
                fact.uses = fact.uses.saturating_add(1);
            }
        }
    }

    /// Product document with derived counts.
    pub fn document(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Value::Object(ref mut map) = value {
            map.insert("count".into(), json!(self.facts.len()));
            map.insert("limit".into(), json!(MEMORY_LIMIT));
        }
        value
    }
}

fn normalize_kind(kind: &str) -> &'static str {
    match kind.trim().to_ascii_lowercase().as_str() {
        "preference" => "preference",
        "role" => "role",
        _ => "fact",
    }
}

/// One explicit memory instruction a person typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCommand {
    Remember { text: String, kind: String },
    Forget { text: String },
}

/// Markers that make a message a memory command rather than work. They are
/// matched only at the start of the message so ordinary prose is never hijacked.
const REMEMBER_MARKERS: [&str; 12] = [
    "remember this:",
    "remember that:",
    "remember this ",
    "remember that ",
    "update memory:",
    "update memory ",
    "save to memory:",
    "save this to memory:",
    "add to memory:",
    "add this to memory:",
    "note to self:",
    "keep in mind:",
];

const FORGET_MARKERS: [&str; 4] = ["forget that:", "forget about ", "forget:", "forget "];

/// Parse an explicit memory command out of an inbound message.
pub fn parse_command(text: &str) -> Option<MemoryCommand> {
    let trimmed = text.trim();
    let lowered = trimmed.to_ascii_lowercase();
    for marker in REMEMBER_MARKERS {
        if let Some(rest) = lowered.strip_prefix(marker) {
            let start = trimmed.len() - rest.len();
            let body = trimmed[start..].trim().trim_end_matches(['.', '!']).trim();
            if body.is_empty() {
                return None;
            }
            return Some(MemoryCommand::Remember {
                text: body.to_string(),
                kind: infer_kind(body).to_string(),
            });
        }
    }
    for marker in FORGET_MARKERS {
        if let Some(rest) = lowered.strip_prefix(marker) {
            let start = trimmed.len() - rest.len();
            let body = trimmed[start..].trim().trim_end_matches(['.', '!']).trim();
            if body.is_empty() {
                return None;
            }
            return Some(MemoryCommand::Forget {
                text: body.to_string(),
            });
        }
    }
    None
}

/// Rough kind for one statement.
pub fn infer_kind(text: &str) -> &'static str {
    let lowered = text.to_ascii_lowercase();
    const ROLE: [&str; 6] = ["my name", "call me", "i am a", "i'm a", "i work", "i live"];
    const PREFERENCE: [&str; 8] = [
        "i prefer",
        "i always",
        "i never",
        "i usually",
        "i like",
        "i don't like",
        "i'd rather",
        "i would rather",
    ];
    if ROLE.iter().any(|marker| lowered.contains(marker)) {
        "role"
    } else if PREFERENCE.iter().any(|marker| lowered.contains(marker)) {
        "preference"
    } else {
        "fact"
    }
}

/// Sentences worth offering to keep. Only durable first-person statements are
/// considered, and only one per message so a suggestion never becomes a chore.
pub fn detect(text: &str) -> Option<(String, String, String)> {
    if parse_command(text).is_some() {
        return None;
    }
    const MARKERS: [&str; 14] = [
        "i prefer",
        "i always",
        "i never",
        "i usually",
        "i work",
        "i live in",
        "i'm based",
        "i am based",
        "my name is",
        "call me",
        "my timezone",
        "i don't like",
        "i'd rather",
        "i would rather",
    ];
    for sentence in text.split(['.', '!', '?', '\n']) {
        let candidate = sentence.trim();
        if candidate.is_empty() || candidate.len() > 320 {
            continue;
        }
        let lowered = candidate.to_ascii_lowercase();
        if MARKERS.iter().any(|marker| lowered.starts_with(marker)) {
            return Some((
                bounded(candidate, MAX_FACT_CHARS),
                infer_kind(candidate).to_string(),
                "Marble noticed a durable detail in your last message.".to_string(),
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_scopes_shared_and_owned_facts_to_the_speaker() {
        let mut memory = MemoryState::default();
        memory
            .remember("shared fact", "fact", "", true, "")
            .unwrap();
        memory
            .remember("ada only", "fact", "c1", true, "marble-ada")
            .unwrap();
        memory
            .remember("default only", "fact", "c2", true, "marble")
            .unwrap();

        let ada = memory.context_block_for("marble-ada").unwrap();
        assert!(ada.contains("shared fact"));
        assert!(ada.contains("ada only"));
        assert!(!ada.contains("default only"));

        let marble = memory.context_block_for("marble").unwrap();
        assert!(marble.contains("shared fact"));
        assert!(marble.contains("default only"));
        assert!(!marble.contains("ada only"));

        let nobody = memory.context_block_for("marble-ben").unwrap();
        assert!(nobody.contains("shared fact"));
        assert!(!nobody.contains("ada only"));

        memory.note_use("marble-ada");
        assert_eq!(memory.facts[0].uses, 1, "shared fact counts");
        assert_eq!(memory.facts[1].uses, 1, "ada's own fact counts");
        assert_eq!(memory.facts[2].uses, 0, "another speaker's fact does not");
    }

    #[test]
    fn explicit_commands_are_parsed_and_ordinary_prose_is_not() {
        assert_eq!(
            parse_command("Remember this: the deploy key lives in 1Password"),
            Some(MemoryCommand::Remember {
                text: "the deploy key lives in 1Password".into(),
                kind: "fact".into()
            })
        );
        assert_eq!(
            parse_command("update memory: I prefer tabs over spaces"),
            Some(MemoryCommand::Remember {
                text: "I prefer tabs over spaces".into(),
                kind: "preference".into()
            })
        );
        assert_eq!(
            parse_command("save to memory: my name is Azhar"),
            Some(MemoryCommand::Remember {
                text: "my name is Azhar".into(),
                kind: "role".into()
            })
        );
        assert_eq!(
            parse_command("Forget: the deploy key lives in 1Password"),
            Some(MemoryCommand::Forget {
                text: "the deploy key lives in 1Password".into()
            })
        );
        assert_eq!(parse_command("remember when we shipped 0.3"), None);
        assert_eq!(parse_command("please fix the deploy script"), None);
        assert_eq!(parse_command("remember this:"), None);
    }

    #[test]
    fn remembering_is_idempotent_and_bounded() {
        let mut memory = MemoryState::default();
        let first = memory
            .remember("I prefer  tabs ", "preference", "c1", true, "")
            .unwrap();
        assert_eq!(first.text, "I prefer tabs");
        let second = memory
            .remember("i prefer tabs", "preference", "c1", false, "")
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(memory.facts.len(), 1);
        assert!(memory.remember("", "fact", "c1", true, "").is_err());
        assert!(
            memory
                .remember(&"x".repeat(MAX_FACT_CHARS + 1), "fact", "c1", true, "")
                .is_err()
        );
    }

    #[test]
    fn identical_facts_keep_their_requested_owner() {
        let mut memory = MemoryState::default();
        let shared = memory
            .remember("Keep it short", "fact", "", true, "")
            .unwrap();
        let private = memory
            .remember("Keep it short", "fact", "", true, "ada")
            .unwrap();
        assert_ne!(shared.id, private.id);
        assert_eq!(private.owner, "ada");
        assert_eq!(
            memory
                .remember("Keep it short", "fact", "", true, "ada")
                .unwrap()
                .id,
            private.id
        );
    }

    #[test]
    fn suggestions_wait_for_a_decision_and_never_duplicate_a_fact() {
        let mut memory = MemoryState::default();
        let suggestion = memory
            .suggest(
                "I prefer short commit messages",
                "preference",
                "noticed",
                "c1",
            )
            .expect("a new suggestion");
        assert!(
            memory
                .suggest(
                    "I prefer short commit messages",
                    "preference",
                    "noticed",
                    "c1"
                )
                .is_none()
        );
        let fact = memory
            .accept(&suggestion.id, "marble")
            .expect("accepted suggestion");
        assert_eq!(fact.text, "I prefer short commit messages");
        assert!(memory.suggestions.is_empty());
        assert!(
            memory
                .suggest(
                    "i prefer short commit messages",
                    "preference",
                    "noticed",
                    "c1"
                )
                .is_none()
        );
        assert!(!memory.dismiss(&suggestion.id));
    }

    #[test]
    fn editing_memory_checks_duplicates_only_within_its_owner() {
        let mut memory = MemoryState::default();
        let ada = memory
            .remember("prefer short replies", "fact", "", true, "ada")
            .unwrap();
        memory
            .remember("prefer short replies", "fact", "", true, "ben")
            .unwrap();
        let updated = memory
            .update(&ada.id, "prefer short replies", "preference")
            .unwrap();
        assert_eq!(updated.kind, "preference");
        assert_eq!(updated.owner, "ada");
        memory
            .remember("check the sources", "fact", "", true, "ben")
            .unwrap();
        assert!(memory.update(&ada.id, "check the sources", "fact").is_ok());
        memory
            .remember("keep a changelog", "fact", "", true, "ada")
            .unwrap();
        assert!(memory.update(&ada.id, "keep a changelog", "fact").is_err());
        assert_eq!(
            memory
                .update("missing", "check the sources", "fact")
                .unwrap_err(),
            "unknown memory"
        );
    }

    #[test]
    fn editing_a_memory_keeps_its_identity_and_rejects_duplicates() {
        let mut memory = MemoryState::default();
        let fact = memory
            .remember("my timezone is UTC", "fact", "c1", true, "")
            .unwrap();
        memory
            .remember("the office wifi is glassnet", "fact", "c1", true, "")
            .unwrap();
        let updated = memory
            .update(&fact.id, "  my timezone is Asia/Kolkata ", "preference")
            .unwrap();
        assert_eq!(updated.id, fact.id);
        assert_eq!(updated.text, "my timezone is Asia/Kolkata");
        assert_eq!(updated.kind, "preference");
        assert!(updated.explicit);
        assert!(memory.update(&fact.id, "", "fact").is_err());
        assert!(memory.update("missing", "anything", "fact").is_err());
        assert!(
            memory
                .update(&fact.id, "the office wifi is glassnet", "fact")
                .is_err(),
            "an edit cannot collide with another memory"
        );
    }

    #[test]
    fn forgetting_removes_by_id_and_by_text() {
        let mut memory = MemoryState::default();
        let fact = memory
            .remember("my name is Azhar", "role", "c1", true, "")
            .unwrap();
        memory
            .remember("the office wifi is glassnet", "fact", "c1", true, "")
            .unwrap();
        assert_eq!(memory.forget_matching("wifi"), 1);
        assert_eq!(memory.facts.len(), 1);
        assert!(memory.forget(&fact.id));
        assert!(memory.facts.is_empty());
    }

    #[test]
    fn detection_only_catches_durable_first_person_statements() {
        assert_eq!(
            detect("Can you deploy this? I prefer squash merges."),
            Some((
                "I prefer squash merges".into(),
                "preference".into(),
                "Marble noticed a durable detail in your last message.".into()
            ))
        );
        assert_eq!(detect("run the tests"), None);
        assert_eq!(detect("Remember this: I prefer tabs"), None);
    }

    #[test]
    fn memory_is_carried_into_a_turn_as_context() {
        let mut memory = MemoryState::default();
        assert!(memory.context_block_for("marble").is_none());
        memory
            .remember("my name is Azhar", "role", "c1", true, "")
            .unwrap();
        let block = memory.context_block_for("marble").unwrap();
        assert!(block.contains("role: my name is Azhar"));
        memory.note_use("marble");
        assert_eq!(memory.facts[0].uses, 1);
        assert_eq!(memory.document()["count"], json!(1));
    }

    #[test]
    fn legacy_documents_decode_with_empty_memory() {
        let memory: MemoryState = serde_json::from_str("{}").unwrap();
        assert!(memory.is_empty());
        assert_eq!(memory.document()["limit"], json!(MEMORY_LIMIT));
    }
}
