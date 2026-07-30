# Local platform evidence

Status: verified locally on 2026-07-30.

This record covers the current development machine only. It is not a support
claim for Linux x86-64, macOS, Windows, or any other environment, and it does
not certify a future published crate without repeating the checks.

## Host

- OS: Linux
- Architecture: ARM64 (`aarch64`)
- Rust host: `aarch64-unknown-linux-gnu`
- Rust: `1.97.0`
- Browser: Chromium `150.0.7871.128` from `/snap/bin/chromium`
- Native extension sandbox: Linux Bubblewrap (`/usr/bin/bwrap`)

## Checks passed

The following checks passed on this host with the checked-out `0.2.1` source:

- `cargo test --all-targets --locked` — 360 passed, 1 ignored;
- `GLASS_E2E=1 GLASS_DISABLE_CHROME_SANDBOX=1 cargo test --test browser_smoke --locked -- --nocapture --test-threads=1` — 11 passed;
- `GLASS_EXTENSION_SANDBOX_E2E=1 GLASS_EXTENSION_SANDBOX_KIND=linux-bubblewrap cargo test --lib extensions::tests::sandboxed_reference_extensions_pass_native_gate --locked` — passed;
- `cargo package --locked --no-verify` — package contents validated; and
- `cargo publish --locked --dry-run --no-verify` — crates.io publication dry-run passed without uploading.

The support status in the README intentionally calls only this Linux ARM64
environment locally verified. Other declared targets remain unverified here.
