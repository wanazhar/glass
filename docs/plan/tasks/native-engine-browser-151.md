---
id: native-engine-browser-151
scope: glass-browser/native-engine/window-proxy-close
status: completed
depends-on: [native-engine-browser-150]
---

# Native engine browser slice 151: live WindowProxy close

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make a script-created browsing context controllable through its returned
`WindowProxy`: calling `close()` must update the proxy-visible state and cause
the parent-owned native target to leave browser topology. Local documents and
HTTP(S) documents crossing the content worker must use the same effect path.

## Contract

- `WindowProxy.close()` emits a typed, bounded native close effect carrying the
  private popup handle and, when available, the direct target context ID;
- local runtimes and content-worker runtimes validate and transfer close effects
  without exposing an engine pointer or trusting worker-supplied source
  identity;
- the parent target owner resolves direct context IDs, source-owned private
  handles, and named targets, then closes the target through the existing
  worker/frame/storage shutdown path;
- a close effect emitted while a popup is being created is processed after the
  creation has been registered, so immediate script close cannot publish a
  stale target; nested popup/message/close effects remain bounded;
- the proxy that requested the close reports `closed === true`, the target is
  removed from `listTargets`, and stale or repeated close requests are
  idempotent no-ops;
- local fixture and HTTP(S) content-worker page-load/direct-script paths share
  the same queue, target resolution, and shutdown implementation;
- the existing versioned content-worker envelope remains protocol 6: close
  effects are an additive field in the established response contract and are
  still size- and shape-validated before publication.

## Tradeoffs

The close request uses a private handle/context identity rather than exposing
the target owner to JavaScript. This keeps target selection serialized in the
parent and makes immediate close deterministic, while preserving the public
`PageTargetInfo` shape. A proxy's local `closed` bit is authoritative for the
requesting realm; live mutation of every previously-created proxy object is a
separate identity-observation concern.

## Implementation surface

- `browser/native_engine/javascript.rs`: typed close command/effect and
  WindowProxy close bootstrap;
- `browser/native_engine/content_process.rs`: protocol-6 close-effect decode,
  worker merge, and runtime-state synchronization;
- `browser/native_engine/engine.rs`: bounded pending-close ownership across
  local and worker runtimes;
- `browser/native_engine/dom.rs` and `browser/native_engine/mod.rs`: host
  effect routing and crate-private type export;
- `browser/native_backend.rs`: target resolution, effect cascade draining, and
  target shutdown;
- `tests/native_engine.rs`: local and HTTP target-removal witnesses.

## Verification

```text
cargo fmt --all
git diff --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine window_proxy_close -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture
```

The scoped check passes. The focused local and HTTP witnesses pass 2/2, and
the full native integration target passes with 428 tests. Strict affected-
package lint, workspace/release gates, documentation validators, remote CI,
and native default promotion remain issue #40 closure gates.
