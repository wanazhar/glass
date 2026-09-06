---
id: native-engine-128
scope: glass-browser/native-engine/cascade-layers-text-decoration-line-revert-layer
status: planned
depends-on: [native-engine-127]
---

# Native bounded `text-decoration-line` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-127 to add
the explicit CSS-wide `revert-layer` keyword to the existing inherited
`text-decoration` line-state owner. Keep the three-bit line representation,
shorthand/longhand shared slot, immutable text command, and software-raster
geometry unchanged.

## Context

The native engine already parses `text-decoration` and
`text-decoration-line` into one declaration-order-aware `TextDecorationValue`
slot. That value represents only `none`, `underline`, `overline`, and
`line-through` combinations. The computed line state is inherited through the
DOM style walk and is consumed by the existing fixed-cell decoration paint
path. This slice adds rollback state beside that existing value; it does not
turn the shorthand into a general multi-component CSS shorthand.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-line-property>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-127.md`

## Contract

### Declaration and cascade state

- `text-decoration-line: revert-layer` and
  `text-decoration: revert-layer` are accepted as one case-insensitive token
  and stored in a private declaration-only state. Both spellings share the
  existing line-state slot, so their declaration order remains the only
  shorthand/longhand interaction represented by this native boundary.
- Concrete line values remain the existing bounded grammar: `none`,
  `underline`, `overline`, `line-through`, and each valid combination of the
  three line tokens. No unresolved keyword reaches `NativeComputedStyle`, an
  immutable text command, capture, or software raster replay.
- Stylesheet candidates use the existing 15 named-layer registry and
  unlayered bucket. First-appearance layer rank precedes specificity; source
  order remains the tie-breaker within one layer; inline declarations remain
  in the unlayered bucket above named layers.
- A winning `RevertLayer` candidate blocks only its current layer and selects
  the highest-priority remaining line candidate. Repeated rollback continues
  through lower named layers and then the inherited fallback; unlayered/inline
  rollback selects the highest named candidate.
- When no candidate remains, the existing inherited line state is used. Root
  fallback remains `none`; a descendant with no local candidate inherits the
  parent's resolved line state through the established DOM walk.
- Unsupported `initial`, `inherit`, `unset`, `revert`, `all`, mixed tokens,
  duplicate line tokens, unknown values, and unsupported CSS-wide forms remain
  typed diagnostics without raw stylesheet echo.

### Existing owners preserved

The resolved three-bit line value continues through the current inherited
style, layout, immutable text command, display-list, capture, and decoded
raster paths. `none` removes all three line pixels; each valid combination
retains its existing underline, overline, and line-through geometry and source
order. Decoration color, style, thickness, skip-ink, skip-spaces,
underline-offset, glyph color, clipping, scrolling, opacity, hit testing, and
semantics remain separate owners and must not change.

Layout, fixed-cell geometry, whitespace, spacing arithmetic, wrapping,
alignment, overflow, clipping, scrolling, source order, and semantic order
remain unchanged. This remains a horizontal-tb, fixed-cell, bounded
software-raster contract and does not claim browser-wide CSS conformance.

## Tradeoffs

- A private declaration enum and layer candidate array extend the proven
  rollback shape without exposing a CSS-wide keyword or changing the compact
  public bitset.
- Supporting both existing spellings is intentional because the parser already
  gives them one declaration-order-aware owner; no additional shorthand
  components are reset or synthesized.
- Inherited fallback is necessary for this property and preserves the current
  root/descendant behavior; it is distinct from the local `None` fallback used
  by native-engine-127 decoration color.
- The slice does not add a generic cascade engine, multiple origins,
  `!important` inversion, layer statements, nested/anonymous/comma layers,
  animations, transitions, script, or browser parity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The implementation must add private line-state declaration storage,
layer-indexed candidate resolution, case-insensitive `revert-layer` parsing,
and parser/cascade plus display-list/raster regressions. It must not change
package dependencies, feature defaults, crate boundaries, the compact public
line-state artifact, or unrelated decoration behavior.

## Verification

The focused gate must cover:

- case-insensitive parsing for both existing spellings and typed rejection of
  unsupported CSS-wide and mixed line-token forms;
- named-layer ordering before specificity, repeated rollback, unlayered and
  inline precedence, inherited descendant resolution, and root `none` fallback;
- declaration-order interaction between `text-decoration` and
  `text-decoration-line` while preserving only the existing shared line-state
  semantics;
- immutable command and decoded-raster evidence for all three line owners,
  including rollback to `none` and rollback to a combination, with style,
  thickness, offset, color, skip-ink, and skip-spaces behavior unchanged.

Then run the established native feature library/integration suites, locked
two-crate tests, strict Clippy, no-default-feature Clippy, warnings-denied
rustdoc, locked package/fuzz/deny/audit checks, and repository static
documentation/reliability/adapter/Web IR validators. Use an isolated target
and record exact commands, counts, durations, warnings, commits, issue
synchronization, and cleanup evidence here. Remote CI, browser parity,
release, registry publication, and a third crate remain outside local task
evidence unless separately executed and verified.

## Cleanup

All expensive gates must use an isolated task target where practical. Before
deleting generated output, verify no Cargo/rustc/rustdoc/Clippy/fuzz/test
writer owns it and no open handle remains. Remove only exact task targets,
reports, scratch entries, and generated evidence created by this task; preserve
source, fixtures, durable data, active processes, and unrelated workloads.
Record the available-byte value immediately before and after deletion and the
measured delta.

## Certification

Pending docs-first design checkpoint. Implementation cannot begin until this
contract is committed and synchronized with issue #40.
