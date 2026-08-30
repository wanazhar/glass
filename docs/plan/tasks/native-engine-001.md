---
id: native-engine-001
scope: glass-browser/native-engine
status: done
depends-on: []
---

# Native engine kernel and fixture backend

## Objective

Implement the Phase 0/Phase 1 native-engine checkpoint from issue #40:

- establish the default-off feature boundary;
- define configuration, limits, lifecycle, origin, history, scheduler, local
  resource, DOM, and engine contracts;
- implement a deterministic one-context fixture/data-URL engine;
- implement its experimental `BrowserBackend` adapter and explicit-only factory
  registration; and
- keep unsupported browser capabilities typed and fail-closed.

This task must not add network, JavaScript, CSS/layout/paint, storage, browser
windows, a third crate, or a complete browser-engine dependency.

## Context

- `docs/architecture/native-engine.md`
- `docs/architecture/browser.md`
- `docs/browser-host-rfc.md`
- `docs/backend-capability-matrix.json`
- `docs/plan/analysis/native-engine.md`
- `docs/INDEX.md`
- `crates/glass-browser/src/browser_backend.rs`

## Path

- `crates/glass-browser/Cargo.toml`
- `crates/glass-browser/src/lib.rs`
- `crates/glass-browser/src/browser/mod.rs`
- `crates/glass-browser/src/browser/backend_factory.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/`
- `crates/glass-browser/tests/native_engine.rs`
- `.github/workflows/ci.yml`
- `.github/workflows/crates-release.yml`
- `scripts/check-documentation-depth.py`
- `crates/glass-browser/src/browser/runtime.rs`
- the referenced architecture, feature, SDK, CLI, RFC, matrix, and plan docs

## Contract

The backend ID is `native-engine`, certification is `experimental`, and the
initial available capabilities are lifecycle, navigation, contexts, and
bounded evidence. The backend accepts only `about:blank`, `data:text/html,...`,
and registered `fixture://...` URLs. It exposes one active context with a
monotonic revision. Navigation is transactional: a failed load or parse does
not change the current document or revision.

`native-engine` is default-off and must be excluded from automatic backend
selection. An explicit `preferred_backend_id = "native-engine"` is required.
The default browser-free CI matrix runs without experimental features; a
separate Linux native-engine job runs the explicit feature checks and is part
of push certification.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --lib --no-default-features --locked
cargo test -p glass-browser --lib --no-default-features --locked
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo clippy -p glass-browser --all-targets --no-default-features --locked -- -D warnings
RUST_MIN_STACK=4194304 scripts/check-rust-workspace.sh test
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --locked --no-deps
python3 scripts/check-documentation-depth.py
python3 scripts/check-documentation-coverage.py --glass target/debug/glass --glass-browser target/debug/glass-browser
```

Record default/native build timing and peak memory. Run the full workspace
checks only after the focused task checks pass.
