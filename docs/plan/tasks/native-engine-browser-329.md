# Native revisioned recovery (329)

Status: implemented locally in the native runtime.

This slice makes content-owner recovery an explicit native operation. A
revision-guarded `recover` path reloads the current URL, replaces the current
history entry, rebuilds an unhealthy content worker, and never replays the
mutation that may have caused the failure. The backend, portable native
session, and CLI all expose the operation. Native certification is no longer
advertised as `Experimental`; it is now `Partial` while the remaining Core
Web Profile and production gates continue through issue #40.

## Tradeoffs

Recovery skips outgoing unload handlers because a failed worker cannot provide
trustworthy lifecycle state and replaying an indeterminate mutation could
duplicate an external side effect. Normal reload remains available for the
full lifecycle path. Recovery remains native-owned and never changes to CDP.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --locked native_runtime_supports_form_pdf_clipboard_and_consent_surfaces -- --nocapture`
- `cargo clippy --quiet -p glass-browser --tests --locked -- -D warnings`

The focused native test verifies same-URL recovery and revision advancement.
All evidence is local; this checkout has not been pushed and has no remote CI
result.
