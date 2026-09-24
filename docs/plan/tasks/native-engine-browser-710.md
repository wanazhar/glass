---
id: native-engine-browser-710
scope: glass-browser/native-engine/javascript/worker-module-identity
status: done
depends-on: [native-engine-browser-709]
---

# Objective

Preserve request-URL module identity separately from the final response URL
base in dedicated, shared, and service-worker module graphs. Query strings and
fragments distinguish module records; fragments are omitted only at resource
I/O. Descendants, including after redirects, resolve from their parent module's
response URL.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-709.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`

## Contract

- Key each module source and graph deduplication entry by its resolved request
  URL, including query and fragment.
- Store the final response URL separately and use it as the module base for
  descendants, including when the root or a dependency redirects.
- Apply the contract to dedicated, shared, and service-worker module graphs.
- Preserve worker location, origin, CSP/CORS, integrity, file-root checks,
  resource bounds, error delivery, and the existing worker import-map policy.
- Keep existing literal dynamic-import prefetch behavior; computed dynamic
  imports, worker import maps, and complete module scheduling are not claimed.
- Verify two fragment-distinct dependency records and a redirected worker root
  whose imports resolve from the final response directory; cover rooted-file
  worker modules as well.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-710.md`

## Verification

- Passed `cargo check -p glass-browser --lib --test native_engine --locked
  --quiet`.
- Passed the focused module-worker test (dedicated and shared fixture workers,
  1/1), the process-backed redirected worker-module identity/base test (1/1),
  the module service-worker registration/interception test (1/1), and the
  rooted-file dedicated/shared worker test (1/1).
- Passed `cargo fmt --all -- --check` and staged/working-tree diff checks.
- All four maintainer documentation gates passed: release truth covered 1,338
  Markdown documents (83 current, 63 previous-version references, 1,481
  semantic hits, zero current-claim failures); depth covered 93 guides and 19
  contracts; shortcut inventory covered 15 implementation keys and 63 markers;
  coverage found 346 MCP tools (101 browser-only), 17 examples, and 22 public
  modules across 1,338 Markdown files.
- Remote CI was not run; the commit remains local and unpushed.
