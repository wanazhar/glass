# Native capture contract (328)

Status: implemented locally in the native runtime.

This slice closes the transport-neutral capture seam for the primary native
backend. Automatic backend selection now treats native as an ordinary
candidate instead of applying an explicit-only filter. Native backend capture
returns the existing logical PNG surface and the bounded native PDF renderer
through `CaptureFormat`, preserving raw bytes and typed unsupported behavior
for formats that still do not have an encoder. `BrowserRuntimeSession::capture`
now exposes that shared contract, and the native integration surface verifies
both PNG and PDF signatures.

## Tradeoffs

PDF is intentionally the existing bounded text/layout representation rather
than a claim of print-engine parity. JPEG remains an explicit typed denial
until a native encoder is added; the API never labels PNG bytes as another
format. Screenshot-containing evidence remains separate because the stable
`EvidenceResult` schema has no image payload.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --locked native_runtime_supports_form_pdf_clipboard_and_consent_surfaces -- --nocapture`
- `cargo clippy --quiet -p glass-browser --tests --locked -- -D warnings`

All evidence is local; this checkout has not been pushed and has no remote CI
result.
