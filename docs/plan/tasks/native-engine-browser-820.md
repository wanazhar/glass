id: native-engine-browser-820
scope: glass-browser/native-engine/profile-cookie-journal
status: done
depends-on: [native-engine-browser-819]
---

# Glass native-engine browser slice 820: synchronize cookies across live sessions

## Objective

Propagate accepted HTTP response-cookie changes between separately created
native sessions that share the same explicit profile, so a live receiver's
next request uses the updated cookie jar without reopening the browser
session.

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for native-browser completion.
- [Glass Core Web Profile](../native-engine-browser-profile.md) defines
  profile isolation and the bounded, operation-boundary cookie notification
  contract.
- [Native browser engine architecture](../../architecture/native-engine.md)
  documents profile ownership and the existing bounded storage event journal.
- Slices 818-819 cover durable page-response cookie updates and same-backend
  fan-out, including a live SharedWorker module request.

## Contract

- Reuse the existing profile lock, bounded event journal, and reader leases;
  persist the cookie delta before publishing it.
- Journal records remain backward-readable and validate cookie keys and
  bounded payloads. A writer ignores its own records.
- At an operation boundary, another live session coalesces journal updates by
  `(name, domain, path)` and applies them to its loader and active content
  process before its next request.
- Runtime-only delivery must not write the change back or cause journal echo.
- This slice covers response `Set-Cookie` updates, including response-driven
  deletion. Cross-session publication of explicit cookie import/clear API
  calls is not covered by this slice.
- Verify one shared-profile live-session flow, latest-value/deletion behavior,
  and a separate-profile control. Do not claim cross-OS or full cookie/WPT
  conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-820.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- Run the exact Slice 820 cross-session process-backed regression and directly
  affected journal tests.
- Run `cargo fmt --all -- --check`, `git diff --check`, and all four maintainer
  documentation gates.
- Commit this bounded checkpoint locally with a Conventional Commit; do not
  push or run remote CI.

## Results

The process-backed separate-session regression passed (1 passed, 870 filtered;
46.24 seconds). A writer accepted response-cookie updates and a second already-
running session using the same profile sent the latest ordinary and HttpOnly
cookies, excluding the deletion; a third session on a distinct profile sent
neither cookie. The journal round-trip, invalid-key rejection, and older-record
decode test passed (1 passed, 1,639 filtered; 0.07 seconds). The scoped
integration-test check passed with existing dead-code warnings from the
superseded HTML parser. Formatting, diff validation, and all maintainer docs
gates passed. Broader cookie/WPT and cross-platform conformance remain open.
