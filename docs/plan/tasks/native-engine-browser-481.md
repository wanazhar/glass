# Native engine browser-complete slice 481: bounded CSS import conditions

- Status: complete
- Scope: `native-engine` / rooted file stylesheet `@import` conditions
- Issue: #40
- Depends on: [native-engine-browser-480](native-engine-browser-480.md)

## Objective

Make rooted file stylesheet imports honor the conditional prelude that was
previously discarded by the bounded import-graph loader.

## Contract

- Literal quoted and `url(...)` imports retain their `layer`, `supports`, and
  media metadata through initial and dynamic rooted-file stylesheet loading.
- Active imports use the configured screen viewport and the native CSS
  declaration parser to evaluate bounded media and Supports conditions.
- Simple named and anonymous layer preludes preserve the existing native
  cascade-layer ordering for admitted imported rules.
- Inactive imports are removed before URL resolution or dependency loading, so
  a missing resource behind a false condition remains inert.
- Non-file stylesheet owners retain their existing loader and parser path.

## Implementation

- Replace tuple-only import discovery with a bounded `NativeCssImport` record.
- Parse quoted/`url(...)` targets, simple `layer`/`layer(name)`, logical
  `supports(...)` declarations, and screen/all/print media with viewport
  width/height/orientation features.
- Thread the configured `Viewport` through initial, dynamic, and recursive
  rooted-file stylesheet expansion.
- Wrap active layered imports through the existing `@layer` cascade parser and
  skip inactive imports before canonical file-root admission.
- Extend the rooted-file stylesheet integration fixture with active and
  inactive conditions and add focused parser/matcher coverage.

## Tradeoffs and remaining scope

The condition evaluator is deliberately bounded to the media features and CSS
declaration grammar already owned by the native engine; unknown conditions are
inactive rather than granting resource access. Nested/complex CSS layer names,
the full Media Queries grammar, full CSS Supports grammar, file fonts and
other CSS resource types, network stylesheet URL-base parity, complete
file-origin semantics, and full Web IDL parity remain issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked css_import_conditions`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
