# Native engine browser slice 133: target preflight

Status: completed

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make native target resolution observable through the same normal preflight
surfaces used before browser actions. A native caller must be able to resolve
one semantic locator, inspect its current node identity and viewport geometry,
and receive an explicit actionability result without changing document state.

## Contract

- resolve `ref=`, `id=`, `role=`, `name=`, `text=`, and `css=` using the native
  document resolver;
- bind the result to the current native revision and semantic node reference;
- report visible viewport geometry when the target has a rendered box;
- distinguish not-found, ambiguous, stale, hidden, disabled, read-only,
  unsupported-action, and outside-viewport outcomes;
- expose the result through `BrowserRuntimeSession`, the native CLI
  `preflight` command, and the native MCP `preflight` tool;
- never scroll, focus, dispatch events, or mutate storage while preflighting;
- preserve the existing Chromium preflight path unchanged.

## Tradeoffs

This slice uses the native engine's semantic projection and deterministic
layout directly. It keeps preflight cheap and side-effect-free, but it does
not pretend that a semantic result proves full browser hit-testing, stacking,
or asynchronous page stability. Those remain separate engine contracts. A
native action that is not implemented must be reported as unsupported rather
than advertised as actionable.

## Implementation surface

- `browser/native_engine/engine.rs`: revision-bound native preflight result and
  actionability calculation;
- `browser/native_backend.rs`: backend-owned preflight lock boundary;
- `browser/runtime.rs`: serialized native session API;
- `cli/runner.rs`: native `preflight` command routing;
- `mcp/server.rs`: native `preflight` tool routing;
- tests: successful, hidden/disabled, stale, and ambiguous locator coverage.

## Verification

Run one batched validation after implementation:

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --lib
cargo check --quiet -p glass-browser --lib
RUST_MIN_STACK=16777216 cargo test --quiet -p glass-browser --features native-engine --lib native_engine::engine::tests::preflight
python3 scripts/check-documentation-depth.py
python3 scripts/check-documentation-coverage.py
git diff --check
```

The strict all-target Clippy command remains a separate baseline gate until
the pre-existing native-engine lint backlog is addressed in a dedicated
batch.

Completed evidence:

- `cargo check --quiet -p glass-browser --features native-engine --tests` passed;
- the three focused native engine preflight tests passed;
- the focused native MCP routing test passed with a preflight round-trip;
- the focused native CLI command-validation test passed with the preflight
  command;
- `cargo fmt --all` and `git diff --check` passed.
