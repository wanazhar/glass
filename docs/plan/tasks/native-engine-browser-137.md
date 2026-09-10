---
id: native-engine-browser-137
scope: glass-browser/native-engine/semantic-storage
status: completed
depends-on: [native-engine-browser-136]
---

# Native engine browser slice 137: page-owned semantic storage

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make semantic native Web Storage use the same origin-keyed state as the page
JavaScript realm. A backend storage read or mutation must be visible to page
script immediately and page-script mutations must be returned by the semantic
storage surface.

## Contract

- route native local/session storage through the engine owner rather than an
  adapter-local map;
- preserve the existing storage profile, session-storage, event-journal, and
  content-process synchronization rules;
- support semantic read, write, and clear operations for localStorage and
  sessionStorage;
- reject cookie storage through the existing typed unsupported-operation
  contract until cookie metadata is exposed by the native backend;
- keep all existing context, lifecycle, size, and origin validation intact;
- prove both directions of visibility with a backend-to-script and clear-path
  integration test.

## Tradeoffs

The semantic operation uses the page realm for writes, so it preserves the
same mutation/event/persistence behavior as JavaScript instead of maintaining
a faster but divergent map. This adds one bounded script dispatch per write or
clear, but correctness and cross-process consistency are more important than
that micro-optimization for the browser contract.

## Implementation surface

- `browser/native_engine/engine.rs`: engine-owned semantic storage dispatch;
- `browser/native_engine/javascript.rs`: bounded origin-map accessor;
- `browser/native_backend.rs`: remove the divergent adapter map;
- `tests/native_engine.rs`: assert semantic writes are visible to page script;
- this task and the native-engine plan/architecture records.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine semantic_storage_uses_bounded_native_backend_state -- --exact
```

Completed evidence:

- the native-engine package test check passed;
- the focused semantic-storage integration test passed;
- localStorage and sessionStorage backend writes were observed through page
  JavaScript, and a backend clear removed the page-visible value;
- cookie storage remains an explicit typed unsupported operation.

The native cookie/session metadata bridge, multi-target/frame ownership,
request-ledger, popup/dialog/download witnesses, and native promotion gates
remain issue #40 work.
