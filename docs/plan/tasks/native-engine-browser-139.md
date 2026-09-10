---
id: native-engine-browser-139
scope: glass-browser/native-engine/history-topology
status: completed
depends-on: [native-engine-browser-138]
---

# Native engine browser slice 139: async history and topology projection

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make native navigation history and the normal target/frame discovery surface
usable through the production-facing runtime, CLI, and MCP seams.

## Contract

- native history traversal uses the asynchronous native navigation owner for
  local and external HTTP(S) entries;
- back/forward boundaries are explicit errors at the backend-facing control
  seam and never become a fresh navigation or a CDP fallback;
- the native runtime projects exactly one active page target and one main
  frame using stable identities (`native-context` and
  `native-context:main`);
- target/frame listing is redacted and bounded like the Chromium topology
  contract;
- selecting the listed native target or main frame is explicit and idempotent;
  unknown identities fail closed;
- CLI target archive/list/select and frame list/select routes use native
  ownership without starting Chromium;
- MCP target/frame list/select routes use native ownership without starting
  Chromium;
- multi-target creation/closure, child-frame execution, popup witnesses,
  downloads, and dialogs remain separate issue #40 slices rather than being
  represented as fake native topology.

## Tradeoffs

The native engine currently owns one browsing context, so the public topology
projection is intentionally exact rather than pretending to offer tabs or
frames it cannot execute. This makes agent routing deterministic and keeps
unknown target/frame selection fail-closed. Async history reuses the existing
content-process and runtime-worker owners; it does not duplicate document
loading or add a second history store. A history traversal that reaches a
network document therefore pays the content-process load cost, while local
fixture/data documents retain deterministic loading.

## Implementation surface

- `browser/native_engine/engine.rs`: asynchronous history traversal and
  history-entry commit through the runtime worker/content process;
- `browser/native_backend.rs`, `browser/runtime.rs`: native history and
  topology seams;
- `browser/session/mod.rs`: public navigation-control result export;
- `cli/runner.rs`: native target archive/list/select and frame list/select
  routing;
- `mcp/server.rs`: native topology routing and regression coverage;
- `tests/native_engine.rs`: runtime-owner history and explicit topology
  identity tests.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_exposes_agent_inspection_and_target_discovery -- --exact
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_exposes_revisioned_history_controls -- --exact
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_async_history_traversal_uses_the_runtime_owner -- --exact
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_async_http_history_traversal_reloads_through_content_process -- --exact
cargo test --quiet -p glass-browser --features native-engine --lib mcp::server::tests::native_mcp_routes_core_browser_tools_without_chromium -- --exact
```

Completed evidence:

- the feature-gated `glass-browser` test target check passed;
- native runtime topology, revisioned history, local async history, and
  HTTP/content-process async history tests passed;
- target and frame identities are explicit, bounded, and selectable;
- native MCP topology routing is covered in the no-Chromium core route test.

Multi-target/frame ownership, popup/dialog/download witnesses, complete
network request accounting, universal workflow parity, and production
promotion remain issue #40 work.
