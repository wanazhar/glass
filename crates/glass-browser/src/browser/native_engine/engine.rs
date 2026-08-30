use super::browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
use super::config::NativeEngineConfig;
use super::dom::NativeDocument;
use super::error::NativeEngineError;
use super::history::NativeHistory;
use super::lifecycle::NativeLifecycleState;
use super::origin::NativeOrigin;
use super::resource_loader::{NativeResource, NativeResourceLoader};
use super::scheduler::{DeterministicScheduler, NativeTask};

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
        let next_revision = self.revision.checked_add(1).ok_or_else(|| {
            NativeEngineError::limit("document revisions", u64::MAX as usize, usize::MAX)
        })?;
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
        let revision = self.revision.checked_add(1).ok_or_else(|| {
            NativeEngineError::limit("document revisions", u64::MAX as usize, usize::MAX)
        })?;
        self.document = prepared.document;
        self.url = prepared.resource.url;
        self.origin = prepared.resource.origin;
        self.revision = revision;
        self.history.push(self.url.clone(), revision);
        Ok(())
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
