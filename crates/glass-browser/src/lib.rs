//! Local, revision-safe browser intelligence for Chrome and Chromium.
//!
//! `glass-browser` provides an owned/attached browser [`BrowserSession`],
//! structured semantic observation, guarded actions, stable Web IR, Task
//! Protocol compilation/execution, workflows, advisory knowledge, policy,
//! MCP, daemon, TUI, backend, surface, presentation, reliability, and optional
//! terminal-native development contracts.
//!
//! Glass does not bundle a browser, host a browser service, or infer an
//! autonomous plan. Callers select operations; Glass validates current
//! evidence, policy, capabilities, revisions, unique targeting, and bounded
//! postconditions. CDP is the production backend. WebDriver BiDi remains an
//! experimental bounded adapter.
//!
//! # Choose an entry point
//!
//! | Goal | Entry point |
//! |---|---|
//! | Drive an owned or attached browser | [`BrowserSession`] and [`SessionOptions`] |
//! | Collect stable evidence | [`extraction`] and [`web_ir`] |
//! | Compile/execute semantic tasks | [`task_protocol`] and [`task_compiler`] |
//! | Run typed workflows | [`browser::session::WorkflowDefinition`] |
//! | Assess scoped historical knowledge | [`KnowledgeStore`] |
//! | Implement/select a backend | [`browser_backend`] and [`browser::BackendFactory`] |
//! | Expose MCP or canonical requests | [`mcp`] and [`protocol`] |
//! | Embed project development | [`development`] with feature `development-runtime` |
//! | Present terminal frames | [`presentation`] and [`terminal_graphics`] |
//!
//! # Browser lifecycle
//!
//! Start, navigate, observe, and explicitly close an owned session:
//!
//! ```rust,no_run
//! use glass_browser::{BrowserSession, SessionOptions};
//!
//! # async fn run() -> glass_browser::BrowserResult<()> {
//! let options = SessionOptions::builder().incognito(true).build()?;
//! let session = BrowserSession::start(&options).await?;
//! let page = session.navigate("https://example.com").await?;
//! let observation = session.observe().await?;
//! println!("{} revision={}", page.url, observation.accessibility.revision);
//! session.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! `close` sends `Browser.close` before process fallback so owned profile state
//! can flush. Attach mode does not own or close the external Chrome process.
//! [`SessionOptions::validate`] rejects incompatible attach/launch settings.
//!
//! # Structured observation and guarded action
//!
//! Compact observation is the default low-cost context. Semantic observation
//! exposes typed page, region, target, record, route, and revision state:
//!
//! ```rust,no_run
//! use glass_browser::browser::session::SemanticObservationLevel;
//! use glass_browser::{BrowserSession, SessionOptions};
//!
//! # async fn run() -> glass_browser::BrowserResult<()> {
//! let session = BrowserSession::start(&SessionOptions::builder().build()?).await?;
//! let semantic = session
//!     .semantic_observe(SemanticObservationLevel::Interactive)
//!     .await?;
//! println!("regions={}", semantic.regions.len());
//!
//! let current = session.observe().await?;
//! let outcome = session
//!     .click_with_revision(
//!         "role=button[name=Save]",
//!         current.accessibility.revision,
//!     )
//!     .await?;
//! println!("status={:?}", outcome.status);
//! session.close().await
//! # }
//! ```
//!
//! Normal observation does not request a screenshot, full DOM, or form values.
//! Those evidence paths are explicit and policy-sensitive. A locator must
//! resolve exactly one actionable current target. A stale expected revision
//! fails before input.
//!
//! # Evidence, Web IR, and tasks
//!
//! [`ExtractionRequest`] selects source-labelled evidence and hard budgets.
//! [`BrowserSession::extract_web_ir`] reconciles it into stable
//! [`GlassWebIrV1`]. Offline callers can validate, diff, and classify
//! continuity without starting Chrome.
//!
//! [`GlassTask::from_json`] validates a strict authored semantic task.
//! [`compile_task`] compiles it against Web IR without browser access or side
//! effects. Plans and receipts contain input names but never authored input
//! values or live browser references. Live execution re-extracts evidence,
//! binds every semantic key to exactly one current revisioned target, applies
//! confirmation/lease rules, and verifies postconditions.
//! The offline JSON example uses the application's direct `serde_json`
//! dependency; typed session APIs do not require callers to serialize
//! intermediate contracts.
//!
//! ```rust,no_run
//! use glass_browser::{compile_task, GlassTask, GlassWebIrV1};
//!
//! fn compile(task_json: &str, ir_json: &str)
//!     -> Result<String, Box<dyn std::error::Error>>
//! {
//!     let task = GlassTask::from_json(task_json)?;
//!     let ir: GlassWebIrV1 = serde_json::from_str(ir_json)?;
//!     ir.validate()?;
//!     Ok(compile_task(&task, &ir)?.to_canonical_json()?)
//! }
//! ```
//!
//! Historical knowledge is advisory. It may explain or rank current evidence,
//! but cannot supply an executable reference or authorize mutation.
//!
//! # Cargo features
//!
//! - `development-runtime` enables the PTY and `glass.toml` implementation
//!   consumed by `glass-dev`; it is disabled by default.
//! - `visual-compare` enables PNG comparison helpers for explicit screenshot
//!   checks.
//! - `fuzzing` enables test-only fuzz hooks and is not for normal applications.
//!
//! docs.rs builds all features. The default library remains browser-focused.
//!
//! # Failure and privacy rules
//!
//! Unknown variants/fields, oversized input, stale revisions, ambiguous
//! targets, unsafe paths, unsupported capabilities, and incompatible backend
//! responses fail explicitly. An `indeterminate` mutation result means Chrome
//! may have accepted input; reconcile current state instead of retrying
//! blindly.
//!
//! Treat DOM, screenshots, PDFs, cookies, storage, evaluated values, profiles,
//! and diagnostics as sensitive. MCP stdout is protocol-only. Structured
//! audit and execution receipts omit typed values, raw selectors, raw CDP
//! errors, and browser handles.
//!
//! # Module map
//!
//! - [`browser`] — Chrome lifecycle, CDP client, DOM/accessibility parsing,
//!   policy, profiles, actions, observations, workflows, and knowledge.
//! - [`browser_backend`] — semantic backend capabilities, requests, responses,
//!   errors, and mandatory dispatcher.
//! - [`capabilities`] — versioned discovery and negotiation manifest.
//! - [`cli`] — Clap argument definitions and command dispatch.
//! - [`connection`] — independent layout, transport, graphics, shell,
//!   multiplexer, presentation-policy, and observatory contracts.
//! - [`daemon`] — local Unix-socket lifecycle, isolated MCP children, leases,
//!   logs, and interrupted-run recovery.
//! - [`development`] — project files, buffers, PTYs, events, graph, replay,
//!   collaboration, Neovim, experiments, and agent harnesses.
//! - [`extensions`] — bounded manifests, permissions, registry, sandbox, and
//!   guarded action boundary.
//! - [`extraction`] — strict bounded evidence requests and source-labelled facts.
//! - [`mcp`] — JSON-RPC/MCP stdio server, prompts, resources, and tools.
//! - [`presentation`] — browser-neutral frame metadata, geometry, mailbox,
//!   ownership, and metrics.
//! - [`protocol`] — canonical versioned request/response operations.
//! - [`reliability`] / [`reliability_runner`] — scenarios, replay evidence,
//!   scorecards, gates, and bounded browser execution.
//! - [`results`] — agent-facing response projections and local diagnostic
//!   artifacts.
//! - [`surfaces`] — nested surface evidence, coverage, provenance, and grants.
//! - [`task_protocol`] / [`task_compiler`] — strict authored tasks and
//!   deterministic execution plans.
//! - [`terminal_graphics`] — Herdr, Kitty, ANSI, and semantic render adapters.
//! - [`tui`] — Ratatui reducer, browser worker, layouts, and remote live view.
//! - [`web_ir`] — stable reconciliation, validation, diff, and continuity.
//! - [`workspace`] — identity, ownership, lifecycle, attachments, and
//!   persistence.
//!
//! # Guides and examples
//!
//! - [Rust SDK](https://github.com/wanazhar/glass/blob/main/docs/rust-sdk.md)
//! - [Runnable examples](https://github.com/wanazhar/glass/blob/main/docs/examples.md)
//! - [Complete feature reference](https://github.com/wanazhar/glass/blob/main/docs/features.md)
//! - [MCP tool catalog](https://github.com/wanazhar/glass/blob/main/docs/mcp-tools.md)
//! - [Security policy](https://github.com/wanazhar/glass/blob/main/SECURITY.md)

