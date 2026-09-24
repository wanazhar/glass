---
id: native-engine-browser-708
scope: glass-browser/native-engine/javascript/dynamic-import-map-registration
status: complete
depends-on: [native-engine-browser-707]
---

# Objective

Register dynamically attached HTML import-map scripts in the page's existing
module-import-map state, and make dynamically attached inline module roots pass
through resource loading and dependency prefetch before evaluation.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-707.md`
- [HTML Standard: scripting](https://html.spec.whatwg.org/multipage/scripting.html)
- [HTML Standard: import maps](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)

## Contract

- Detect newly attached HTML `script[type=importmap]` elements alongside
  dynamically attached classic and module script elements.
- Preserve the order in which dynamic script elements become connected when
  handing maps and scripts to the loader. Parse and merge each valid map
  atomically into the runtime's current map, retaining existing entries and
  resolved-specifier locks.
- A dynamically attached module root must use the resource loader to prefetch
  its static dependency graph using the effective map before module evaluation.
  This must work both when its import map is attached in the same mutation and
  when the root is attached in a later host turn.
- Preserve CSP rejection/error events, import-map parse/merge failures, module
  fetch behavior, and existing single-shot script-start semantics.
- Local/no-loader contexts must register valid inline maps in runtime state;
  external resources still require a process-backed loader.
- Do not claim computed dynamic imports, worker maps, file-origin propagation,
  fragment-distinct module identity, or complete parser/module scheduling.

## Verification

- Passed `cargo check -p glass-browser --lib --test native_engine --locked
  --quiet` after the attachment-order correction.
- Passed the `import_map` test filter: 16 library tests and 4
  process-backed tests, including the opposite-creation/attachment-order case,
  no-loader registration, and later-host-turn mapped-module case.
- Passed `native_file_document_loads_rooted_dynamic_subresources` with the
  dynamically mapped inline module and rooted dependency.
- Passed `cargo fmt --all -- --check` and `git diff --check`.
- Passed release-documentation truth across 1,336 Markdown files (83 current,
  63 previous-version references, zero stale-current-claim failures);
  documentation depth (93 guides, 19 contracts); shortcut inventory (15 keys,
  63 markers); and documentation coverage (1,336 files, 346 MCP tools including
  101 browser-only, 17 examples, 22 public modules).
- Remote CI was not run. This is local slice evidence only; issue #40 remains
  open.
