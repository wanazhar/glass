# Native engine browser-complete slice 475: rooted file module graphs

- Status: complete
- Scope: `native-engine` / bounded module imports in rooted `file:` pages
- Issue: #40
- Depends on: [native-engine-browser-474](native-engine-browser-474.md)

## Objective

Complete the local-file module-script boundary started in slice 474. A rooted
file page's external module scripts must prefetch and execute their bounded
local dependency graph through the existing QuickJS module loader.

## Contract

- File module scripts may resolve absolute or relative `file:` imports from
  static `import`, `export ... from`, and literal dynamic `import()` source
  references.
- Every graph entry is canonicalized through the configured roots, bounded by
  the native module-entry and aggregate-script-byte limits, integrity checked
  where the root script metadata supplies it, and installed before module
  evaluation.
- Duplicate graph entries are evaluated once. Cycles remain finite and are
  left to the module evaluator's normal semantics; bare package specifiers,
  network/data/blob imports, credentials, root escapes, missing files, and
  oversized sources fail closed and dispatch the owning module's `error`
  event.
- Dynamic file module attachments use the same graph loader. Classic file
  scripts and all non-file document behavior remain unchanged.

## Implementation

- Add a synchronous rooted-file module graph prefetch helper that reuses the
  existing bounded lexical import discovery and QuickJS module source map.
- Use it for static local module scripts and dynamically attached local module
  scripts, preserving load/error event ordering.
- Add integration coverage for successful relative imports and denied
  out-of-root/module-network cases, then update the architecture and plan
  records.

## Tradeoffs and remaining scope

This is a bounded prefetch graph, not a full JavaScript module resolver:
QuickJS remains authoritative for grammar/evaluation, while Glass discovers
literal URL dependencies before evaluation. Import maps, package resolution,
non-literal dynamic imports, CSS URL base semantics, file workers/fonts,
downloads, complete file-origin semantics, and remaining Web IDL parity stay
open issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_`
  (6 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version
  --report /tmp/glass-release-documentation-475.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
