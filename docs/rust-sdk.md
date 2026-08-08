# Rust SDK

The `glass-browser` package exports the `glass_browser` library. The library
owns the same policy, revision, semantic, workflow, workspace, and result
contracts used by the CLI, TUI, MCP server, and daemon.

## Dependency and features

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1" # only needed when your application reads/writes JSON contracts
```

Cargo aliases the package to `glass`, so Rust imports use `glass::…`. Without
the alias, imports use `glass_browser::…`.

| Feature | Default | Purpose |
|---|---:|---|
| `development-runtime` | no | Real PTY process manager and `glass.toml` support used by `glass-dev` |
| `visual-compare` | no | PNG comparison helpers for explicit screenshot checks |
| `fuzzing` | no | Fuzz-only hooks; do not enable in normal applications |

docs.rs builds all features. A default library build remains browser-focused
and does not pull the PTY/config dependencies.

## Session ownership

`BrowserSession` is one owned or attached browser control session. Methods take
`&self` because operation serialization and mutable browser state are internal.
The session owns target/frame selection, revision counters, bounded caches,
policy interception, presentation state, and an optional Chrome child.

```rust,no_run
use glass_browser::{BrowserSession, SessionOptions};

# async fn run() -> glass_browser::BrowserResult<()> {
let options = SessionOptions::builder()
    .incognito(true)
    .headed(false)
    .port(9222)
    .build()?;
let session = BrowserSession::start(&options).await?;
let page = session.navigate("https://example.com").await?;
println!("{}", page.url);
session.close().await?;
# Ok(())
# }
```

Call `close` for owned sessions. It sends `Browser.close` before process
fallback so named-profile state can flush. Dropping is a fallback, not the
preferred persistence boundary. An attached session never owns or closes the
external Chrome process.

`SessionOptions::validate` rejects incompatible attach settings before
connection. An explicit target ID is required when the endpoint exposes
multiple page targets.

## Structured observation and guarded actions

Compact observation is the low-cost operational context. Semantic observation
adds page/region/target types and explicit levels.

```rust,no_run
use glass_browser::browser::session::SemanticObservationLevel;
use glass_browser::{BrowserSession, SessionOptions};

# async fn run() -> glass_browser::BrowserResult<()> {
let session = BrowserSession::start(&SessionOptions::builder().build()?).await?;
let semantic = session
    .semantic_observe(SemanticObservationLevel::Interactive)
    .await?;
println!("revision={} regions={}", semantic.revision, semantic.regions.len());

let compact = session.observe().await?;
let outcome = session
    .click_with_revision("role=button[name=Save]", compact.accessibility.revision)
    .await?;
println!("status={:?}", outcome.status);
session.close().await
# }
```

Do not retain a revisioned browser reference across navigation or unverified
page drift. Re-observe and resolve again. Unique resolution, actionable state,
policy, and expected revision are preconditions; failure occurs before input
when those preconditions cannot be proven.

## Evidence extraction and Web IR

`ExtractionRequest` is strict and non-mutating. The caller selects sources and
hard budgets. `extract_evidence` returns source-labelled facts;
`extract_web_ir` additionally reconciles them into stable Glass Web IR v1.

```rust,no_run
use glass_browser::{
    BrowserSession, EvidenceSource, ExtractionBudgets, ExtractionRequest,
    ExtractionScope, SessionOptions, EXTRACTION_CONTRACT_SCHEMA_VERSION,
};

# async fn run() -> glass_browser::BrowserResult<()> {
let session = BrowserSession::start(&SessionOptions::builder().build()?).await?;
let request = ExtractionRequest {
    schema_version: EXTRACTION_CONTRACT_SCHEMA_VERSION,
    scope: ExtractionScope::Document,
    sources: vec![EvidenceSource::Accessibility, EvidenceSource::Forms],
    budgets: ExtractionBudgets::default(),
};
let ir = session.extract_web_ir(&request).await?;
println!("revision={} entities={}", ir.revision, ir.entities.len());
session.close().await
# }
```

Requesting `Dom` is an explicit deep-inspection choice. Forms evidence may
contain sensitive state and is policy-gated. Returned evidence declares
missing sources, truncation, coverage, and opaque regions instead of inventing
certainty.

Offline callers can use `reconcile_evidence`, `GlassWebIrV1::validate`,
`GlassWebIrV1::diff`, and `classify_entity_continuity`. Validation does not
turn an offline graph into browser authority.

## Task Protocol and deterministic compilation

Authored tasks contain semantic scope, named inputs, budgets, risk, ambiguity,
revision policy, and postconditions. They never contain selectors or CDP
handles. Compilation consumes validated Web IR and emits a value-free plan.

```rust,no_run
use glass_browser::{compile_task, GlassTask, GlassWebIrV1};

