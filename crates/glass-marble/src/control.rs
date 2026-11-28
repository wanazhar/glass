//! Marble's own layer of control.
//!
//! Core owns the model loop and its native tools; this module owns the answer to
//! "may this governed integration run, and who decided?". Marble never inherits
//! a decision from the engine, so the same workspace can be handed to a model in
//! observe mode, in approving mode, or unattended, and every governed call is
//! classified here first.
//!
//! The layer is four small pieces that stay legible and reversible:
//!
//! - a profile (`observe`, `workspace`, `unattended`) describing standing posture,
//! - path rules that are refused regardless of posture or approval,
//! - revocable grants that answer a repeated question once,
//! - an audit ledger and a FIFO admission queue the human can pause and drain.

use crate::tools::{WORKSPACE_WRITE, is_write_tool};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Decisions retained for the audit ledger.
pub const LEDGER_LIMIT: usize = 200;
/// Messages a paused instance holds before it refuses new ones.
pub const QUEUE_LIMIT: usize = 100;
/// Standing grants one instance remembers at a time.
pub const GRANT_LIMIT: usize = 32;

/// Paths refused by default: revision metadata, keys, and dependencies that are
/// generated or secret rather than authored.
pub const DEFAULT_DENIED_PATHS: [&str; 4] = [".git", ".ssh", ".env", "node_modules"];

/// Paths are matched as whole components so `.env` also covers `.env.local`
/// without refusing an authored file such as `.gitignore`.
fn denied_rule_for(rule: &str, path: &str) -> bool {
    let rule = rule.trim().trim_matches('/');
    if rule.is_empty() {
        return false;
    }
    path.split('/')
        .any(|component| component == rule || component.starts_with(&format!("{rule}.")))
}

/// Whether one grant scope contains `path`. An empty scope is the whole
/// workspace.
fn scope_contains(scope: &str, path: &str) -> bool {
    let scope = scope.trim().trim_matches('/');
    if scope.is_empty() {
        return true;
    }
    path == scope || path.starts_with(&format!("{scope}/"))
}

/// Grant scope for one approval: the directory the human just approved in.
pub fn directory_scope(path: &str) -> String {
    let normalized = path.trim().trim_start_matches("./").trim_end_matches('/');
    match normalized.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => String::new(),
    }
}

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

/// Standing autonomy posture for the whole instance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlProfile {
    /// Read the workspace, never change it, never prompt.
    Observe,
    /// Ask before each governed call unless a grant already answers it.
    #[default]
    Workspace,
    /// Approve governed calls without prompting. Path rules still apply.
    Unattended,
}

impl ControlProfile {
    pub const ALL: [Self; 3] = [Self::Observe, Self::Workspace, Self::Unattended];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "observe" => Some(Self::Observe),
            "workspace" => Some(Self::Workspace),
            "unattended" => Some(Self::Unattended),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Workspace => "workspace",
            Self::Unattended => "unattended",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Observe => "Observe",
            Self::Workspace => "Workspace",
            Self::Unattended => "Unattended",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Observe => "Read the workspace without changing it. Nothing to approve.",
            Self::Workspace => "Ask before each governed tool call, remembering grants you allow.",
            Self::Unattended => "Run governed tools without prompting. Denied paths still apply.",
        }
    }

    /// Only writable postures may change the workspace.
    pub fn allows_writes(self) -> bool {
        !matches!(self, Self::Observe)
    }

    /// Whether the posture answers the approval question itself.
    pub fn stands_alone(self) -> bool {
        matches!(self, Self::Unattended)
    }

    /// Selectable postures for product surfaces.
    pub fn catalog() -> Value {
        Value::Array(
            Self::ALL
                .iter()
                .map(|profile| {
                    json!({
                        "id": profile.as_str(),
                        "label": profile.label(),
                        "description": profile.describe(),
                        "allowsWrites": profile.allows_writes(),
                        "prompts": !profile.stands_alone() && profile.allows_writes(),
                    })
                })
                .collect(),
        )
    }
}

/// Standing answer for one tool and one path scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlGrant {
    pub id: String,
    /// Governed tool name this grant answers for.
    pub tool: String,
    /// Workspace-relative directory the grant covers; empty means everything.
    #[serde(default)]
    pub path_prefix: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub uses: u64,
}

impl ControlGrant {
    fn covers(&self, tool: &str, path: &str) -> bool {
        self.tool == tool && scope_contains(&self.path_prefix, path)
    }

    /// Human-readable scope, used by the surfaces.
    pub fn scope_label(&self) -> String {
        if self.path_prefix.is_empty() {
            "whole workspace".to_string()
        } else {
            self.path_prefix.clone()
        }
    }
}

