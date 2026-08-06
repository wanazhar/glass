/// Typed CDP adapter implementing the transport-neutral browser backend boundary.
pub mod backend_adapter;
/// Low-level CDP WebSocket client for Chrome DevTools Protocol communication.
pub mod cdp;
/// Chrome/Chromium process lifecycle, binary resolution, and health checks.
pub mod chrome;
/// DOM and accessibility tree parsing with compact projection.
pub mod dom;
/// Mouse movement engine with bounded smooth pointer paths.
pub mod mouse;
/// Security policy engine with capability-based operation gating.
pub mod policy;
/// Chrome user-data directory profiles for persistent sessions.
pub mod profile;
/// Central browser session orchestrating all CDP operations.
pub mod session;
pub(crate) use backend_adapter::CdpSessionBackend;

// Re-export key session types to the browser module level.

/// A structured diff between two accessibility snapshots.
pub use session::AccessibilityDiff;
/// Count-only summary of accessibility changes after an action.
pub use session::AccessibilityDiffSummary;
/// Error returned when a revision-aware action cannot run safely.
pub use session::ActionContractError;
/// Stable failure category for revision-aware actions.
pub use session::ActionFailureKind;
/// The action kind used in revision-aware action outcomes.
pub use session::ActionKind;
/// Compact result envelope for a completed input action.
pub use session::ActionOutcome;
/// Status of a revision-aware action.
pub use session::ActionStatus;
/// Bounded evidence collected after an action.
pub use session::ActionVerificationEvidence;
/// Type alias for fallible browser operations.
pub use session::BrowserResult;
/// A headful or headless browser session that drives Chrome via CDP.
pub use session::BrowserSession;
/// An HTTP cookie with name, value, domain, path, and expiration.
pub use session::Cookie;
/// A single change (added, removed, or modified) in an accessibility diff.
pub use session::DiffChange;
/// An element within an accessibility diff change.
pub use session::DiffElement;
/// The result of filling a single form field.
pub use session::FillFieldResult;
/// The aggregate outcome of a multi-field form fill.
pub use session::FillFormOutcome;
/// Latitude/longitude geolocation override.
pub use session::GeoLocation;
/// Pointer movement mode: human (bounded smooth) or fast (direct).
pub use session::InteractionMode;
/// A guard that disables CDP Fetch domain interception on drop.
pub use session::InterceptGuard;
/// Fresh-state scope and landmark assessment for one knowledge record.
pub use session::KnowledgeAssessment;
/// One bounded assessment signal.
pub use session::KnowledgeAssessmentSignal;
/// Assessment status after scope and freshness checks.
pub use session::KnowledgeAssessmentStatus;
/// Versioned confidence and lifecycle state for persisted browser knowledge.
pub use session::KnowledgeConfidence;
/// Invalidation rules for persisted browser knowledge.
pub use session::KnowledgeInvalidation;
/// Auditable confidence-state transition for persisted browser knowledge.
pub use session::KnowledgeLifecycleEvent;
/// Current session dimensions used to assess remembered knowledge.
pub use session::KnowledgeLookupContext;
/// Explicit session inputs for constructing a knowledge lookup context.
pub use session::KnowledgeLookupOptions;
/// Whether a semantic observation was fresh-only or store-assessed.
pub use session::KnowledgeObservationMode;
/// Fresh semantic observation with non-authorizing knowledge assessments.
pub use session::KnowledgeObservationReport;
/// Profile isolation class for persisted browser knowledge.
pub use session::KnowledgeProfileScope;
/// Result of purging records scoped to one origin.
pub use session::KnowledgePurgeResult;
/// One persisted browser knowledge record.
pub use session::KnowledgeRecord;
/// Inputs for building a bounded page-family knowledge record.
pub use session::KnowledgeRecordBuildOptions;
/// Knowledge record category.
pub use session::KnowledgeRecordKind;
/// Scope dimensions preventing knowledge leakage across sessions.
pub use session::KnowledgeScope;
/// Exact origin/profile/workspace selector for safe knowledge management.
pub use session::KnowledgeScopeSelector;
/// Assessment signal category for remembered knowledge.
pub use session::KnowledgeSignalKind;
/// Provenance for a persisted browser knowledge record.
pub use session::KnowledgeSource;
/// Crash-safe local persistence for bounded knowledge records.
pub use session::KnowledgeStore;
/// Result of a knowledge store mutation, including pruning evidence.
pub use session::KnowledgeStoreChange;
/// Knowledge store I/O, contract, and capacity error.
pub use session::KnowledgeStoreError;
/// Record and byte limits for a knowledge store.
pub use session::KnowledgeStoreLimits;
/// Top-level persisted knowledge store document.
pub use session::KnowledgeStoreSnapshot;
/// Bounded record counts and serialized size for a knowledge store.
pub use session::KnowledgeStoreStats;
/// Validation error for the knowledge contract.
pub use session::KnowledgeValidationError;
/// Revision-aware navigation result.
pub use session::NavigationOutcome;
/// A single captured network request/response entry.
pub use session::NetworkEntry;
/// A bounded network event recorder producing [`NetworkRecording`].
pub use session::NetworkRecorder;
/// A collection of captured network entries from a recording session.
pub use session::NetworkRecording;
/// Page URL and title returned by navigation and page inspection.
pub use session::PageInfo;
/// PDF generation options (page size, margins, background).
pub use session::PdfOptions;
/// The outcome of a popup-expecting click with causal verification.
pub use session::PopupClickOutcome;
/// A URL or pattern for CDP Fetch domain request interception.
pub use session::RequestPattern;
/// Retry configuration for transport/CDP protocol errors.
pub use session::RetryPolicy;
/// Predicate controlling which errors trigger a retry.
pub use session::RetryPredicate;
/// Options used to launch or attach to a browser session.
pub use session::SessionOptions;
/// Fluent builder for [`SessionOptions`].
pub use session::SessionOptionsBuilder;
/// A single localStorage or sessionStorage key-value entry.
pub use session::StorageEntry;
/// Bounded collection of DOM storage items (≤ 64 entries).
pub use session::StorageItems;
/// A guard managing a virtual WebAuthn authenticator lifecycle.
pub use session::WebAuthnGuard;
/// Configuration for creating a virtual WebAuthn authenticator.
pub use session::WebAuthnOptions;
/// Computes a structured diff between two accessibility snapshots.
pub use session::diff_accessibility;
/// Bounded task-oriented inspection, targeting, verification, extraction, and recovery results.
pub use session::{
    ActAndVerifyResult, ExtractionField, ExtractionKind, FindTargetResult, InspectPageResult,
    RecoverRunResult, StructuredExtractionRequest, StructuredExtractionResult,
};
/// Versioned, redacted local session snapshots and deterministic diffs.
pub use session::{
    SESSION_SNAPSHOT_SCHEMA_VERSION, SessionSnapshot, SessionSnapshotDiff, SessionSnapshotStore,
    default_session_snapshot_path,
};
/// Bounded browser-backed Task Protocol execution results.
pub use session::{TaskExecutionResult, TaskStepResult};
/// Versioned declarative workflow definition and validation types.
pub use session::{
    WORKFLOW_SCHEMA_VERSION, WorkflowBranchDecision, WorkflowBudgets, WorkflowCheckpoint,
    WorkflowCheckpointPage, WorkflowCheckpointStep, WorkflowDefinition, WorkflowDraft,
    WorkflowDraftStep, WorkflowInput, WorkflowIntentEvidence, WorkflowIntentStep, WorkflowOutput,
    WorkflowOutputDeclaration, WorkflowOutputEvidence, WorkflowOutputSource,
    WorkflowRecordedTarget, WorkflowRecorder, WorkflowRecordingConfidence, WorkflowResumeError,
    WorkflowResumePlan, WorkflowRunResult, WorkflowRunStatus, WorkflowStep, WorkflowStepRecord,
    WorkflowStepState, WorkflowTerminalProof, WorkflowTrace, WorkflowTraceEvent,
    WorkflowTransactionClass, WorkflowValidationError, WorkflowValueType,
};
