# Local platform evidence

Status: verified locally on 2026-08-01.

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

The following checks passed against the local `0.2.4` development checkout at
commit `41b5684`:

- `cargo test --all --locked` — 472 passed, 1 ignored;
- `GLASS_E2E=1 cargo test --test browser_smoke --locked -- --nocapture
  --test-threads=1` — 11 passed;
- `cargo package --locked --list` — package contents inspected;
- `cargo package --locked --no-verify` and
  `cargo publish --locked --dry-run --no-verify` — publication shape passed
  without uploading;
- `GLASS_PREVIOUS_VERSION=0.2.3 scripts/smoke-clean-install.sh` — clean
  install and upgrade smoke passed from 0.2.3 to 0.2.4;
- `cargo deny check` and `cargo audit` — no license, source, or vulnerability
  findings.
- `cargo run -- --experimental-extensions capabilities` — reported the
  `extensions` capability as `experimental` on this host.

## 0.2.5 readiness

The following checks passed against the local `0.2.5` development checkout at
commit `d6b9935`:

- `cargo test --all --locked` — 474 passed, 1 ignored;
- `GLASS_E2E=1 cargo test --test browser_smoke --locked -- --nocapture
  --test-threads=1` — 11 passed;
- `cargo package --locked --no-verify` packaged 203 files;
- `cargo publish --locked --dry-run --no-verify` completed without uploading;
- `GLASS_PREVIOUS_VERSION=0.2.4 scripts/smoke-clean-install.sh` — clean
  install and upgrade smoke passed from 0.2.4 to 0.2.5;
- `cargo deny check` and `cargo audit` — no license, source, or vulnerability
  findings.

The support status in the README intentionally calls only this Linux ARM64
environment locally verified. Other declared targets remain unverified here.
