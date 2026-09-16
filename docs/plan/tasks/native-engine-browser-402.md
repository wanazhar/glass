# Native WebSocket CSP report delivery (402)

status: done
scope: native-engine/websocket-csp-report-delivery
issue: 40

## Objective

Close the remaining WebSocket owner gap in the native report-only CSP
lifecycle. A page or worker WebSocket connection that violates an enforced
owner's report-only `connect-src` declaration must deliver a bounded,
structured `SecurityPolicyViolationEvent` to the initiating JavaScript realm
before the corresponding WebSocket open/error observation, without changing
the enforced authorization decision.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-400.md`
- `docs/plan/tasks/native-engine-browser-401.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

Slices 399-401 established the structured CSP record, page/worker dispatch
boundary, asynchronous EventSource transport, and worker response-policy
ownership. `websocket_target` already evaluates enforced `connect-src`, but it
does not retain the report-only records for the asynchronous WebSocket owner.

## Contract

- Resolve the WebSocket URL and its HTTP(S) policy equivalent exactly once for
  the owner. A report-only declaration reports when both representations are
  outside its source list; this preserves the existing `ws`/`wss` enforcement
  compatibility with HTTP(S) CSP source expressions without duplicate records.
- Carry records produced while admitting the connection through the native
  WebSocket event channel. Successful opens deliver records before `open`; a
  handshake failure delivers them before `error`; later message, close, and
  transport-error events carry no duplicate records.
- Deliver page-owned records through the persistent page event batch and
  worker-owned records through the structured worker dispatch path. Each record
  is delivered once and remains subject to existing event and queue bounds.
- Report-only records are observational only: they never authorize a blocked
  WebSocket, weaken mixed-content checks, or alter handshake behavior.
- Preserve existing cookie, protocol, origin, timeout, message-size, close,
  and error semantics. Do not log policy source bytes or report payloads.
- Keep non-network document owners and invalid URLs on their existing typed
  error paths. Do not add a CDP fallback or a second WebSocket transport.

## Non-goals

This slice does not add `report-uri`/`report-to` network delivery, dynamic
policy-container mutation, a complete CSP source-expression grammar, or
WebSocket response-header policy inspection. Service Worker and SharedWorker
policy reporting beyond their existing worker owner path remain separate
issue #40 work.

## Implementation path

- `resource_loader.rs`: compute bounded WebSocket report records alongside the
  existing enforced policy decision and carry them in `NativeWebSocketTarget`.
- `content_process.rs`: preserve records through connection startup and attach
  them to the first WebSocket event delivered to the page or worker owner.
- `javascript.rs`: admit records to page and worker WebSocket event dispatch
  using the existing structured CSP event constructor.
- `native_engine.rs`: add a deterministic in-process WebSocket report witness
  if the shared loader contract needs direct coverage.
- `tests/native_engine.rs`: cover page and dedicated-worker report delivery,
  ordering, and the unchanged enforced block path.
- Update the architecture, plan, analysis checkpoint, and this task with exact
  delivered behavior and verification evidence.

## Delivered

- Page and dedicated-worker WebSocket targets now compute bounded report-only
  `connect-src` records alongside the existing enforced decision.
- WebSocket open and handshake-error events carry those records over the
  asynchronous connection channel and deliver them before the first owner
  observation; later message, close, and transport-error events do not repeat
  the records.
- Page and worker dispatchers use the existing structured CSP event bridge,
  preserving the actual `ws`/`wss` blocked URI while accepting the normalized
  HTTP(S) policy URL for source matching.
- Report-only records remain observational and do not alter authorization,
  mixed-content checks, cookies, protocols, timeouts, or WebSocket lifecycle
  semantics.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_delivers_report_only_connect_events_for_websocket --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_delivers_report_only_connect_events_for_websocket --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine websocket --locked -- --nocapture` (7 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked --no-fail-fast` (1,117 passed, 1 ignored)
- `cargo fmt --all -- --check`
- `git diff --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-402.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`

Remote CI, push, release, tag, and registry publication remain outside this
local-only checkpoint.
