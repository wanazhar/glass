---
id: native-engine-browser-707
scope: glass-browser/native-engine/javascript/import-map-resolved-set
status: complete
depends-on: [native-engine-browser-706]
---

# Objective

Process parser-discovered import maps in document order relative to module
roots, and prevent later maps from changing a module specifier that has already
been resolved during graph prefetch.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-704.md`
- `docs/plan/tasks/native-engine-browser-705.md`
- `docs/plan/tasks/native-engine-browser-706.md`
- [HTML Standard: import-map processing and resolved module set](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)

## Contract

- Interleave parser-discovered import maps and module roots by document node
  order during prefetch. A module graph must not resolve through an import map
  that occurs later in the document.
- Record each successful module specifier resolution as a bounded
  `(referrer URL, normalized specifier) -> resolved URL` entry. Repeated
  resolution of that pair must return the recorded URL.
- When merging a later map, retain existing first-wins entries and remove a
  new exact key or slash-terminated prefix when its normalized key prefixes a
  recorded normalized specifier. Apply scope filtering only when the recorded
  referrer matches that scope; continue accepting unrelated new entries.
- Keep the effective map and resolution records installed for QuickJS so
  prefetch and runtime resolution cannot disagree. Preserve import-map
  integrity merge behavior from slice 706.
- Cap resolved records at 1,024. Overflow fails the affected module resolution
  closed; parsing and map merges remain atomic.
- Do not claim computed dynamic imports, dynamically inserted import-map
  registration, worker maps, fragment-distinct module identity, or complete
  parser/module scheduling.

## Paths

- `crates/glass-browser/src/browser/native_engine/module_import_map.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- this task record

## Verification

- Add unit tests for locked exact and prefix resolution, scope applicability,
  non-conflicting later entries, and resolution-record overflow.
- Add a process-backed document with modules before and after multiple import
  maps. Assert actual request paths and evaluated values prove a later exact
  rule cannot replace an earlier resolved prefix while an unrelated later
  mapping remains usable.
- Run the locked `glass-browser` library and `native_engine` target check
  before the focused import-map test batch.
- Run formatting, whitespace, release-documentation truth, documentation-depth,
  shortcut, and documentation-coverage gates. Record local and remote evidence
  separately.

## Verification

- Passed `cargo check -p glass-browser --lib --test native_engine --locked
  --quiet`.
- Passed the focused module batch with `cargo test -p glass-browser --lib
  --test native_engine --locked --quiet module`: 14 resolver/module unit tests
  and 11 process-backed module tests passed; no failures.
- Passed the import-map-specific `cargo test -p glass-browser --lib
  --test native_engine --locked --quiet import_map` filter: 14 unit tests and 3
  process-backed tests passed. This selection overlaps the module-filter run.
- The process-backed ordering fixture observed `/v1/item.js` for both module
  roots, never observed the later `/v2/item.js` override, loaded the unrelated
  `/later.js` mapping, and evaluated both expected values without script errors.
- Passed `cargo fmt --all -- --check` and `git diff --check`.
- Passed release-documentation truth across 1,335 Markdown files (83 current,
  63 previous-version references, 1,478 semantic audit hits, zero current
  claim failures); documentation depth (93 guides, 19 contracts); shortcut
  inventory (15 keys, 63 markers); and documentation coverage (1,335 files,
  346 full-product MCP tools including 101 browser-only, 17 examples, 22 public
  modules).
- Remote CI was not run. This is local slice evidence only; issue #40 remains
  open.
