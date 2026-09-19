# Native engine browser slice 640: bounded pseudo classes

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS selector model now carries a bounded pseudo-class list in each
compound selector. Supported structural pseudo-classes are `:root`,
`:first-child`, `:last-child`, `:only-child`, and `:empty`; supported state and
link pseudo-classes are `:checked`, `:disabled`, `:enabled`, `:required`,
`:optional`, `:link`, and `:any-link`.

Selectors add bounded pseudo-class specificity, match structural relationships
against the attached `NativeDocument`, and use local node attributes for the
bounded checked, disabled, required, and link states. Unsupported functional,
dynamic, and pseudo-element forms remain rejected instead of being treated as
ordinary selectors. The same document-aware matcher is used by selector action
locators and the stylesheet cascade.

Full CSS Selectors grammar, dynamic interaction state beyond the bounded
attributes, pseudo-elements, sibling and child combinators, and complete CSS
parity remain issue #40 gates.

## Focused coverage

- `selector_parser_supports_bounded_pseudo_classes` verifies bounded
  specificity and fail-closed rejection of hover, functional, and
  pseudo-element forms.
- `document_selector_matches_bounded_structural_pseudo_classes` verifies
  root, first-child, last-child, only-child, and empty matching against
  attached document relationships.
- `document_selector_matches_bounded_state_pseudo_classes` verifies checked,
  disabled, enabled, required, optional, link, and any-link matching plus
  rejection of unsupported functional selectors.
- `document_pseudo_classes_apply_during_style_cascade` verifies document-aware
  pseudo selectors affect computed CSS values, not only action locator output.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib pseudo --locked -- --nocapture
```

Observed focused result: 4 passed, 0 failed, 1,405 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,409 passed, 0 failed, 1 ignored, 1,408 filtered out.

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Observed package gate result: the fast workspace check, locked
`glass-browser` and `glass-dev` binary builds, and locked metadata validation
all passed without warnings.

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed documentation gate result: 1,290 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
