---
id: native-engine-browser-136
scope: glass-browser/native-engine/act-and-verify
status: completed
depends-on: [native-engine-browser-135]
---

# Native engine browser slice 136: guarded act-and-verify

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Close the native gap between candidate discovery and verified mutation. A
caller-selected semantic candidate must resolve and dispatch atomically in the
native session, then expose the shared Glass action/execution envelope and
optional postcondition result through CLI and MCP.

## Contract

- validate the execution request and resolve it against one current native
  semantic observation;
- keep candidate resolution and mutation under one native session lock so a
  concurrent revision cannot be acted on accidentally;
- map the supported semantic intent actions to native click, type, clear,
  check, uncheck, and select operations;
- return the shared `SemanticIntentExecutionResult`, `ActionOutcome`, and
  `ActAndVerifyResult` contracts, including execution IDs and revision deltas;
- return standard not-executed/retry guidance when the candidate or resolution
  policy is not eligible, and preserve the dispatched-unverified versus
  indeterminate distinction;
- release the mutation lock before polling the optional verification predicate;
- expose the operation through native CLI and MCP without creating Chromium or
  falling through to CDP.

## Tradeoffs

The native action envelope generates a session-local execution ID and derives
bounded before/after URL, title, route, and revision evidence from native
snapshots. This preserves the agent-facing contract without duplicating
Chromium's topology registry. Popup/dialog/download witnesses and request
lifecycle evidence still require their own native ledgers; a generic
postcondition cannot be presented as those specialized witnesses.

## Implementation surface

- `browser/runtime.rs`: locked native resolution/dispatch, action envelope,
  execution IDs, and post-dispatch verification;
- `browser/session/intent.rs` and `browser/session/mod.rs`: shared resolution
  ID visibility for the portable runtime;
- `cli/runner.rs`: native `act-and-verify` routing;
- `mcp/server.rs`: native `actAndVerify` routing;
- `tests/native_engine.rs`: successful guarded action and verification;
- MCP/CLI tests: transport and command acceptance;
- native-engine plan, architecture, analysis, and CLI documentation.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
RUST_MIN_STACK=16777216 cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_exposes_bounded_wait_and_verification -- --nocapture
RUST_MIN_STACK=16777216 cargo test --quiet -p glass-browser --features native-engine --lib native_mcp_routes_core_browser_tools_without_chromium -- --nocapture
RUST_MIN_STACK=16777216 cargo test --quiet -p glass-browser --features native-engine --lib native_runtime_accepts_mapped_inspection_commands_only_for_native -- --nocapture
python3 scripts/check-documentation-depth.py
python3 scripts/check-documentation-coverage.py
python3 scripts/check-release-documentation.py --previous-version 0.3.13
git diff --check
```

Completed evidence:

- the package-scoped native test check passed;
- the focused native runtime test passed with a guarded click and verified
  postcondition;
- the focused native MCP core-routing test passed with `actAndVerify`;
- the focused native CLI mapped-command validation test passed;
- formatting and the documentation/release gates passed after the docs update.

Native request accounting, specialized popup/dialog/download witnesses,
universal MCP workflow parity, TUI/profile/frame ownership, security/process
gates, and native promotion remain issue #40 work.
