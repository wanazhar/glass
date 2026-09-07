---
id: native-engine-161
scope: glass-browser/native-engine/cascade-border-current-color
status: complete
depends-on: [native-engine-160]
---

# Native bounded physical `border-color: currentColor`

## Objective

Resolve the exact case-insensitive `currentColor` keyword for standalone
physical `border-color`, `border-top-color`, `border-right-color`,
`border-bottom-color`, and `border-left-color` declarations. Preserve the
existing fixed-color grammar, private per-side cascade stream, physical
shorthand expansion, and public `NativeColor`/border-artifact surfaces.

This slice is intentionally limited to color-only physical border properties.
It does not make complete `border: Npx style currentColor` values valid, and it
does not generalize `currentColor` to backgrounds, text decorations, arbitrary
CSS color syntax, or other properties.

## Context

`native-engine-151` added a private per-side color stream for physical
`border-color` and its four physical color longhands. The stream currently
accepts only the existing bounded named, hexadecimal, and functional RGB color
grammar. The native engine already resolves an element's inherited `color`
before descendants are laid out, so a bounded `currentColor` value can be
substituted at the existing computed-style boundary without changing layout,
display-list, raster, capture, hit-test, or semantic owners.

Normative references:

- <https://www.w3.org/TR/css-color-4/#currentcolor-color>
- <https://www.w3.org/TR/css-backgrounds-3/#border-color>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-160.md`
- `docs/plan/tasks/native-engine-151.md`

## Contract

### Declaration and cascade state

- The five physical color property names accept the existing bounded color
  grammar plus one exact, case-insensitive `currentColor` token. The
  `border-color` shorthand expands one to four color/current-color tokens
  using the existing physical top/right/bottom/left mapping. Each physical
  color longhand accepts one color/current-color token.
- Standalone case-insensitive `revert-layer` remains supported. Mixed
  `revert-layer` tokens, empty values, malformed expansions, unsupported color
  syntax, other CSS-wide keywords, and invalid complete border values remain
  typed unsupported-value diagnostics and preserve the existing valid-before-
  invalid declaration behavior.
- `currentColor` resolves to the element's computed `color` after its local
  color cascade and inherited fallback have resolved. If the bounded direct
  computation has no inherited color, the existing native black initial color
  is used for substitution; the public optional `color()` result is unchanged.
- The color candidate stream remains independent from width and style. A
  `currentColor` declaration changes only the selected border color; it does
  not invent missing width/style or paint a zero-width border. Layer priority,
  specificity, source order, unlayered/inline precedence, same-block
  declaration order, repeated rollback, and valid-before-invalid preservation
  remain authoritative.

### Existing owners preserved

- `NativeBorderColorValue::CurrentColor` is private declaration/cascade state
  only. Resolved computed borders continue to expose `NativeColor`; no public
  keyword, enum variant, diagnostic payload, display-list field, raster schema,
  dependency, feature default, or crate boundary changes.
- Resolved physical colors continue through existing border width/style
  composition, box-model insets, display-list commands, rounded masks,
  clipping, opacity, viewport projection, capture, software raster,
  point-hit, and semantic/source-order projection.
- Complete `border` and physical border shorthands continue to require a
  bounded concrete color. Complete `Npx style currentColor`, arbitrary
  omitted-component defaults, CSS-wide reset machinery, logical sides,
  gradients, system colors, color spaces, percentages, animation, multiple
  origins, `!important` inversion, collapsed-table conflict resolution, and
  browser-wide CSS color/border conformance remain outside the boundary.

The slice remains fixture-relative, horizontal-tb, integer-pixel, non-table,
and feature-gated behind `native-engine`.

## Tradeoffs

- A private enum preserves the distinction between a literal color and a
  deferred `currentColor` reference until the element's own color is resolved,
  without leaking CSS syntax into stable protocol or artifact types.
- Resolving at computed-style construction reuses the existing inherited color
  walk and avoids a second paint-time lookup. It deliberately does not model
  cycles or the full CSS-wide/inherited color grammar because `color` itself
  remains in the existing bounded literal-color surface.
- Supporting only standalone physical color properties keeps the parser and
  cascade change small and independently testable. Complete border shorthand
  `currentColor` is left as an explicit next design rather than being silently
  accepted with incomplete component semantics.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive `currentColor` parsing and one-to-four-value physical
  `border-color` expansion, all four physical color longhands, concrete color
  preservation, standalone `revert-layer`, and typed rejection of mixed,
  malformed, CSS-wide, and complete-shorthand forms;
- substitution from an explicit local `color`, inherited parent color, and the
  bounded black initial fallback;
- independent width/style versus color ownership, named-layer/specificity/
  source-order/inline precedence, same-block order, repeated rollback,
  valid-before-invalid preservation, and no public keyword leakage; and