/// Browser control modules and the reusable [`browser::BrowserSession`] API.
pub mod browser;
/// Transport-neutral backend capability contract.
pub mod browser_backend;
/// Versioned Glass protocol and capability negotiation.
pub mod capabilities;
/// Command-line argument definitions and dispatch helpers.
pub mod cli;
/// Independent connection-environment and presentation-policy contracts.
pub mod connection;
/// Local Unix-socket daemon lifecycle and MCP bridge.
pub mod daemon;
/// Terminal-native project development runtime contracts.
pub mod development;
/// Validated extension metadata and permission boundaries.
pub mod extensions;
/// Experimental bounded browser-evidence extraction contracts.
pub mod extraction;
/// MCP stdio server, prompts, resources, and tool dispatch.
pub mod mcp;
/// Browser-neutral terminal frame presentation contracts.
pub mod presentation;
/// Transport-neutral versioned request and response envelopes.
pub mod protocol;
/// Versioned browser-free reliability scenario contracts.
pub mod reliability;
/// Bounded browser execution for reliability scenarios and replay evidence.
pub mod reliability_runner;
/// Bounded agent-facing response projections and local result artifacts.
pub mod results;
/// Bounded, evidenced browser surface contracts.
pub mod surfaces;
/// Browser-free deterministic Task Protocol execution-plan compiler.
pub mod task_compiler;
/// Strict, bounded authored Task Protocol v1 inputs.
pub mod task_protocol;
/// Bounded terminal graphics adapters.
pub mod terminal_graphics;
/// Ratatui terminal interface.
pub mod tui;
mod update;
/// Stable Glass Web IR v1 reconciliation and validation.
pub mod web_ir;
/// Bounded workspace identity and lifecycle contracts.
pub mod workspace;

