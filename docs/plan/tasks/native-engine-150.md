---
id: native-engine-150
scope: glass-browser/native-engine/cascade-layers-border-revert-layer
status: done
depends-on: [native-engine-149]
---

# Native bounded physical border `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-149 to
add standalone, case-insensitive `revert-layer` to the existing local
`border`, `border-top`, `border-right`, `border-bottom`, and `border-left`
owners. A rollback must expose the highest-priority lower concrete side value,
or the existing zero-width/no-paint side fallback, while preserving the current
box-model inset, border display-list, capture, raster, point-hit, and
semantic/source-order owners.

## Context

The native engine already parses a bounded physical border grammar and expands
`border` into four independently painted sides. Side-specific solid, dashed,
and dotted values feed the same integer box-model, display-list, software
surface, clipping, and hit-testing owners. The stylesheet and inline paths
currently retain one concrete winner per side, so a higher-priority rollback
cannot expose a lower side value or the zero-side default. This slice adds only
private per-side candidate state and reuses the bounded 15-layer registry and
local resolver.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>
- <https://www.w3.org/TR/css-backgrounds-3/#border-width>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-149.md`
- `docs/plan/tasks/native-engine-014.md`
- `docs/plan/tasks/native-engine-019.md`

## Contract

### Declaration and cascade state

- `border`, `border-top`, `border-right`, `border-bottom`, and `border-left`
  accept one standalone, case-insensitive `revert-layer` token in addition to
  the existing bounded side grammar. Mixed tokens, other CSS-wide keywords,
  malformed shorthands, unsupported styles, and out-of-range dimensions remain
  typed rejections.
- Each matching stylesheet rule and inline declaration contributes private
  candidates to the four physical side owners. A shorthand contributes the
  same rollback or concrete side candidate to every side at its declaration
  position; a side longhand changes only its own candidate. Named-layer
  priority, specificity, source order, unlayered precedence, inline
  precedence, and the existing `!important` stripping remain authoritative.
- A winning rollback blocks only its current bounded layer and resolves each
  side through the highest-priority remaining concrete candidate. Repeated
  rollback continues through lower candidates. With no concrete candidate, the
  existing zero-width/no-paint side fallback remains in force.
- Valid border declarations before malformed declarations in one block retain
  the existing valid candidate. `revert-layer` never reaches the public
  `NativeBorder` value as an unresolved keyword.

### Existing owners preserved

- Resolved sides continue to feed the same outer/content box insets, physical
  border display-list command, rounded mask, ancestor clips, opacity groups,
  viewport projection, point hit testing, capture dimensions, software raster,
  and semantic/source-order projection.
- The selected side's existing width, style, color, pattern anchoring, and
  source-order behavior remain unchanged. A rollback changes only which
  already-supported side value is selected.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded to the current
physical solid/dashed/dotted border grammar. It does not add logical sides,
border-image, gradients, corner-longhand or percentage geometry, other border
styles, animation, multiple origins, or browser-wide CSS border conformance.

## Tradeoffs

- Four fixed candidate arrays make shorthand and longhand ownership explicit
  and let one side roll back without accidentally changing its neighbors.
- Reusing the existing side parser and display/raster owners keeps the build and
  artifact surface small, but it does not model the full CSS border shorthand or
  logical-side cascade.
- The integration regression will assert side-specific rollback through box
  geometry, border commands, decoded pixels, point hits, capture, and semantic
  visibility. This catches owner divergence without claiming browser parity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, unsupported, and out-of-range border values;
- named-layer priority, repeated rollback, shorthand/longhand precedence,
  unlayered/inline precedence, zero-side fallback, valid-before-invalid
  preservation, and `!important` stripping;
- side-specific solid/dashed/dotted selection through box-model geometry,
  display-list, capture, raster, point hit testing, and semantic/source order;
  and
- no false unsupported-value diagnostics or public rollback leakage, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Implementation

Implemented in `1fdbe75d`. The implementation keeps four independent private
side candidate arrays, reuses the bounded 15-layer resolver, preserves existing
shorthand expansion and side-longhand precedence, and adds no public
computed-style, display-list, raster, diagnostic transport, dependency,
feature-default, or crate-boundary changes.

## Evidence

Local certification completed:

- `CARGO_TARGET_DIR=/tmp/glass-150-focused cargo check -q -p glass-browser --features native-engine --tests --locked` passed.
- Focused parser/cascade unit: 1 passed, 960 filtered.
- Focused integration: 1 passed, 187 filtered.
- Full native integration: 188 passed, 0 failed.
- Affected library with `RUST_MIN_STACK=16777216`: 960 passed, 1 ignored.
- Strict affected-package Clippy with `-D warnings` passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- `CARGO_TARGET_DIR=/tmp/glass-150-focused cargo check -q -p glass-dev --locked` and
  `CARGO_TARGET_DIR=/tmp/glass-150-focused cargo build -q -p glass-dev --locked`
  passed.
- `cargo fmt --all` and `git diff --check` passed.
- Static documentation gates passed: 564 Markdown files; 83 current-version
  references; 57 previous-version references; 656 semantic audit hits; 0
  current-claim failures; feature parity 14 capabilities across 4 targets; TUI
  shortcut parity 15 implementation keys and 63 documentation markers; depth
  93 guides and 19 substantive contracts; reliability 6 scenarios across 4
  targets; public read-only adapters 5; Web IR 8 fixtures, 8 scenarios, and
  11 categories.
- Documentation coverage passed: 564 Markdown files, 345 full-product MCP
  tools (100 browser-only), 17 examples, and 22 public modules.
- Remote CI remains unclaimed because the checkout is local-only.

## Cleanup evidence

- After all build, test, documentation, and coverage gates passed, the isolated
  regenerable target `/tmp/glass-150-focused` was audited and removed with
  bounded, same-filesystem deletion. It contained 5,459,222,528 bytes, 9,193
  files, and 1,187 directories; it was a real directory, not a symlink, had no
  active Rust/Cargo consumer, and had no open handles.
- Available space on `/tmp` increased from 78,398,373,888 bytes to
  83,857,588,224 bytes. `/home/ubuntu/work/glass/target` remains absent.
