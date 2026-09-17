# Native engine browser-complete slice 477: rooted file CSS import graphs

- Status: complete
- Scope: `native-engine` / bounded `@import` graphs in rooted file CSS
- Issue: #40
- Depends on: [native-engine-browser-476](native-engine-browser-476.md)

## Objective

Make rooted file stylesheets compose ordinary multi-file applications by
loading bounded literal `@import` dependencies before CSS parsing, while
preserving each imported stylesheet's own URL base and cascade order.

## Contract

- Rooted file stylesheets may import quoted or `url(...)` file-relative CSS
  references with optional trailing media/layer text treated as the existing
  bounded CSS profile permits.
- Imported stylesheets are canonicalized against their owning stylesheet,
  pass the configured file-root, credential, symlink, UTF-8, and byte checks,
  and are inserted at the import position before the existing CSS cascade.
- Graph entries and aggregate stylesheet bytes are bounded. Duplicate imports
  are admitted once and cycles terminate without recursive growth. Missing,
  unsupported-scheme, out-of-root, malformed, or oversized imports fail closed
  for the owning stylesheet; no network/cache fallback is introduced.
- Initial and dynamically attached rooted file stylesheets use the same graph
  owner. Non-file stylesheet behavior remains unchanged.

## Implementation

- Add bounded lexical CSS import discovery that ignores comments and quoted
  text and recognizes literal quoted/`url(...)` import targets.
- Expand each rooted file stylesheet graph recursively at import positions,
  canonicalizing each source's relative URL tokens against its own file URL.
- Add nested-directory integration coverage for static and dynamic imports,
  duplicate/cycle termination, and denied imports, then update architecture
  and plan records.

## Tradeoffs and remaining scope

This is a bounded literal import graph, not a full CSS parser or loader:
media-condition evaluation, `layer()`/`supports()` import semantics, escaped
CSS URL grammar, network stylesheet imports, file fonts, other CSS resource
types, and remaining file-origin/Web IDL parity stay open under issue #40.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_` (7 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-477.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
