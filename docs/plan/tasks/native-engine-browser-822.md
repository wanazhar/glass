---
id: native-engine-browser-822
scope: glass-browser/native-engine/shared-worker-fetch-api
status: done
depends-on: [native-engine-browser-821]
---

# Glass native-engine browser slice 822: exercise Fetch API from SharedWorker

## Objective

Supply the local content-process SharedWorker route with its owner-derived
constructor storage key, then verify that a live process-backed SharedWorker
can issue direct Fetch API requests, consume response bodies, and observe the
tested cookie credential behavior.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/maintainers/README.md`

## Contract

- Use a local HTTP fixture and a connected module SharedWorker; trigger fetch
  only after the page observes the worker connection.
- Exercise `credentials: include` and `credentials: omit`, response body
  consumption, response-cookie update, and the worker's subsequent request.
- Verify ordinary and HttpOnly request cookies, omission under `omit`, and
  deletion/latest-value behavior from the HTTP server's observed requests.
- Keep the work scoped to direct SharedWorker Fetch API requests. Do not infer
  complete Fetch Standard, WPT, cross-platform, or browser-completion coverage.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-822.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet` passed;
  only existing dead-code warnings from the superseded HTML parser were emitted.
- `cargo test -p glass-browser --test native_engine native_content_process_shared_worker --locked --quiet -- --test-threads=1` passed (4 passed, 868 filtered; 68.32 seconds), including the direct Fetch API regression.
- `cargo test -p glass-browser --test native_engine native_content_process_resolves_runtime_shared_worker_module_imports --locked --quiet -- --exact --test-threads=1` passed (1 passed, 871 filtered; 18.78 seconds).
- `cargo fmt --all -- --check` and `git diff --check` passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation.json` passed (1,450 Markdown documents; zero current-claim failures).
- `python3 scripts/check-documentation-depth.py` passed (93 current guides; 19 substantive contracts).
- `python3 scripts/check-tui-shortcuts.py` passed (15 implementation help keys; 63 documentation markers).
- `python3 scripts/check-documentation-coverage.py` passed (1,450 Markdown files; 346 MCP tools, 17 examples, and 22 public modules).
