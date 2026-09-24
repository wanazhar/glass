---
id: native-engine-browser-706
scope: glass-browser/native-engine/javascript/module-integrity
status: complete
depends-on: [native-engine-browser-705]
---

# Objective

Enforce import-map integrity metadata for fetched descendant modules, and make
external module fetches use the profile-required CORS behavior before any
source is admitted to QuickJS.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-704.md`
- `docs/plan/tasks/native-engine-browser-705.md`
- [HTML Standard: import maps and module fetch options](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)
- [Subresource Integrity](https://www.w3.org/TR/SRI/)

## Contract

- Accept the import-map `integrity` member as a bounded URL-to-metadata map.
  Normalize absolute and URL-like keys against the document URL. Ignore
  non-URL-like keys and non-string values as the HTML processing model does.
  Ignore unknown top-level extension keys instead of invalidating otherwise
  valid mappings.
- Count integrity entries against the existing combined import-map entry
  limit, cap an individual metadata value at 4 KiB, and make parse/merge
  failures atomic. Earlier map entries win when maps merge.
- Preserve source order while normalizing imports, scopes, and integrity keys:
  if multiple keys in one JSON map normalize to the same key, the later source
  entry replaces the earlier one; a later map merge still cannot replace an
  entry from an earlier map.
- Apply URL-keyed metadata to descendant static and literal-dynamic module
  requests, including URLs that are not introduced by an `imports` mapping.
  Reuse the resource loader's existing SHA-256/SHA-384/SHA-512 SRI verifier;
  compare response bytes before UTF-8 decoding or JavaScript evaluation.
- An integrity mismatch is a failed module dependency: do not install or
  execute that source, surface the owning module failure, and keep the
  document/session recoverable.
- External module roots and descendant module requests use CORS with
  same-origin credentials by default. An external module root may opt into
  `use-credentials`; descendant imports retain the standard same-origin
  credentials mode.
- Preserve existing script-element `integrity` handling. This task does not
  claim computed dynamic imports, dynamic map registration/resolved-specifier
  locking, fragment-distinct module identity, worker/file-origin propagation,
  or complete parser/task/module scheduling.

## Paths

- `crates/glass-browser/src/browser/native_engine/module_import_map.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- this task record

## Verification

- Add deterministic parser/merge/bounds tests for integrity maps and unknown
  extension keys.
- Add a process-backed two-origin test proving default module CORS, omission of
  cross-origin credentials, successful SHA-384 verification, and fail-closed
  behavior for a mismatched digest before execution.
- After the coherent implementation batch, run the locked browser library and
  integration-test target check before the focused import-map test batch.
- Run formatting, whitespace, release-documentation truth, documentation-depth,
  shortcut, and documentation-coverage gates. Record local and remote evidence
  separately.
- Passed `cargo check -p glass-browser --lib --test native_engine --locked
  --quiet`.
- Passed `cargo test -p glass-browser --lib --test native_engine --locked
  --quiet import_map`: 11 parser/resolver tests and 2 process-backed import-map
  tests.
- Passed `cargo fmt --all -- --check` and `git diff --check`.
- Release-documentation truth validated 1,334 Markdown documents (83 current,
  63 previous-version hits, 1,476 semantic hits, zero current-claim failures).
  Documentation depth validated 93 current guides and 19 substantive
  contracts. Shortcut inventory validated 15 keys and 63 markers.
  Documentation coverage validated 1,334 Markdown files, 346 full-product
  MCP tools (101 browser-only), 17 examples, and 22 public modules.
- Remote CI was not run. Native-only CI, browser conformance, cross-platform
  certification, packaging, performance, and release gates remain open.
