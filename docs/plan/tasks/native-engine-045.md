---
id: native-engine-045
scope: glass-browser/native-engine/root-horizontal-scroll
status: done
depends-on: [native-engine-044]
---

# Native bounded root horizontal scrolling

## Objective

Extend the existing explicit root viewport scroll owner to expose bounded
horizontal overflow width and independently clamp horizontal and vertical
scroll deltas. Make wide deterministic local content, especially unwrapped
`white-space: pre` runs, reachable through the already shared layout,
viewport, hit-test, display-list, raster, action, effects, and history paths.

## Contract

`NativeLayoutSnapshot` derives a bounded `content_width` at least as wide as
the configured viewport from visible layout boxes and non-empty, non-truncated
text runs. The snapshot exposes an x component in `max_scroll_offset()` equal
to `content_width - viewport.width`; the existing y component remains derived
from content height. Layout boxes and text runs remain in document coordinates.

`NativeAction::Scroll` accepts signed horizontal and vertical deltas and clamps
each requested coordinate independently to the snapshot's maximum x/y offset.
An edge/no-op action does not advance the revision or append effects. A moved
action advances the revision once, records the full x/y offset in the current
history entry, and emits the existing root scroll effect. Scroll remains
explicit; no semantic target is implicitly brought into view.

The existing viewport rectangle projection, point hit testing, display-list
translation, raster replay, fragment fallback, and history traversal consume
the same two-axis `NativePoint`. Same-document fragment targets continue to
select their existing vertical target with x reset to zero; unresolved targets
retain both coordinates. Nested scroll containers, scrollbars, smooth or
keyboard scrolling, scroll anchoring/snap, axis-specific CSS overflow, and
general browser scrolling parity remain unsupported.

The width calculation is derived from the current visible layout output and
does not create a second geometry owner. Hidden/non-layout content does not
increase the scroll extent. Text remains fixed-cell and rendering remains
logical software output; this slice does not add font metrics, reflow,
scrollbar painting, or physical-pixel behavior.

## Tradeoffs

- Reusing `NativePoint` keeps history and all consumers aligned, but it does
  not model nested scroll state.
- Measuring rendered output makes wide preformatted fixtures reachable without
  broadening CSS overflow semantics, but content whose layout is deliberately
  clipped or wrapped still has no horizontal extent beyond its layout output.
- Independent clamping permits diagonal deltas while preserving deterministic
  no-op/revision behavior at each edge.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- wide visible preformatted content reports bounded `content_width` and a
  non-zero horizontal maximum while ordinary content remains at x=0;
- direct native actions clamp x/y independently, update revision/effects only
  on movement, and restore both coordinates through history;
- viewport rectangles, point hit testing, display-list metadata, raster pixels,
  and backend dispatcher action all follow horizontal projection;
- negative, over-wide, diagonal, edge/no-op, hidden/non-layout, and unsupported
  nested/overflow cases preserve the contract;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implementation and focused validation are complete locally. The code and
synchronized docs are committed as a focused Conventional Commit, and the
evidence is recorded on issue #40.

Validation evidence:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- horizontal and dispatcher projection tests passed;
- full native integration suite: 58 passed;
- native-engine module unit suite: 42 passed;
- strict Clippy passed with all features and with no default features;
- documentation depth, release-documentation, version-sync, and
  feature-parity validators passed; binary documentation coverage remained
  green from the 044 gate (458 Markdown files, 345 full-product MCP tools,
  17 examples, and 22 public modules).
