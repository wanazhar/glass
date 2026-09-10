---
id: native-engine-browser-149
scope: glass-browser/native-engine/cross-context-post-message
status: completed
depends-on: [native-engine-browser-148]
---

# Native engine browser slice 149: cross-context messaging

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Give script-created native browsing contexts a usable, parent-owned
`postMessage` channel. Messages must cross local and HTTP(S) page realms and
the content-worker boundary without exposing engine pointers or CDP.

## Contract

- `window.open()` returns a bounded WindowProxy-shaped handle whose stable
  private handle can address the newly created or named target;
- `WindowProxy.postMessage(data, targetOrigin)` queues a typed host effect,
  and the global `postMessage()` form targets the current context's opener;
- message data uses a bounded JSON structured-clone projection and is rejected
  when it exceeds the native effect limit;
- a delivered `message` event exposes cloned `data`, serialized `origin`, a
  bounded source WindowProxy, and an empty `ports` list;
- the parent target registry resolves direct context IDs, private handles, and
  named targets, including replies sent through `event.source`;
- `*`, same-origin `/`, and valid absolute HTTP(S) target origins follow
  explicit matching rules; blocked, missing, closed, malformed, or stale
  destinations are dropped or fail closed without mutating the sender;
- messages emitted during local page load, direct script execution, DOM events,
  lifecycle dispatch, fetch continuation, content-worker page load/script
  execution, and nested target creation use the same bounded queue;
- the versioned content-worker protocol transfers message effects as protocol
  version 6, while source context and origin are re-established by the trusted
  parent owner;
- delivery may trigger bounded nested browser effects, preserving target
  selection and target-local state.

## Tradeoffs

The message payload is cloned through bounded JSON rather than attempting to
model transferables, `MessagePort`, `ArrayBuffer`, prototypes, or arbitrary
cross-origin Window property access. This is deterministic and keeps the
content worker isolated, but it is deliberately narrower than the full Web IDL
surface. Private target handles remain outside the public target schema so
callers keep stable `PageTargetInfo` IDs while the parent controls routing.

## Implementation surface

- `browser/native_engine/javascript.rs`: WindowProxy handles, message event
  bootstrap, bounded payload validation, and typed runtime effects;
- `browser/native_engine/content_process.rs`: protocol-6 message transfer and
  worker runtime-state synchronization;
- `browser/native_engine/engine.rs`: trusted source metadata, bounded queues,
  and message-event dispatch;
- `browser/native_backend.rs`: target/handle resolution, target-origin
  matching, delivery, and nested-effect draining;
- `browser/native_engine/dom.rs` and `browser/native_engine/mod.rs`: host
  effect routing and crate-private type exports;
- `tests/native_engine.rs`: local origin-filter/reply coverage and HTTP
  content-worker round-trip coverage.

## Verification

```text
cargo fmt --all
git diff --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine post_message -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture
```

The focused local and HTTP witnesses pass, and the full native integration
target passes with 424 tests. Strict affected-package lint, workspace/release
gates, remote CI, and native default promotion remain issue #40 closure gates.
Direct cross-context property scripting, `window.opener`/mutable
`window.name`, popup permission and geometry, complete frame
lifecycle/scripting, and wider browser Web IDL parity remain subsequent slices.
