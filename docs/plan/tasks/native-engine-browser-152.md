---
id: native-engine-browser-152
scope: glass-browser/native-engine/window-proxy-location
status: completed
depends-on: [native-engine-browser-151]
---

# Native engine browser slice 152: WindowProxy location control

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make a script-created browsing context navigable through its returned
`WindowProxy`. Local documents and HTTP(S) documents crossing the content
worker must expose useful location reads and route location mutations through
the parent-owned native target/navigation path.

## Contract

- every WindowProxy exposes a bounded URL snapshot through `location.href`,
  `toString()`, and the standard URL component getters;
- `location.href = value`, `assign(value)`, `replace(value)`, and `reload()`
  emit typed, bounded navigation effects with explicit history mode;
- relative URLs resolve against the target location snapshot, while the parent
  validates the final URL and owns target selection, navigation, history,
  frame, storage, and worker state;
- local runtimes and protocol-6 content-worker runtimes validate and transfer
  navigation effects without exposing an engine pointer or trusting worker
  source identity;
- the parent target registry resolves direct context IDs, source-owned private
  handles, and names, including effects emitted while a popup is being created;
- popup child initialization transfers a validated opener URL snapshot so
  `opener.location.href` is deterministic in local and HTTP(S) child realms;
- stale, closed, malformed, oversized, or unresolvable requests fail closed
  within the existing bounded effect budget, and nested effects remain
  parent-owned and bounded.

## Tradeoffs

The proxy location is a deterministic snapshot rather than a live object shared
with every realm. That keeps cross-context access capability-shaped and avoids
exposing native engine state to page JavaScript, while a later identity slice
can add live observation updates. Navigation reuses the existing target owner,
so it preserves established URL policy, frame reset, storage, and history
behavior instead of introducing a second navigation state machine. The
protocol remains additive at version 6, with bounded JSON-shaped effect data.

## Implementation surface

- `browser/native_engine/config.rs`: validated opener URL metadata;
- `browser/native_engine/javascript.rs`: location projection, URL resolution,
  navigation commands, and opener URL bootstrap data;
- `browser/native_engine/content_process.rs`: protocol-6 navigation-effect
  decode, validation, worker transfer, and runtime synchronization;
- `browser/native_engine/engine.rs`: local/worker navigation-effect ownership,
  bounded queues, and replace-history navigation entry point;
- `browser/native_engine/dom.rs` and `browser/native_engine/mod.rs`: host
  effect routing and crate-private type export;
- `browser/native_backend.rs`: target resolution, opener propagation, and
  parent-owned navigation/effect cascade processing;
- `tests/native_engine.rs`: local parked-target and HTTP content-worker
  location navigation witnesses.

## Verification

```text
cargo fmt --all
git diff --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine window_proxy_location -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture
```

The focused location witnesses pass 2/2, and the full native integration
target passes with 430 tests. Documentation, formatting, and release-claim
validation are recorded with this checkpoint. Strict affected-package lint,
workspace/release gates, remote CI, and native default promotion remain issue
#40 closure gates.