- physical border geometry, display-list color, decoded raster, clipping,
  capture, point-hit testing, semantic/source order, focused feature checks,
  full native integration/library tests, strict affected-package Clippy,
  rustdoc, paired-crate check/build, formatting, documentation, final static
  gates, and bounded regenerable-target cleanup. Remote CI remains unclaimed
  until an explicitly authorized push.

## Implementation

Implemented in `299de40d` (`feat(native-engine): resolve currentColor borders`)
with the test-lint follow-up `2042fb3e` and design checkpoint `c5a58f29`. The
private `NativeBorderColorValue` enum now carries either a concrete
`NativeColor` or deferred `CurrentColor` through the existing per-side cascade
stream. Computed-style construction resolves the element's local color or
inherited color first, substitutes the bounded black initial color when direct
computation has no inherited color, and projects only concrete `NativeColor`
values into `NativeBorder`. Complete border shorthands remain concrete-color
only; public computed/artifact schemas and all existing border consumers are
unchanged.

## Evidence

Local certification passed on 2026-09-07 UTC:

- `cargo fmt --all` and `git diff --check` passed.
- Feature-enabled locked `glass-browser` check passed in the isolated
  regenerable target `/tmp/glass-161-focused` with
  `RUST_MIN_STACK=16777216`.
- Focused border parser/cascade units passed 2/2. An initial substring filter
  was mistakenly combined with `--exact` and ran zero tests; it is not counted
  as evidence. The corrected non-exact filter passed the two intended tests.
- The targeted `currentColor` integration test passed 1/1 with 198 filtered;
  the full native integration suite passed 199/199.
- The feature-enabled library suite passed 967 tests with 1 ignored. Strict
  affected-package Clippy (`--all-targets --all-features --locked -D warnings`)
  and feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- Locked `glass-dev` check and build passed in the same isolated target.
  Both configured debug binaries were present there: `glass` and
  `glass-browser`.
- The final workspace validator passed with
  `RUST_MIN_STACK=33554432`; its TUI suite passed 15/15 and its development
  runtime suite passed 4/4. This stack setting is required for the known
  default-stack overflow in the pre-existing
  `cli::args::tests::agent_readiness_commands_are_explicit` test.
- Final static truth passed: version sync at 0.3.14; feature parity 14
  capabilities across 4 targets; TUI 15 implementation keys/63 documentation
  markers; documentation depth 93 guides/19 contracts; reliability 6
  scenarios across 4 targets; public read-only adapters 5; Web IR 8
  fixtures/8 scenarios/11 categories; release documentation 575 Markdown
  documents, 83 current-version documents, 57 previous-version hits, 665
  semantic-audit hits, and 0 current-claim failures; documentation coverage
  passed at 575 Markdown files, 345 full-product MCP tools (100
  browser-only), 17 examples, and 22 public modules.

The focused and full integration coverage exercises case-insensitive
`currentColor` shorthand expansion, all physical color longhands, local and
inherited substitution, black fallback, layer rollback, same-block order,
valid-before-invalid preservation, complete-shorthand rejection, concrete
public/artifact projection, border geometry, display-list/raster/capture,
point-hit, semantic visibility, and diagnostic behavior. No public keyword or
unsupported raw CSS text is exposed.

## Cleanup evidence

- After all implementation, test, documentation, coverage, and workspace gates
  passed, `/tmp/glass-161-focused` was verified as a real regenerable
  directory with no active Cargo/Rust consumer or open handle. It contained
  5,431,152,640 bytes, 9,070 files, and 1,185 directories.
- The exact target was removed with bounded same-filesystem
  `find -P /tmp/glass-161-focused -xdev -depth -delete`, and its absence was
  verified.
- Free space on `/tmp` rose from 77,410,668,544 to 82,841,812,992 bytes,
  reclaiming 5,431,144,448 bytes. The directory's `du` usage was
  5,431,152,640 bytes; the 8,192-byte difference is filesystem accounting.
- The final workspace validator target was separately verified as a real
  regenerable directory with no active Cargo/Rust consumer or open handle. It
  contained 5,040,037,888 bytes, 6,215 files, and 694 directories. The named
  `/tmp/glass-161-workspace.log` report contained 162,831 bytes. Both exact
  paths were removed with bounded same-filesystem `find -P` deletion and their
  absence was verified. Free space rose from 77,795,774,464 to
  82,835,935,232 bytes, reclaiming 5,040,160,768 bytes; the difference from
  the inventoried path sizes is filesystem accounting and the named report's
  removal.
- No repository `target/`, focused target, or named workspace report remains.
- Remote CI, push, release, tag, registry publication, and browser-parity
  claims remain unmade because the checkout is local-only.
