---
id: native-engine-030
scope: glass-browser/native-engine/descendant-selectors
status: done
depends-on: [native-engine-029]
---

# Native bounded descendant-selector matching

## Objective

Extend the existing one-compound CSS selector subset with a bounded descendant
combinator. Real fixture styles commonly scope a control or presentation rule
through a containing region; rejecting every selector with whitespace makes the
current stylesheet path needlessly disconnected from the DOM hierarchy.

## Contract

The native stylesheet parser accepts one to eight existing compound selectors
joined by descendant whitespace. The rightmost compound must match the target
element; each preceding compound must match an ancestor in order, with any
number of intervening ancestors allowed within the bounded DOM depth. Existing
universal/type, ID, class, and attribute-presence/exact-value atoms retain their
matching and specificity rules, and specificity is the sum of the compounds.

Matching is used by the same stylesheet cascade and visibility/layout paths as
the current one-compound grammar. It must not create a second DOM owner,
change inline precedence, or expose CSS selectors through the stable backend
contract.

Direct-child (`>`), adjacent-sibling (`+`), general-sibling (`~`), pseudo,
functional, namespace, and other selector forms remain unsupported and produce
the existing bounded diagnostics. Selector parsing, ancestor traversal, and
diagnostic detail remain bounded; malformed or over-limit input must not
partially commit a rule or mutate a document.

## Tradeoffs

- Ancestor matching makes scoped fixture styles useful and exercises the real
  DOM ownership chain, but it is still not general selector conformance.
- A fixed maximum of eight compounds and the existing DOM-depth limit keep
  matching deterministic, while deeply complex selectors remain explicit
  unsupported input.
- Descendant matching can inspect ancestors for every computed style, so the
  implementation favors a bounded, readable traversal over a style cache that
  could introduce invalidation or ownership bugs.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- parser/unit coverage for valid descendant chains, specificity, bounds, and
  rejected combinators;
- native integration coverage proving descendant styles feed the real DOM
  cascade, visibility, layout, and paint path;
- malformed/unsupported selector diagnostics remain bounded and sanitized;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented in the `feat(native-engine): support bounded descendant selectors`
checkpoint. The native stylesheet path now matches one-to-eight compound
selector chains through the owned DOM ancestry, sums compound specificity, and
feeds the existing cascade, visibility, layout, and paint consumers without
changing the stable backend contract.

- Focused descendant-selector integration: 1 passed.
- Native integration suite: 43 passed.
- Native unit suite: 36 passed.
- Strict default-feature and `native-engine` all-target Clippy gates pass with
  warnings denied.
- Full locked `glass-browser` all-target/all-feature matrix: 818 unit tests
  passed, 1 ignored; all integration and example targets passed, including 43
  native integration tests.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Documentation coverage, depth, and release-truth validators pass after the
  task synchronization; no current-claim failure was introduced.
- No new dependency, third crate, stable transport capability, automatic
  backend path, general CSS conformance, or browser-parity claim was
  introduced. Direct-child/sibling/pseudo/functional/namespace selectors
  remain explicit unsupported input.
