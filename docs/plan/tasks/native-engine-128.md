---
id: native-engine-128
scope: glass-browser/native-engine/cascade-layers-text-decoration-line-revert-layer
status: complete
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

Implemented at `15fc761c` in
`crates/glass-browser/src/browser/native_engine/css.rs` and
`crates/glass-browser/tests/native_engine.rs`. The private
`NativeTextDecorationDeclaration` carries either the existing bounded
`TextDecorationValue` or `RevertLayer`; stylesheet and inline candidates reuse
the bounded 15-layer registry and unlayered bucket; resolution blocks only the
winning layer until a concrete value or the inherited/root fallback is found.
Both `text-decoration-line` and `text-decoration` feed the existing shared
three-bit line-state owner. The public computed style, immutable text command,
display list, capture, raster geometry, dependencies, features, and crate
boundaries remain unchanged.

## Verification

The focused gate covered:

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

The established native feature library/integration suites, locked two-crate
tests, strict Clippy, no-default-feature Clippy, warnings-denied rustdoc,
locked package/fuzz/deny/audit checks, and repository static
documentation/reliability/adapter/Web IR validators were also run using an
isolated target; all commands, counts, durations, warnings, commits, issue
synchronization, and cleanup evidence are recorded here. Remote CI, browser
parity, release, registry publication, and a third crate remain outside local
task evidence unless separately executed and verified.

### Results

- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-128-focused cargo test -p glass-browser --features native-engine --lib text_decoration --locked -- --nocapture`: 18 passed, 0 failed in 7m11s.
- `CARGO_TARGET_DIR=/tmp/glass-128-focused cargo test -p glass-browser --features native-engine --test native_engine native_text_decoration_line_revert_layer_preserves_shared_line_owner --locked -- --nocapture`: 1 passed, 0 failed in 26.43s after correcting an inline-overline raster sample in the test; the implementation was not changed by that assertion correction.
- Full native feature suites passed: 926 library tests, 1 ignored, and 165 native integration tests.
- The final locked `scripts/check-rust-workspace.sh test` passed for both crates and all targets/features. The browser all-target matrix passed, including 165 native integration tests; `glass-dev` reported 365 unit tests, 4 integration tests, and 15 PTY tests. A first cold invocation had one transient rust-analyzer diagnostic-probe failure; the same probe passed alone, the complete `glass-dev` matrix passed, and the exact wrapper rerun passed.
- `CARGO_TARGET_DIR=/tmp/glass-128-focused scripts/check-rust-workspace.sh clippy` passed with warnings denied (browser 8m16s; dev 7m04s). No-default-feature browser Clippy passed in 4m54s. Warnings-denied workspace rustdoc passed in 3m32s.
- Locked packaging passed: `glass-browser` packaged 196 files/5.1 MiB (975.7 KiB compressed), `glass-dev` packaged 69 files/2.6 MiB (512.2 KiB compressed), and packaged dependency validation resolved `glass-browser` exactly at `0.3.14`.
- `cargo fetch --manifest-path fuzz/Cargo.toml --locked` plus locked offline all-target fuzz checking passed in 8m39s.
- `cargo deny check` and `cargo audit` passed. Existing warnings remain: duplicate dependency versions, unmaintained `bincode` and `yaml-rust`, the allowed `lru` advisory, and yanked `chacha20`.
- Static documentation, release-truth, coverage, reliability, adapter, Web IR,
  version/feature, formatting, and script validators all passed after this
  closeout synchronization: release truth measured 542 Markdown documents, 83
  current-version documents, 57 previous-version references, 639 semantic hits,
  and zero current-claim failures; the release-documentation unit suite passed
  9/9; coverage measured 542 Markdown files, 345 full-product MCP entries (100
  browser-only), 17 examples, and 22 public modules; TUI measured 15
  implementation help keys and 63 documentation markers; documentation depth
  measured 93 routed/audited guides and 19 substantive contracts; reliability
  measured 6 scenarios across 4 targets; public read-only adapters measured 5;
  Web IR measured 8 fixtures, 8 scenarios, and 11 categories. `cargo fmt
  --all -- --check` and `git diff --check` passed.

## Cleanup

All expensive gates used the isolated task target `/tmp/glass-128-focused`.
Immediately before deletion it measured 9,294,659,584 bytes across 14,466
files. The exact validator reports measured 185,663 bytes total:
`/tmp/glass-128-release-documentation.json` (176,955 bytes),
`/tmp/glass-128-reliability.json` (3,482 bytes),
`/tmp/glass-128-adapters.json` (1,418 bytes), and
`benchmarks/results/.glass-128-web-ir.json` (3,808 bytes). An accidental
`./--help` report emitted by a help probe measured 1,418 bytes and was removed
with the same bounded cleanup. The 155 exact PID-1198025/PID-1233978 scratch
entries measured 1,335,296 bytes. No Cargo/rustc/rustdoc/Clippy/fuzz/test
writer was running; recursive `lsof` checks found no open handle for the task
target or inspected exact scratch roots. Bounded `find -P <exact-path>
-xdev -depth -delete` operations removed only these candidates; no process was
terminated and no source, fixture, durable data, active process, or unrelated
temporary entry was touched. Available bytes moved from 76,142,186,496 to
85,438,488,576, a measured delta of 9,296,302,080 bytes. The exact target,
reports, scratch paths, repo-local `target/`, and top-level `/tmp/target/` are
absent. The three pre-existing Glass processes were preserved.

## Certification

Pending final exact-output cleanup and issue synchronization. The behavioral
implementation and local acceptance gates are complete; this section is closed
only after the cleanup measurements and authenticated issue evidence are
recorded below.
