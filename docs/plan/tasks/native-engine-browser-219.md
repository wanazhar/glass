# Native engine browser slice 219: image accessibility semantics

Status: completed locally.

## Objective

Expose the standard accessibility projection for native `img` elements so
browser callers can discover images and their author-provided alternative
text through the existing semantic-node and locator surfaces.

## Contract

- An `img` element without an overriding supported `role` projects the
  standard `img` semantic role.
- When present, `alt` supplies the accessible name after the existing
  `aria-label` and `aria-labelledby` precedence rules.
- `alt=""` intentionally keeps the `img` role while exposing an empty name;
  decorative images are therefore distinguishable from non-image nodes without
  inventing visible text.
- The projection is revision-bound through the existing `NativeSemanticNode`
  surface and participates in the existing `role=img[name=...]` locator grammar.
- The shared semantic owner applies the same rule to local documents and
  validated content-process snapshots. No loader, decoder, or page-secret data
  is added to the semantic surface.

## Implementation

`dom.rs` adds `img` to the supported-role vocabulary, maps the native `img`
element to that role, and resolves `alt` through the existing bounded name
collapse path. The integration witness covers a named image and a decorative
empty-`alt` image after asynchronous native initialization and verifies both
semantic fields through the public engine API.

## Tradeoffs and follow-up

This slice addresses the first image accessibility gap without claiming the
full platform accessibility tree. Figure/figcaption relationships, image-map
and area semantics, `input type="image"` name/coordinate behavior, accessible
description computation, and complete Web IDL/AXTree parity remain follow-up
surfaces. The role vocabulary remains intentionally bounded and unknown ARIA
roles continue to fail closed through the existing locator validation.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_image -- --nocapture` (2 passed, 0 failed)

Implementation checkpoint: `641d18a0`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
