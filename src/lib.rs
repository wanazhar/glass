//! Glass — lightweight local browser control for Chrome and Chromium.
//!
//! Drives Chrome or Chromium directly through the Chrome DevTools Protocol
//! (CDP), without Playwright, WebDriver, or an embedded browser runtime.
//!
//! # Quick start
//!
//! Build a session with [`SessionOptions`], start Chrome, navigate, and close
//! the session when the work is complete:
//!
//! ```rust,no_run
//! use glass::{BrowserSession, SessionOptions};
//!
//! # async fn run() -> glass::BrowserResult<()> {
//! let options = SessionOptions::builder().build()?;
//! let session = BrowserSession::start(&options).await?;
//! let page = session.navigate("https://example.com").await?;
//! println!("{}", page.url);
//! session.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! The [`browser`] module contains the reusable Rust API. The [`cli`] module
//! backs the `glass` binary, [`mcp`] exposes the MCP stdio server, and [`tui`]
//! provides the terminal interface. User-facing guides are available in the
//! repository's [`docs`](https://github.com/wanazhar/glass/tree/main/docs).
//!
//! # Cargo features
//!
//! - `visual-compare` enables PNG comparison helpers for screenshot checks.
//! - `fuzzing` enables test-oriented fuzzing hooks and is not needed by normal
//!   applications.
//!
//! # Modules
//!
//! - [`browser`] — Chrome lifecycle, CDP client, DOM/accessibility parsing,
//!   mouse movement, security policy, profiles, and the central
//!   [`BrowserSession`].
//! - [`cli`] — Clap argument definitions and command dispatch.
//! - [`mcp`] — JSON-RPC/MCP stdio server for MCP-compatible clients.
//! - [`tui`] — Ratatui terminal interface.

/// Browser control modules and the reusable [`browser::BrowserSession`] API.
pub mod browser;
/// Transport-neutral backend capability contract.
pub mod browser_backend;
/// Browser-neutral terminal frame presentation contracts.
pub mod presentation;
/// Bounded, evidenced browser surface contracts.
pub mod surfaces;
/// Bounded terminal graphics adapters.
pub mod terminal_graphics;
/// Bounded workspace identity and lifecycle contracts.
pub mod workspace;
/// Versioned Glass protocol and capability negotiation.
pub mod capabilities;
/// Command-line argument definitions and dispatch helpers.
pub mod cli;
/// Local Unix-socket daemon lifecycle and MCP bridge.
pub mod daemon;
/// Validated extension metadata and permission boundaries.
pub mod extensions;
/// Experimental bounded browser-evidence extraction contracts.
pub mod extraction;
/// MCP stdio server, prompts, resources, and tool dispatch.
pub mod mcp;
/// Transport-neutral versioned request and response envelopes.
pub mod protocol;
/// Versioned browser-free reliability scenario contracts.
pub mod reliability;
/// Bounded browser execution for reliability scenarios and replay evidence.
pub mod reliability_runner;
/// Bounded agent-facing response projections and local result artifacts.
pub mod results;
/// Browser-free deterministic Task Protocol execution-plan compiler.
pub mod task_compiler;
/// Strict, bounded authored Task Protocol v1 inputs.
pub mod task_protocol;
/// Ratatui terminal interface.
pub mod tui;
/// Stable Glass Web IR v1 reconciliation and validation.
pub mod web_ir;

