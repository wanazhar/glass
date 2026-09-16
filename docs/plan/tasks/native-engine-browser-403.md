# Native CSP network report delivery (403)

status: done
scope: native-engine/csp-network-report-delivery
issue: 40

## Objective

Deliver report-only CSP violations to their declared network endpoints from
the native HTTP(S) owner. `report-uri` uses the legacy
`application/csp-report` envelope; `report-to` uses a bounded Reporting API
payload resolved through `Reporting-Endpoints` (and the legacy `Report-To`
response header). Delivery is observational and must never delay, authorize,
or otherwise change the protected request.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-399.md`
- `docs/plan/tasks/native-engine-browser-400.md`
- `docs/plan/tasks/native-engine-browser-401.md`
- `docs/plan/tasks/native-engine-browser-402.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [CSP Level 3](https://www.w3.org/TR/CSP/)

Slices 399-402 established the Rust-owned report-only record and its page,
dedicated-worker, EventSource, Fetch, and WebSocket delivery boundaries. The
remaining gap is that records stop at the JavaScript observation boundary;
declared network reporting endpoints are not contacted.

## Contract

- Parse `report-uri` and `report-to` only from report-only declarations, while
  preserving the original policy text and all existing enforcement behavior.
- Resolve report URI references against the protected document URL; accept
  only credential-free HTTP(S) endpoints, strip fragments, bound endpoint
  count and size, and ignore malformed endpoints without failing the owner.
- Prefer a matching `report-to` group when the declaration has `report-to`;
  do not fall back to `report-uri` when that modern declaration is present.
  Resolve groups from bounded `Reporting-Endpoints` and legacy `Report-To`
  response metadata.
- Send one bounded POST per endpoint and violation with the legacy CSP JSON
  envelope or Reporting API JSON envelope and the corresponding media type.
  Reports carry no page cookies, authorization headers, request body, or
  redirect chain.
- Delivery is best-effort and asynchronous behind a process-wide bounded
  concurrency gate. Endpoint failures, non-success responses, and runtime
  shutdown are ignored; they never change the triggering request or surface
  report payloads in logs.
- Schedule reports for page and worker resource violations, WebSocket target
  violations, and inline violations that already cross the native CSP event
  bridge. Each generated violation produces at most one delivery per
  resolved endpoint.
- Preserve existing event ordering, queue limits, cookie state, cache state,
  CORS, mixed-content, timeout, and cancellation semantics. Do not add CDP
  fallback or use the report endpoint as a general page fetch.

## Non-goals

This slice does not implement the complete CSP source-expression grammar,
strict-dynamic trust propagation, dynamic policy-container mutation, Service
Worker policy-container reporting, Reporting API retry/expiry semantics,
browser-wide batching/worker scheduling, or enforced-policy violation events.

## Implementation path

- `resource_loader.rs`: retain report endpoint metadata in CSP declarations and
  policies; parse response reporting headers; construct bounded legacy and
  Reporting API payloads; add the asynchronous capped POST reporter; attach
  endpoint deliveries to URL, inline, and WebSocket report creation.
- `content_process.rs`: preserve WebSocket report deliveries through target
  admission and schedule them at the owner boundary.
- `javascript.rs`: retain endpoint metadata in the inline-policy snapshot and
  schedule inline-script report deliveries when the existing bridge creates
  the structured violation record.
- `tests/native_engine.rs`: verify actual `report-uri` and `report-to`
  requests, payload/media type, no cookie leakage, and unchanged report-only
  fetch behavior.
- Update architecture, plan, analysis, and this task with exact evidence.

## Delivered

- Report-only CSP declarations now retain bounded `report-uri` references and
  `report-to` group names. Response `Reporting-Endpoints` and legacy
  `Report-To` metadata are parsed into bounded groups without changing the
  enforced policy.
- Each URL or inline report created by the existing Rust CSP owner is encoded
  once per resolved endpoint. Legacy endpoints receive the
  `application/csp-report` `{"csp-report": ...}` envelope; modern groups
  receive the `application/reports+json` Reporting API envelope.
- Endpoint resolution strips fragments, rejects credentials and unsupported
  schemes, applies mixed-content protection, and bounds URL and payload
  sizes. Report requests use a dedicated no-cookie client with redirects
  disabled and a five-second timeout.
- Delivery is fire-and-forget behind a process-wide 32-task semaphore.
  Transport errors, non-success responses, absent runtimes, and shutdown are
  ignored; no report body is logged and the protected request remains
  unchanged.
- Page/worker URL and inline reports, plus page/dedicated-worker WebSocket
  reports, now enter the same reporter. WebSocket reporting is scheduled at
  target admission so the owner event remains single-delivery.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp_report_delivery --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine network_reports --locked -- --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --test native_engine report --locked -- --nocapture` (12 passed)
- `cargo test --quiet -p glass-browser --test native_engine websocket --locked -- --nocapture` (7 passed)
- `cargo test --quiet -p glass-browser --test native_engine event_source --locked -- --nocapture` (6 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked --no-fail-fast` (1,118 passed, 1 ignored)
- `cargo fmt --all -- --check`
- `git diff --check`
- documentation validators

Remote CI, push, release, tag, and registry publication remain outside this
local-only checkpoint.
