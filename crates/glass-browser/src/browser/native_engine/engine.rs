use super::browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
use super::config::NativeEngineConfig;
use super::dom::NativeDocument;
use super::error::NativeEngineError;
use super::history::NativeHistory;
use super::interaction::{MAX_NATIVE_EFFECTS, NativeAction, NativeEffect, NativeEventKind};
use super::layout::NativeLayoutSnapshot;
use super::lifecycle::NativeLifecycleState;
use super::origin::NativeOrigin;
use super::paint::NativeDisplayList;
use super::resource_loader::{NativeResource, NativeResourceLoader};
use super::scheduler::{DeterministicScheduler, NativeTask};
use std::collections::VecDeque;

/// Bounded observation of the current native document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEngineSnapshot {
    pub lifecycle: NativeLifecycleState,
    pub url: String,
    pub origin: NativeOrigin,
    pub title: String,
    pub title_truncated: bool,
    pub visible_text: String,
    pub text_truncated: bool,
    pub revision: u64,
    pub viewport: super::config::Viewport,
}

/// Result of an accepted native semantic action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeActionResult {
    pub revision: u64,
    pub accepted: bool,
}

/// Bounded native effects observed since a caller's revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEffectsSnapshot {
    pub revision: u64,
    pub changed: bool,
    pub effects: Vec<NativeEffect>,
}

/// Single-owner native browser kernel.
pub struct NativeEngine {
    config: NativeEngineConfig,
    loader: NativeResourceLoader,
    scheduler: DeterministicScheduler,
    history: NativeHistory,
    lifecycle: NativeLifecycleState,
    document: NativeDocument,
    url: String,
    origin: NativeOrigin,
    revision: u64,
    effects: VecDeque<NativeEffect>,
}

impl NativeEngine {
    pub fn new(config: NativeEngineConfig) -> Result<Self, NativeEngineError> {
        config.validate()?;
        let loader = NativeResourceLoader::new(&config)?;
        loader.load(&config.initial_url)?;
        let scheduler = DeterministicScheduler::new(config.limits.max_scheduler_tasks)?;
        let max_history_entries = config.limits.max_history_entries;
        Ok(Self {
            url: config.initial_url.clone(),
            config,
            loader,
            scheduler,
            history: NativeHistory::new(max_history_entries),
            lifecycle: NativeLifecycleState::New,
            document: NativeDocument::empty(),
            origin: NativeOrigin::Opaque,
            revision: 0,
            effects: VecDeque::new(),
        })
    }

    pub fn config(&self) -> &NativeEngineConfig {
        &self.config
    }

