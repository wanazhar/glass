---
id: native-engine-browser-850
scope: glass-browser/native-engine/autonomous-service-worker-stream-upload-parent-authority
status: complete
depends-on: [native-engine-browser-849]
---

# Glass native-engine browser slice 850: autonomous ServiceWorker stream uploads

## Objective

Verify that a due ServiceWorker timer can issue a Fetch with a `ReadableStream`
request body while the page is otherwise idle, using the exact page owner's
parent broker and without transferring cookie authority or falling back to
child HTTP(S) transport.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-843.md` — parent-only cookie authority
  and remaining request coverage.
- `docs/plan/tasks/native-engine-browser-846.md` — parent-dispatched timer
  owner turns.
- `docs/plan/tasks/native-engine-browser-849.md` — DedicatedWorker timer
  upload boundary and buffering tradeoff.
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A due ServiceWorker timer may upload a bounded `ReadableStream` body to an
  HTTP(S) target while its page owner is otherwise idle. The event and broker
  remain bound to the captured context, frame, generation, and document URL.
- Upload pull callbacks cross the existing demand-driven bounded IPC path and
  are collected under current byte/chunk limits before parent network
  dispatch. The request preserves method, content type, credentials mode, and
  body bytes. This does not claim socket-level streaming or network
  backpressure.
- The parent alone matches request cookies, accepts `Set-Cookie`, and persists
  the jar. The regression checks an HttpOnly seed, an owner-tagged visible page
  cookie write made before ServiceWorker registration, HttpOnly Worker script
  cookies, response-cookie rotation before upload, and the upload response
  cookie on a later Fetch. HttpOnly values remain absent from
  `document.cookie`.
- Existing owner validation and the child HTTP(S) fail-closed guard remain
  unchanged; this slice adds no child-side retry or fallback. Stale-owner and
  cancellation-specific cases are not newly certified here.
- A real loopback process-backed test drives the async-effect notification and
  verifies request order, method, content type, body, and parent-cookie
  sequencing without a browser-session operation to trigger the timer.
- This slice does not close the broader network-authority audit or certify
  WPT, remote CI, cross-platform behavior, or general ServiceWorker lifetime
  semantics.

## Path

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-843.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-850.md`
- `docs/plan/reviews/native-engine-browser-850-01.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests --locked --quiet`
  passed with existing unused/dead-code warnings.
- `cargo build -p glass-browser --features native-engine --bin glass-native-content-worker --locked --quiet`
  passed.
- Focused timer-cookie tests passed: 2 passed, 1,692 filtered, 25.35 seconds.
  The first run exposed test setup ordering: a zero-delay ServiceWorker timer
  was registered before the page cookie setter, so it could fetch before that
  later write existed. Moving registration after the write makes the intended
  parent-journal ordering explicit; no production cookie code or owner check
  changed.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Documentation gates passed: 1,483 Markdown files with zero current-claim
  failures; 93 current guides and 19 contracts; 15 shortcut keys and 63
  markers; 346 MCP tools (101 browser-only), 17 examples, and 22 public
  modules.
- Direct self-review confirms the existing exact-owner validation and
  parent-only matching, `Set-Cookie` acceptance, and persistence remain
  unchanged. No independent agent review, remote CI, or cross-platform result
  is claimed.
