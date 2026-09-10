---
id: native-engine-browser-135
scope: glass-browser/native-engine/wait-verification
status: completed
depends-on: [native-engine-browser-134]
---

# Native engine browser slice 135: wait and verification

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Carry the native discovery contract into bounded synchronization. Native
callers must be able to wait for observable page/target state and verify
postconditions without creating a Chromium session or inventing a second
result protocol.

## Contract

- expose native `wait` and `verify` through `BrowserRuntimeSession`, the CLI,
  and MCP;
- preserve the existing `WaitOutcome`, `WaitTimeout`,
  `VerificationOutcome`, and typed `ActionVerificationError` envelopes;
- support lifecycle, exact/prefix URL, text, semantic region, JavaScript
  boolean, attached/visible/hidden/enabled/stable target waits;
- support URL, title, visibility, text, revision, and bounded `all`/`any`/`not`
  verification predicates;
- poll at a bounded interval and reject zero or excessively long deadlines;
- require the existing evaluate capability before executing JavaScript wait
  conditions;
- keep the Chromium wait/verify path unchanged and never fall through from a
  native call into CDP.

## Tradeoffs

The native wait loop samples the atomic semantic observation and target
preflight surfaces already used by inspection. This keeps synchronization
revision-aware and avoids a duplicate event-observer protocol, while a native
request lifecycle ledger, popup/dialog/download topology, and action-specific
postcondition executor remain required for universal browser parity. Target
stability is deliberately a consecutive geometry sample rather than a claim
of compositor-frame stability.

## Implementation surface

- `browser/runtime.rs`: native wait/verification polling and typed failures;
- `cli/runner.rs`: native `verify` and `wait` dispatch plus evaluate gating;
- `mcp/server.rs`: native tool dispatch and predicate validation;
- `tests/native_engine.rs`: runtime success, composition, stability, script,
  and timeout coverage;
- MCP/CLI tests: native transport and command acceptance;
- `docs/cli.md` and native-engine plan/architecture records.

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
- the focused native runtime wait/verification test passed;
- the focused native MCP core-routing test passed with wait, JavaScript wait,
  and composed verification round-trips;
- the focused native CLI mapped-command validation test passed;
- formatting and the documentation/release gates passed after the docs update.

Native request accounting, popup/dialog/download topology, action-specific
`act-and-verify`, universal MCP workflow parity, TUI/profile/frame owners,
security/process gates, and native promotion remain issue #40 work.
