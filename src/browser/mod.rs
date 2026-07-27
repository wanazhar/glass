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
/// Versioned declarative workflow definition and validation types.
pub use session::{
    WORKFLOW_SCHEMA_VERSION, WorkflowBranchDecision, WorkflowBudgets, WorkflowCheckpoint,
    WorkflowCheckpointPage, WorkflowCheckpointStep, WorkflowDefinition, WorkflowInput,
    WorkflowOutput, WorkflowOutputDeclaration, WorkflowOutputSource, WorkflowResumeError,
    WorkflowResumePlan, WorkflowRunResult, WorkflowRunStatus, WorkflowStep, WorkflowStepRecord,
    WorkflowStepState, WorkflowTerminalProof, WorkflowTrace, WorkflowTraceEvent,
    WorkflowTransactionClass, WorkflowValidationError, WorkflowValueType,
};
