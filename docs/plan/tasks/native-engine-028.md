---
id: native-engine-028
scope: glass-browser/native-engine/css-diagnostics
status: done
depends-on: [native-engine-027]
---

# Native bounded unsupported-CSS diagnostics

## Objective

Make the native engine's intentionally narrow CSS behavior observable. The
current parser ignores unsupported selectors, properties, malformed
declarations, and unsupported values while retaining the rest of a document.
That fallback is useful for local fixtures, but silent omission makes a
rendered or hit-tested result difficult to audit.

## Contract

The parsed native document records a bounded, read-only diagnostic list for
stylesheet and inline-style input. Diagnostics identify one of the following
stable categories:

- unsupported selector syntax;
- unsupported property;
- unsupported value for a known property; or
- malformed CSS declaration/rule.

Each diagnostic carries a bounded source kind, a bounded source offset, and a
sanitized detail token. The detail never copies a raw declaration value,
selector, URL, or stylesheet body. Diagnostics are warnings: the existing
supported declarations still cascade and unsupported input retains the
existing deterministic omission/fallback behavior.

`NativeEngine::diagnostics` returns a revisioned snapshot with an explicit
truncation bit. Navigation prepares the next document and its diagnostics
before commit, so failed navigation preserves the previous document and
diagnostic state. The surface is Rust-native and does not change the stable
`BrowserBackend` evidence schema or add a CLI/MCP capability.

## Tradeoffs

- A diagnostic list makes unsupported CSS reviewable without pretending the
  native engine implements general CSS or changing fixture rendering.
- A fixed list bound prevents hostile or generated stylesheets from creating an
  unbounded memory path, but callers may need to fix the first reported issue
  before later issues become visible.
- Source kind and sanitized tokens are actionable while avoiding raw CSS/value
  echo, but this is not a full CSS parser error-recovery location model.
- Diagnostics are exposed through the explicit native Rust API only; expanding
  the stable transport or CLI would create a separate capability and release
  contract.

## Path

- `crates/glass-browser/src/browser/native_engine/diagnostics.rs`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- stylesheet and inline-style diagnostics identify unsupported selectors,
  properties, values, and malformed rules;
- diagnostics remain bounded and sanitized;
- supported CSS behavior and navigation atomicity remain unchanged;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented in the `feat(native-engine): expose unsupported CSS diagnostics`
checkpoint. The native Rust API now reports bounded, revisioned diagnostics for
unsupported selectors, properties, values, and malformed stylesheet or inline
CSS without echoing raw CSS or changing stable backend evidence.

- Focused diagnostic integration tests: 3 passed.
- Native integration tests: 41 passed.
- Native unit tests: 35 passed.
- Strict default-feature and `native-engine` Clippy gates pass with warnings
  denied.
- Full locked `glass-browser` all-target/all-feature matrix: 817 passed, 1
  ignored; all integration suites passed, including 41 native integration
  tests.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Documentation coverage: 442 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- No new dependency, third crate, stable transport capability, automatic
  backend path, or browser-parity/security claim was introduced.
