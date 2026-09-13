# Glass native engine browser slice 319: native snapshot ownership

Status: completed locally; issue #40 task/workflow parity and persistent
session ownership remain open.

## Objective

Make the native runtime own the browser-backed session snapshot path in both
the CLI and MCP surfaces. Snapshot creation must consume the same revisioned
semantic observation as native inspection and must not start a Chromium/CDP
session.

## Contract

- `glass --browser-runtime native snapshot create` captures a native semantic
  observation, redacts it through `SessionSnapshot::from_observation`, and
  stores it in the selected profile's snapshot store.
- Native MCP `sessionSnapshot` with `operation: "create"` uses the same native
  observation and redaction path.
- Native MCP startup uses the selected profile's Rust-owned storage path unless
  `--incognito` is set; the existing viewport configuration is retained.
- Native snapshot output remains revisioned and route-bound. Existing list,
  inspect, diff, and purge operations stay browser-free and unchanged.
- No native snapshot path invokes `BrowserSession`, Chrome, CDP, or a silent
  alternate backend.

## Implementation

`BrowserRuntimeSession::native_observe` exposes the complete semantic
observation already used internally by native inspection. CLI alternative
runtime dispatch and MCP native tool dispatch use it to create and persist
snapshots. The MCP native session initializer now shares the CLI profile path
contract, including the volatile incognito case.

## Tradeoffs and follow-up

Snapshots intentionally redact URLs, text, and sensitive labels according to
the existing snapshot contract; they are not a raw DOM dump. The native
snapshot path now has parity with the Chromium path, but native Task Protocol
execution, workflow resume, and long-lived session IPC still need their own
native owners before the complete Glass workflow can replace CDP.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo check --quiet -p glass-browser --no-default-features --tests --locked`
- native CLI/MCP routing tests and existing native integration coverage
- default-native strict Clippy
- documentation coverage/depth and `git diff --check`

The affected local gates pass. Remote CI and issue #40 completion remain
pending.
