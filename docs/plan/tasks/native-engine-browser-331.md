# Native content-worker liveness refresh (331)

Status: implemented locally in the native runtime.

This slice closes a recovery reliability gap in the native content boundary.
The content owner now refreshes its cached health state with a non-blocking
`try_wait` before recovery, navigation, script, action, lifecycle, storage,
and close decisions. An externally exited child is classified as a typed
`Exited` worker failure before Glass writes another IPC request, allowing the
explicit native recovery operation to rebuild it. Transport errors from the
health probe are classified as `Transport` and fail closed.

## Tradeoffs

- The health probe is intentionally local and non-blocking; it does not add a
  ping round-trip or increase normal operation latency with IPC.
- Recovery still requires an explicit caller operation and does not replay an
  indeterminate mutation. Liveness detection does not make a failed mutation
  safe to retry.
- A process that remains alive but is wedged is still detected by the existing
  bounded exchange timeouts; the probe complements, rather than replaces,
  those timeouts.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --lib refresh_health_detects_an_exited_content_worker -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --locked native_runtime_supports_form_pdf_clipboard_and_consent_surfaces -- --exact --nocapture` (1 passed)
- `cargo clippy --quiet -p glass-browser --tests --locked -- -D warnings`

All evidence is local; this checkout has not been pushed and has no remote CI
result.