/// Marble's classification of one governed call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Marble allows the call without asking the human.
    Allow { reason: String },
    /// The human has to decide.
    Ask,
    /// Marble refuses the call and tells Core why.
    Deny { reason: String },
}

/// One recorded control decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlEntry {
    pub at: u64,
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub task_id: String,
    pub tool: String,
    #[serde(default)]
    pub path: String,
    /// `allowed`, `approved`, or `denied`.
    pub decision: String,
    #[serde(default)]
    pub reason: String,
}

/// One message held while admission is paused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedTurn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_time_zone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_message: Option<crate::state::ClientMessage>,
    #[serde(default)]
    pub thread_id: Option<String>,
    pub id: String,
    #[serde(default)]
    pub marble_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) everyone_round: bool,
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub at: u64,
    /// Set when a routine, rather than a person, put this message here, so a
    /// surface can say which routine is waiting and why.
    #[serde(default)]
    pub routine_id: Option<String>,
    /// Set when a person ran the routine by hand while it was disabled. The
    /// schedule is paused, but this firing was asked for, so it still runs.
    #[serde(default)]
    pub routine_override: bool,
}

/// Durable control document: profile, switch, rules, grants, ledger, and queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlState {
    #[serde(default)]
    pub profile: ControlProfile,
    /// While paused, new messages wait in the queue instead of becoming turns.
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub grants: Vec<ControlGrant>,
    /// Path components refused regardless of profile, grant, or approval.
    #[serde(default = "default_denied_paths")]
    pub denied_paths: Vec<String>,
    #[serde(default)]
    pub ledger: Vec<ControlEntry>,
    #[serde(default)]
    pub queue: Vec<QueuedTurn>,
}

fn default_denied_paths() -> Vec<String> {
    DEFAULT_DENIED_PATHS
        .iter()
        .map(|path| (*path).to_string())
        .collect()
}

impl Default for ControlState {
    fn default() -> Self {
        Self {
            profile: ControlProfile::default(),
            paused: false,
            grants: Vec::new(),
            denied_paths: default_denied_paths(),
            ledger: Vec::new(),
            queue: Vec::new(),
        }
    }
}

impl ControlState {
    /// Classify one governed call before it runs.
    ///
    /// Order matters: an unknown namespace cannot be governed at all, deny rules
    /// win over every posture, a read-only posture cannot write, and only then
    /// does the standing posture or a grant answer the question.
    pub fn classify(&self, tool: &str, path: &str) -> Verdict {
        if !tool.starts_with("glass.") {
            return Verdict::Deny {
                reason: format!("marble governs glass.* tools only, not {tool}"),
            };
        }
        // Asking another Marble for help is governed internally — refusal,
        // depth, and count live in the pool, not on an approval card.
        if tool == crate::tools::MARBLE_ASK {
            return Verdict::Allow {
                reason: "asking another Marble needs no approval".into(),
            };
        }
        let path = path.trim().trim_start_matches("./");
        if let Some(rule) = self
            .denied_paths
            .iter()
            .find(|rule| denied_rule_for(rule, path))
        {
            return Verdict::Deny {
                reason: format!("{path} matches the denied path rule {rule}"),
            };
        }
        if is_write_tool(tool) && !self.profile.allows_writes() {
            return Verdict::Deny {
                reason: format!(
                    "the {} profile is read-only, so {tool} cannot change the workspace",
                    self.profile.as_str()
                ),
            };
        }
        if self.profile.stands_alone() {
            return Verdict::Allow {
                reason: format!(
                    "the {} profile answers governed calls",
                    self.profile.as_str()
                ),
            };
        }
        if matches!(self.profile, ControlProfile::Observe) {
            return Verdict::Allow {
                reason: "the observe profile allows reads only".into(),
            };
        }
        if let Some(grant) = self.grants.iter().find(|grant| grant.covers(tool, path)) {
            return Verdict::Allow {
                reason: format!(
                    "grant {} already allows {} in {}",
                    grant.id,
                    grant.tool,
                    grant.scope_label()
                ),
            };
        }
        Verdict::Ask
    }

    /// Record one decision, keeping the ledger bounded and newest last.
    pub fn record(&mut self, entry: ControlEntry) {
        self.ledger.push(entry);
        if self.ledger.len() > LEDGER_LIMIT {
            let excess = self.ledger.len() - LEDGER_LIMIT;
            self.ledger.drain(..excess);
        }
    }

    /// Record a decision with the current time.
    pub fn note(
        &mut self,
        conversation_id: &str,
        task_id: &str,
        tool: &str,
        path: &str,
        decision: &str,
        reason: &str,
    ) {
        self.record(ControlEntry {
            at: now_millis(),
            conversation_id: conversation_id.to_string(),
            task_id: task_id.to_string(),
            tool: tool.to_string(),
            path: path.to_string(),
            decision: decision.to_string(),
            reason: reason.to_string(),
        });
    }

