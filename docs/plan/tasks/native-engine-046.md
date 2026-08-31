---
id: native-engine-046
scope: glass-browser/native-engine/nowrap-whitespace
status: done
depends-on: [native-engine-045]
---

# Native bounded `white-space: nowrap`

## Objective

Add inherited `white-space: nowrap` to the existing bounded text-flow model.
Collapse supported source whitespace as in `normal`, prevent soft wrapping for
the resulting text flow, and expose measured one-line overflow through the
045 root horizontal scroll path.

## Contract

The CSS parser accepts `white-space: nowrap` in stylesheet and inline
declarations. The value inherits through the existing DOM parent style walk.
`nowrap` uses the current collapsed fixed-cell text policy: author whitespace
is reduced to the existing bounded separator behavior, while supported text
characters retain the current fixed-cell width. Unlike `normal`, the flow does
not soft-wrap at the available content width.

Visible `nowrap` text runs retain their document-space origins and are included
in the existing measured `content_width`; the root `max_scroll_offset().x`
therefore reaches the right side of a wide one-line run. Layout, viewport
rectangle projection, point hit testing, display-list translation, raster
replay, scroll effects, and history continue to consume the same state owners.

The mode does not add preserved spaces/tabs/newlines, word-breaking policy,
font metrics, nested scroll containers, scrollbars, smooth/keyboard/snap
scrolling, axis-specific overflow, or browser line-breaking parity. Hidden,
non-layout, and truncated content does not create a new scroll extent beyond
the existing visible-layout measurement rules.

## Tradeoffs

- Reusing the collapsed text policy keeps the feature deterministic and small,
  but does not reproduce browser-preserved whitespace semantics.
- Disabling only soft wrapping makes the 045 root horizontal owner useful for
  ordinary text, but leaves nested containers and scrollbar behavior for later
  tasks.
- No new dependency is introduced; fixed-cell measurement remains an explicit
  native limitation.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- parser/cascade tests accept inherited and inline `nowrap` while rejecting
  unrelated values;
- one-line collapsed text remains unwrapped, reports measured overflow, and
  projects through horizontal layout, hit-test, display-list, and raster paths;
- direct and dispatcher scroll actions clamp and report revisions/effects as
  in 045, including edge no-ops and history restoration;
- hidden/non-layout content, preserved whitespace modes, malformed input, and
  vertical/diagonal scroll behavior retain their existing boundaries;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implementation and focused validation are complete locally. The code and
synchronized docs are committed as a focused Conventional Commit; the issue
#40 checkpoint follows before the next slice.

Validation evidence:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused nowrap integration test: 1 passed;
- full native integration suite: 59 passed;
- native-engine module unit suite: 42 passed;
- strict Clippy passed with all features and with no default features;
- dispatcher horizontal-capture coverage also passes with a `nowrap` fixture;
- synchronized README, feature, SDK, architecture, plan, and task docs now
  describe bounded inherited `white-space: nowrap` and root horizontal scroll;
- documentation depth, release-documentation, version-sync, and
  feature-parity validators passed: 93 current guides, 460 Markdown documents,
  0 current-claim failures, synchronized 0.3.14 versions, and 14 capabilities
  across 4 targets.
