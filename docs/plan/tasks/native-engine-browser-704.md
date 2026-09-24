---
id: native-engine-browser-704
scope: glass-browser/native-engine/javascript/import-maps
status: complete
depends-on: [native-engine-browser-703]
---

# Objective

Resolve page-module imports through bounded inline HTML import maps instead of
rejecting every bare specifier. The implementation remains inside the native
page runtime and existing resource-policy owner.

## Contract

- Discover attached inline `script[type="importmap"]` elements before page
  module evaluation; apply their `imports` maps to static and literal dynamic
  module dependency prefetch and to QuickJS module resolution.
- Support exact specifier keys and trailing-slash prefix keys. Normalize
  URL-like keys and targets against the document URL; resolve URL-like import
  specifiers before matching while preserving bare specifier identity.
- Merge bounded maps in document order with existing entries taking
  precedence. Invalid JSON, unsupported map members, blocked inline content,
  invalid targets, and credential-bearing URLs do not create usable mappings
  and produce an error event for the map element.
- Preserve existing module graph entry/byte bounds, redirect/MIME/CSP/network
  policy, and fail-closed behavior for unresolved bare specifiers.
- Do not claim complete import-map conformance from this slice. `scopes`,
  integrity metadata, external maps, dynamic map registration, workers, and
  complete parser/module timing remain separate issue #40 gates.

## Standards and context

- [HTML Standard: import maps](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)
- [HTML Standard: script type and import-map processing](https://html.spec.whatwg.org/multipage/scripting.html#the-script-element)
- Earlier module-graph boundaries: `native-engine-browser-030` and
  `native-engine-browser-031`.

## Paths

- `crates/glass-browser/src/browser/native_engine/module_import_map.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- this task record and issue #40

## Verification

- Run the locked `glass-browser` library/test-target check before tests.
- Test exact mappings, prefix mappings, URL-like remapping, unresolved bare
  specifiers, invalid/oversized map input, and multiple-map precedence.
- Run a process-backed external-module integration test proving the mapped
  URLs are the actual bounded requests and module evaluation uses those exact
  fetched sources.
- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed.
- `cargo test -p glass-browser --lib --test native_engine import_map --locked
  --quiet` passed: 5 module-map unit tests and 1 process-backed browser test.
  The integration test verified exact and prefix mappings, actual fetched URLs,
  and a literal dynamic import.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Release-documentation, documentation-depth, and shortcut checks passed. The
  documentation coverage check passed across 1,332 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Remote CI was not run; this slice does not claim native promotion.
