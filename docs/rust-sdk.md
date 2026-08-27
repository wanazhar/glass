# Rust SDK

Status: Current 0.3.13 source behavior (including current-source work in this checkout)

The workspace publishes two Rust packages with different boundaries:

* `glass-browser` (`glass_browser`) is the browser-control library and
  browser-only CLI product.
* `glass-dev` (`glass_dev`) is the development runtime, resident agent/Pi
  integration, project/editor/process tooling, and the `glass` CLI product.

The public `browser_workspace` module provides the bounded, revision-safe
controller, state, intent, action, capability, entity, target, layout, focus,
ownership, and presentation contracts used by the browser and development
terminal products.

The `glass-browser` package exports the `glass_browser` library. It owns the
policy, revision, semantic, workflow, workspace, and result contracts used by
the browser CLI, browser TUI, MCP server, daemon, and Rust callers.

The focused browser API is documented at
[`docs.rs/glass-browser`](https://docs.rs/glass-browser). The development
runtime API is documented at
[`docs.rs/glass-dev`](https://docs.rs/glass-dev).

docs.rs renders published Rust API artifacts. This guide follows the checked-in
source, which is version `0.3.13` with current-source work; published `0.3.12`
docs.rs pages are immutable release evidence. Verify newer source-level
surfaces, including `browser_workspace`, against this checkout rather than
assuming they were part of the published release. Some benchmark-style Cargo
examples use development-only dependencies and are not all listed in the
docs.rs example index; the checked-in examples catalog remains the source-level
inventory.


## Dependency and features

Use the focused browser crate when embedding browser control:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1" # only needed when your application reads/writes JSON contracts
```

Cargo aliases the package to `glass`, so Rust imports use `glass::…`. Without
the alias, imports use `glass_browser::…`. The development runtime is a
separate package:

```toml
glass-dev = "0.3"
```

Use `glass_dev::…` for project/editor/process/LSP, resident Pi, harness,
collaboration, Git, and TUI APIs. `glass-dev` depends on `glass-browser`; the
browser crate does not depend on `glass-dev`, so browser-only embeddings do not
acquire the development runtime.

| `glass-browser` feature | Default | Purpose |
|---|---:|---|
| `visual-compare` | no | PNG comparison helpers for explicit screenshot checks |
| `fuzzing` | no | Fuzz-only hooks; do not enable in normal applications |

docs.rs builds all features. The default `glass-browser` library remains
browser-focused; development runtime dependencies such as PTY integration are
in `glass-dev`, not optional browser features.

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

The focused `glass-browser` crate does not export the Development Runtime;
project files, PTYs, LSP, Pi, and Neovim ownership remain in `glass-dev`.


## Development Runtime

Depend on `glass-dev` to embed project tooling:

```toml
glass-dev = "0.3"
```

```rust,no_run
use glass_dev::development::ProjectWorkspace;

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

### Managed Pi boundary

`glass_dev::pi_runtime` exposes `PiReadiness`, `PiReadinessComponent`,
`PiReadinessState`, `PiSessionRequest`, and `PINNED_PI_SDK_VERSION`
(`0.84.3`). `PiReadiness` checks Node (currently 22.19.0 or newer), the
managed SDK, authentication, provider, and session state. `PiSessionRequest`
is the native resident-session protocol used by the development agent runtime;
it is distinct from the CLI's legacy one-shot `PiHarness` RPC adapter.

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
- `terminal_graphics` provides Herdr-owned pane, Kitty protocol, ANSI canvas,
  and semantic render adapters. The development TUI's live policy selects
  `Herdr`, explicit `Kitty`, or bounded `Ansi`; `Auto` may remain
  semantic-only when no native backend is detected.
- Live quality is bounded to data (~3 FPS), balanced (~6 FPS), or smooth (~12
  FPS). ANSI fit supports `contain`, `cover`, and `actual`; native image paths
  use `contain`. These are CLI/TUI policy inputs, not browser-session
  ownership.


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

Module paths are package-qualified below because both packages expose a
`browser` and a `tui` module. This is the checked-in public Rust surface; the
generated rustdoc pages and source exports are the authority for individual
items.

### `glass-browser` (`glass_browser`)

| Module | Contract |
|---|---|
| `browser` | Chrome/CDP lifecycle, session API, policies, profiles, actions, observations, workflows, and advisory knowledge |
| `browser_backend` | Transport-neutral semantic backend contract and dispatcher |
| `browser_workspace` | Bounded revision-safe browser UI state, actions, focus, ownership, layout, and presentation contracts |
| `capabilities` | Versioned discovery and negotiation manifest |
| `cli` | Clap argument definitions and browser command dispatch |
| `connection` | Independent connection environment, presentation profiles, policy reasons, and observatory metrics |
| `daemon` | Local socket, isolated MCP sessions, leases, and recovery |
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
| `terminal_graphics` | Herdr, Kitty, ANSI, and semantic terminal render adapters |
| `tui` | Standalone browser TUI reducer, responsive layouts, semantic selection, and bounded live presentation |
| `web_ir` | Stable Web IR reconciliation, validation, diff, and continuity |
| `workspace` | Workspace identity, ownership, attachments, lifecycle, and persistence |

`update` is an implementation module and is not public. The browser crate does
not export the Development Runtime; project files, native editors, PTYs, LSP,
Pi, Neovim, Git, and collaboration belong to `glass-dev`.

### `glass-dev` (`glass_dev`)

| Module | Contract |
|---|---|
| `agents` | Resident Pi scheduling, lifecycle, evidence, and approval state |
| `browser` | Development-browser service configuration, state, and worker handle |
| `cli` | `glass` command dispatch for development routes |
| `customization` | User/project configuration, skills, and custom commands |
| `daemon` | Resident development daemon integration |
| `debugger` | Debugger and semantic breakpoint support |
| `development` | Project files, buffers, editors, PTYs, LSP, graph, replay, and collaboration |
| `experiments` | Isolated Git worktree experiments and comparison |
| `external_agents` | One-shot adapters for installed external coding agents |
| `git` | Governed Git workspace operations |
| `github` | GitHub status, review, and pull-request shipping operations |
| `harness` | Discovery and safe launch of installed coding harnesses |
| `intelligence` | Development graph and causal intelligence projections |
| `kernels` | Kernel process and runtime integration |
| `lsp` | LSP-facing language-service integration |
| `mcp` | Development MCP server and tool integration |
| `pi_runtime` | Managed Pi runtime readiness and sessions |
| `tasks` | Task scheduling, retry, evidence, and verification requirements |
| `testing` | Test execution and result collection |
| `tools` | Governed development-tool routing |
| `trust` | Workspace trust decisions and persistence |
| `tui` | Resident development terminal application and surface routing |
| `workspace` | Workspace ownership and shared handles |

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
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked
cargo test --workspace --doc --all-features --locked
```
