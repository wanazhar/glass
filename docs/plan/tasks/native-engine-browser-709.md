---
id: native-engine-browser-709
scope: glass-browser/native-engine/javascript/module-identity
status: done
depends-on: [native-engine-browser-708]
---

# Objective

Preserve the request URL as module identity while using the response URL as
the module's base URL. Query strings and fragments must distinguish module
records, while fragments are omitted only from network/file I/O. Make the same
contract apply to import-map resolution, page module graph prefetch, QuickJS
module lookup, and rooted-file module loading.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-708.md`
- [HTML Standard: module maps and module fetching](https://html.spec.whatwg.org/multipage/webappapis.html#fetching-scripts)
- [HTML Standard: import-map resolution](https://html.spec.whatwg.org/multipage/webappapis.html#module-specifier-resolution)

The HTML module map is keyed by the request URL, including its query and
fragment, while the module script base URL comes from the response URL. URL
fragments are not sent in fetch requests but remain part of module identity.

## Contract

- Keep the parsed/resolved request URL, including query and fragment, as the
  module-source key and graph deduplication identity.
- Keep the final response URL separately as the module base used for descendant
  resolution, including after redirects.
- Preserve mapped-address fragments and referrer identity in import-map
  resolution and resolved-specifier locking.
- Strip fragments only at resource I/O boundaries; do not use the stripped
  fetch URL as the module identity.
- Cover external page modules and rooted-file modules, including two fragments
  that fetch the same path but instantiate distinct module records, and a
  redirect whose descendants resolve from the final response URL.
- Keep CSP, CORS, integrity, graph bounds, and failure behavior intact.
- Do not claim computed dynamic imports, worker module identity, or complete
  parser/module scheduling in this slice.

## Path

- `crates/glass-browser/src/browser/native_engine/module_import_map.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-709.md`

## Verification

- Passed `cargo check -p glass-browser --lib --test native_engine --locked --quiet`.
- Passed the `fragment` filter: 23 library tests and 19 integration tests,
  including the redirected HTTP root, two fragment-keyed module records, and
  rooted-file module graph.
- Passed the `import_map` filter: 17 library tests and 4 process-backed
  integration tests.
- Passed `cargo fmt --all` and `git diff --check`.
- All four maintainer documentation gates passed locally: release truth covered
  1,337 Markdown documents, 83 current guides, and 1,479 semantic hits with
  zero current-claim failures; depth covered 93 guides and 19 contracts;
  shortcuts covered 15 implementation keys and 63 documentation markers;
  coverage found 346 MCP tools (101 browser-only), 17 examples, and 22 public
  modules across 1,337 Markdown files.
- Remote CI was not run; the local checkpoint remains unpushed.