    /// Remember a standing grant, reusing an identical one. The scope is
    /// validated here so a grant can never reach outside the workspace.
    pub fn grant(&mut self, tool: &str, path_prefix: &str) -> Result<ControlGrant, String> {
        let tool = tool.trim();
        if !tool.starts_with("glass.") {
            return Err(format!("a grant must name a glass.* tool, not {tool}"));
        }
        if is_write_tool(tool) && tool != WORKSPACE_WRITE {
            return Err(format!("{tool} is not a grantable tool"));
        }
        let raw = path_prefix.trim();
        if raw.starts_with('/') || raw.split('/').any(|part| part == "..") {
            return Err("a grant scope must stay inside the workspace".into());
        }
        let prefix = raw.trim_matches('/');
        if prefix.len() > 256 {
            return Err("a grant scope is too long".into());
        }
        if let Some(existing) = self
            .grants
            .iter()
            .find(|grant| grant.tool == tool && grant.path_prefix == prefix)
        {
            return Ok(existing.clone());
        }
        if self.grants.len() >= GRANT_LIMIT {
            return Err(format!(
                "marble keeps at most {GRANT_LIMIT} grants; revoke one first"
            ));
        }
        let grant = ControlGrant {
            id: next_id("grant"),
            tool: tool.to_string(),
            path_prefix: prefix.to_string(),
            created_at: now_millis(),
            uses: 0,
        };
        self.grants.push(grant.clone());
        Ok(grant)
    }

    /// Remove one grant; returns whether it existed.
    pub fn revoke(&mut self, id: &str) -> bool {
        let before = self.grants.len();
        self.grants.retain(|grant| grant.id != id);
        self.grants.len() != before
    }

    /// Count one use of every grant that answers this call.
    pub fn use_grants(&mut self, tool: &str, path: &str) {
        for grant in self.grants.iter_mut() {
            if grant.covers(tool, path) {
                grant.uses = grant.uses.saturating_add(1);
            }
        }
    }

    /// Hold one message while admission is paused.
    pub fn enqueue(
        &mut self,
        conversation_id: Option<String>,
        text: impl Into<String>,
    ) -> Result<QueuedTurn, String> {
        self.enqueue_turn(conversation_id, None, false, text, None)
    }

    pub fn enqueue_marble(
        &mut self,
        marble_id: String,
        conversation_id: Option<String>,
        text: String,
    ) -> Result<QueuedTurn, String> {
        self.enqueue_turn(conversation_id, None, false, text, Some(marble_id))
    }

    /// Hold one routine firing while admission is paused, keeping its routine.
    ///
    /// `override_pause` keeps a hand-run firing of a disabled routine intact
    /// through the queue, so resuming still runs work a person asked for.
    pub fn enqueue_routine(
        &mut self,
        routine_id: impl Into<String>,
        conversation_id: Option<String>,
        text: impl Into<String>,
        override_pause: bool,
    ) -> Result<QueuedTurn, String> {
        self.enqueue_turn(
            conversation_id,
            Some(routine_id.into()),
            override_pause,
            text,
            None,
        )
    }

    fn enqueue_turn(
        &mut self,
        conversation_id: Option<String>,
        routine_id: Option<String>,
        routine_override: bool,
        text: impl Into<String>,
        marble_id: Option<String>,
    ) -> Result<QueuedTurn, String> {
        if self.queue.len() >= QUEUE_LIMIT {
            return Err(format!(
                "marble holds at most {QUEUE_LIMIT} queued messages"
            ));
        }
        let entry = QueuedTurn {
            device_time_zone: None,
            client_message: None,
            thread_id: None,
            id: next_id("queued"),
            marble_id,
            everyone_round: false,
            conversation_id,
            text: text.into(),
            at: now_millis(),
            routine_id,
            routine_override,
        };
        self.queue.push(entry.clone());
        Ok(entry)
    }

    /// Remove and return every queued message in arrival order.
    pub fn take_queue(&mut self) -> Vec<QueuedTurn> {
        std::mem::take(&mut self.queue)
    }

