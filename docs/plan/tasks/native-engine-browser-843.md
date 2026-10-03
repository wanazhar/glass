---
id: native-engine-browser-843
scope: glass-browser/native-engine/content-process-cookie-authority
status: in-progress
depends-on: [native-engine-browser-842]
---

# Glass native-engine browser slice 843: parent-owned content-process cookies

## Objective

Remove the content process's authoritative cookie mirror and cookie-profile
persistence. Route cookie-bearing page/worker network operations through the
browser parent, which owns matching, response-cookie acceptance, and durable
profile updates, while preserving the script-visible `document.cookie`
projection and existing native Fetch behavior.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-842.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Fetch Standard credentials mode](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [HTML Standard `document.cookie`](https://html.spec.whatwg.org/multipage/dom.html#dom-document-cookie)

## Contract

- The browser parent is the sole authority for complete per-context cookie
  state, including request matching, `Set-Cookie` acceptance/rejection,
  expiration/deletion, profile persistence, and shared-profile journal
  publication. A content process or worker realm is not a cookie authority or
  durable writer.
- Do not send a complete cookie profile, HttpOnly value, raw `Cookie` or
  `Set-Cookie` header, or cookie-bearing profile path into the content process.
  Remove content-process cookie-profile load/save and persistence paths.
- Give each document only a URL-scoped `document.cookie` projection of
  script-visible non-HttpOnly cookies. A setter returns a bounded typed write
  carrying exact context, frame, document generation, and source URL; the
  parent validates and applies it, then refreshes the projection. Synchronous
  same-turn preview does not confer authority on the child.
- Broker cookie-bearing page and worker network requests through the parent
  and its existing network policy. Preserve `include`, `same-origin`, and
  `omit` at every redirect hop, CORS/referrer policy, response streaming,
  cancellation, and response-cookie update/deletion. Broker errors are
  explicit; no direct child-loader fallback is allowed.
- Commit response-cookie mutations once in the parent before later requests
  can observe them. `Set-Cookie` remains unreadable to script. Cookie import,
  clear, restart, same-profile live-session synchronization, and separate
  profile isolation continue to work through the parent authority.
- Validate every child request/write against its captured owner identity.
  Stale owners cannot redirect to the selected target. IPC and queues remain
  bounded; overflow, process exit, cancellation, and protocol errors surface
  typed failures.
- Preserve the currently supported native cookie attributes and matching
  behavior. Do not claim full Cookie/Web IDL or WPT conformance beyond selected
  passing cases.

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
- `docs/plan/tasks/native-engine-browser-843.md`
- `CHANGELOG.md`

## Verification

- Process-protocol tests prove no full profile, HttpOnly value, raw cookie
  header, or cookie-bearing profile path crosses into the child; setter writes
  are bounded and owner-tagged.
- Process-backed tests verify the `document.cookie` projection and setter,
  HttpOnly invisibility, worker/page credentials modes, redirect hops,
  response-cookie update/deletion, import/clear, restart, same-profile
  synchronization, and separate-profile isolation.
- Verify a child shutdown/restart cannot mutate or lose the parent jar and
  that parent-broker failure cannot trigger a direct child request.
- Run selected Fetch/Cookie/SharedWorker WPT cases and record exact selections
  and deviations; mocked-loader checks do not replace network regressions.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
- Run focused process-backed cookie tests after the check; reserve workspace
  and all-feature validation for the final issue #40 gate.
- `cargo fmt --all -- --check`
- `git diff --check`
- Run relevant documentation inventory/coverage checks.

## Current Evidence

- `cargo check -p glass-browser --features native-engine --lib --test native_engine --locked --quiet` passed with existing dead-code warnings only.
- `NativeEngine::cookies_async` now reads the parent loader after synchronizing
  external profile events, overlaid with queued child cookie changes that the
  parent has already collected. The content-process cookie-list IPC command
  and its full-profile response decoder are removed. The process-backed HTTP
  regression passes against the parent import/clear path (1 passed;
  58.03-second test runtime). Initial runs exposed and fixed the missing parent
  overlay for response `Set-Cookie` and stale queued changes after clear.
  The parent process now applies child-reported cookie mutations to the
  durable profile, and child content snapshots no longer apply their cookie
  changes there. The child still receives the shared path and full profile and
  can rewrite the combined snapshot, so child write capability is not removed
  yet. Child profile loading/mirroring, mutation authority, and network
  brokering remain outstanding. The restart persistence regression passed
  (1 passed; 93.27-second runtime); its test name now describes the observed
  process-restart behavior rather than attributing durable writes to the child.
  Cookie import and clear now commit through the parent profile writer; the
  content process only refreshes its mirror and discards local change records.
  Clear reconciles pending child changes before emitting parent deletion
  records, so the next API read cannot restore cookies from the old queue.
- `rustfmt --edition 2024 --check` passes for the modified Rust modules. The
  workspace `cargo fmt --all -- --check` still reports formatting in unchanged
  `native_engine/mod.rs:81`. Documentation coverage still reports only the
  pre-existing missing MCP schema measurement in `docs/mcp-schema-budget.md`.
- Slice 842 is complete: browser-coordinated SharedWorker creation derives
  cookies from the exact parent source owner. Its parent snapshot/override/
  deletion process regression passed (1 passed; 48.31-second test runtime),
  and the independent static review found no lock/order defect.
- The broader content-process mirror, child profile-path/read/write capability,
  and general parent request broker remain outstanding. An adjacent
  content-process credentials/redirect regression timed out twice at
  `tests/native_engine.rs:33229`; Slice 843 must diagnose that failure while
  implementing the parent broker.
- The Slice 842 HTTP regression successfully bound a local listener. The
  earlier blanket claim that local TCP binding is denied is stale for this
  checkout; the separate credentials/redirect test remains unverified and
  must not be inferred from Slice 842.
- Implementation is in progress on `task/native-engine-browser-843`.
- Issue #40 remains open.
