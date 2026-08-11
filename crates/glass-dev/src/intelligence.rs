//! Bounded causal graph and observable development replay.

use glass_browser::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_NODES: usize = 8_192;
const MAX_EDGES: usize = 32_768;
const MAX_EVENTS: usize = 16_384;
const MAX_EVIDENCE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DevelopmentNodeKind {
    Repository,
    File,
    Symbol,
    EditorRevision,
    GitChange,
    Process,
    Port,
    Request,
    Debugger,
    Breakpoint,
    StackFrame,
    Thread,
    BrowserTarget,
    BrowserRevision,
    SemanticEntity,
    WebIrEntity,
    Workflow,
    Test,
    Diagnostic,
    GitCommit,
    Agent,
    Task,
    ToolCall,
    Experiment,
    Verification,
    Kernel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentNode {
    pub id: String,
    pub kind: DevelopmentNodeKind,
    pub label: String,
    pub revision: u64,
    pub stale: bool,
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentEdge {
    pub id: u64,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub revision: u64,
    pub stale: bool,
    pub provenance: String,
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservableDevelopmentEvent {
    pub sequence: u64,
    pub occurred_at_ms: u128,
    pub actor: String,
    pub subsystem: String,
    pub kind: String,
    pub resource: Option<String>,
    pub workspace_revision: u64,
    pub evidence: Value,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CausalPath {
    pub nodes: Vec<DevelopmentNode>,
    pub edges: Vec<DevelopmentEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayDiff {
    pub from_sequence: u64,
    pub to_sequence: u64,
    pub actors: BTreeSet<String>,
    pub subsystems: BTreeSet<String>,
    pub resources: BTreeSet<String>,
    pub events: Vec<ObservableDevelopmentEvent>,
}

pub struct ObservableEventInput<'a> {
    pub actor: &'a str,
    pub subsystem: &'a str,
    pub kind: &'a str,
    pub resource: Option<&'a str>,
    pub workspace_revision: u64,
    pub evidence: Value,
    pub rationale: Option<&'a str>,
}

/// One graph and replay timeline shared by every development subsystem.
pub struct DevelopmentIntelligence {
    nodes: BTreeMap<String, DevelopmentNode>,
    edges: VecDeque<DevelopmentEdge>,
    events: VecDeque<ObservableDevelopmentEvent>,
    next_edge: u64,
    next_event: u64,
}

impl Default for DevelopmentIntelligence {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: VecDeque::new(),
            events: VecDeque::new(),
            next_edge: 1,
            next_event: 1,
        }
    }
}

impl DevelopmentIntelligence {
    pub fn upsert_node(&mut self, node: DevelopmentNode) -> DevelopmentResult<()> {
        validate_identifier(&node.id, "node")?;
        validate_text(&node.label, "node label", 1024)?;
        validate_evidence(&node.evidence)?;
        if self.nodes.len() == MAX_NODES && !self.nodes.contains_key(&node.id) {
            return Err(DevelopmentError::Conflict(format!(
                "development graph reached its {MAX_NODES} node limit"
            )));
        }
        if let Some(existing) = self.nodes.get(&node.id)
            && node.revision < existing.revision
        {
            return Err(DevelopmentError::Conflict(format!(
                "stale node revision {} for {} at revision {}",
                node.revision, node.id, existing.revision
            )));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    pub fn link(
        &mut self,
        from: &str,
        to: &str,
        relation: &str,
        revision: u64,
        provenance: &str,
        evidence: Value,
    ) -> DevelopmentResult<u64> {
        if !self.nodes.contains_key(from) || !self.nodes.contains_key(to) {
            return Err(DevelopmentError::NotFound(
                "causal edges require existing endpoint nodes".into(),
            ));
        }
        validate_text(relation, "edge relation", 128)?;
        validate_text(provenance, "edge provenance", 1024)?;
        validate_evidence(&evidence)?;
        if self.edges.len() == MAX_EDGES {
            self.edges.pop_front();
        }
        let id = self.next_edge;
        self.next_edge = self.next_edge.checked_add(1).ok_or_else(|| {
            DevelopmentError::Conflict("development graph edge sequence overflowed".into())
        })?;
        self.edges.push_back(DevelopmentEdge {
            id,
            from: from.into(),
            to: to.into(),
            relation: relation.into(),
            revision,
            stale: false,
            provenance: provenance.into(),
            evidence,
        });
        Ok(id)
    }

    pub fn record(&mut self, input: ObservableEventInput<'_>) -> DevelopmentResult<u64> {
        validate_text(input.actor, "event actor", 256)?;
        validate_text(input.subsystem, "event subsystem", 128)?;
        validate_text(input.kind, "event kind", 128)?;
        if let Some(resource) = input.resource {
            validate_text(resource, "event resource", 4096)?;
        }
        if let Some(rationale) = input.rationale {
            validate_text(rationale, "event rationale", 4096)?;
        }
        validate_evidence(&input.evidence)?;
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        let sequence = self.next_event;
        self.next_event = self.next_event.checked_add(1).ok_or_else(|| {
            DevelopmentError::Conflict("development replay sequence overflowed".into())
        })?;
        self.events.push_back(ObservableDevelopmentEvent {
            sequence,
            occurred_at_ms: now_ms(),
            actor: input.actor.into(),
            subsystem: input.subsystem.into(),
            kind: input.kind.into(),
            resource: input.resource.map(str::to_string),
            workspace_revision: input.workspace_revision,
            evidence: input.evidence,
            rationale: input.rationale.map(str::to_string),
        });
        Ok(sequence)
    }

    pub fn invalidate_before(&mut self, revision: u64) -> usize {
        let mut invalidated = 0;
        for node in self.nodes.values_mut() {
            if node.revision < revision && !node.stale {
                node.stale = true;
                invalidated += 1;
            }
        }
        for edge in &mut self.edges {
            if edge.revision < revision && !edge.stale {
                edge.stale = true;
                invalidated += 1;
            }
        }
        invalidated
    }

    pub fn node(&self, id: &str) -> Option<&DevelopmentNode> {
        self.nodes.get(id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &DevelopmentNode> {
        self.nodes.values()
    }

    pub fn edges(&self) -> impl Iterator<Item = &DevelopmentEdge> {
        self.edges.iter()
    }

    pub fn query_kind(&self, kind: DevelopmentNodeKind) -> Vec<&DevelopmentNode> {
        self.nodes
            .values()
            .filter(|node| node.kind == kind && !node.stale)
            .collect()
    }

    pub fn path(&self, from: &str, to: &str) -> DevelopmentResult<CausalPath> {
        if !self.nodes.contains_key(from) || !self.nodes.contains_key(to) {
            return Err(DevelopmentError::NotFound("causal path endpoint".into()));
        }
        let mut queue = VecDeque::from([from.to_string()]);
        let mut visited = BTreeSet::from([from.to_string()]);
        let mut previous = BTreeMap::<String, (String, u64)>::new();
        while let Some(current) = queue.pop_front() {
            if current == to {
                break;
            }
            for edge in self.edges.iter().filter(|edge| !edge.stale) {
                let next = if edge.from == current {
                    Some(edge.to.as_str())
                } else if edge.to == current {
                    Some(edge.from.as_str())
                } else {
                    None
                };
                if let Some(next) = next
                    && visited.insert(next.to_string())
                {
                    previous.insert(next.to_string(), (current.clone(), edge.id));
                    queue.push_back(next.to_string());
                }
            }
        }
        if !visited.contains(to) {
            return Err(DevelopmentError::NotFound(format!(
                "causal path from {from} to {to}"
            )));
        }
        let mut node_ids = vec![to.to_string()];
        let mut edge_ids = Vec::new();
        let mut cursor = to;
        while cursor != from {
            let (parent, edge) = previous.get(cursor).ok_or_else(|| {
                DevelopmentError::Conflict("causal path reconstruction failed".into())
            })?;
            edge_ids.push(*edge);
            node_ids.push(parent.clone());
            cursor = parent;
        }
        node_ids.reverse();
        edge_ids.reverse();
        Ok(CausalPath {
            nodes: node_ids
                .iter()
                .filter_map(|id| self.nodes.get(id).cloned())
                .collect(),
            edges: edge_ids
                .iter()
                .filter_map(|id| self.edges.iter().find(|edge| edge.id == *id).cloned())
                .collect(),
        })
    }

    pub fn replay(
        &self,
        since: u64,
        limit: usize,
    ) -> DevelopmentResult<Vec<ObservableDevelopmentEvent>> {
        if limit == 0 || limit > 4096 {
            return Err(DevelopmentError::InvalidInput(
                "replay limit must be 1..=4096".into(),
            ));
        }
        Ok(self
            .events
            .iter()
            .filter(|event| event.sequence > since)
            .take(limit)
            .cloned()
            .collect())
    }

    pub fn replay_diff(&self, from: u64, to: u64) -> DevelopmentResult<ReplayDiff> {
        if from >= to {
            return Err(DevelopmentError::InvalidInput(
                "replay diff requires an increasing sequence range".into(),
            ));
        }
        let events = self
            .events
            .iter()
            .filter(|event| event.sequence > from && event.sequence <= to)
            .cloned()
            .collect::<Vec<_>>();
        Ok(ReplayDiff {
            from_sequence: from,
            to_sequence: to,
            actors: events.iter().map(|event| event.actor.clone()).collect(),
            subsystems: events.iter().map(|event| event.subsystem.clone()).collect(),
            resources: events
                .iter()
                .filter_map(|event| event.resource.clone())
                .collect(),
            events,
        })
    }
}

fn validate_identifier(value: &str, description: &str) -> DevelopmentResult<()> {
    if value.is_empty()
        || value.len() > 512
        || value.chars().any(|character| character.is_control())
    {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} identifier must contain 1..=512 non-control bytes"
        )));
    }
    Ok(())
}

fn validate_text(value: &str, description: &str, limit: usize) -> DevelopmentResult<()> {
    if value.is_empty() || value.len() > limit || value.contains('\0') {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} must contain 1..={limit} bytes without NUL"
        )));
    }
    Ok(())
}

