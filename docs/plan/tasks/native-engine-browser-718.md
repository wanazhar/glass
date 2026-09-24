---
id: native-engine-browser-718
scope: glass-browser/javascript/rooted-file-parser-import-maps
status: done
depends-on: [native-engine-browser-717]
---

# Glass native-engine browser slice 718: rooted-file parser import maps

## Objective

Initialize parser-sourced Document import maps during initial rooted-file
navigation and use the effective map for subsequent script module graphs and
runtime-valued imports.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-707.md`
- `docs/plan/tasks/native-engine-browser-708.md`
- `docs/plan/tasks/native-engine-browser-717.md`
- [HTML Standard: import maps](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- On initial rooted-file navigation, collect bounded parser import-map sources
  and register them in document order before processing each later parser
  script. Preserve atomic merge behavior and resolved-specifier locks used by
  the existing process-backed Document import-map path.
- Apply the effective map to static dependencies of inline and external module
  scripts and to runtime-valued `import()` from classic/module scripts. Resolve
  an external module script's own `src` as a document-relative URL; import maps
  apply to its module specifiers, not the script-element URL.
- Pass all mapped module targets through the configured-root file loader and
  existing integrity, identity, entry, edge, and aggregate-byte bounds. Never
  route rooted-file module loading through HTTP, CDP, or another backend.
- Pass parser maps and inline classic/module roots through the resource
  loader's existing inline-script policy check before registration, evaluation,
  or dependency loading. If the check denies a source, report its script error
  and do not load its graph. Missing/oversized/external, malformed, or
  unmergeable maps report the owning script error and leave the previously
  accepted map intact without aborting unrelated page work.
- Do not claim full CSP conformance for file-origin documents: the current
  loader's CSP policy registry is network-document scoped, so adding and
  enforcing file-origin response/meta policies remains a separate issue #40
  gate.
- Maps remain Document-scoped. Dynamically attached map ordering, worker
  semantics, Service Worker import settlement, import options/attributes, and
  complete parser/module scheduling are not changed by this slice.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-718.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed. It reported two dead-code warnings for the replaced relative-only
  loader helpers; those helpers were removed, and the final focused test builds
  the resulting source.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_file_document_applies_parser_import_maps_to_initial_scripts --
  --nocapture` passed 1/1. It covers parser order, static and nested mapped
  modules, runtime imports from classic/module scripts, later mappings,
  conflict preservation, malformed-map isolation, and out-of-root rejection.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_file_document_resolves_runtime_ -- --nocapture` passed 2/2 for the
  preceding initial/late rooted-file runtime-import regressions.
- `rustfmt --edition 2024 --check` and `git diff --check` passed before removal
  of the superseded helpers; final test compilation passed after their removal.
- Maintainer gates passed: release truth covered 1,346 Markdown files with
  zero current-claim failures; depth covered 93 guides/19 contracts; shortcut
  inventory covered 15 keys/63 markers; coverage validated 346 MCP tools (101
  browser-only), 17 examples, and 22 public modules.
- Local only; remote CI was not run. Issue #40 remains open.
