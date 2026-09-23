---
id: native-engine-browser-672
scope: native-engine/browser/css-selectors
status: done
depends-on: [native-engine-browser-671]
---

# Native Engine Browser Slice 672: HTML Default Attribute Selector Matching

## Objective

Implement host-language default value matching for native CSS attribute
selectors. In HTML documents, attribute selectors on HTML-namespace elements
compare values ASCII-case-insensitively for the fixed 46 names specified by
[HTML Standard §4.16.2](https://html.spec.whatwg.org/multipage/semantics-other.html#case-sensitivity-of-selectors):
`accept`, `accept-charset`, `align`, `alink`, `axis`, `bgcolor`, `charset`,
`checked`, `clear`, `codetype`, `color`, `compact`, `declare`, `defer`, `dir`,
`direction`, `disabled`, `enctype`, `face`, `frame`, `hreflang`,
`http-equiv`, `lang`, `language`, `link`, `media`, `method`, `multiple`,
`nohref`, `noresize`, `noshade`, `nowrap`, `readonly`, `rel`, `rev`,
`rules`, `scope`, `scrolling`, `selected`, `shape`, `target`, `text`,
`type`, `valign`, `valuetype`, and `vlink`.

The default applies to namespace-less attributes only. Explicit `i` and `s`
modifiers override the default. Other attribute values remain case-sensitive.
Matching uses ASCII-only folding and applies uniformly to all supported
operators, independent of whether the named HTML attribute affects that
element. The shared CSS selector matcher preserves the behavior across
document queries/action locators and stylesheet cascade. This task does not
claim parity for the separate page-realm JavaScript selector implementation.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- [HTML Standard §4.16.2](https://html.spec.whatwg.org/multipage/semantics-other.html#case-sensitivity-of-selectors)

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-672.md`

## Verification

- `cargo check -p glass-browser --lib --tests --locked` — passed; finished in
  1m12s with no warnings.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib html_default_attribute_value_case_rules --locked -- --nocapture` — 2 passed, 0 failed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib browser::native_engine::css::tests --locked -- --nocapture` — 443 passed, 0 failed; 1020 filtered out.
- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/css.rs crates/glass-browser/src/browser/native_engine/dom.rs` — passed.
- `python3 scripts/check-release-documentation.py --require-previous-version` — 1300 Markdown documents; 0 current-claim failures.
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` — 15 implementation keys and 63 documentation markers validated.
- `python3 scripts/check-documentation-coverage.py` — 1300 Markdown files, 346 full-product MCP tools (101 browser-only), 17 examples, and 22 public modules validated.
- Final post-documentation `git diff --check` passed.