// Keep the most common embedding types on the crate root. Lower-level and
// capability-specific APIs remain organized under `glass::browser`.
pub use browser::{
    AccessibilityDiffSummary, ActionContractError, ActionFailureKind, ActionKind, ActionOutcome,
    ActionStatus, ActionVerificationEvidence, BrowserResult, BrowserSession, KnowledgeAssessment,
    KnowledgeAssessmentSignal, KnowledgeAssessmentStatus, KnowledgeConfidence,
    KnowledgeInvalidation, KnowledgeLifecycleEvent, KnowledgeLookupContext, KnowledgeLookupOptions,
    KnowledgeObservationMode, KnowledgeObservationReport, KnowledgeProfileScope,
    KnowledgePurgeResult, KnowledgeRecord, KnowledgeRecordBuildOptions, KnowledgeRecordKind,
    KnowledgeScope, KnowledgeSignalKind, KnowledgeSource, KnowledgeStore, KnowledgeStoreChange,
    KnowledgeStoreError, KnowledgeStoreLimits, KnowledgeStoreSnapshot, KnowledgeStoreStats,
    KnowledgeValidationError, NavigationOutcome, PageInfo, SessionOptions, SessionOptionsBuilder,
    TaskExecutionResult, TaskStepResult, WORKFLOW_SCHEMA_VERSION, WorkflowBudgets,
    WorkflowCheckpoint, WorkflowCheckpointPage, WorkflowCheckpointStep, WorkflowDefinition,
    WorkflowInput, WorkflowOutput, WorkflowOutputDeclaration, WorkflowOutputSource,
    WorkflowResumeError, WorkflowResumePlan, WorkflowRunResult, WorkflowRunStatus, WorkflowStep,
    WorkflowStepRecord, WorkflowStepState, WorkflowTerminalProof, WorkflowTrace,
    WorkflowTraceEvent, WorkflowTransactionClass, WorkflowValidationError, WorkflowValueType,
};

pub use task_protocol::{
    GlassTask, TASK_PROTOCOL_SCHEMA_VERSION, TaskAmbiguityPolicy, TaskKind, TaskLimits,
    TaskPostcondition, TaskPostconditionKind, TaskProtocolError, TaskRevisionPolicy, TaskRiskClass,
    TaskScope,
};

pub use task_compiler::{
    TASK_COMPILER_VERSION, TASK_PLAN_SCHEMA_VERSION, TaskCompilationError,
    TaskEvidenceRequirements, TaskExecutionPlan, TaskPlanOperation, TaskPlanPrecondition,
    TaskPlanStep, compile_task,
};

pub use protocol::{
    GLASS_PROTOCOL_VERSION, TASK_COMPILE_OPERATION, TASK_VALIDATE_OPERATION, TaskCompilePayload,
    TaskCompileResult, TaskValidationPayload, TaskValidationResult, WEB_IR_CONTINUITY_OPERATION,
    WEB_IR_DIFF_OPERATION, WEB_IR_INSPECT_OPERATION, WEB_IR_VALIDATE_OPERATION,
    WebIrContinuityPayload, WebIrContinuityResult, WebIrDiffPayload, WebIrDiffResult,
    WebIrInspectionResult, WebIrPayload, WebIrValidationResult, compile_task_request,
    compile_task_result, validate_task_result, web_ir_continuity_result, web_ir_diff_result,
    web_ir_inspect_result, web_ir_validate_result,
};

/// Re-export the bounded extraction contract for embedding callers.
pub use extraction::{
    EXTRACTION_CONTRACT_SCHEMA_VERSION, EvidenceCoverage, EvidenceFact, EvidenceQuality,
    EvidenceRelationshipHint, EvidenceSource, ExtractionBudgets, ExtractionContractError,
    ExtractionEvidence, ExtractionEvidenceLimits, ExtractionRequest, ExtractionScope,
    MAX_EXTRACTION_DEPTH, MAX_EXTRACTION_DURATION_MS, MAX_EXTRACTION_NODES,
    MAX_EXTRACTION_OUTPUT_BYTES, MAX_EXTRACTION_TEXT_BYTES, extract_page_context,
};

/// Re-export the stable Glass Web IR v1 contract for embedding callers.
pub use web_ir::{
    GlassWebIrDiff, GlassWebIrV1, RelationshipHintDiagnosticStatus, WEB_IR_SCHEMA_VERSION,
    WebIrAction, WebIrChangeKind, WebIrDocument, WebIrEntity, WebIrEntityChange,
    WebIrEntityContinuity, WebIrEntityContinuityStatus, WebIrEntityDetails, WebIrEntityKind,
    WebIrEntityState, WebIrFixtureExpectation, WebIrRelationship, WebIrRelationshipChange,
    WebIrRelationshipHintDiagnostic, WebIrRelationshipKind, WebIrScopeKind, WebIrSensitivity,
    WebIrValidationError, reconcile_evidence,
};
