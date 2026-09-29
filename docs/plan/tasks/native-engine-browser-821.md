---
id: native-engine-browser-821
scope: glass-browser/native-engine/profile-cookie-api-journal
status: done
depends-on: [native-engine-browser-820]
---

# Glass native-engine browser slice 821: synchronize cookie import and clear APIs

## Objective

Publish explicit native cookie-import and cookie-clear API changes to other
live sessions sharing the same configured profile, with profile persistence
committed before journal notification.

## Contract

- Synchronize pending external profile changes before applying an explicit
  cookie import or clear, so clearing includes cookies added by peers.
- Persist the merged cookie profile before appending the bounded event-journal
  delta; the receiver applies it at its next operation boundary without
  writing it back.
- Keep separate-profile isolation and preserve existing response-cookie
  behavior.
- Verify with the existing process-backed cross-session regression, including
  import, clear, HttpOnly/ordinary response cookies, and isolated profile.
- Do not claim cross-OS or full cookie/WPT conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-821.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- Run the exact process-backed cross-session cookie regression; the bounded
  journal implementation itself is unchanged from Slice 820.
- Run `cargo fmt --all -- --check`, `git diff --check`, and all four maintainer
  documentation gates.
- Commit this bounded checkpoint locally with a Conventional Commit; do not
  push or run remote CI.

## Results

The updated process-backed regression passed (1 passed, 870 filtered; 45.52
seconds). It verifies response-cookie update/deletion, an HttpOnly cookie
imported through the explicit native API, cross-session visibility on the
receiver's next request, a peer-only API cookie being included by the
clearer's pre-operation profile sync and removed by API clear, and
separate-profile isolation. The scoped integration-test check passed with
existing dead-code warnings from the superseded HTML parser in `dom.rs`.
Formatting, diff validation, and the four maintainer documentation gates
passed. This does not establish direct SharedWorker Fetch API behavior,
broader cookie/WPT conformance, or cross-platform coverage.
