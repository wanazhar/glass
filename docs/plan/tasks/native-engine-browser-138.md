---
id: native-engine-browser-138
scope: glass-browser/native-engine/cookies
status: completed
depends-on: [native-engine-browser-137]
---

# Native engine browser slice 138: cookie profile surfaces

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Expose the native HTTP cookie jar through the normal Glass runtime, CLI, and
MCP surfaces without flattening cookie metadata or creating a Chromium
session.

## Contract

- inspect cookies matching the active HTTP(S) document, including HTTP-only
  entries, with domain/path/security/expiry information preserved;
- import and clear cookies through the native parent and content-process
  owners;
- keep cookie mutations profile-backed and visible to the active
  `document.cookie` realm and subsequent native requests;
- keep session cookies session-scoped and retain the existing profile lock and
  bounded entry limits;
- route native CLI `cookies`, `export-cookies`, and `import-cookies`, plus MCP
  `cookies`, `setCookies`, and `clearCookies`, without CDP fallback;
- reject the map-shaped semantic cookie write that lacks required metadata,
  while allowing the metadata-preserving high-level import surface.

## Tradeoffs

The worker protocol adds explicit cookie commands rather than using
`document.cookie` for import, because that API cannot represent HTTP-only
cookies and would silently discard domain/path semantics. The public Glass
cookie type still does not carry every browser-specific attribute (for
example SameSite priority), so those fields are preserved only where the
current public contract supports them and are not fabricated.

## Implementation surface

- `browser/native_engine/resource_loader.rs`: scoped cookie projection,
  profile import, replacement, and clear operations;
- `browser/native_engine/content_process.rs`: bounded cookie IPC and worker
  ownership;
- `browser/native_engine/engine.rs`: public native cookie lifecycle and
  public/private cookie conversion;
- `browser/native_backend.rs`, `browser/runtime.rs`: backend and session
  seams;
- `cli/runner.rs`, `mcp/server.rs`: native command/tool routing;
- `tests/native_engine.rs` and native MCP coverage.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine semantic_storage_uses_page_owned_native_state -- --exact
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_synchronizes_document_cookie_with_http_session -- --exact
cargo test --quiet -p glass-browser --features native-engine --lib native_mcp_routes_core_browser_tools_without_chromium -- --exact
```

Completed evidence:

- the native-engine package test check passed;
- semantic Web Storage and content-process cookie integration tests passed;
- HTTP-only cookie metadata was returned, metadata-preserving import reached
  the active worker realm, and clear removed the active cookie profile;
- native MCP cookie inspection and clear routing passed without Chromium.

Multi-target/frame ownership, request accounting, dialogs, downloads,
universal workflow parity, and native production promotion remain issue #40
work.
