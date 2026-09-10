---
id: native-engine-browser-134
scope: glass-browser/native-engine/agent-inspection
status: completed
depends-on: [native-engine-browser-133]
---

# Native engine browser slice 134: agent inspection and target discovery

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Expose native page inspection and intent candidate discovery through the
existing Glass agent-facing result contracts. Agents using native runtime
must be able to observe a revision, discover candidates, and then preflight or
act without switching to a Chromium session.

## Contract

- build one semantic observation from an atomic native page, node, and layout
  snapshot;
- return the normal `InspectPageResult` envelope with page metadata, a main
  region, semantic targets, revision, bounded text, and viewport limits;
- run the existing pure Glass intent resolver over native semantic targets for
  `findTarget`, preserving normalized intent, confidence, constraints,
  ambiguity, revision, and candidate fingerprints;
- expose both contracts through `BrowserRuntimeSession`, native CLI, and
  native MCP;
- keep the operation read-only and revision-consistent;
- preserve Chromium session behavior and prevent native paths from falling
  through to CDP.

## Tradeoffs

The first native semantic observation has one bounded main region and the
engine's supported interactive nodes. This reuses the mature Glass resolver
and avoids a second intent-ranking implementation, while landmark-specific
regions, rich accessibility trees, structured extraction, and knowledge-backed
historical resolution remain later profile work. The result is useful for the
normal discovery loop without overstating the native engine's browser parity.

## Implementation surface

- `browser/native_engine/engine.rs`: atomic page/node/layout inspection;
- `browser/native_backend.rs`: backend inspection boundary;
- `browser/runtime.rs`: semantic observation, `inspectPage`, and `findTarget`;
- `cli/runner.rs`: native command routing;
- `mcp/server.rs`: native tool routing;
- `tests/native_engine.rs` and MCP/CLI unit tests: public contract coverage.

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --tests` passed;
- `native_runtime_session_exposes_agent_inspection_and_target_discovery` passed;
- native MCP core routing passed with `inspectPage` and `findTarget` round-trips;
- native CLI mapped-command validation passed;
- `cargo fmt --all -- --check` and `git diff --check` passed.

Universal MCP workflow parity, TUI/profile/frame/download/prompt/recovery,
full web-platform behavior, security/process gates, conformance, and native
promotion remain issue #40 work.