fn validate_evidence(evidence: &Value) -> DevelopmentResult<()> {
    if serde_json::to_vec(evidence)?.len() > MAX_EVIDENCE_BYTES {
        return Err(DevelopmentError::InvalidInput(format!(
            "development evidence exceeds {MAX_EVIDENCE_BYTES} bytes"
        )));
    }
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_explains_paths_and_invalidates_stale_evidence() {
        let mut graph = DevelopmentIntelligence::default();
        for (id, kind) in [
            ("file:checkout.rs", DevelopmentNodeKind::File),
            ("test:checkout", DevelopmentNodeKind::Test),
            ("browser:error", DevelopmentNodeKind::SemanticEntity),
        ] {
            graph
                .upsert_node(DevelopmentNode {
                    id: id.into(),
                    kind,
                    label: id.into(),
                    revision: 4,
                    stale: false,
                    evidence: Value::Null,
                })
                .unwrap();
        }
        graph
            .link(
                "file:checkout.rs",
                "test:checkout",
                "coveredBy",
                4,
                "cargo test",
                Value::Null,
            )
            .unwrap();
        graph
            .link(
                "test:checkout",
                "browser:error",
                "observes",
                4,
                "workflow evidence",
                Value::Null,
            )
            .unwrap();
        let path = graph.path("file:checkout.rs", "browser:error").unwrap();
        assert_eq!(path.nodes.len(), 3);
        assert_eq!(path.edges.len(), 2);
        assert_eq!(graph.invalidate_before(5), 5);
        assert!(graph.path("file:checkout.rs", "browser:error").is_err());
    }

    #[test]
    fn replay_diff_contains_only_observable_evidence() {
        let mut replay = DevelopmentIntelligence::default();
        replay
            .record(ObservableEventInput {
                actor: "agent-1",
                subsystem: "tool",
                kind: "called",
                resource: Some("file:src/lib.rs"),
                workspace_revision: 1,
                evidence: serde_json::json!({"tool":"glass.file.write"}),
                rationale: Some("apply requested change"),
            })
            .unwrap();
        replay
            .record(ObservableEventInput {
                actor: "tester",
                subsystem: "test",
                kind: "passed",
                resource: Some("test:unit"),
                workspace_revision: 2,
                evidence: serde_json::json!({"exitCode":0}),
                rationale: None,
            })
            .unwrap();
        let diff = replay.replay_diff(0, 2).unwrap();
        assert_eq!(diff.events.len(), 2);
        assert!(diff.actors.contains("agent-1"));
        assert!(diff.subsystems.contains("test"));
    }
}
