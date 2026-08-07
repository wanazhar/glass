use super::{DevelopmentGraph, RuntimeLink};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticEntityState {
    pub role: Option<String>,
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub actionable: bool,
}

pub type SemanticSnapshot = BTreeMap<String, SemanticEntityState>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SemanticBreakpoint {
    EntityDisappears { entity_id: String },
    AccessibleNameMissing { entity_id: Option<String> },
    RoleChanges { entity_id: String },
    ActionabilityLost { entity_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticBreakpointHit {
    pub breakpoint: SemanticBreakpoint,
    pub entity_id: String,
    pub before: Option<SemanticEntityState>,
    pub after: Option<SemanticEntityState>,
    pub likely_source: Option<RuntimeLink>,
}

pub fn evaluate_breakpoints(
    breakpoints: &[SemanticBreakpoint],
    before: &SemanticSnapshot,
    after: &SemanticSnapshot,
    graph: &DevelopmentGraph,
) -> Vec<SemanticBreakpointHit> {
    let mut hits = Vec::new();
    for breakpoint in breakpoints.iter().take(128) {
        let candidates: Vec<String> = match breakpoint {
            SemanticBreakpoint::AccessibleNameMissing { entity_id: None } => {
                before.keys().cloned().collect()
            }
            SemanticBreakpoint::EntityDisappears { entity_id }
            | SemanticBreakpoint::RoleChanges { entity_id }
            | SemanticBreakpoint::ActionabilityLost { entity_id }
            | SemanticBreakpoint::AccessibleNameMissing {
                entity_id: Some(entity_id),
            } => vec![entity_id.clone()],
        };
        for entity_id in candidates {
            let previous = before.get(&entity_id);
            let current = after.get(&entity_id);
            let triggered = match breakpoint {
                SemanticBreakpoint::EntityDisappears { .. } => {
                    previous.is_some() && current.is_none()
                }
                SemanticBreakpoint::AccessibleNameMissing { .. } => {
                    previous
                        .and_then(|state| state.name.as_deref())
                        .is_some_and(|name| !name.trim().is_empty())
                        && current.is_some_and(|state| {
                            state
                                .name
                                .as_deref()
                                .is_none_or(|name| name.trim().is_empty())
                        })
                }
                SemanticBreakpoint::RoleChanges { .. } => {
                    previous.is_some()
                        && current.is_some()
                        && previous.and_then(|state| state.role.as_ref())
                            != current.and_then(|state| state.role.as_ref())
                }
                SemanticBreakpoint::ActionabilityLost { .. } => {
                    previous.is_some_and(|state| state.actionable)
                        && current.is_some_and(|state| !state.actionable)
                }
            };
            if triggered {
                hits.push(SemanticBreakpointHit {
                    breakpoint: breakpoint.clone(),
                    entity_id: entity_id.clone(),
                    before: previous.cloned(),
                    after: current.cloned(),
                    likely_source: graph.best_link(&entity_id).cloned(),
                });
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_breakpoint_reports_regression_and_source_evidence() {
        let mut before = SemanticSnapshot::new();
        before.insert(
            "action.checkout.submit".into(),
            SemanticEntityState {
                role: Some("button".into()),
                name: Some("Pay now".into()),
                enabled: Some(true),
                actionable: true,
            },
        );
        let mut after = before.clone();
        after.get_mut("action.checkout.submit").unwrap().actionable = false;
        let hits = evaluate_breakpoints(
            &[SemanticBreakpoint::ActionabilityLost {
                entity_id: "action.checkout.submit".into(),
            }],
            &before,
            &after,
            &DevelopmentGraph::default(),
        );
        assert_eq!(hits.len(), 1);
        assert!(hits[0].before.as_ref().unwrap().actionable);
        assert!(!hits[0].after.as_ref().unwrap().actionable);
    }
}
