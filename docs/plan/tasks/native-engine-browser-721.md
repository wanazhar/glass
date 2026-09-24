---
id: native-engine-browser-721
scope: glass-browser/javascript/demand-driven-dynamic-module-imports
status: completed
depends-on: [native-engine-browser-720]
---

# Glass native-engine browser slice 721: demand-driven dynamic imports

## Objective

Route every JavaScript `ImportCall` through the owning asynchronous module
loader only when invoked. Implement the dynamic `options` argument for the
supported `type: "json"` module and preserve module identity, referrer,
import-map, service-worker, and rooted-file rules across page and Worker
realms.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-720.md`
- [ECMAScript: EvaluateImportCall](https://tc39.es/ecma262/multipage/ecmascript-language-expressions.html#sec-evaluateimportcall)
- [HTML Standard: HostLoadImportedModule](https://html.spec.whatwg.org/multipage/webappapis.html#hostloadimportedmodule)
- [HTML Standard: module type from module request](https://html.spec.whatwg.org/multipage/webappapis.html#module-type-from-module-request)
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- A dynamic `import()` is not a static module-graph edge. Do not fetch a
  literal target before the `ImportCall` executes. Literal and runtime-valued
  specifiers use the same bounded host fetch queue when invoked.
- Preserve argument-expression evaluation order. Convert the specifier with
  ECMAScript `ToString` semantics; process an optional options object by
  reading `with`, enumerating its own enumerable string keys, and reading each
  value. Conversion/getter failures reject the returned promise. Invalid
  options, non-string attribute values, unsupported keys, and unsupported
  module types reject with `TypeError` before any module request is issued.
- The supported dynamic module requests are default JavaScript and
  `with: { type: "json" }`. JSON requests use the existing strict JSON MIME,
  parse, and default-export path. Text/CSS and other module types remain
  explicitly unsupported; they must not become JavaScript requests.
- Module-map identity is resolved request URL plus module type, including
  query and fragment. Redirect response URLs remain descendant bases for
  JavaScript modules. Reuse only an entry with the same URL and type; importing
  one URL as both JavaScript and JSON remains distinct.
- Documents apply the active script's Document import map. Dedicated and
  SharedWorkers use their active script/module URL and never inherit a Document
  map. Nested dynamic imports retain the loaded module's final response URL.
- Dynamic imports in ServiceWorkers still reject with `TypeError` and issue no
  fetch. Evaluate and validate the specifier/options conversion before the
  host rejection, including for classic `importScripts()` sources and module
  dependencies.
- Configured-root file Documents load dynamic JavaScript and JSON modules only
  through the configured-root loader. Preserve import-map resolution, typed
  identity, graph/byte bounds, and out-of-root rejection; never send a file
  target to network transport.
- Keep existing request, graph-entry, graph-byte, and per-turn bounds. A
  rejected import must not cancel unrelated work or leave a pending module
  alias behind.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-721.md`

## Verification

- Unit coverage for literal/computed import rewriting, nested calls, options
  argument preservation, and exclusion of member methods named `import`.
- Process-backed page coverage for runtime JSON options, same-URL typed
  identity, option side-effect/error ordering, and no request for an uncalled
  literal dynamic import.
- Process-backed dedicated/shared Worker coverage for JSON dynamic imports and
  nested dynamic modules. ServiceWorker coverage verifies option evaluation,
  `TypeError`, and an exact request set with no dynamic target fetch.
- Rooted-file coverage verifies a dynamic JSON import stays under the admitted
  root, plus out-of-root rejection and existing mapped/nested behavior.
- Run the locked `glass-browser` package check before the focused unit and
  process-backed test batches; then formatting, whitespace, and maintainer
  documentation gates. Do not run the full workspace or remote CI for this
  slice.

## Results

Dynamic import targets now load only when invoked; options use ECMAScript
specifier coercion and getter/attribute processing, and JSON uses a distinct
URL-plus-type module identity through pages, dedicated/shared Workers, classic
`importScripts()` dependencies, and rooted files. Service Worker calls still
reject with `TypeError` without a dynamic fetch. Regressions verify same-URL
JSON/JavaScript responses, invalid-option/no-request behavior, uncalled literal
no-request behavior, boxed-Symbol conversion failure, option side-effect
order, rooted-file boundaries, and exact Service Worker requests.

`cargo check -p glass-browser --test native_engine --locked --quiet` passed.
The seven `native_static_dynamic_import_tests` passed, as did six focused
process-backed integration tests for page dynamic JSON, uncalled imports,
rooted files, classic `importScripts()` Workers, dedicated/shared module
Workers, and Service Workers. `cargo fmt --all` and `git diff --check` passed.
Remote CI was not run. Other module types, broader file-origin propagation,
and complete module scheduling remain open issue #40 work.
