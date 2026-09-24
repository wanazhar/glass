---
id: native-engine-browser-717
scope: glass-browser/javascript/rooted-file-runtime-dynamic-imports
status: done
depends-on: [native-engine-browser-716]
---

# Glass native-engine browser slice 717: rooted-file runtime dynamic imports

## Objective

Resolve runtime-valued JavaScript `ImportCall`s from rooted-file page scripts
through the owning native page runtime. Cover initial classic/module scripts,
dynamically attached local scripts, and nested runtime imports from loaded file
modules.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-713.md`
- `docs/plan/tasks/native-engine-browser-716.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Resolve each request with its active script/module referrer and the current
  Document import map; preserve request URL module identity and each loaded
  resource's response URL as its descendant base.
- Load only `file:` targets through the document's configured rooted-file
  loader. Reuse module-map entries, graph-entry/edge/byte limits, integrity
  metadata, and the existing module-source owner. Never route a rooted-file
  import through HTTP, CDP, or another backend.
- Settle the original JavaScript import promise with the loaded module key.
  Missing, malformed, unsupported-scheme, or out-of-root targets reject that
  promise without exposing file contents or aborting unrelated page work.
- Support nested runtime-valued imports from newly loaded modules and imports
  initiated by dynamically attached rooted-file scripts. Preserve existing
  module identity and bound the number of host requests per turn.
- Non-module network commands, Service Worker classic-import settlement,
  import options/attributes, and complete asynchronous module scheduling remain
  separate issue #40 gates.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-717.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_file_document_resolves_runtime_ -- --nocapture` passed 2/2. It covers
  initial classic/module imports, dynamically attached classic/module scripts,
  a dynamically registered import map, nested imports, duplicate identity, and
  out-of-root rejection.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_file_document_loads_rooted -- --nocapture` passed 4/4.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_uses_dynamic_import_map_for_a_later_module_root --
  --nocapture` passed 1/1.
- `rustfmt --edition 2024 --check` on the changed Rust files and `git diff
  --check` passed.
- Maintainer gates passed: release truth validated 1,345 Markdown files with
  zero current-claim failures; depth validated 93 guides and 19 contracts;
  shortcut inventory validated 15 implementation keys and 63 markers; coverage
  validated 346 MCP tools, 17 examples, and 22 public modules.
- Record local-only evidence; do not claim remote CI, platform certification,
  release promotion, or issue #40 closure from these checks.
