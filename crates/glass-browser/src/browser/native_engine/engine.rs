use super::browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
use super::config::{
    NativeEngineConfig, resolve_fixture_relative_url, validate_url_text, without_fragment,
};
use super::diagnostics::NativeDiagnostic;
use super::dom::NativeDocument;
use super::error::NativeEngineError;
use super::history::{NativeHistory, NativeHistoryDirection};
use super::interaction::{MAX_NATIVE_EFFECTS, NativeAction, NativeEffect, NativeEventKind};
use super::layout::{NativeLayoutSnapshot, NativePoint};
use super::lifecycle::NativeLifecycleState;
use super::origin::NativeOrigin;
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
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

/// Bounded diagnostics associated with the current native document revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDiagnosticsSnapshot {
    pub revision: u64,
    pub diagnostics: Vec<NativeDiagnostic>,
    pub truncated: bool,
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
    scroll_offset: NativePoint,
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
            scroll_offset: NativePoint { x: 0, y: 0 },
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

    /// Return the current root viewport scroll offset.
    pub const fn scroll_offset(&self) -> NativePoint {
        self.scroll_offset
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
        let resource = self.loader.load(&url)?;
        if self.is_same_document_navigation(&resource.url) {
            self.commit_same_document_navigation(resource.url, HistoryCommit::Push)?;
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            self.commit_navigation(prepared)?;
        }
        Ok(self.snapshot_unchecked())
    }

    /// Move to the previous bounded local history entry.
    ///
    /// `None` is an explicit boundary no-op and leaves the engine unchanged.
    pub fn go_back(&mut self) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history(NativeHistoryDirection::Back, "go back")
    }

    /// Move to the next bounded local history entry.
    ///
    /// `None` is an explicit boundary no-op and leaves the engine unchanged.
    pub fn go_forward(&mut self) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.traverse_history(NativeHistoryDirection::Forward, "go forward")
    }

    pub fn snapshot(&self) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("evidence")?;
        Ok(self.snapshot_unchecked())
    }

    /// Return diagnostics for CSS that the bounded native presentation model
    /// intentionally ignored or could not parse.
    pub fn diagnostics(&self) -> Result<NativeDiagnosticsSnapshot, NativeEngineError> {
        self.require_running("diagnostics")?;
        Ok(NativeDiagnosticsSnapshot {
            revision: self.revision,
            diagnostics: self.document.diagnostics().to_vec(),
            truncated: self.document.diagnostics_truncated(),
        })
    }

    pub fn semantic_nodes(&self) -> Result<Vec<super::dom::NativeSemanticNode>, NativeEngineError> {
        self.require_running("semantic DOM")?;
        Ok(self.document.semantic_nodes())
    }

    /// Return the current document's derived integer-pixel layout.
    pub fn layout(&self) -> Result<NativeLayoutSnapshot, NativeEngineError> {
        self.require_running("layout")?;
        self.document
            .layout(self.config.viewport)?
            .with_scroll_offset(self.scroll_offset)
    }

    /// Hit test one point in the configured viewport without scrolling or
    /// adjusting the requested coordinates.
    pub fn hit_test(
        &self,
        x: i64,
        y: i64,
    ) -> Result<Option<super::dom::NativeNodeId>, NativeEngineError> {
        self.require_running("hit testing")?;
        self.layout()?.hit_test(x, y)
    }

    /// Return the current document's immutable Rust display-list projection.
    pub fn display_list(&self) -> Result<NativeDisplayList, NativeEngineError> {
        self.require_running("display list")?;
        NativeDisplayList::build(&self.document, &self.layout()?)
    }

    /// Replay the current document's display list into a bounded Rust surface.
    pub fn rasterize(&self) -> Result<NativeSurface, NativeEngineError> {
        self.require_running("raster surface")?;
        self.display_list()?.rasterize()
    }

    /// Encode the current logical renderer surface as bounded PNG bytes.
    pub fn capture_png(&self) -> Result<Vec<u8>, NativeEngineError> {
        self.require_running("capture")?;
        self.rasterize()?.to_png()
    }

    /// Apply one semantic action and advance the document revision exactly
    /// once. Target resolution and actionability checks happen before state
    /// mutation, so rejected actions leave the document unchanged.
    pub fn action(
        &mut self,
        action: NativeAction,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.require_running("action")?;
        let (events, accepted) = match action {
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                if !self.document.is_hidden_for_layout(id) {
                    self.require_layout_actionable(id)?;
                }
                if let Some(href) = self.document.link_href(id).map(str::to_owned)
                    && !href.is_empty()
                {
                    return self.activate_link(id, &href);
                }
                (self.document.apply_click(id)?, true)
            }
            NativeAction::Type { target, text } => {
                let id = self.document.resolve_target(&target)?;
                if !self.document.is_hidden_for_layout(id) {
                    self.require_layout_actionable(id)?;
                }
                (self.document.apply_type(id, &text)?, true)
            }
            NativeAction::Scroll { delta_x, delta_y } => {
                let moved = self.apply_scroll(delta_x, delta_y)?;
                let events = moved
                    .then(|| (self.document.root(), NativeEventKind::Scroll))
                    .into_iter()
                    .collect();
                (events, moved)
            }
        };
        if !accepted {
            return Ok(NativeActionResult {
                revision: self.revision,
                accepted: false,
            });
        }
        let next_revision = self.next_revision()?;
        self.document.set_revision(next_revision);
        self.revision = next_revision;
        self.history.update_current_scroll(self.scroll_offset);
        self.record_effects(events);
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn activate_link(
        &mut self,
        id: super::dom::NativeNodeId,
        href: &str,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let target_url = self.resolve_link_href(href)?;
        let resource = self.loader.load(&target_url)?;
        let revision = self.next_revision()?;
        if self.is_same_document_navigation(&resource.url) {
            let scroll_offset = self.fragment_scroll_offset(&resource.url)?;
            self.run_commit_task(
                NativeTask::CommitSameDocumentNavigation,
                "link same-document navigation",
            )?;
            let events = self.document.apply_click(id)?;
            self.document.set_revision(revision);
            self.url = resource.url.clone();
            self.scroll_offset = scroll_offset;
            self.revision = revision;
            self.history.push(resource.url, revision, scroll_offset);
            self.record_effects(events);
            return Ok(NativeActionResult {
                revision,
                accepted: true,
            });
        }

        let prepared = self.prepare_navigation_resource(resource)?;
        let scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task(NativeTask::CommitNavigation, "link navigation")?;
        let _events = self.document.apply_click(id)?;
        self.document = prepared.document;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.history.push(self.url.clone(), revision, scroll_offset);
        Ok(NativeActionResult {
            revision,
            accepted: true,
        })
    }

    fn resolve_link_href(&self, href: &str) -> Result<String, NativeEngineError> {
        validate_url_text("link href", href)?;
        if let Some(fragment) = href.strip_prefix('#') {
            let target = format!("{}#{fragment}", without_fragment(&self.url));
            validate_url_text("link target URL", &target)?;
            return Ok(target);
        }
        if url::Url::parse(href).is_ok() {
            return Ok(href.to_owned());
        }
        resolve_fixture_relative_url(&self.url, href)
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
        self.prepare_navigation_resource(resource)
    }

    fn prepare_navigation_resource(
        &self,
        resource: NativeResource,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let next_revision = self.next_revision()?;
        let generation = u32::try_from(next_revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let document =
            NativeDocument::parse_with_generation(&resource.body, &self.config.limits, generation)?;
        Ok(PreparedNavigation { resource, document })
    }

    fn commit_navigation(&mut self, prepared: PreparedNavigation) -> Result<(), NativeEngineError> {
        let scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task(NativeTask::CommitNavigation, "navigation")?;
        let revision = self.next_revision()?;
        self.document = prepared.document;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.history.push(self.url.clone(), revision, scroll_offset);
        Ok(())
    }

    fn commit_same_document_navigation(
        &mut self,
        url: String,
        history_commit: HistoryCommit,
    ) -> Result<(), NativeEngineError> {
        let scroll_offset = match &history_commit {
            HistoryCommit::Push => self.fragment_scroll_offset(&url)?,
            HistoryCommit::Activate(index) => self
                .history
                .entry(*index)
                .map(|entry| entry.scroll_offset)
                .ok_or_else(|| NativeEngineError::Scheduler {
                    reason: "history target is no longer available".into(),
                })?,
        };
        self.run_commit_task(
            NativeTask::CommitSameDocumentNavigation,
            "same-document navigation",
        )?;
        let revision = self.next_revision()?;
        self.document.set_revision(revision);
        self.url = url.clone();
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        match history_commit {
            HistoryCommit::Push => self.history.push(url, revision, scroll_offset),
            HistoryCommit::Activate(index) => {
                self.history.activate(index, revision).ok_or_else(|| {
                    NativeEngineError::Scheduler {
                        reason: "history entry disappeared during same-document traversal".into(),
                    }
                })?;
            }
        }
        Ok(())
    }

    fn commit_history_navigation(
        &mut self,
        prepared: PreparedNavigation,
        history_index: usize,
    ) -> Result<(), NativeEngineError> {
        if self.history.entry(history_index).is_none() {
            return Err(NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            });
        }
        let saved_scroll = self
            .history
            .entry(history_index)
            .map(|entry| entry.scroll_offset)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?;
        let max_scroll_y = prepared
            .document
            .layout(self.config.viewport)?
            .max_scroll_offset()
            .y;
        let scroll_offset = NativePoint {
            x: 0,
            y: saved_scroll.y.min(max_scroll_y),
        };
        self.run_commit_task(NativeTask::TraverseHistory, "history traversal")?;
        let revision = self.next_revision()?;
        self.document = prepared.document;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.history
            .activate(history_index, revision)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target disappeared during traversal".into(),
            })?;
        Ok(())
    }

    fn traverse_history(
        &mut self,
        direction: NativeHistoryDirection,
        operation: &str,
    ) -> Result<Option<NativeEngineSnapshot>, NativeEngineError> {
        self.require_running(operation)?;
        let Some(history_index) = self.history.target_index(direction) else {
            return Ok(None);
        };
        let target_url = self
            .history
            .entry(history_index)
            .ok_or_else(|| NativeEngineError::Scheduler {
                reason: "history target is no longer available".into(),
            })?
            .url
            .clone();
        let resource = self.loader.load(&target_url)?;
        if self.is_same_document_navigation(&resource.url) {
            self.commit_same_document_navigation(
                resource.url,
                HistoryCommit::Activate(history_index),
            )?;
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            self.commit_history_navigation(prepared, history_index)?;
        }
        Ok(Some(self.snapshot_unchecked()))
    }

    fn is_same_document_navigation(&self, target_url: &str) -> bool {
        self.url != target_url && without_fragment(&self.url) == without_fragment(target_url)
    }

    fn fragment_scroll_offset(&self, target_url: &str) -> Result<NativePoint, NativeEngineError> {
        self.fragment_scroll_offset_for_document(&self.document, target_url, self.scroll_offset)
    }

    fn fragment_scroll_offset_for_document(
        &self,
        document: &NativeDocument,
        target_url: &str,
        fallback: NativePoint,
    ) -> Result<NativePoint, NativeEngineError> {
        let Some((_, fragment)) = target_url.split_once('#') else {
            return Ok(fallback);
        };
        if fragment.is_empty() {
            return Ok(fallback);
        }
        let Some(target_id) = document.fragment_target(fragment) else {
            return Ok(fallback);
        };
        let layout = document.layout(self.config.viewport)?;
        let Some(target_box) = layout.box_for(target_id) else {
            return Ok(fallback);
        };
        if target_box.width == 0 || target_box.height == 0 {
            return Ok(fallback);
        }
        Ok(NativePoint {
            x: 0,
            y: target_box.y.min(layout.max_scroll_offset().y),
        })
    }

    fn run_commit_task(
        &mut self,
        expected: NativeTask,
        operation: &str,
    ) -> Result<(), NativeEngineError> {
        self.scheduler.schedule(expected, 0)?;
        let Some(task) = self.scheduler.pop_ready() else {
            return Err(NativeEngineError::Scheduler {
                reason: format!("{operation} commit was not ready at the current logical time"),
            });
        };
        if task.task != expected {
            return Err(NativeEngineError::Scheduler {
                reason: format!("{operation} produced an unexpected task kind"),
            });
        }
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
        let hit = self.layout()?.hit_test(x, y)?;
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
        let layout = self.layout()?;
        let visible = layout
            .viewport_rect_for(id)
            .is_some_and(|rect| rect.width > 0 && rect.height > 0);
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

    fn apply_scroll(&mut self, delta_x: i32, delta_y: i32) -> Result<bool, NativeEngineError> {
        if delta_x != 0 {
            return Err(NativeEngineError::invalid(
                "native scroll action",
                "horizontal scrolling is unsupported",
            ));
        }
        if delta_y == 0 {
            return Ok(false);
        }
        let max_scroll_y = self
            .document
            .layout(self.config.viewport)?
            .max_scroll_offset()
            .y;
        let current = i64::from(self.scroll_offset.y);
        let requested = current.saturating_add(i64::from(delta_y));
        let next = requested.clamp(0, i64::from(max_scroll_y));
        let next = u32::try_from(next).map_err(|_| {
            NativeEngineError::invalid("native scroll action", "scroll offset exceeds bounds")
        })?;
        if next == self.scroll_offset.y {
            return Ok(false);
        }
        self.scroll_offset.y = next;
        Ok(true)
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

enum HistoryCommit {
    Push,
    Activate(usize),
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
