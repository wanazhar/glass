# Native CSP report lifecycle delivery (400)

status: done
scope: native-engine/csp-report-delivery
issue: 40

## Objective

Complete the report-only CSP lifecycle for the resource owners already covered
by slice 399. Every bounded report-only violation produced by a native loader
operation must reach the owning JavaScript realm exactly once, without leaving
stale records to contaminate a later host turn or silently discarding reports
when an asynchronous transport crosses a worker boundary.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-399.md`

Slice 399 added structured `SecurityPolicyViolationEvent` delivery, but the
current request paths still have three lifecycle hazards: Fetch drains its
page-event batch before the connect-policy owner runs, repeated inline-style
refreshes can report unchanged nodes again, and EventSource uses a cloned
loader whose records are not returned with the stream event.

## Contract

- Fetch and Fetch response-stream continuations deliver connect-policy
  report-only records in the same owning page turn as the response/event that
  caused them.
- EventSource open/reconnect failures and successful opens carry report-only
  records across the asynchronous connection boundary; the page and dedicated
  worker owners receive them through their own event dispatchers.
- Inline style elements and style attributes are evaluated against the current
  enforced policy on every refresh, but an unchanged node/source is not
  reported again. A newly attached or changed inline style reports once, and a
  detached node cannot leave an unbounded report ledger.
- All records remain bounded, structured, validated, and excluded from logs.
  Enforced CSP remains authoritative; report-only records never authorize or
  block a resource.
- Every `take_csp_violations` call has a matching owner: records are delivered
  to a page/worker realm or intentionally discarded only during teardown.

## Implementation path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
  - preserve report records until the request owner can consume them;
  - keep report-only style decisions explicit and bounded.
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
  - move Fetch report draining after the loader operation;
  - carry EventSource reports over the stream channel;
  - maintain a document-owned inline-style observation ledger and dispatch
    records at the owning host turn.
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
  - accept CSP records on page and worker transport-event dispatches while
    preserving the existing structured event constructor.
- `crates/glass-browser/tests/native_engine.rs`
  - add focused Fetch, style-refresh, and EventSource delivery assertions.
- Update the architecture, plan, analysis checkpoint, and this task with
  exact delivered behavior and verification evidence.

## Tradeoffs and non-goals

This slice does not add `report-uri`/`report-to` network POST delivery,
`strict-dynamic`, dynamic policy-container mutation, WebSocket report delivery,
or the complete CSP source-expression grammar. It preserves bounded queues and
the existing asynchronous transport model; report ordering follows the host
task that owns the triggering operation. The style ledger is document-local so
it can be reclaimed with the document and does not turn the resource loader
into a global history store.

## Delivered

- Fetch report-only `connect-src` records are drained after the actual loader
  operation and delivered in the same owning page evaluation as the resolved
  response, including response-stream continuation paths.
- EventSource open, reconnect, timeout, and connection-error records cross the
  asynchronous stream channel and are dispatched by the page or dedicated
  worker owner that initiated the connection.
- Inline style elements and attributes rerun the current enforced policy on
  every refresh while a document-local ledger suppresses duplicate reports for
  unchanged node/source signatures and prunes detached nodes.
- All new page/worker deliveries use the existing bounded structured event
  validation and do not change report-only authorization behavior.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test -p glass-browser --test native_engine native_content_process_delivers_report_only --locked -- --nocapture` (4 passed)
- `cargo test -p glass-browser --test native_engine event_source --locked -- --nocapture` (5 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked --no-fail-fast` (1,117 passed, 1 ignored)
- `cargo fmt --all -- --check`
- `git diff --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-400.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`

Remote CI, push, release, tag, and registry publication remain outside this
local-only checkpoint.
