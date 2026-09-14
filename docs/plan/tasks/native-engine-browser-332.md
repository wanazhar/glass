# Native visual encoders and MCP parity (332)

Status: implemented locally in the native runtime.

This slice completes still-image encoding for the native visual capture
contract. Native CLI, `BrowserSession`, and MCP screenshot calls now share the
same bounded PNG, JPEG, and WebP path, including quality handling, viewport
scale, clipping, full-page, semantic element capture, and metadata. The MCP
route no longer rejects valid visual options or drops the metadata-bearing
capture contract.

PNG remains lossless. WebP is lossless when quality is omitted and uses the
bounded native lossy encoder when quality is supplied. JPEG quality is passed
to the native encoder; alpha is flattened according to the existing software
surface contract. Encoded payloads remain subject to the existing 4 MP and
8 MiB limits. Screenshot-containing semantic evidence remains a separate
schema because `EvidenceResult` carries structured page evidence rather than
image bytes; callers use the explicit capture operation for images.

## Tradeoffs

- Pure-Rust JPEG/WebP encoders keep the native backend independent of
  Chromium, platform codec availability, and external processes, at the cost
  of dependency graph and compile weight.
- The software renderer's deterministic geometry and bounded surface remain
  authoritative; this slice does not claim browser-GPU color management or
  pixel parity.
- Native image encoding is now complete for the declared Glass capture
  contract, but Core Web Profile conformance and cross-platform production
  certification remain issue #40 gates.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests`
- `cargo test --quiet -p glass-browser --locked native_backend_captures_all_visual_formats_without_mutating_revision -- --exact --nocapture`
- `cargo clippy --quiet -p glass-browser --tests --locked -- -D warnings`

The native MCP integration test also covers PNG metadata and JPEG output
through the live native MCP dispatcher. All evidence is local; this checkout
has not been pushed and has no remote CI result.
