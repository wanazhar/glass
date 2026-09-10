---
id: native-engine-browser-150
scope: glass-browser/native-engine/browsing-context-identity
status: completed
depends-on: [native-engine-browser-149]
---

# Native engine browser slice 150: browsing-context identity

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Give every native page target the identity state required by script-created
browsing contexts. Local and HTTP(S) pages must be able to observe their
`window.name`, observe whether an opener exists, read the opener's name at
creation time, mutate their own name, and retain the renamed target for later
named-target routing.

## Contract

- `NativeEngineConfig` carries bounded `window_name` and opener context
  metadata through local and content-worker initialization;
- `window.name` is a persistent, bounded page-global property whose setter
  emits a typed host command and whose value survives same-context navigation
  and target parking;
- `window.opener` is `null` for root contexts and a bounded WindowProxy-shaped
  object for popup contexts, with the opener context identity and name exposed
  without engine pointers or CDP;
- opener-originated `postMessage` calls use the same trusted parent routing
  path as other WindowProxy messages and expose a source proxy to the parent;
- local and HTTP(S) page-load, direct-script, mutation, navigation, and
  target-reuse paths synchronize the resulting name back to the parent target
  registry;
- named-target lookup observes a changed `window.name`, so a later
  `window.open(url, renamedName)` reuses the existing browsing context rather
  than publishing a duplicate target;
- invalid control characters, oversized names, invalid context IDs, and
  malformed worker metadata fail closed before the state is published.

## Tradeoffs

The name is bounded to the native identity budget and is transferred as
validated UTF-8 text rather than exposing a live engine reference to JavaScript.
The opener proxy is capability-shaped and keeps public `PageTargetInfo`
unchanged; the parent remains the sole owner of target selection, routing, and
name reuse. Opener metadata is snapshotted when a child context is created,
which keeps worker startup deterministic while later slices can extend live
cross-context property behavior.

## Implementation surface

- `browser/native_engine/config.rs`: bounded browsing-context identity fields;
- `browser/native_engine/javascript.rs`: `window.name` setter and
  `window.opener` bootstrap projection;
- `browser/native_engine/content_process.rs`: identity transfer through the
  versioned worker start protocol and runtime-state response synchronization;
- `browser/native_engine/engine.rs`: local/worker identity ownership and
  mutation synchronization;
- `browser/native_engine/dom.rs`: host identity commands excluded from DOM
  mutation application;
- `browser/native_backend.rs`: opener metadata propagation and mutable-name
  target routing;
- `tests/native_engine.rs`: local and HTTP identity, opener messaging, and
  renamed-target reuse witnesses.

## Verification

```text
cargo fmt --all
git diff --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine identity -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture
```

The identity filter passes 4/4 tests, and the full native integration target
passes with 426 tests. Strict affected-package lint, workspace/release gates,
documentation validators, remote CI, and native default promotion remain
issue #40 closure gates.
