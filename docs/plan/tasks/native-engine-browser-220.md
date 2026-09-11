# Native engine browser slice 220: responsive image source selection

Status: completed locally.

## Objective

Make common responsive images behave as images rather than always fetching the
legacy `src` fallback. Candidate choice must be deterministic, viewport-aware,
and owned by the same Rust document/resource lifecycle that owns intrinsic
dimensions, `currentSrc`, painting, and events.

## Contract

- `img[srcset]` accepts a bounded list of either density candidates (`1x`,
  `2x`) or width candidates (`400w`, `800w`). Mixed descriptor families,
  malformed candidates, duplicate descriptors, and empty candidates are
  ignored or rejected as one invalid selection, preserving the `src` fallback.
- Density candidates choose the nearest usable density for the configured
  device scale, preferring the smallest candidate at or above the target and
  the largest available candidate when all candidates are below it.
- Width candidates use the configured viewport width by default. A bounded
  `sizes` list can select `px` or `vw` source lengths under `max-width`,
  `min-width`, or exact `width` media conditions; the selected width follows
  the same nearest-at-or-above rule with a largest-candidate fallback.
- The selected raw source is the source requested by the content worker and is
  reflected in `currentSrc` after URL resolution against the active document.
  Successful and failed attempts retain the selected source identity, and
  intrinsic dimensions/`complete` state correspond to that selected attempt.
- Script writes/removal of `src`, `srcset`, or `sizes` invalidate stale image
  state. The existing content-process policy, cache, typed document-wire, and
  terminal load/error event paths then handle the newly selected source.
- Content-wire validation accepts only a selected source declared by `src` or
  a valid `srcset` candidate; resource bytes and loader details remain behind
  the existing bounded transfer surface.

## Implementation

`dom.rs` owns candidate parsing, simple bounded `sizes` evaluation, viewport/
device-scale selection, current-source projection, and source-identity checks
for content-wire resources. `content_process.rs` passes the configured
viewport through initial and mutation image loading. `javascript.rs` reflects
`srcset`, `sizes`, and the selected-source `currentSrc`, and invalidates image
state when any source-selection input changes. Integration witnesses cover
both descriptor families, URL-reflected current source, real HTTP selection,
and a second fetch after `sizes` mutation.

## Tradeoffs and follow-up

The selector deliberately keeps a small deterministic grammar: it does not
implement `<picture>` source-set/media/type precedence, complex media queries,
`calc()`/container-query source sizes, preload priorities, art direction, or
browser-grade subpixel candidate selection. Candidate bounds prevent an
attribute from turning one image into an unbounded request fan-out, while the
`src` fallback keeps malformed responsive metadata recoverable. Those omitted
surfaces remain explicit Issue #40 promotion work rather than silent CDP
delegation.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_images_ -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_reloads_the_selected_srcset_candidate_after_sizes_mutation -- --nocapture` (1 passed, 0 failed)
- Existing image lifecycle/load/error witnesses: 3 passed, 0 failed

Implementation checkpoint: `acb3f476`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