    /// Product-surface document combining the layer with its selectable profiles.
    pub fn document(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Value::Object(ref mut map) = value {
            map.insert("profiles".into(), ControlProfile::catalog());
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_is_read_only_and_never_prompts() {
        let mut control = ControlState {
            profile: ControlProfile::Observe,
            ..ControlState::default()
        };
        assert!(matches!(
            control.classify("glass.workspace.read", "docs/notes.md"),
            Verdict::Allow { .. }
        ));
        let denied = control.classify("glass.workspace.write", "docs/notes.md");
        assert!(matches!(denied, Verdict::Deny { .. }), "{denied:?}");
        control.profile = ControlProfile::Workspace;
        assert_eq!(
            control.classify("glass.workspace.write", "docs/x"),
            Verdict::Ask
        );
    }

    #[test]
    fn workspace_asks_until_a_grant_answers_the_same_scope() {
        let mut control = ControlState::default();
        assert_eq!(
            control.classify("glass.workspace.read", "docs/a.md"),
            Verdict::Ask
        );
        let grant = control.grant("glass.workspace.read", "docs").unwrap();
        assert_eq!(grant.scope_label(), "docs");
        assert!(matches!(
            control.classify("glass.workspace.read", "docs/a.md"),
            Verdict::Allow { .. }
        ));
        assert_eq!(
            control.classify("glass.workspace.read", "code/a.rs"),
            Verdict::Ask
        );
        assert_eq!(
            control.classify("glass.workspace.write", "docs/a.md"),
            Verdict::Ask
        );
        control.use_grants("glass.workspace.read", "docs/a.md");
        assert_eq!(control.grants[0].uses, 1);
        assert!(control.revoke(&grant.id));
        assert_eq!(
            control.classify("glass.workspace.read", "docs/a.md"),
            Verdict::Ask
        );
    }

    #[test]
    fn unattended_answers_but_denied_paths_still_win() {
        let control = ControlState {
            profile: ControlProfile::Unattended,
            ..ControlState::default()
        };
        assert!(matches!(
            control.classify("glass.workspace.write", "docs/x.md"),
            Verdict::Allow { .. }
        ));
        for path in [
            ".git/config",
            ".env.local",
            ".ssh/id_ed25519",
            "node_modules/x.js",
        ] {
            let verdict = control.classify("glass.workspace.read", path);
            assert!(
                matches!(verdict, Verdict::Deny { .. }),
                "{path}: {verdict:?}"
            );
        }
        assert!(matches!(
            control.classify("glass.workspace.read", ".gitignore"),
            Verdict::Allow { .. }
        ));
    }

    #[test]
    fn grants_cannot_escape_the_workspace() {
        let mut control = ControlState::default();
        assert!(control.grant("glass.workspace.read", "../outside").is_err());
        assert!(control.grant("glass.workspace.read", "/etc").is_err());
        assert!(control.grant("shell.exec", "docs").is_err());
        assert_eq!(
            control
                .grant("glass.workspace.read", "")
                .unwrap()
                .scope_label(),
            "whole workspace"
        );
    }

    #[test]
    fn a_paused_instance_queues_in_arrival_order() {
        let mut control = ControlState {
            paused: true,
            ..ControlState::default()
        };
        let first = control.enqueue(None, "first").unwrap();
        let second = control.enqueue(Some("c1".into()), "second").unwrap();
        assert_eq!(control.queue.len(), 2);
        let drained = control.take_queue();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, first.id);
        assert_eq!(drained[1].id, second.id);
        assert_eq!(drained[1].conversation_id.as_deref(), Some("c1"));
        assert!(control.queue.is_empty());
    }

    #[test]
    fn the_ledger_stays_bounded_and_records_reasons() {
        let mut control = ControlState::default();
        for index in 0..(LEDGER_LIMIT + 12) {
            control.note("c1", "t1", "glass.workspace.read", "a", "approved", "human");
            assert_eq!(control.ledger.last().unwrap().reason, "human");
            if index == 0 {
                continue;
            }
        }
        assert_eq!(control.ledger.len(), LEDGER_LIMIT);
    }

    #[test]
    fn legacy_documents_take_the_safe_defaults() {
        let legacy: ControlState = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.profile, ControlProfile::Workspace);
        assert!(!legacy.paused);
        assert_eq!(legacy.denied_paths, default_denied_paths());
        assert!(legacy.grants.is_empty());
    }

    #[test]
    fn profiles_round_trip_through_the_document() {
        for profile in ControlProfile::ALL {
            let encoded = serde_json::to_string(&profile).unwrap();
            assert_eq!(
                ControlProfile::parse(encoded.trim_matches('"')),
                Some(profile)
            );
        }
        assert_eq!(ControlProfile::parse("nonsense"), None);
        let catalog = ControlProfile::catalog();
        assert_eq!(catalog.as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn directory_scope_is_the_parent_of_the_approved_file() {
        assert_eq!(directory_scope("docs/notes.md"), "docs");
        assert_eq!(directory_scope("docs/deep/notes.md"), "docs/deep");
        assert_eq!(directory_scope("notes.md"), "");
        assert_eq!(directory_scope("./docs/notes.md"), "docs");
    }
}
