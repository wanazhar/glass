//! Canonical human-facing browser workspace shared by both Glass products.
//!
//! Browser backends execute
//! [`BrowserWorkspaceAction`](crate::browser_workspace::BrowserWorkspaceAction)
//! values. This module owns
//! only bounded presentation, selection, focus, input authority, and recovery
//! state; it never owns Chrome or weakens browser revision checks.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_ENTITIES: usize = 512;
const MAX_TARGETS: usize = 64;
const MAX_TRANSIENT_ERRORS: usize = 8;
const MAX_TEXT_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserWorkspaceLayout {
    Phone,
    Compact,
    #[default]
    Desktop,
}

/// Product shell adapting an execution backend to the canonical workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserWorkspaceAdapterKind {
    Standalone,
    EmbeddedDevelopment,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserConnectionPhase {
    #[default]
    Detached,
    Starting,
    Connected,
    Recovering,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserInputOwner {
    #[default]
    Glass,
    Human,
    Agent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserFocus {
    Controls,
    Visual,
    #[default]
    Semantic,
    Footer,
    Palette,
    Address,
    Recovery,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserPresentationPath {
    Herdr,
    Kitty,
    Sixel,
    Ansi,
    #[default]
    SemanticOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWorkspaceEntity {
    pub reference: String,
    pub role: String,
    pub name: String,
    pub actionable: bool,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWorkspaceTarget {
    pub id: String,
    pub title: String,
    pub url: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCapability {
    pub available: bool,
    pub reason: Option<String>,
}

impl BrowserCapability {
    pub fn available() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(bounded(reason.into())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserOperation {
    Start,
    Stop,
    Reconnect,
    State,
    Observe,
    Snapshot,
    Semantic,
    Diff,
    Targets,
    SelectTarget,
    Navigate,
    Back,
    Forward,
    Reload,
    StopLoading,
    Click,
    Type,
    Scroll,
    Screenshot,
    WorkflowList,
    WorkflowRun,
    WorkflowPause,
    WorkflowResume,
    WorkflowCancel,
    WorkflowVerify,
    RemoteViewOpen,
    RemoteViewStatus,
    RemoteViewRevoke,
}

impl BrowserOperation {
    pub const ALL: [Self; 28] = [
        Self::Start,
        Self::Stop,
        Self::Reconnect,
        Self::State,
        Self::Observe,
        Self::Snapshot,
        Self::Semantic,
        Self::Diff,
        Self::Targets,
        Self::SelectTarget,
        Self::Navigate,
        Self::Back,
        Self::Forward,
        Self::Reload,
        Self::StopLoading,
        Self::Click,
        Self::Type,
        Self::Scroll,
        Self::Screenshot,
        Self::WorkflowList,
        Self::WorkflowRun,
        Self::WorkflowPause,
        Self::WorkflowResume,
        Self::WorkflowCancel,
        Self::WorkflowVerify,
        Self::RemoteViewOpen,
        Self::RemoteViewStatus,
        Self::RemoteViewRevoke,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrowserWorkspaceAction {
    Start,
    Stop,
    Reconnect,
    State,
    Observe,
    Snapshot,
    Semantic,
    Diff,
    Targets,
    SelectTarget {
        target_id: String,
    },
    Navigate {
        url: String,
        expected_revision: u64,
    },
    Back {
        expected_revision: u64,
    },
    Forward {
        expected_revision: u64,
    },
    Reload {
        expected_revision: u64,
    },
    StopLoading {
        expected_revision: u64,
    },
    Click {
        target: String,
        expected_revision: u64,
    },
    Type {
        target: Option<String>,
        text: String,
        expected_revision: u64,
    },
    Scroll {
        dx: f64,
        dy: f64,
        expected_revision: u64,
    },
    Screenshot,
    WorkflowList,
    WorkflowRun {
        definition: serde_json::Value,
    },
    WorkflowPause,
    WorkflowResume,
    WorkflowCancel,
    WorkflowVerify,
    RemoteViewOpen,
    RemoteViewStatus,
    RemoteViewRevoke,
}

impl BrowserWorkspaceAction {
    pub fn operation(&self) -> BrowserOperation {
        match self {
            Self::Start => BrowserOperation::Start,
            Self::Stop => BrowserOperation::Stop,
            Self::Reconnect => BrowserOperation::Reconnect,
            Self::State => BrowserOperation::State,
            Self::Observe => BrowserOperation::Observe,
            Self::Snapshot => BrowserOperation::Snapshot,
            Self::Semantic => BrowserOperation::Semantic,
            Self::Diff => BrowserOperation::Diff,
            Self::Targets => BrowserOperation::Targets,
            Self::SelectTarget { .. } => BrowserOperation::SelectTarget,
            Self::Navigate { .. } => BrowserOperation::Navigate,
            Self::Back { .. } => BrowserOperation::Back,
            Self::Forward { .. } => BrowserOperation::Forward,
            Self::Reload { .. } => BrowserOperation::Reload,
            Self::StopLoading { .. } => BrowserOperation::StopLoading,
            Self::Click { .. } => BrowserOperation::Click,
            Self::Type { .. } => BrowserOperation::Type,
            Self::Scroll { .. } => BrowserOperation::Scroll,
            Self::Screenshot => BrowserOperation::Screenshot,
            Self::WorkflowList => BrowserOperation::WorkflowList,
            Self::WorkflowRun { .. } => BrowserOperation::WorkflowRun,
            Self::WorkflowPause => BrowserOperation::WorkflowPause,
            Self::WorkflowResume => BrowserOperation::WorkflowResume,
            Self::WorkflowCancel => BrowserOperation::WorkflowCancel,
            Self::WorkflowVerify => BrowserOperation::WorkflowVerify,
            Self::RemoteViewOpen => BrowserOperation::RemoteViewOpen,
            Self::RemoteViewStatus => BrowserOperation::RemoteViewStatus,
            Self::RemoteViewRevoke => BrowserOperation::RemoteViewRevoke,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrowserWorkspaceIntent {
    Navigate { url: String },
    ActivateSelected,
    TypeSelected { text: String },
    ScrollBrowser { dx: f64, dy: f64 },
    Back,
    Forward,
    Reload,
    StopLoading,
    TakeHumanControl,
    ReturnControl,
    MoveSelection { delta: i32 },
    MoveFocus { backwards: bool },
    OpenPalette,
    CloseOverlay,
    Recover,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWorkspaceState {
    pub layout: BrowserWorkspaceLayout,
    pub connection: BrowserConnectionPhase,
    pub generation: u64,
    pub owned: bool,
    pub endpoint: Option<String>,
    pub recovery_reason: Option<String>,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub browser_revision: Option<u64>,
    pub geometry_revision: Option<u64>,
    pub entities: Vec<BrowserWorkspaceEntity>,
    pub selected_entity: Option<usize>,
    pub semantic_invalidated: bool,
    pub targets: Vec<BrowserWorkspaceTarget>,
    pub presentation: BrowserPresentationPath,
    pub presentation_reason: Option<String>,
    pub frame_revision: Option<u64>,
    pub workflow: String,
    pub input_owner: BrowserInputOwner,
    pub takeover_pending_reconcile: bool,
    pub focus: BrowserFocus,
    pub semantic_scroll: usize,
    pub palette_open: bool,
    pub address_open: bool,
    pub transient_errors: Vec<String>,
    pub capabilities: BTreeMap<BrowserOperation, BrowserCapability>,
}

impl Default for BrowserWorkspaceState {
    fn default() -> Self {
        Self {
            layout: BrowserWorkspaceLayout::Desktop,
            connection: BrowserConnectionPhase::Detached,
            generation: 0,
            owned: false,
            endpoint: None,
            recovery_reason: None,
            title: "No page".into(),
            url: String::new(),
            loading: false,
            browser_revision: None,
            geometry_revision: None,
            entities: Vec::new(),
            selected_entity: None,
            semantic_invalidated: true,
            targets: Vec::new(),
            presentation: BrowserPresentationPath::SemanticOnly,
            presentation_reason: Some("visual presentation has not been requested".into()),
            frame_revision: None,
            workflow: "idle".into(),
            input_owner: BrowserInputOwner::Glass,
            takeover_pending_reconcile: false,
            focus: BrowserFocus::Semantic,
            semantic_scroll: 0,
            palette_open: false,
            address_open: false,
            transient_errors: Vec::new(),
            capabilities: BrowserOperation::ALL
                .into_iter()
                .map(|operation| (operation, BrowserCapability::available()))
                .collect(),
        }
    }
}

impl BrowserWorkspaceState {
    pub fn selected(&self) -> Option<&BrowserWorkspaceEntity> {
        self.selected_entity
            .and_then(|index| self.entities.get(index))
    }

    pub fn connection_label(&self) -> &'static str {
        match self.connection {
            BrowserConnectionPhase::Detached => "Detached",
            BrowserConnectionPhase::Starting => "Starting",
            BrowserConnectionPhase::Connected => "Connected",
            BrowserConnectionPhase::Recovering => "Recovering",
            BrowserConnectionPhase::Failed => "Failed",
        }
    }

    pub fn presentation_label(&self) -> &'static str {
        match self.presentation {
            BrowserPresentationPath::Herdr => "Herdr",
            BrowserPresentationPath::Kitty => "Kitty",
            BrowserPresentationPath::Sixel => "Sixel",
            BrowserPresentationPath::Ansi => "ANSI",
            BrowserPresentationPath::SemanticOnly => "Semantic",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BrowserWorkspaceController {
    state: BrowserWorkspaceState,
}

impl BrowserWorkspaceController {
    pub fn new(layout: BrowserWorkspaceLayout) -> Self {
        let state = BrowserWorkspaceState {
            layout,
            ..BrowserWorkspaceState::default()
        };
        Self { state }
    }

    pub fn for_adapter(
        layout: BrowserWorkspaceLayout,
        adapter: BrowserWorkspaceAdapterKind,
    ) -> Self {
        let mut controller = Self::new(layout);
        if adapter == BrowserWorkspaceAdapterKind::Standalone {
            for operation in [
                BrowserOperation::RemoteViewOpen,
                BrowserOperation::RemoteViewStatus,
                BrowserOperation::RemoteViewRevoke,
            ] {
                controller.set_capability(
                    operation,
                    BrowserCapability::unavailable(
                        "use Glass Dev for resident workflows and Remote View",
                    ),
                );
            }
        }
        controller
    }

    pub fn state(&self) -> &BrowserWorkspaceState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut BrowserWorkspaceState {
        &mut self.state
    }

    pub fn set_capability(&mut self, operation: BrowserOperation, capability: BrowserCapability) {
        self.state.capabilities.insert(operation, capability);
    }

    pub fn connected(&mut self, owned: bool, endpoint: Option<String>, revision: Option<u64>) {
        self.state.connection = BrowserConnectionPhase::Connected;
        self.state.generation = self.state.generation.saturating_add(1);
        self.state.owned = owned;
        self.state.endpoint = endpoint.map(bounded);
        self.state.recovery_reason = None;
        self.observe_revision(revision);
    }

    pub fn disconnected(&mut self, reason: impl Into<String>, recoverable: bool) {
        self.state.connection = if recoverable {
            BrowserConnectionPhase::Recovering
        } else {
            BrowserConnectionPhase::Failed
        };
        self.state.recovery_reason = Some(bounded(reason.into()));
        self.invalidate_semantics();
    }

    pub fn update_page(
        &mut self,
        title: impl Into<String>,
        url: impl Into<String>,
        loading: bool,
        revision: Option<u64>,
    ) {
        self.state.title = bounded(title.into());
        self.state.url = bounded(url.into());
        self.state.loading = loading;
        self.observe_revision(revision);
    }

    pub fn replace_entities(&mut self, revision: u64, entities: Vec<BrowserWorkspaceEntity>) {
        let previous = self.state.selected().map(|entity| entity.reference.clone());
        self.state.entities = entities
            .into_iter()
            .take(MAX_ENTITIES)
            .map(|mut entity| {
                entity.reference = bounded(entity.reference);
                entity.role = bounded(entity.role);
                entity.name = bounded(entity.name);
                entity.revision = revision;
                entity
            })
            .collect();
        self.state.browser_revision = Some(revision);
        self.state.semantic_invalidated = false;
        self.state.selected_entity = previous
            .and_then(|reference| {
                self.state
                    .entities
                    .iter()
                    .position(|entity| entity.reference == reference)
            })
            .or_else(|| (!self.state.entities.is_empty()).then_some(0));
        self.keep_selection_visible(8);
    }

    pub fn replace_targets(&mut self, targets: Vec<BrowserWorkspaceTarget>) {
        self.state.targets = targets
            .into_iter()
            .take(MAX_TARGETS)
            .map(|mut target| {
                target.id = bounded(target.id);
                target.title = bounded(target.title);
                target.url = bounded(target.url);
                target
            })
            .collect();
    }

    pub fn fail_action(&mut self, error: impl Into<String>, stale: bool) {
        if stale {
            self.invalidate_semantics();
        }
        if self.state.transient_errors.len() == MAX_TRANSIENT_ERRORS {
            self.state.transient_errors.remove(0);
        }
        self.state.transient_errors.push(bounded(error.into()));
    }

    pub fn reconcile_takeover(&mut self) {
        self.state.takeover_pending_reconcile = false;
        self.state.input_owner = BrowserInputOwner::Glass;
    }

    pub fn reduce(
        &mut self,
        intent: BrowserWorkspaceIntent,
    ) -> Result<Option<BrowserWorkspaceAction>, String> {
        let action = match intent {
            BrowserWorkspaceIntent::Navigate { url } => {
                self.state.address_open = false;
                Some(BrowserWorkspaceAction::Navigate {
                    url: bounded(url),
                    expected_revision: self.required_revision()?,
                })
            }
            BrowserWorkspaceIntent::ActivateSelected => {
                let selected = self
                    .state
                    .selected()
                    .filter(|entity| entity.actionable)
                    .ok_or("no actionable semantic entity is selected")?;
                Some(BrowserWorkspaceAction::Click {
                    target: selected.reference.clone(),
                    expected_revision: selected.revision,
                })
            }
            BrowserWorkspaceIntent::TypeSelected { text } => {
                let selected = self
                    .state
                    .selected()
                    .ok_or("no semantic entity is selected")?;
                Some(BrowserWorkspaceAction::Type {
                    target: Some(selected.reference.clone()),
                    text: bounded(text),
                    expected_revision: selected.revision,
                })
            }
            BrowserWorkspaceIntent::ScrollBrowser { dx, dy } => {
                Some(BrowserWorkspaceAction::Scroll {
                    dx,
                    dy,
                    expected_revision: self.required_revision()?,
                })
            }
            BrowserWorkspaceIntent::Back => Some(BrowserWorkspaceAction::Back {
                expected_revision: self.required_revision()?,
            }),
            BrowserWorkspaceIntent::Forward => Some(BrowserWorkspaceAction::Forward {
                expected_revision: self.required_revision()?,
            }),
            BrowserWorkspaceIntent::Reload => Some(BrowserWorkspaceAction::Reload {
                expected_revision: self.required_revision()?,
            }),
            BrowserWorkspaceIntent::StopLoading => Some(BrowserWorkspaceAction::StopLoading {
                expected_revision: self.required_revision()?,
            }),
            BrowserWorkspaceIntent::TakeHumanControl => {
                if self.state.input_owner == BrowserInputOwner::Agent {
                    self.state.takeover_pending_reconcile = true;
                }
                self.state.input_owner = BrowserInputOwner::Human;
                None
            }
            BrowserWorkspaceIntent::ReturnControl => {
                if self.state.takeover_pending_reconcile {
                    return Err("reconcile the agent checkpoint before returning control".into());
                }
                self.state.input_owner = BrowserInputOwner::Glass;
                None
            }
            BrowserWorkspaceIntent::MoveSelection { delta } => {
                self.move_selection(delta);
                None
            }
            BrowserWorkspaceIntent::MoveFocus { backwards } => {
                self.move_focus(backwards);
                None
            }
            BrowserWorkspaceIntent::OpenPalette => {
                self.state.palette_open = true;
                self.state.focus = BrowserFocus::Palette;
                None
            }
            BrowserWorkspaceIntent::CloseOverlay => {
                self.state.palette_open = false;
                self.state.address_open = false;
                if self.state.input_owner == BrowserInputOwner::Human {
                    self.state.input_owner = BrowserInputOwner::Glass;
                }
                self.state.focus = BrowserFocus::Semantic;
                None
            }
            BrowserWorkspaceIntent::Recover => Some(BrowserWorkspaceAction::Reconnect),
        };
        if let Some(action) = action.as_ref() {
            self.require_capability(action.operation())?;
            if self.state.input_owner == BrowserInputOwner::Agent
                && matches!(
                    action.operation(),
                    BrowserOperation::Navigate
                        | BrowserOperation::Back
                        | BrowserOperation::Forward
                        | BrowserOperation::Reload
                        | BrowserOperation::StopLoading
                        | BrowserOperation::Click
                        | BrowserOperation::Type
                        | BrowserOperation::Scroll
                )
            {
                return Err("agent owns browser mutation; take control first".into());
            }
        }
        Ok(action)
    }

    fn observe_revision(&mut self, revision: Option<u64>) {
        if let Some(revision) = revision
            && self
                .state
                .browser_revision
                .is_some_and(|old| old != revision)
        {
            self.invalidate_semantics();
        }
        if revision.is_some() {
            self.state.browser_revision = revision;
        }
    }

    fn invalidate_semantics(&mut self) {
        self.state.semantic_invalidated = true;
        self.state.entities.clear();
        self.state.selected_entity = None;
        self.state.semantic_scroll = 0;
    }

    fn required_revision(&self) -> Result<u64, String> {
        self.state
            .browser_revision
            .ok_or_else(|| "observe the page before performing a browser action".into())
    }

    fn require_capability(&self, operation: BrowserOperation) -> Result<(), String> {
        let capability = self
            .state
            .capabilities
            .get(&operation)
            .ok_or_else(|| "backend did not declare this browser operation".to_string())?;
        if capability.available {
            Ok(())
        } else {
            Err(capability
                .reason
                .clone()
                .unwrap_or_else(|| "browser operation is unavailable".into()))
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.state.entities.is_empty() {
            self.state.selected_entity = None;
            return;
        }
        let current = self.state.selected_entity.unwrap_or(0) as i32;
        let maximum = self.state.entities.len().saturating_sub(1) as i32;
        self.state.selected_entity = Some((current + delta).clamp(0, maximum) as usize);
        self.keep_selection_visible(8);
    }

    fn keep_selection_visible(&mut self, visible_rows: usize) {
        let Some(selected) = self.state.selected_entity else {
            return;
        };
        if selected < self.state.semantic_scroll {
            self.state.semantic_scroll = selected;
        } else if selected >= self.state.semantic_scroll.saturating_add(visible_rows) {
            self.state.semantic_scroll = selected.saturating_sub(visible_rows - 1);
        }
    }

    fn move_focus(&mut self, backwards: bool) {
        const ORDER: [BrowserFocus; 4] = [
            BrowserFocus::Controls,
            BrowserFocus::Visual,
            BrowserFocus::Semantic,
            BrowserFocus::Footer,
        ];
        let index = ORDER
            .iter()
            .position(|focus| *focus == self.state.focus)
            .unwrap_or(0);
        self.state.focus = if backwards {
            ORDER[(index + ORDER.len() - 1) % ORDER.len()]
        } else {
            ORDER[(index + 1) % ORDER.len()]
        };
    }
}

fn bounded(mut value: String) -> String {
    if value.len() <= MAX_TEXT_BYTES {
        return value;
    }
    let mut boundary = MAX_TEXT_BYTES;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(reference: &str, revision: u64) -> BrowserWorkspaceEntity {
        BrowserWorkspaceEntity {
            reference: reference.into(),
            role: "button".into(),
            name: reference.into(),
            actionable: true,
            revision,
        }
    }

    #[test]
    fn visible_selection_supplies_revision_and_invalidates_when_stale() {
        let mut workspace = BrowserWorkspaceController::default();
        workspace.connected(true, Some("127.0.0.1:9222".into()), Some(7));
        workspace.replace_entities(7, vec![entity("r7:b42", 1)]);
        let action = workspace
            .reduce(BrowserWorkspaceIntent::ActivateSelected)
            .unwrap()
            .unwrap();
        assert_eq!(
            action,
            BrowserWorkspaceAction::Click {
                target: "r7:b42".into(),
                expected_revision: 7
            }
        );
        workspace.fail_action("stale browser revision", true);
        assert!(workspace.state().semantic_invalidated);
        assert!(workspace.state().selected().is_none());
    }

    #[test]
    fn focus_selection_bounds_and_takeover_are_deterministic() {
        let mut workspace = BrowserWorkspaceController::default();
        workspace.replace_entities(4, (0..20).map(|n| entity(&format!("b{n}"), 4)).collect());
        workspace
            .reduce(BrowserWorkspaceIntent::MoveSelection { delta: 19 })
            .unwrap();
        assert_eq!(workspace.state().selected_entity, Some(19));
        assert_eq!(workspace.state().semantic_scroll, 12);
        workspace.state_mut().input_owner = BrowserInputOwner::Agent;
        workspace
            .reduce(BrowserWorkspaceIntent::TakeHumanControl)
            .unwrap();
        assert!(workspace.state().takeover_pending_reconcile);
        assert!(
            workspace
                .reduce(BrowserWorkspaceIntent::ReturnControl)
                .is_err()
        );
        workspace.reconcile_takeover();
        assert_eq!(workspace.state().input_owner, BrowserInputOwner::Glass);
    }

    #[test]
    fn unsupported_capability_is_visible_and_fails_before_execution() {
        let mut workspace = BrowserWorkspaceController::default();
        workspace.set_capability(
            BrowserOperation::RemoteViewOpen,
            BrowserCapability::unavailable("standalone remote view is disabled"),
        );
        let capability = &workspace.state().capabilities[&BrowserOperation::RemoteViewOpen];
        assert!(!capability.available);
        assert_eq!(
            capability.reason.as_deref(),
            Some("standalone remote view is disabled")
        );
    }

    #[test]
    fn state_is_bounded_and_recovery_does_not_discard_workspace() {
        let mut workspace = BrowserWorkspaceController::default();
        workspace.replace_entities(2, (0..700).map(|n| entity(&format!("b{n}"), 2)).collect());
        workspace.replace_targets(
            (0..100)
                .map(|n| BrowserWorkspaceTarget {
                    id: n.to_string(),
                    title: "target".into(),
                    url: "https://example.com".into(),
                    selected: n == 0,
                })
                .collect(),
        );
        workspace.disconnected("compatible endpoint disappeared", true);
        assert_eq!(workspace.state().entities.len(), 0);
        assert_eq!(workspace.state().targets.len(), MAX_TARGETS);
        assert_eq!(
            workspace.state().connection,
            BrowserConnectionPhase::Recovering
        );
    }

    #[test]
    fn standalone_and_embedded_adapters_share_revision_selection_and_recovery_contract() {
        for adapter in [
            BrowserWorkspaceAdapterKind::Standalone,
            BrowserWorkspaceAdapterKind::EmbeddedDevelopment,
        ] {
            let mut workspace =
                BrowserWorkspaceController::for_adapter(BrowserWorkspaceLayout::Phone, adapter);
            workspace.connected(
                adapter == BrowserWorkspaceAdapterKind::Standalone,
                None,
                Some(9),
            );
            workspace.replace_entities(9, vec![entity("r9:b8", 9)]);
            assert_eq!(
                workspace
                    .reduce(BrowserWorkspaceIntent::ActivateSelected)
                    .unwrap(),
                Some(BrowserWorkspaceAction::Click {
                    target: "r9:b8".into(),
                    expected_revision: 9,
                })
            );
            workspace.disconnected("target closed", true);
            assert_eq!(
                workspace.state().connection,
                BrowserConnectionPhase::Recovering
            );
            assert!(workspace.state().semantic_invalidated);
            assert!(
                workspace
                    .reduce(BrowserWorkspaceIntent::Recover)
                    .unwrap()
                    .is_some()
            );
        }
    }
}
