---
id: native-engine-browser-720
scope: glass-browser/javascript/json-static-module-imports
status: done
depends-on: [native-engine-browser-719]
---

# Glass native-engine browser slice 720: JSON static module imports

## Objective

Implement `with { type: "json" }` for static module dependencies across page,
dedicated/shared/service-worker, and configured-root file module graphs. Fetch
JSON dependencies under their own module type, enforce the JSON MIME contract,
and evaluate them as JSON modules with only a default export. Preserve request
URL plus module type as identity and the final response URL as the descendant
base for JavaScript modules. Keep the work bounded by existing module and
resource limits.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-719.md`
- [HTML Standard: import attributes and module fetching](https://html.spec.whatwg.org/multipage/webappapis.html#hostgetsupportedimportattributes)
- [MIME Sniffing Standard: JSON MIME type](https://mimesniff.spec.whatwg.org/#mime-type-groups)
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The host supports the `type` import attribute. A static dependency with
  `type: "json"` loads a JSON module; absent attributes continue to load a
  JavaScript module. Unsupported attribute keys or module types fail explicitly
  and never silently become JavaScript requests.
- HTTP JSON module responses require a JSON MIME type (`application/json`,
  `text/json`, or a `+json` subtype), are UTF-8 decoded, and are parsed using a
  realm-captured JSON parser. Invalid JSON and a MIME mismatch fail module
  loading; they do not execute as JavaScript.
- A JSON module exposes the parsed JSON value only as its immutable default
  export. JSON object semantics, including duplicate keys and an own
  `__proto__` property, match the realm's JSON parser.
- Module identity is the request URL and module type together. Redirected
  JavaScript modules retain their request identity and use their final response
  URL for descendants. JSON modules have no descendant imports.
- QuickJS coalesces static requests with the same literal specifier before the
  resolver receives import attributes. The runtime therefore rewrites only
  static JSON request literals to an internal marker before parsing, then
  removes that marker before import-map resolution. Fetch and module identity
  continue to use the original resolved URL and module type.
- Apply the same typed loader path to HTTP page graphs, all module Worker
  graphs, and configured-root file page graphs. File-root validation and page
  import maps remain in force.
- Runtime-valued dynamic-import options, text/CSS modules, and complete module
  scheduling remain separate open work; this slice must not claim them done.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-720.md`

## Verification

- Add parser tests for side-effect imports, `from` imports, re-exports, comments,
  whitespace, and `type: "json"`; unsupported types must not be discovered as
  JavaScript requests.
- Add process-backed page and Worker tests for valid JSON, strict MIME checks,
  invalid JSON, default-only export, JSON object semantics, redirects, and the
  exact requested resources. Cover the configured-root file graph and preserve
  its root boundary.
- Run the scoped `glass-browser` check before focused tests, followed by format
  and documentation gates. Do not run the full workspace or remote CI for this
  slice.

## Results

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`:
  passed.
- `cargo test -p glass-browser --lib --locked --quiet native_static_dynamic_import_tests`:
  10 passed, including same-literal JSON/JavaScript imports through an import
  map, an authored JavaScript specifier using the internal marker text, and a
  transitive mapped JSON dependency.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_json_module_`: 6 process-backed tests passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Maintainer documentation gates passed: 1,348 Markdown files with zero
  current-claim failures; 93 guides/19 contracts; 15 shortcut keys/63 markers;
  346 MCP tools (101 browser-only), 17 examples, and 22 public modules.
- Remote CI was not run. Dynamic import options, unsupported module types,
  broader file-origin propagation, and complete module scheduling remain
  separate open work.
