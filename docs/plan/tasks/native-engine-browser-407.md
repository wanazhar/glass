# Native Service Worker document policy-container propagation (407)

status: done
scope: native-engine/service-worker-document-policy-container
issue: 40

## Objective

Carry the protected document's CSP policy container across the native Service
Worker interception boundary. A Service Worker must not become a way around a
controlled page's enforced `connect-src` policy, and a worker-served navigation
must install its response policy before the resulting document starts parsing
or executing script.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-406.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

The native page Fetch path already applied `connect-src` in the resource
loader, but a controlled request was offered to the Service Worker first. If
the worker returned a synthetic `Response`, the page policy was never reached;
the worker could therefore satisfy a request that the document policy denied.
The navigation path had the complementary response-side operation, but it
needed an integration witness proving that a synthetic worker response's CSP
was installed before page Fetch ran.

## Contract

- For a controlled non-navigation request, the current document's enforced
  `connect-src` policy and mixed-content boundary are checked before the
  Service Worker fetch event is evaluated. A denied request produces no worker
  response and no network request.
- A request that is not handled by a Service Worker falls through to the
  normal resource loader, which remains the owner of its own policy check and
  report record. The preflight must not duplicate that fallback report.
- A handled request records each applicable report-only document `connect-src`
  violation once, after the worker has accepted the request. Report-only state
  remains observational and never authorizes or blocks the request.
- A Service Worker-served navigation installs all CSP response headers,
  report-only declarations, and reporting metadata under the final response
  URL before document parsing, meta-policy processing, subresource loading, or
  page-script execution.
- Service Worker script and worker-owned Fetch policy remain distinct from the
  controlled page policy. Worker-owned network operations continue to use the
  worker script response policy, while the page policy governs the page request
  that enters the worker.
- URL validation, credential rejection, policy limits, mixed-content checks,
  bounded report queues, and the single shared Rust matcher remain unchanged.
  No CDP or second policy implementation is introduced.

## Non-goals

This slice does not implement the remaining CSP grammar, redirect-aware path
matching, navigation-specific `navigate-to` policy, report-only meta policy,
or full Service Worker browser-wide scheduling. Those remain separate issue
#40 conformance gates.

## Implementation path

- Add shared `NativeResourceLoader` operations for enforced and report-only
  document `connect-src` decisions at a pre-interception URL boundary.
- Invoke the enforced operation in `NativeServiceWorkerRegistry` only after a
  matching active registration is found and before worker event evaluation.
- Record the report-only decision only for a handled worker request so an
  unhandled request can use the ordinary loader without duplicate delivery.
- Preserve the existing final-URL response-header installation in the worker
  navigation owner and add an HTTP integration witness for its interaction
  with a controlled page Fetch.
- Add a focused loader unit witness and synchronize the architecture, active
  plan, analysis, and task evidence.

## Tradeoffs

- The preflight duplicates only the small policy decision boundary, not the
  network loader or matcher. Keeping the matcher and policy storage in the
  resource loader prevents authorization drift while allowing the worker to be
  rejected before JavaScript side effects.
- Report-only records are delayed until the worker confirms a handled
  response. This preserves exactly-once ownership with the normal fallback
  loader and keeps a worker that declines a request observationally identical
  to an ordinary network request.
- Navigation response policies remain keyed by the final response URL. This
  makes synthetic worker navigations follow the same document-owner lookup as
  HTTP navigations, at the cost of retaining one bounded policy entry per
  visited URL until normal loader eviction.

## Delivered

- Controlled page Fetch/XHR requests now pass the page's enforced
  `connect-src` and mixed-content preflight before Service Worker evaluation.
- A worker cannot satisfy a request denied by the page policy; the regression
  witness confirms that the worker handler and `/blocked` network endpoint are
  both bypassed.
- Handled worker requests use the existing report-only CSP record path, while
  unhandled requests retain ordinary loader ownership without duplicate
  records.
- Synthetic Service Worker navigation responses retain the existing final-URL
  CSP installation and are proven to govern the first page Fetch.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib service_worker_interception_uses_the_document_connect_policy_once --locked -- --nocapture` (1 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --test native_engine native_content_process_propagates_service_worker_document_csp_to_controlled_fetch --locked -- --nocapture` (1 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --test native_engine service_worker --locked -- --nocapture` (18 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked --no-fail-fast` (1,124 passed, 1 ignored)
- `cargo fmt --all -- --check`
- `git diff --check`
- release-documentation, documentation-depth, TUI-shortcut, and documentation-coverage validators

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
