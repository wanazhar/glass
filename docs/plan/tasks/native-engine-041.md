---
id: native-engine-041
scope: glass-browser/native-engine/percent-decoded-fragment-targets
status: done
depends-on: [native-engine-040]
---

# Native bounded percent-decoded fragment targets

## Objective

Make percent-encoded local fragment identifiers resolve to visible element
`id` attributes through the existing bounded fragment-navigation and history
scroll owner, without adding a general URL decoder, `name` anchors, or text
fragments.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

For a successful local navigation whose URL contains a non-empty fragment,
the engine decodes bounded `%HH` byte sequences in the fragment as UTF-8
before performing the existing exact, case-sensitive `id` lookup. Unescaped
UTF-8 bytes remain unchanged and `+` remains a literal plus; form-style
`+`-to-space conversion is not part of fragment handling. A decoded fragment
must match exactly one visible element `id` to affect root scrolling. Missing,
empty, hidden, non-layout, or duplicate decoded targets preserve the current
scroll offset while the existing URL/revision/history transition remains
successful.

Malformed or non-UTF-8 percent sequences are treated as an unresolved target,
not as a partial match and not as an error. The existing bounded URL-size
validation still limits the input before decoding. `name` anchors, text
fragments, duplicate-id recovery, smooth/nested/horizontal scrolling, and
browser URL-parsing parity remain outside this slice.

The behavior is reachable through direct navigation, explicit Rust history
traversal, and semantic local link activation. Resource lookup continues to
remove the raw fragment, and no network, filesystem, or transport-level
navigation capability is added.

## Tradeoffs

- Decoding only the fragment identifier makes common spaces and UTF-8 IDs
  usable while keeping the existing resource URL and history representation
  stable.
- Invalid sequences fail closed to the existing unresolved-target behavior;
  this avoids inventing a URL-error policy for a navigation that the local
  loader already accepted.
- Duplicate detection occurs after decoding, so raw spellings that converge
  to the same ID do not bypass the existing duplicate-target safety rule.
- The decoder is a small bounded helper rather than a new dependency; this
  keeps the default build graph unchanged but does not claim full URL or HTML
  character-reference conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, feature, SDK, README,
  experimental-capability, and plan docs

## Verification

- percent-encoded spaces and UTF-8 bytes resolve exact visible local IDs;
- literal `+` remains distinct from a space and duplicate decoded IDs remain
  unresolved;
- malformed and invalid-UTF-8 fragments preserve scroll without mutating the
  successful navigation contract;
- existing raw fragment, fixture-relative, missing/hidden, history, and
  dispatcher link behavior remains green;
- native integration and unit suites, strict Clippy, formatting, and
  whitespace checks; and
- documentation coverage, depth, release-truth, version-sync, and
  feature-parity validators.

## Completion evidence

Implemented in the local checkpoint that adds bounded UTF-8 percent decoding
to fragment-target lookup and covers encoded spaces, UTF-8 IDs, literal `+`,
duplicate decoded IDs, malformed escapes, invalid UTF-8, link activation, and
history-safe scroll behavior.

Validation passed:

- focused config unit: 1 passed;
- focused native integration: 1 passed;
- full native integration: 53 passed;
- native-engine module units: 39 passed;
- locked all-target/all-feature browser matrix: 821 library tests passed, 1
  ignored, and every integration/example target passed;
- locked native-feature doctests: 4 passed;
- strict all-feature and no-default-feature Clippy passed;
- complete workspace contract: 821 browser library tests passed with 1
  ignored, 365 `glass-dev` unit tests passed, 4 development-runtime tests
  passed, and 15 PTY tests passed;
- documentation coverage, depth, release-truth, version-sync, and
  feature-parity validators passed; and
- formatting and whitespace checks passed.

The checkpoint is committed locally and recorded on issue #40 before the next
slice. Remote CI remains pending because this branch has not been pushed.