// Keep the most common embedding types on the crate root. Lower-level and
// capability-specific APIs remain organized under `glass_browser::browser`.
pub use browser::{
    AccessibilityDiffSummary, ActionContractError, ActionFailureKind, ActionKind, ActionOutcome,
    ActionStatus, ActionVerificationEvidence, BackendFactory, BackendStartup, BidiBackendConfig,
    BidiBrowserBackend, BrowserResult, BrowserSession, KnowledgeAssessment,
    KnowledgeAssessmentSignal, KnowledgeAssessmentStatus, KnowledgeBackendCapability,
    KnowledgeBackendKind, KnowledgeBackendProvenance, KnowledgeConfidence,
    KnowledgeCurrentValidation, KnowledgeCurrentValidationStatus, KnowledgeEmbeddingProvider,
    KnowledgeEvidenceQuality, KnowledgeGraph, KnowledgeGraphEdge, KnowledgeGraphNode,
    KnowledgeGraphNodeKind, KnowledgeGraphTraversal, KnowledgeInvalidation,
    KnowledgeLearningPolicy, KnowledgeLearningRequest, KnowledgeLearningResult,
    KnowledgeLifecycleEvent, KnowledgeLookupContext, KnowledgeLookupOptions,
    KnowledgeMemoryInfluence, KnowledgeObservationMode, KnowledgeObservationReport,
    KnowledgePortability, KnowledgeProfileScope, KnowledgePurgeResult, KnowledgeRecord,
    KnowledgeRecordBuildOptions, KnowledgeRecordKind, KnowledgeRejectionReason,
    KnowledgeRetrievalCandidate, KnowledgeRetrievalExplanation, KnowledgeRetrievalQuery,
    KnowledgeRetrievalReport, KnowledgeRetrievalSignal, KnowledgeRetrievalSignalKind,
    KnowledgeScope, KnowledgeScopeSelector, KnowledgeSignalKind, KnowledgeSource, KnowledgeStore,
    KnowledgeStoreChange, KnowledgeStoreError, KnowledgeStoreLimits, KnowledgeStoreSnapshot,
    KnowledgeStoreStats, KnowledgeSurfaceCoverage, KnowledgeSurfaceKind,
    KnowledgeSurfaceProvenance, KnowledgeUnderstandingLevel, KnowledgeValidationError,
    KnowledgeVerifiedWorkflowEvidence, MAX_KNOWLEDGE_RECORDS, NavigationOutcome, PageInfo,
    ProofBackend, SessionOptions, SessionOptionsBuilder, StartedBackend, TaskExecutionReceipt,
    TaskExecutionResult, TaskPostconditionReceipt, TaskStepResult, WorkflowBudgets,
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
    TASK_COMPILER_VERSION, TASK_PLAN_SCHEMA_VERSION, TaskCompilationError, TaskCompilationOptions,
    TaskEntityBindingKey, TaskEntityEvidenceRequirement, TaskEvidenceRequirements,
    TaskExecutionPlan, TaskPlanOperation, TaskPlanPrecondition, TaskPlanStep,
    TaskRuntimeCapability, compile_task, compile_task_with_knowledge, compile_task_with_options,
};

pub use protocol::{
    GLASS_PROTOCOL_VERSION, TASK_COMPILE_OPERATION, TASK_VALIDATE_OPERATION, TaskCompilePayload,
    TaskCompileResult, TaskValidationPayload, TaskValidationResult, WEB_IR_CONTINUITY_OPERATION,
    WEB_IR_DIFF_OPERATION, WEB_IR_INSPECT_OPERATION, WEB_IR_VALIDATE_OPERATION, WebIrCompactEntity,
    WebIrContinuityPayload, WebIrContinuityResult, WebIrDiffPayload, WebIrDiffResult,
    WebIrInspectionResult, WebIrPayload, WebIrValidationResult, compile_task_request,
    compile_task_result, validate_task_result, web_ir_continuity_result, web_ir_diff_result,
    web_ir_inspect_result, web_ir_validate_result,
};
pub use results::{
    DetailAvailability, ExperienceResult, OperationResult, RESULT_SCHEMA_VERSION, ResponseMode,
    ResultArtifact, ResultStore, ResultStoreError,
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