fn compile(task_json: &str, ir_json: &str) -> Result<String, Box<dyn std::error::Error>> {
    let task = GlassTask::from_json(task_json)?;
    let ir: GlassWebIrV1 = serde_json::from_str(ir_json)?;
    ir.validate()?;
    let plan = compile_task(&task, &ir)?;
    Ok(plan.to_canonical_json()?)
}
```

`compile_task_with_knowledge` and `compile_task_with_options` may attach
advisory memory provenance. Executable entity selection, preconditions, and
postconditions still derive only from current IR.

For live execution, prefer `BrowserSession::execute_task`. It validates,
extracts current evidence, compiles, binds semantic keys to exactly one current
revisioned target, enforces confirmation/lease rules, performs the operation,
and verifies postconditions. Receipts exclude authored values and live browser
references.

## Intent, workflows, and recovery

The session intent API separates normalization, candidate resolution,
selection, and execution. `resolve_intent` is browser-free when supplied
candidate evidence; `executeIntent`-equivalent session methods re-observe and
re-resolve before action.

Workflow types define typed inputs, budgets, steps, conditions, outputs,
checkpoints, terminal proof, and traces. Authoring helpers compile strict YAML
or JSON and report source locations without echoing sensitive input.

After a possibly dispatched mutation fails, inspect `ActionOutcome`,
`TaskExecutionResult`, or `WorkflowRunResult`. An `indeterminate` result means
mutation may have occurred. Use `recover_run`, current observation, checkpoint,
or returned retry classification; do not replay the mutation blindly.

## Knowledge

`KnowledgeStore` is a bounded, crash-safe local snapshot. Build lookup context
from fresh observation plus explicit profile/workspace/backend/surface inputs.
Assessment and retrieval are read-only. Learning requires successful,
non-private, non-truncated, non-sensitive evidence and matching scope
provenance.

Knowledge records never retain executable target references. Historical
fingerprints can explain or rank current candidates but cannot authorize a
mutation.

## Development Runtime

Enable `development-runtime` to embed project tooling:

```toml
glass = { package = "glass-browser", version = "0.3", features = ["development-runtime"] }
```

```rust,no_run
use glass_browser::development::ProjectWorkspace;

fn inspect(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let workspace = ProjectWorkspace::open(root)?;
    println!("{:?}", workspace.detection());
    for entry in workspace.list_files()? {
        println!("{}", entry.path);
    }
    Ok(())
}
```

`ProjectWorkspace` owns canonical-root confinement, native buffers, event
timeline, graph, replay, and process manager. File and process limits are
enforced before retention. Mutations should carry an `Actor`; external-agent
edits and links remain attributable.

`AgentToolGateway` validates a fixed descriptor catalog, call envelope, JSON
schema, authorization, and confirmation. Audit events store argument byte
count/digest and result metadata, not argument values.

## Backends, surfaces, and presentation

- `browser_backend` defines semantic requests, responses, profiles,
  capabilities, errors, and `BrowserBackendDispatcher`. Call through the
  dispatcher; it validates capability dependency closure and results.
- `BackendFactory` selects an explicit or best certified backend without
  iteration-order dependence. An explicit unusable backend fails; it does not
  silently fall back.
- `surfaces` models nested browser-hosted boundaries, evidence, coverage,
  provenance, bridge grants, and action requirements. Detection is never an
  input grant.
- `presentation` owns frame metadata, geometry/revision mapping, latest-frame
  mailbox, payload ownership events, and metrics. It does not own browser or
  terminal transports.
- `terminal_graphics` selects Herdr, Kitty, ANSI, or semantic output and keeps
  payload retention bounded.

## Protocol, MCP, daemon, and results

`protocol` contains canonical Glass request/response envelopes. MCP adds
JSON-RPC framing but maps tools into those payloads. `mcp::serve` owns stdio;
stdout must contain only protocol frames.

`daemon` provides local Unix-socket lifecycle, isolated MCP children,
mutation leases, bounded status/logs, and explicit interrupted-run recovery.
It is not a network service.

`ExperienceResult` and `OperationResult` project bounded minimal, normal, or
diagnostic output. Large diagnostic detail belongs in `ResultStore`; callers
receive a local result ID rather than an unbounded transport payload.

## Public module map

| Module | Contract |
|---|---|
| `browser` | Chrome/CDP lifecycle, session API, policies, profiles, actions, observations, workflows, knowledge |
| `browser_backend` | Transport-neutral semantic backend contract and dispatcher |
| `capabilities` | Versioned discovery and negotiation manifest |
| `cli` | Clap types and shared command runner |
| `connection` | Independent connection environment, presentation profiles, policy reasons, and observatory metrics |
| `daemon` | Local socket, isolated MCP sessions, leases, and recovery |
| `development` | Project files, buffers, PTYs, events, graph, replay, harnesses, collaboration |
| `extensions` | Manifest, permission, registry, sandbox, and guarded-action boundary |
| `extraction` | Strict source-labelled evidence extraction |
| `mcp` | JSON-RPC/MCP stdio server, prompts, resources, and tool dispatch |
| `presentation` | Browser-neutral frame metadata, geometry, ownership, mailbox, and metrics |
| `protocol` | Canonical versioned operations and responses |
| `reliability` | Browser-free scenario, fixture, replay, scorecard, and gate contracts |
| `reliability_runner` | Bounded browser execution for reliability scenarios |
| `results` | Agent-facing projections and local diagnostic artifacts |
| `surfaces` | Bounded multi-surface understanding and bridge grants |
| `task_compiler` | Deterministic Task Protocol to execution-plan compiler |
| `task_protocol` | Strict authored semantic task contract |
| `terminal_graphics` | Herdr, Kitty, ANSI, and semantic render adapters |
| `tui` | Ratatui application, reducer, worker, layouts, and remote live view |
| `web_ir` | Stable Web IR reconciliation, validation, diff, and continuity |
| `workspace` | Workspace identity, ownership, attachments, lifecycle, and persistence |

## Errors and privacy

Browser methods return `BrowserResult<T>`. Stable higher-level contracts use
typed errors such as `ActionContractError`, `TaskProtocolError`,
`TaskCompilationError`, `WebIrValidationError`, `KnowledgeStoreError`, and
backend/workspace errors. Do not parse display strings when a typed result is
available.

Treat DOM, screenshots, PDFs, cookies, storage, evaluated values, profiles,
and diagnostic logs as sensitive. Do not write them to tracing or durable
artifacts unless the caller explicitly selected that evidence path.

## Examples and validation

See [runnable examples](examples.md). Library documentation is validated with:

```console
RUSTDOCFLAGS='-D warnings' cargo doc -p glass-browser --all-features --no-deps
cargo test -p glass-browser --doc --all-features
```
