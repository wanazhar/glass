# Native semantic MCP contracts and stack-safe lifecycle probes (327)

Status: implemented locally in the native runtime.

This slice routes the shared semantic MCP contracts through the native
resident session. Native MCP now has explicit adapters for bootstrap
observation, structured extraction, intent resolution and execution,
knowledge-backed resolution, observation deltas, checkpoints, diagnostics,
verification, waiting, actions, targets, storage, and the existing core
browser operations. The browser-only and development-suite dispatchers keep
their product-specific catalog boundaries; native selection never silently
creates or attaches a Chromium/CDP session.

The native adapter shares the existing extraction, policy, intent, checkpoint,
verification, and result envelopes instead of maintaining a second semantic
schema. A missing native target defaults to the explicitly selected focused
target where the MCP contract permits it. Diagnostics expose sanitized native
parser/style evidence and preserve the stable snake-case diagnostic fields.

The runtime also hardens the expensive native layout path. CSS cascade scratch
state is heap-owned and initialized field-by-field, avoiding a large temporary
value on the caller stack. JavaScript evaluations that only compare
`document.readyState` with a literal use a no-layout bootstrap snapshot;
ordinary scripts still receive the full layout-backed document snapshot. This
keeps lifecycle waits cheap without changing layout semantics for general
script evaluation.

## Tradeoffs

Heap-owned cascade scratch adds an allocation to computed-style/layout
evaluation, but removes a stack-size cliff from nested MCP and lifecycle
calls. Field-by-field initialization is more verbose than a zeroed allocation
because Rust validity invariants must be preserved for every `Option` and
enum. The no-layout path is deliberately restricted to exact literal
`document.readyState` comparisons; broader source matching would be faster but
could change observable script behavior.

## Local evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --locked native_mcp_routes_ -- --test-threads=1`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_runtime_exposes_shared_semantic_session_contracts -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`

The focused native MCP route witnesses execute without Chromium. This
checkpoint is local-only; it has not been pushed and has no remote CI result.
Native Core Web Profile conformance, complete operation coverage, production
process isolation, cross-platform evidence, and release promotion remain
issue #40 gates.
