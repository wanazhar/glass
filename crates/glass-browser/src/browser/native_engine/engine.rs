use super::browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
use super::config::{
    NativeEngineConfig, decode_percent_encoded_fragment, decode_text_fragment_terms,
    is_network_url, resolve_fixture_relative_url, validate_url_text, without_fragment,
};
use super::content_process::{
    NativeContentLoad, NativeContentMutation, NativeContentNavigation, NativeContentProcess,
};
use super::diagnostics::NativeDiagnostic;
use super::dom::{NativeDocument, NativeNodeId};
use super::error::NativeEngineError;
use super::error::NativeWorkerFailureKind;
use super::history::{NativeHistory, NativeHistoryDirection};
use super::interaction::{MAX_NATIVE_EFFECTS, NativeAction, NativeEffect, NativeEventKind};
use super::javascript::{
    NativeJavaScriptRuntime, NativeScriptEvaluation, execute_inline_scripts, host_event_script,
};
use super::layout::{NativeLayoutSnapshot, NativePoint};
use super::lifecycle::NativeLifecycleState;
use super::origin::NativeOrigin;
use super::paint::NativeDisplayList;
use super::raster::NativeSurface;
use super::resource_loader::{
    NativeFetchResponse, NativeResource, NativeResourceLoader, referrer_for_navigation,
};
use super::runtime::{NativeRuntimeState, NativeRuntimeTraceEvent};
use super::scheduler::{DeterministicScheduler, NativeTask};
use super::worker::{NativeRuntimeShared, NativeRuntimeWorker};
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
    runtime: NativeRuntimeShared,
    runtime_worker: Option<NativeRuntimeWorker>,
    content_process: Option<NativeContentProcess>,
    javascript: Option<NativeJavaScriptRuntime>,
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
        if !is_network_url(&config.initial_url) {
            loader.load(&config.initial_url)?;
        }
        let runtime = NativeRuntimeShared::new(config.limits.max_scheduler_tasks)?;
        let max_history_entries = config.limits.max_history_entries;
        Ok(Self {
            url: config.initial_url.clone(),
            config,
            loader,
            runtime,
            runtime_worker: None,
            content_process: None,
            javascript: None,
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
        self.runtime.start()?;
        if let Err(error) = self.commit_navigation(prepared) {
            self.runtime.rollback_start()?;
            return Err(error);
        }
        self.lifecycle = NativeLifecycleState::Running;
        Ok(())
    }

    pub async fn initialize_async(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => return Ok(()),
            NativeLifecycleState::Closed => {
                return Err(
                    self.lifecycle_error("initialize", "a closed native engine cannot be reopened")
                );
            }
            NativeLifecycleState::New => {}
        }
        let initial_url = self.config.initial_url.clone();
        let mut content_process = if is_network_url(&initial_url) {
            Some(NativeContentProcess::spawn().await?)
        } else {
            None
        };
        if let Some(process) = content_process.as_mut() {
            process.start().await?;
        }
        let prepared = if let Some(process) = content_process.as_mut() {
            let viewport = self.config.viewport;
            let resource = process
                .load(&initial_url, &self.config.limits, viewport, None)
                .await?;
            self.prepare_navigation_content(resource)?
        } else {
            self.prepare_navigation_async(&initial_url).await?
        };
        let worker = NativeRuntimeWorker::spawn_shared(self.runtime.clone())?;
        worker.start().await?;
        if let Some(process) = content_process.as_mut() {
            process.commit().await?;
        }
        if let Err(error) = self.commit_navigation_async(prepared, &worker).await {
            worker.rollback_start().await?;
            return Err(error);
        }
        self.runtime_worker = Some(worker);
        self.content_process = content_process;
        self.lifecycle = NativeLifecycleState::Running;
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => {
                self.runtime.close()?;
                self.runtime_worker.take();
                self.content_process.take();
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

    pub async fn close_async(&mut self) -> Result<(), NativeEngineError> {
        match self.lifecycle {
            NativeLifecycleState::Running => {
                self.runtime.close()?;
                self.runtime_worker.take();
                self.lifecycle = NativeLifecycleState::Closed;
                if let Some(process) = self.content_process.take() {
                    if process.is_healthy() {
                        process.close().await?;
                    }
                }
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

    pub async fn navigate_async(
        &mut self,
        url: impl Into<String>,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        self.require_running("navigate")?;
        let url = url.into();
        if is_network_url(&url) {
            let referrer = referrer_for_navigation(&self.url, &url)?;
            self.ensure_content_process().await?;
            let viewport = self.config.viewport;
            let content = self
                .content_process
                .as_mut()
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "content process load".into(),
                    reason: "native content process is not running".into(),
                })?
                .load(&url, &self.config.limits, viewport, referrer.as_deref())
                .await?;
            self.commit_content_process().await?;
            if let Some(worker) = self.runtime_worker.clone() {
                return self.navigate_content_async(content, &worker).await;
            }
            return self.navigate_content(content);
        }
        self.content_process.take();
        let resource = self.loader.load_async(&url).await?;
        if let Some(worker) = self.runtime_worker.clone() {
            self.navigate_resource_async(resource, &worker).await
        } else {
            if is_network_url(&resource.url) {
                self.commit_content_process().await?;
            }
            self.navigate_resource(resource)
        }
    }

    fn navigate_resource(
        &mut self,
        resource: NativeResource,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&resource.url) {
            self.commit_same_document_navigation(resource.url, HistoryCommit::Push)?;
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            self.commit_navigation(prepared)?;
        }
        Ok(self.snapshot_unchecked())
    }

    async fn ensure_content_process(&mut self) -> Result<(), NativeEngineError> {
        if self
            .content_process
            .as_ref()
            .is_some_and(|process| !process.is_healthy())
        {
            self.content_process.take();
        }
        if self.content_process.is_none() {
            let mut process = NativeContentProcess::spawn().await?;
            process.start().await?;
            self.content_process = Some(process);
        }
        Ok(())
    }

    async fn commit_content_process(&mut self) -> Result<(), NativeEngineError> {
        let Some(process) = self.content_process.as_mut() else {
            return Err(NativeEngineError::Worker {
                operation: "commit content process".into(),
                reason: "native content process is not running".into(),
            });
        };
        process.commit().await
    }

    async fn navigate_resource_async(
        &mut self,
        resource: NativeResource,
        worker: &NativeRuntimeWorker,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if is_network_url(&resource.url) {
            self.commit_content_process().await?;
        } else {
            self.content_process.take();
        }
        if self.is_same_document_navigation(&resource.url) {
            self.commit_same_document_navigation_async(resource.url, HistoryCommit::Push, worker)
                .await?;
        } else {
            let prepared = self.prepare_navigation_resource(resource)?;
            self.commit_navigation_async(prepared, worker).await?;
        }
        Ok(self.snapshot_unchecked())
    }

    fn navigate_content(
        &mut self,
        content: NativeContentLoad,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&content.url) {
            self.commit_same_document_navigation(content.url, HistoryCommit::Push)?;
        } else {
            let prepared = self.prepare_navigation_content(content)?;
            self.commit_navigation(prepared)?;
        }
        Ok(self.snapshot_unchecked())
    }

    async fn navigate_content_async(
        &mut self,
        content: NativeContentLoad,
        worker: &NativeRuntimeWorker,
    ) -> Result<NativeEngineSnapshot, NativeEngineError> {
        if self.is_same_document_navigation(&content.url) {
            self.commit_same_document_navigation_async(content.url, HistoryCommit::Push, worker)
                .await?;
        } else {
            let prepared = self.prepare_navigation_content(content)?;
            self.commit_navigation_async(prepared, worker).await?;
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

    /// Execute one bounded native GET/fetch request from the current external
    /// document. This is the first executable consumer of the shared
    /// connect/CSP/CORS policy; it is deliberately narrower than the eventual
    /// JavaScript Fetch/Web IDL surface and accepts no custom headers or body.
    pub async fn fetch_async(
        &mut self,
        href: impl Into<String>,
        credentials: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        self.require_running("fetch")?;
        if !is_network_url(&self.url) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native fetch requires an HTTP(S) document".into(),
            });
        }
        let href = href.into();
        self.ensure_content_process().await?;
        self.content_process
            .as_mut()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "content process fetch".into(),
                reason: "native content process is not running".into(),
            })?
            .fetch(&self.url, &href, credentials)
            .await
    }

    /// Evaluate bounded ECMAScript in the current page realm. Network
    /// documents execute in the child-owned content process; local documents
    /// use the same runtime implementation in the engine owner.
    pub async fn evaluate_async(
        &mut self,
        source: impl Into<String>,
    ) -> Result<serde_json::Value, NativeEngineError> {
        self.require_running("script")?;
        let source = source.into();
        if let Some(process) = self.content_process.as_mut() {
            if !process.is_healthy() {
                return Err(NativeEngineError::worker_failure(
                    "content process script",
                    process
                        .failure_kind()
                        .unwrap_or(NativeWorkerFailureKind::Exited),
                    "content process is unavailable after a failed operation; navigate to recover it",
                ));
            }
            let evaluation = process.evaluate(&source).await?;
            if let Some(mutation) = evaluation.mutation {
                let navigation = mutation.navigation.clone();
                self.apply_content_process_mutation(mutation)?;
                if let Some(navigation) = navigation {
                    self.navigate_script_navigation_async(navigation).await?;
                }
            }
            return Ok(evaluation.value);
        }
        let javascript = self
            .javascript
            .get_or_insert(NativeJavaScriptRuntime::new()?);
        let evaluation = javascript.evaluate(
            &source,
            &self.document,
            &self.url,
            &self.origin,
            self.config.viewport,
        )?;
        self.apply_local_script_commands(&evaluation.commands, true)?;
        Ok(evaluation.value)
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
                    return self.activate_link(id, &href, false);
                }
                if self.javascript.is_some() {
                    return self.action_local_click_with_event_preflight(id);
                }
                (self.document.apply_click(id)?, true)
            }
            NativeAction::Type { target, text } => {
                let id = self.document.resolve_target(&target)?;
                if !self.document.is_hidden_for_layout(id) {
                    self.require_layout_actionable(id)?;
                }
                if self.javascript.is_some() {
                    return self.action_local_type_with_event_transaction(id, &text);
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
        self.record_effects(events.clone());
        self.dispatch_local_events(&events)?;
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    /// Apply an action through the child-owned document when the current
    /// navigation is process-backed. Navigation actions remain parent-owned so
    /// a link cannot mutate the child document without also committing a new
    /// resource and history entry.
    pub async fn action_async(
        &mut self,
        action: NativeAction,
    ) -> Result<NativeActionResult, NativeEngineError> {
        self.require_running("action")?;
        let Some(process) = self.content_process.as_ref() else {
            return self.action(action);
        };
        if !process.is_healthy() {
            return Err(NativeEngineError::worker_failure(
                "content process mutation",
                process
                    .failure_kind()
                    .unwrap_or(NativeWorkerFailureKind::Exited),
                "content process is unavailable after a failed mutation; navigate to recover it",
            ));
        }
        match action {
            NativeAction::Click { target } => {
                let id = self.resolve_click_target(&target)?;
                if !self.document.is_hidden_for_layout(id) {
                    self.require_layout_actionable(id)?;
                }
                if self
                    .document
                    .link_href(id)
                    .is_some_and(|href| !href.is_empty())
                {
                    self.content_process.take();
                    return self.action(NativeAction::Click { target });
                }
                let mut preview = self.document.clone();
                preview.apply_click(id)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process click preflight".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_click_with_event_preflight(id.index())
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_at(next_revision, mutation)
            }
            NativeAction::Type { target, text } => {
                let id = self.document.resolve_target(&target)?;
                if !self.document.is_hidden_for_layout(id) {
                    self.require_layout_actionable(id)?;
                }
                let mut preview = self.document.clone();
                preview.apply_type(id, &text)?;
                let mutation = {
                    let process =
                        self.content_process
                            .as_mut()
                            .ok_or_else(|| NativeEngineError::Worker {
                                operation: "content process type event bridge".into(),
                                reason: "native content process is not running".into(),
                            })?;
                    process
                        .mutate_type_with_event_bridge(id.index(), text)
                        .await?
                };
                let next_revision = self.next_revision()?;
                self.apply_content_process_mutation_at(next_revision, mutation)
            }
            NativeAction::Scroll { .. } => self.action(action),
        }
    }

    fn apply_local_script_commands(
        &mut self,
        commands: &[super::javascript::NativeScriptCommand],
        allow_script_navigation: bool,
    ) -> Result<(), NativeEngineError> {
        if commands.is_empty() {
            return Ok(());
        }
        let mut document = self.document.clone();
        let events = if allow_script_navigation {
            document.apply_script_commands_allowing_links(commands)?
        } else {
            document.apply_script_commands(commands)?
        };
        let navigation = if allow_script_navigation {
            self.script_navigation_target(&document, commands)?
        } else {
            None
        };
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history.update_current_scroll(self.scroll_offset);
        self.record_effects(events);
        if let Some(navigation) = navigation {
            match navigation {
                ScriptNavigationTarget::Link { id, href } => {
                    self.activate_link(id, &href, true)?;
                }
                ScriptNavigationTarget::Form { href } => {
                    self.navigate(href)?;
                }
            }
        }
        Ok(())
    }

    fn script_navigation_target(
        &self,
        document: &NativeDocument,
        commands: &[super::javascript::NativeScriptCommand],
    ) -> Result<Option<ScriptNavigationTarget>, NativeEngineError> {
        let mut navigation = None;
        for command in commands {
            let target = match command {
                super::javascript::NativeScriptCommand::Click { node_index } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    if let Some(href) = document.link_href(id).filter(|href| !href.is_empty()) {
                        Some(ScriptNavigationTarget::Link {
                            id,
                            href: href.to_owned(),
                        })
                    } else if document.submit_control_form(id).is_some() {
                        Some(ScriptNavigationTarget::Form {
                            href: document.form_submission_url(id, &self.url)?,
                        })
                    } else {
                        None
                    }
                }
                super::javascript::NativeScriptCommand::SubmitForm { node_index } => {
                    let id = NativeNodeId::from_parts(document.generation(), *node_index);
                    Some(ScriptNavigationTarget::Form {
                        href: document.form_submission_url(id, &self.url)?,
                    })
                }
                _ => None,
            };
            let Some(target) = target else {
                continue;
            };
            if navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one script batch cannot activate multiple navigations".into(),
                });
            }
            navigation = Some(target);
        }
        Ok(navigation)
    }

    async fn navigate_script_navigation_async(
        &mut self,
        navigation: NativeContentNavigation,
    ) -> Result<(), NativeEngineError> {
        let id = NativeNodeId::from_parts(self.document.generation(), navigation.node_index);
        let href = if let Some(href) = self.document.link_href(id).filter(|href| !href.is_empty()) {
            href.to_owned()
        } else {
            self.document.form_submission_url(id, &self.url)?
        };
        if href != navigation.href {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script navigation target changed during transfer".into(),
            });
        }
        let target_url = self.resolve_link_href(&href)?;
        if self.is_same_document_navigation(&target_url) {
            if let Some(worker) = self.runtime_worker.clone() {
                self.commit_same_document_navigation_async(
                    target_url,
                    HistoryCommit::Push,
                    &worker,
                )
                .await?;
            } else {
                self.commit_same_document_navigation(target_url, HistoryCommit::Push)?;
            }
            return Ok(());
        }
        self.navigate_async(target_url).await.map(|_| ())
    }

    fn dispatch_local_events(
        &mut self,
        events: &[(NativeNodeId, NativeEventKind)],
    ) -> Result<(), NativeEngineError> {
        let Some(evaluation) = self.evaluate_local_events(&self.document, events)? else {
            return Ok(());
        };
        self.apply_local_script_commands(&evaluation.commands, false)
    }

    fn evaluate_local_events(
        &self,
        document: &NativeDocument,
        events: &[(NativeNodeId, NativeEventKind)],
    ) -> Result<Option<NativeScriptEvaluation>, NativeEngineError> {
        let event_metadata = events
            .iter()
            .map(|(node_id, kind)| (node_id.index(), *kind))
            .collect::<Vec<_>>();
        let Some(source) = host_event_script(&event_metadata)? else {
            return Ok(None);
        };
        let Some(javascript) = self.javascript.as_ref() else {
            return Ok(None);
        };
        Ok(Some(javascript.evaluate(
            &source,
            document,
            &self.url,
            &self.origin,
            self.config.viewport,
        )?))
    }

    fn action_local_click_with_event_preflight(
        &mut self,
        id: NativeNodeId,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut events = document.apply_script_focus(id)?;
        if let Some(evaluation) = self.evaluate_local_events(&document, &events)? {
            events.extend(document.apply_script_commands(&evaluation.commands)?);
        }

        let click_evaluation = self
            .evaluate_local_events(&document, &[(id, NativeEventKind::Click)])?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native click event preflight".into(),
                reason: "native JavaScript realm disappeared during click preflight".into(),
            })?;
        let click_allowed = click_evaluation
            .value
            .as_array()
            .and_then(|values| values.first())
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "native click event preflight".into(),
                reason: "native click event result was invalid".into(),
            })?;
        events.extend(document.apply_script_commands(&click_evaluation.commands)?);
        if click_allowed {
            events.extend(document.apply_click(id)?);
        } else {
            events.push((id, NativeEventKind::Click));
        }
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native click event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }

        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history.update_current_scroll(self.scroll_offset);
        self.record_effects(events);
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn action_local_type_with_event_transaction(
        &mut self,
        id: NativeNodeId,
        text: &str,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let mut document = self.document.clone();
        let mut events = document.apply_type(id, text)?;
        let default_events = events.clone();
        for event in default_events {
            if let Some(evaluation) = self.evaluate_local_events(&document, &[event])? {
                events.extend(document.apply_script_commands(&evaluation.commands)?);
            }
            if events.len() > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "native type event effects",
                    MAX_NATIVE_EFFECTS,
                    events.len(),
                ));
            }
        }
        let next_revision = self.next_revision()?;
        document.set_revision(next_revision);
        self.document = document;
        self.revision = next_revision;
        self.history.update_current_scroll(self.scroll_offset);
        self.record_effects(events);
        Ok(NativeActionResult {
            revision: next_revision,
            accepted: true,
        })
    }

    fn apply_content_process_mutation(
        &mut self,
        mutation: NativeContentMutation,
    ) -> Result<(), NativeEngineError> {
        let next_revision = self.next_revision()?;
        self.apply_content_process_mutation_at(next_revision, mutation)?;
        Ok(())
    }

    fn apply_content_process_mutation_at(
        &mut self,
        next_revision: u64,
        mutation: NativeContentMutation,
    ) -> Result<NativeActionResult, NativeEngineError> {
        let generation = self.document.generation();
        let mut document = match NativeDocument::from_content_wire(
            mutation.document,
            &self.config.limits,
            generation,
        ) {
            Ok(document) => document,
            Err(error) => {
                self.content_process.take();
                return Err(error);
            }
        };
        let events = match mutation
            .events
            .into_iter()
            .map(|event| {
                let node_id = NativeNodeId::from_parts(generation, event.node_index);
                document
                    .node(node_id)
                    .map(|_| (node_id, event.kind))
                    .ok_or(NativeEngineError::DetachedTarget)
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(events) => events,
            Err(error) => {
                self.content_process.take();
                return Err(error);
            }
        };
        document.set_revision(next_revision);
        self.document = document;
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
        click_already_applied: bool,
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
            let events = if click_already_applied {
                Vec::new()
            } else {
                self.document.apply_click(id)?
            };
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
        if !click_already_applied {
            let _events = self.document.apply_click(id)?;
        }
        self.document = prepared.document;
        self.javascript = None;
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
        if let Ok(base) = url::Url::parse(without_fragment(&self.url))
            && is_network_url(base.as_str())
        {
            let resolved = base
                .join(href)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "relative network link reference is malformed".into(),
                })?;
            let resolved = resolved.to_string();
            validate_url_text("link target URL", &resolved)?;
            return Ok(resolved);
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
            origin: self.origin.clone(),
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
            origin: self.origin.clone(),
            active: true,
        })
    }

    pub fn history(&self) -> &NativeHistory {
        &self.history
    }

    pub fn scheduler(&self) -> Result<DeterministicScheduler, NativeEngineError> {
        self.runtime.scheduler()
    }

    pub fn runtime_state(&self) -> NativeRuntimeState {
        self.runtime.state()
    }

    pub fn runtime_trace(&self) -> Vec<NativeRuntimeTraceEvent> {
        self.runtime.trace()
    }

    fn prepare_navigation(&self, url: &str) -> Result<PreparedNavigation, NativeEngineError> {
        let resource = self.loader.load(url)?;
        self.prepare_navigation_resource(resource)
    }

    async fn prepare_navigation_async(
        &mut self,
        url: &str,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let resource = self.loader.load_async(url).await?;
        self.prepare_navigation_resource(resource)
    }

    fn prepare_navigation_content(
        &self,
        content: NativeContentLoad,
    ) -> Result<PreparedNavigation, NativeEngineError> {
        let next_revision = self.next_revision()?;
        let generation = u32::try_from(next_revision).map_err(|_| {
            NativeEngineError::limit("document generations", u32::MAX as usize, usize::MAX)
        })?;
        let document =
            NativeDocument::from_content_wire(content.document, &self.config.limits, generation)?;
        Ok(PreparedNavigation {
            resource: NativeResource {
                url: content.url,
                origin: content.origin,
                body: String::new(),
            },
            document,
            execute_inline_scripts: false,
        })
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
        Ok(PreparedNavigation {
            resource,
            document,
            execute_inline_scripts: true,
        })
    }

    fn commit_navigation(
        &mut self,
        mut prepared: PreparedNavigation,
    ) -> Result<(), NativeEngineError> {
        let mut javascript = None;
        if prepared.execute_inline_scripts {
            execute_inline_scripts(
                &mut prepared.document,
                &mut javascript,
                &prepared.resource.url,
                &prepared.resource.origin,
                self.config.viewport,
            )?;
        }
        let scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task(NativeTask::CommitNavigation, "navigation")?;
        let revision = self.next_revision()?;
        self.document = prepared.document;
        self.javascript = javascript;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.scroll_offset = scroll_offset;
        self.revision = revision;
        self.history.push(self.url.clone(), revision, scroll_offset);
        Ok(())
    }

    async fn commit_navigation_async(
        &mut self,
        mut prepared: PreparedNavigation,
        worker: &NativeRuntimeWorker,
    ) -> Result<(), NativeEngineError> {
        let mut javascript = None;
        if prepared.execute_inline_scripts {
            execute_inline_scripts(
                &mut prepared.document,
                &mut javascript,
                &prepared.resource.url,
                &prepared.resource.origin,
                self.config.viewport,
            )?;
        }
        let scroll_offset = self.fragment_scroll_offset_for_document(
            &prepared.document,
            &prepared.resource.url,
            NativePoint { x: 0, y: 0 },
        )?;
        self.run_commit_task_async(NativeTask::CommitNavigation, "navigation", worker)
            .await?;
        let revision = self.next_revision()?;
        self.document = prepared.document;
        self.javascript = javascript;
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

    async fn commit_same_document_navigation_async(
        &mut self,
        url: String,
        history_commit: HistoryCommit,
        worker: &NativeRuntimeWorker,
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
        self.run_commit_task_async(
            NativeTask::CommitSameDocumentNavigation,
            "same-document navigation",
            worker,
        )
        .await?;
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
        let max_scroll = prepared
            .document
            .layout(self.config.viewport)?
            .max_scroll_offset();
        let scroll_offset = NativePoint {
            x: saved_scroll.x.min(max_scroll.x),
            y: saved_scroll.y.min(max_scroll.y),
        };
        self.run_commit_task(NativeTask::TraverseHistory, "history traversal")?;
        let revision = self.next_revision()?;
        self.document = prepared.document;
        self.javascript = None;
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
        let layout = document.layout(self.config.viewport)?;
        let target_id = if fragment.starts_with(":~:text=") {
            let Some(terms) = decode_text_fragment_terms(fragment) else {
                return Ok(fallback);
            };
            document.text_fragment_target(&layout, &terms)
        } else {
            let Some(decoded_fragment) = decode_percent_encoded_fragment(fragment) else {
                return Ok(fallback);
            };
            document.fragment_target(&decoded_fragment)
        };
        let Some(target_id) = target_id else {
            return Ok(fallback);
        };
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
        self.runtime.schedule(expected, 0)?;
        let Some(task) = self.runtime.pop_ready()? else {
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

    async fn run_commit_task_async(
        &mut self,
        expected: NativeTask,
        operation: &str,
        worker: &NativeRuntimeWorker,
    ) -> Result<(), NativeEngineError> {
        worker.schedule(expected, 0).await?;
        let Some(task) = worker.pop_ready().await? else {
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
        if delta_x == 0 && delta_y == 0 {
            return Ok(false);
        }
        let max_scroll = self
            .document
            .layout(self.config.viewport)?
            .max_scroll_offset();
        let requested_x = i64::from(self.scroll_offset.x).saturating_add(i64::from(delta_x));
        let next_x = requested_x.clamp(0, i64::from(max_scroll.x));
        let next_x = u32::try_from(next_x).map_err(|_| {
            NativeEngineError::invalid("native scroll action", "scroll offset exceeds bounds")
        })?;
        let requested_y = i64::from(self.scroll_offset.y).saturating_add(i64::from(delta_y));
        let next_y = requested_y.clamp(0, i64::from(max_scroll.y));
        let next_y = u32::try_from(next_y).map_err(|_| {
            NativeEngineError::invalid("native scroll action", "scroll offset exceeds bounds")
        })?;
        if next_x == self.scroll_offset.x && next_y == self.scroll_offset.y {
            return Ok(false);
        }
        self.scroll_offset = NativePoint {
            x: next_x,
            y: next_y,
        };
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
    execute_inline_scripts: bool,
}

enum ScriptNavigationTarget {
    Link { id: NativeNodeId, href: String },
    Form { href: String },
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