    pub const fn lifecycle(&self) -> NativeLifecycleState {
        self.lifecycle
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn initialize(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => return Ok(()),
            NativeLifecycleState::Closed => {
                return Err(
                    self.lifecycle_error("initialize", "a closed native engine cannot be reopened")
                );
            }
            NativeLifecycleState::New => {}
        }
        let prepared = self.prepare_navigation(&self.config.initial_url.clone())?;
        self.commit_navigation(prepared)?;
        self.lifecycle = NativeLifecycleState::Running;
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => {
                self.lifecycle = NativeLifecycleState::Closed;
                Ok(())
            }
            NativeLifecycleState::New => Err(self.lifecycle_error(
                "close",
                "the native engine must be initialized before close",
            )),
            NativeLifecycleState::Closed => {
                Err(self.lifecycle_error("close", "the native engine is already closed"))
            }
        }
    }

    pub fn navigate(
        &mut self,
        url: impl Into<String>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("navigate")?;
        let url = url.into();
        let prepared = self.prepare_navigation(&url)?;
        self.commit_navigation(prepared)?;
        Ok(self.snapshot_unchecked())
    }

    pub fn snapshot(&self) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("evidence")?;
        Ok(self.snapshot_unchecked())
    }

    pub fn semantic_nodes(&self) -> Result<Vec<super::dom::NativeSemanticNode>, NativeEngineError> {
        self.require_running("semantic DOM")?;
        Ok(self.document.semantic_nodes())
    }

    /// Return the current document's derived integer-pixel layout.
    pub fn layout(&self) -> Result<NativeLayoutSnapshot, NativeEngineError> {
        self.require_running("layout")?;
        self.document.layout(self.config.viewport)
    }

    /// Hit test one point in the configured viewport without scrolling or
    /// adjusting the requested coordinates.
    pub fn hit_test(
        &self,
        x: i64,
        y: i64,
    ) -> Result<Option<super::dom::NativeNodeId>, NativeEngineError> {
        self.require_running("hit testing")?;
        self.document.hit_test(self.config.viewport, x, y)
    }

    /// Return the current document's immutable Rust display-list projection.
    pub fn display_list(&self) -> Result<NativeDisplayList, NativeEngineError> {
        self.require_running("display list")?;
        self.document.display_list(self.config.viewport)
    }

    /// Apply one semantic action and advance the document revision exactly
    /// once. Target resolution and actionability checks happen before state
    /// mutation, so rejected actions leave the document unchanged.
    pub fn action(
        &mut self,
        action: NativeAction,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.require_running("action")?;
        let events = match action {
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                if !self.document.is_hidden_for_layout(id) {
                    self.require_layout_actionable(id)?;
                }
                self.document.apply_click(id)?
            }
            NativeAction::Type { target, text } => {
                let id = self.document.resolve_target(&target)?;
                if !self.document.is_hidden_for_layout(id) {
                    self.require_layout_actionable(id)?;
                }
                self.document.apply_type(id, &text)?
            }
        };
        let next_revision = self.next_revision()?;
        self.document.set_revision(next_revision);
        self.revision = next_revision;
        self.record_effects(events);
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    pub fn effects_since(
        &self,
        since_revision: u64,
    ) -> Result<NativeEffectsSnapshot, NativeEngineError> {
        self.require_running("effects")?;
        if since_revision > self.revision {
            return Err(NativeEngineError::invalid(
                "since revision",
                "since revision cannot exceed current native revision",
            ));
        }
        Ok(NativeEffectsSnapshot {
            revision: self.revision,
            changed: since_revision < self.revision,
            effects: self
                .effects
                .iter()
                .filter(|effect| effect.revision > since_revision)
                .copied()
                .collect(),
        })
    }

    fn snapshot_unchecked(&self) -> NativeEngineSnapshot {
        let (title, title_truncated) = self.document.title(self.config.limits.max_text_bytes);
        let (visible_text, text_truncated) = self
            .document
            .visible_text(self.config.limits.max_text_bytes);
        NativeEngineSnapshot {
            lifecycle: self.lifecycle,
            url: self.url.clone(),
            origin: self.origin,
            title,
            title_truncated,
            visible_text,
            text_truncated,
            revision: self.revision,
            viewport: self.config.viewport,
        }
    }

    pub fn context(&self) -> Result<NativeBrowsingContext, NativeEngineError> {
        self.require_running("contexts")?;
        Ok(NativeBrowsingContext {
            context_id: NATIVE_CONTEXT_ID.into(),
            url: self.url.clone(),
            origin: self.origin,
            active: true,
        })
    }

    pub fn history(&self) -> &NativeHistory {
        &self.history
    }

    pub fn scheduler(&self) -> &DeterministicScheduler {
        &self.scheduler
    }

    fn prepare_navigation(&self, url: &str) -> Result<PreparedNavigation, NativeEngineError> {
        let resource = self.loader.load(url)?;
        let next_revision = self.next_revision()?;
        let generation = u32::try_from(next_revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let document =
            NativeDocument::parse_with_generation(&resource.body, &self.config.limits, generation)?;
        Ok(PreparedNavigation { resource, document })
    }

    fn commit_navigation(&mut self, prepared: PreparedNavigation) -> Result<(), NativeEngineError> {
        self.scheduler.schedule(NativeTask::CommitNavigation, 0)?;
        let Some(task) = self.scheduler.pop_ready() else {
            return Err(NativeEngineError::Scheduler {
                reason: "navigation commit was not ready at the current logical time".into(),
            });
        };
        if task.task != NativeTask::CommitNavigation {
            return Err(NativeEngineError::Scheduler {
                reason: "navigation produced an unexpected task kind".into(),
            });
        }
        let revision = self.next_revision()?;
        self.document = prepared.document;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.revision = revision;
        self.history.push(self.url.clone(), revision);
        Ok(())
    }

    fn next_revision(&self) -> Result<u64, NativeEngineError> {
        self.revision.checked_add(1).ok_or_else(|| {
            NativeEngineError::limit("document revisions", u64::MAX as usize, usize::MAX)
        })
    }

    fn resolve_click_target(
        &self,
        target: &str,
    ) -> Result<super::dom::NativeNodeId, NativeEngineError> {
        let Some((x, y)) = parse_point_target(target)? else {
            return self.document.resolve_target(target);
        };
        let hit = self.document.hit_test(self.config.viewport, x, y)?;
        let hit = hit.ok_or_else(|| NativeEngineError::TargetNotActionable {
            reason: "point hit no visible element".into(),
        })?;
        self.document
            .nearest_clickable_ancestor(hit)
            .ok_or_else(|| NativeEngineError::TargetNotActionable {
                reason: "point hit no actionable semantic control".into(),
            })
    }

    fn require_layout_actionable(
        &self,
        id: super::dom::NativeNodeId,
    ) -> Result<(), NativeEngineError> {
        let layout = self.document.layout(self.config.viewport)?;
        let visible = layout.box_for(id).is_some_and(|rect| {
            rect.width > 0
                && rect.height > 0
                && rect.x < self.config.viewport.width
                && rect.y < self.config.viewport.height
        });
        if visible {
            return Ok(());
        }
        Err(NativeEngineError::TargetNotActionable {
            reason: "target has no visible layout box in the native viewport".into(),
        })
    }

    fn record_effects(&mut self, events: Vec<(super::dom::NativeNodeId, NativeEventKind)>) {
        for (node_id, kind) in events {
            while self.effects.len() >= MAX_NATIVE_EFFECTS {
                self.effects.pop_front();
            }
            self.effects.push_back(NativeEffect {
                revision: self.revision,
                node_id,
                kind,
            });
        }
    }

    fn require_running(&self, operation: &str) -> Result<(), NativeEngineError> {
        if self.lifecycle == NativeLifecycleState::Running {
            return Ok(());
        }
        Err(self.lifecycle_error(operation, "initialize the native engine first"))
    }

    fn lifecycle_error(&self, operation: &str, reason: &str) -> NativeEngineError {
        NativeEngineError::Lifecycle {
            operation: operation.into(),
            state: self.lifecycle,
            reason: reason.into(),
        }
    }
}

struct PreparedNavigation {
    resource: NativeResource,
    document: NativeDocument,
}

fn parse_point_target(target: &str) -> Result<Option<(i64, i64)>, NativeEngineError> {
    let Some(value) = target.strip_prefix("point=") else {
        return Ok(None);
    };
    let Some((x, y)) = value.split_once(',') else {
        return Err(NativeEngineError::invalid(
            "action locator",
            "point target must use point=<unsigned-x>,<unsigned-y>",
        ));
    };
    if x.is_empty() || y.is_empty() || y.contains(',') {
        return Err(NativeEngineError::invalid(
            "action locator",
            "point target must use point=<unsigned-x>,<unsigned-y>",
        ));
    }
    let x = x.parse::<u32>().map_err(|_| {
        NativeEngineError::invalid(
            "action locator",
            "point coordinates must be unsigned integers",
        )
    })?;
    let y = y.parse::<u32>().map_err(|_| {
        NativeEngineError::invalid(
            "action locator",
            "point coordinates must be unsigned integers",
        )
    })?;
    Ok(Some((i64::from(x), i64::from(y))))
}
