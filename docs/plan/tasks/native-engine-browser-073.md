---
id: native-engine-browser-073
scope: glass-browser/native-engine/session-storage-context-routing
status: done
depends-on: [native-engine-browser-072]
---

# BE-02/BE-03/BE-04/BE-07: session-storage browsing-context routing

## Objective

Give each native engine instance an explicit, bounded browsing-context identity
and carry that identity through local and sandboxed JavaScript realms. Route
page `sessionStorage` changes only to other live documents representing the
same browsing context, while retaining origin filtering, source exclusion, and
the existing per-engine volatile session store.

## Contract

- `NativeEngineConfig.context_id` identifies the active native browsing
  context. It defaults to `native-context` for compatibility and accepts a
  non-empty value no longer than `MAX_BACKEND_ID_BYTES`.
- `NativeEngine::context()` and every context-bearing native backend response
  use the configured identity. Backend requests must name that identity;
  hard-coded acceptance of only `native-context` is not valid.
- The local QuickJS realm and the sandboxed content worker retain the active
  context identity. Storage changes carry a bounded `source_context_id` in the
  private Rust/worker event descriptor; that field is never exposed through
  the page `StorageEvent`.
- `localStorage` events retain same-profile, origin-filtered routing across
  live native engines regardless of context identity. `sessionStorage` events
  are accepted only when the receiving engine's configured context ID matches
  the source context ID. The source realm is excluded by the coordinator, and
  different context IDs receive neither the event nor the state mutation.
- Session state remains volatile and engine-owned. It is not serialized into
  the durable local-storage profile and is not a substitute for tab, frame,
  popup, opener, or browsing-context topology.
- The worker protocol version advances when the event descriptor gains the
  source identity, so an old helper cannot be mistaken for a compatible
  worker.

## Ownership and sequence

```text
NativeEngineConfig.context_id
  -> local realm / content-worker start handshake
  -> storage mutation descriptor.source_context_id
  -> in-process profile coordinator fan-out
  -> origin filter + session context filter
  -> receiver storage state + page StorageEvent
```

The event coordinator remains process-local. This slice makes the session
route correct for multiple engine owners in one Glass process; it does not
claim delivery across independent Glass processes.

## Deliberate boundary and tradeoffs

An explicit caller-provided identity makes the session-storage route
observable and testable without inventing a third crate or a fake global tab
registry. The default keeps existing one-context callers behavior-compatible.
The identity is a routing contract, not an ownership lease: callers that
construct two engines with the same ID are deliberately modeling documents in
one browsing context and accept shared session event delivery. Distinct IDs
model separate contexts and keep their session maps isolated.

The existing full-state profile behavior is unchanged. Physical profile I/O is
locked, but independent stale local-storage snapshots still need an explicit
ownership or merge protocol. Cross-process storage-event transport, true tab /
frame / popup topology, IndexedDB, quota policy, full Storage Web IDL
identity, and the remaining browser-context gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The final ordered gates for this slice are:

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_session_storage_routes_events_by_browsing_context -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_routes_session_storage_events_by_browsing_context -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_uses_configured_context_identity -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine storage -- --nocapture` — 10 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 365 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 723 Markdown files; 345 full-product MCP tools (100 browser-only); 17 examples; 22 public modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` — 723 Markdown documents; 83 current documents; 59 previous-version hits; 898 semantic audit hits; 0 current-claim failures

The strict affected-package command
`cargo clippy --quiet -p glass-browser --features native-engine --all-targets -- -D warnings`
was also attempted. It remains red on the same 32 pre-existing diagnostics in
untouched native backend, content-process, DOM, engine, resource-loader,
sandbox, and runtime-worker code; no new diagnostic was introduced by this
slice. The baseline is recorded honestly rather than relabeled as passing.
