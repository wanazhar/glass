---
id: native-engine-browser-842
scope: glass-browser/native-engine/parent-owned-cookie-authority
status: in-progress
depends-on: [native-engine-browser-821, native-engine-browser-822]
---

# Glass native-engine browser slice 842: parent-owned cookie authority

## Objective

Make the native browser parent the sole authority for cookie state and
cookie-bearing network requests across process-backed pages and workers. Remove
the complete cookie jar from the content-process boundary while preserving
script-visible `document.cookie`, Fetch credential modes, response-cookie
behavior, profile persistence, and shared-profile synchronization.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-821.md`
- `docs/plan/tasks/native-engine-browser-822.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Fetch Standard credentials mode](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [HTML Standard `document.cookie`](https://html.spec.whatwg.org/multipage/dom.html#dom-document-cookie)

## Contract

- The parent native browser backend owns the complete per-context cookie jar.
  Cookie matching, `Set-Cookie` acceptance/rejection, expiration/deletion,
  profile persistence, and shared-profile journal publication happen only in
  the parent. The content process and its page/worker realms are not cookie
  authorities or durable writers.
- Do not serialize a full cookie profile, an HttpOnly value, a raw
  `Cookie`/`Set-Cookie` header, or a cookie-bearing profile path to the content
  process. Remove child-side cookie-profile load/save and persistence paths.
- The content realm receives only the current document URL's script-visible,
  non-HttpOnly `document.cookie` projection. A setter returns a bounded typed
  write carrying the exact context, frame, document generation, and source
  URL. The parent validates and applies it, then refreshes the projection;
  synchronous same-turn setter preview must not grant authority to the child.
- All content-originated page and worker network requests that can send or
  accept cookies run through a bounded parent broker and the parent's existing
  network policy. Preserve `include`, `same-origin`, and `omit` behavior at
  every redirect hop, current redirect/referrer/CORS behavior, response body
  streaming, cancellation, and response-cookie update/deletion. No direct
  child-loader retry is permitted if the parent broker fails.
- Cookie-bearing responses and changes are committed once in the parent
  before later owner requests can observe them. `Set-Cookie` is never exposed
  as a script-readable response header. Profile import/clear, restart, and
  same-profile live-session journal behavior continue to use the same parent
  authority; separate profiles remain isolated.
- Every child request/write is checked against its captured context/frame and
  document generation. Stale owners fail explicitly and cannot redirect to a
  currently selected target. IPC messages and queues stay bounded; overflow,
  process exit, cancellation, or protocol failure surfaces a typed error.
- Preserve the cookie attributes and matching behavior already supported by
  the native profile. Do not claim full Cookie/Web IDL or Web Platform Test
  conformance beyond selected passing cases.

## Path

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-842.md`
- `CHANGELOG.md`

## Verification

- Add a process-protocol regression proving that no full cookie profile,
  HttpOnly value, raw cookie header, or cookie-bearing storage path crosses
  into the content process, and that child cookie writes are owner-tagged.
- Process-backed HTTP tests verify script-visible projection versus HttpOnly
  invisibility; `document.cookie` set/read and subsequent requests; worker
  `include`/`same-origin`/`omit`; redirects; response updates/deletion; and
  request behavior after import, clear, and same-profile synchronization.
  Include separate-profile isolation and stale-owner rejection.
- Verify the parent still owns cookie state after child shutdown/restart and
  that child-side code cannot write cookie-bearing profile data.
- Run selected Fetch/Cookie/SharedWorker WPT cases and record exact selections
  and deviations; do not substitute mocked-loader checks for network tests.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
- Run the focused native-engine cookie process-backed test(s) after the check;
  keep unrelated package/workspace suites for the final issue #40 gate.
- `cargo fmt --all -- --check`
- `git diff --check`
- Run relevant documentation inventory/coverage checks.

## Current Evidence

- The current content-process protocol accepts complete cookie profiles and
  cookie changes; the child still owns a `NativeResourceLoader` and persists
  cookie state. This checkpoint removes the full profile from externally
  routed SharedWorker creation. The parent resolves the exact source frame,
  checks context and document generation under that owner's lock, seeds its
  SharedWorker loader from the parent engine's profile, and replays the
  coordinator's cookie overrides. It does not remove the child's general
  profile mirror/persistence or implement the general parent network broker.
- Slices 820-823 provide process-backed evidence for profile journals,
  import/clear, and SharedWorker Fetch credential behavior. Those results are
  baseline behavior to preserve, not evidence that the parent-only boundary is
  implemented.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
  passed (exit 0; existing superseded HTML-parser dead-code warnings).
- `shared_worker_create_serialization_has_no_cookie_profile` passed (1 passed,
  1,682 filtered). `native_content_process_shared_worker_fetch_api_uses_live_cookies`
  passed (1 passed, 891 filtered).
- `native_content_process_shared_worker_fetch_credentials_modes_follow_redirects`
  failed twice (exit 101), timing out while waiting for the worker's Fetch
  sequence to settle at `tests/native_engine.rs:33229`. The cause is
  undiagnosed; credential/redirect behavior is not verified by this
  checkpoint.
- Formatting, `git diff --check`, the exact-owner stale-create regression,
  selected WPT, and broader parent-only acceptance tests have not been run.
- The documentation coverage check currently reports an unrelated stale live
  measurement in `docs/mcp-schema-budget.md` (`Serialized tools` array); the
  new Slice 842 documents themselves are inventoried. Do not fold that separate
  MCP measurement repair into this cookie task.
- Issue #40 remains open.
