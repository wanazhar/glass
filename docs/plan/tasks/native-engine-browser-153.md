---
id: native-engine-browser-153
scope: glass-browser/native-engine/window-proxy-identity
status: completed
depends-on: [native-engine-browser-152]
---

# Native engine browser slice 153: live WindowProxy identity refresh

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Keep cached WindowProxy objects useful after another browser target navigates,
renames, or closes. Local and HTTP(S) realms crossing the content worker must
observe the current bounded target state without replacing the proxy object or
exposing a native engine pointer.

## Contract

- cached proxies retain object identity while parent-owned refreshes update
  their target URL, name, context identity, and `closed` state before the next
  observable script or event operation;
- refreshes cover direct context proxies, opener/source proxies, and private
  source-owned popup handles, including handles that were created before the
  target was registered;
- local runtimes and content-worker runtimes use one validated, bounded refresh
  message and apply it after host bootstrap so the same operation observes the
  new state;
- target closure records a bounded tombstone with its last URL/name and keeps
  the private handle mapping from resolving to an unrelated named target;
- navigation, renaming, and closure refreshes preserve the existing parent
  target/frame/history/storage/worker owners and selected-target semantics;
- malformed, oversized, missing, or stale refresh records fail closed within
  the established effect/update budgets.

## Tradeoffs

The refresh is just-in-time before observable work, not an asynchronous push
into every parked realm. This keeps the parent as the sole topology owner and
avoids a second cross-realm state machine, while still making reads current at
the point JavaScript can observe them. Closed tombstones are bounded; once an
old tombstone is evicted, its already-refreshed proxy remains closed locally,
but a never-observed stale reference is intentionally no longer recoverable.

## Implementation surface

- `browser/native_engine/javascript.rs`: stable proxy state, refresh queue,
  and post-bootstrap application;
- `browser/native_engine/content_process.rs`: validated WindowProxy refresh
  IPC and content-worker runtime forwarding;
- `browser/native_engine/engine.rs`: local/worker synchronization owner;
- `browser/native_engine/mod.rs`: crate-private refresh type export;
- `browser/native_backend.rs`: target snapshots, bounded closed tombstones,
  stale-handle routing, and operation-boundary synchronization;
- `tests/native_engine.rs`: local and HTTP navigation/closure identity
  witnesses.

## Verification

```text
cargo fmt --all
git diff --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine window_proxy_identity -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture
```

The identity witnesses pass 2/2, and the full native integration target passes
with 432 tests. Documentation and formatting/release-claim validation are
recorded with this checkpoint. Strict affected-package lint, workspace/release
gates, remote CI, and native default promotion remain issue #40 closure gates.
