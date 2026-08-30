---
id: native-engine-007
scope: glass-browser/native-engine/css-presentation
status: done
depends-on: [native-engine-006]
---

# Native bounded CSS presentation seed

## Objective

Introduce the first narrow Phase 3 presentation layer without pretending to
implement browser CSS or layout:

- parse bounded `<style>` blocks and inline `style` declarations;
- match a deliberately small selector grammar: universal/type, `#id`,
  `.class`, and attribute presence/exact-value selectors in one compound
  selector;
- apply deterministic specificity and source-order cascade for `display` and
  `visibility` only, with inline declarations taking precedence; and
- feed the resulting presentation state into visible-text projection and the
  existing semantic actionability gate.

This slice is a CSS presentation seed, not a CSS engine. It does not add
general selectors, inheritance, computed-style completeness, layout, hit
testing, scrolling, painting, screenshots, or a compatibility claim.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-006.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

Stylesheet rules are read from bounded `style` element text after the HTML
raw-text parser has isolated it. A supported selector is one compound
selector with an optional type, ID, class, and attribute presence or exact
value component; combinators, pseudo-classes, malformed rules, and
unsupported declarations do not match. Supported declarations are
`display:none` versus another display value and `visibility:hidden` versus
another visibility value. Declarations cascade per property by selector
specificity, source order, and finally inline-style precedence. `!important`
is not a separate priority tier in this slice.

An element is presentation-hidden when its own or an ancestor's explicit
`hidden`/`aria-hidden` signal or computed supported display/visibility state
requires exclusion. `opacity`, geometry, clipping, overflow, and paint do not
affect visibility or actionability yet. The existing bounded semantic and
revision contracts remain unchanged.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo test -p glass-browser --all-targets --all-features --locked
```
