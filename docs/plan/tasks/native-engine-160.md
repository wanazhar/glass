---
id: native-engine-160
scope: glass-browser/native-engine/cascade-border-none-complete
status: complete
depends-on: [native-engine-159]
---

# Native bounded complete `border: Npx none color`

## Objective

Accept the bounded complete-value `Npx none color` form for `border`,
`border-top`, `border-right`, `border-bottom`, and `border-left`, with
case-insensitive `none`. Preserve the exact omitted-component `none` and
`hidden` forms, the complete painted `Npx style color` grammar, the complete
`Npx hidden color` form, and standalone `revert-layer`.

The complete none form must carry its declared width and color through the
existing private component streams while mapping only its style to the private
`NativeBorderStyleValue::None` sentinel. A winning none style must suppress
current non-table paint and retain the existing no-side/zero-width public
result. No public `None` paint variant or display-list schema is permitted.

## Context

The native engine now accepts exact omitted-component `border:none` and
`border:hidden`, complete painted values, and complete hidden values. Its
complete parser still rejects `2px none red` because the public
`NativeBorderSide` stores painted styles only. The existing private none style
sentinel already owns no-paint cascade behavior. This slice adds a private
complete-none declaration rather than leaking a no-style value into public
computed values or raster commands.

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-159.md`
- `docs/plan/tasks/native-engine-158.md`
- `docs/plan/tasks/native-engine-157.md`

## Contract

### Declaration and cascade state

- The five physical border shorthand properties accept the existing complete
  bounded `Npx style color` form for public painted styles, the exact
  case-insensitive omitted-component `none` or `hidden` token, the complete
  bounded `Npx none color` or `Npx hidden color` form, or standalone
  case-insensitive `revert-layer`.
- A complete none declaration is represented by a private declaration-only
  value carrying its bounded width and parsed color. Its style projection is
  `NativeBorderStyleValue::None`; its width and color projections are the
  declared values at the same declaration order. It does not add a public
  `NativeBorderStyle::None` variant.
- A winning complete none style blocks current non-table paint and resolves to
  the existing no-side/zero-width result. Width and color remain typed private
  candidates for future table/conflict work, but cannot resurrect a painted
  side in current computed-style composition. A later bounded `revert-layer`
  can expose an existing lower painted component.
- Complete none values with missing width/color, extra tokens, unsupported
  styles, other CSS-wide keywords, empty values, malformed dimensions/colors,
  style-only values other than the exact omitted-component tokens, and
  unsupported CSS syntax remain typed unsupported-value diagnostics. Raw CSS
  text is not added to diagnostics or public protocol output.
- Named-layer priority, specificity, source order, unlayered/inline
  precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative for all three
  component streams.

### Existing owners preserved

- Exact omitted-component `none` continues to use its existing private
  declaration and does not invent width/color candidates. Exact omitted
  `hidden`, complete hidden, and complete painted borders remain unchanged.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add collapsed-table conflict resolution, logical
  sides, arbitrary omitted defaults, CSS-wide reset machinery, `currentColor`,
  gradients, border-image, animation, multiple origins, `!important`
  inversion, or browser-wide CSS border conformance.

## Tradeoffs

- A private complete-none value preserves the declared width/color provenance
  needed by the existing component-cascade model and future table work without
  expanding public enums or claiming that current non-table rendering paints
  a none border.
- The parser remains exact and bounded: it adds `Npx none color`, not
  arbitrary omitted combinations, default-width/current-color inference,
  CSS-wide keyword semantics, or a general border conflict model.
- Retaining width/color privately while suppressing them at current computed
  style is deliberate. The private/public separation must be proven by tests;
  the current renderer must never emit a border command for a winning none
  style.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- case-insensitive complete `Npx none color` parsing for the shorthand and
  all four physical properties, preservation of complete painted/omitted and
  complete hidden values, standalone `revert-layer`, and typed rejection of
  incomplete, mixed, malformed, CSS-wide, and unsupported inputs;
- private complete-none width/style/color separation, declaration order,
  named-layer/specificity/source-order/inline precedence, same-block order,
  repeated rollback, and valid-before-invalid preservation;
- current no-side/zero-width geometry and absent border commands/raster while
  declared width/color remain private, plus clipping, point-hit, capture,
  semantic/source order, and unchanged public schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

Implemented in `a880c570` (`feat(native-engine): support complete none
borders`), from the docs-first design checkpoint `bf1a7236`. The private
`NativeBorderDeclaration` wrapper now carries complete none values with their
declared width and color. Complete none projects width and color into the
independent private component candidates while projecting none style to
`NativeBorderStyleValue::None`; current non-table composition suppresses paint
without changing public computed or artifact schemas. Exact omitted-component
none/hidden, complete hidden, and complete painted values remain unchanged.

## Evidence

Local certification passed on 2026-09-07 UTC:

- `cargo fmt --all` and `git diff --check` passed.
- Feature-enabled locked `glass-browser` check passed with
  `RUST_MIN_STACK=16777216` and `CARGO_TARGET_DIR=/tmp/glass-160-focused`.
- Focused border library selection passed: 17 passed, 951 filtered.
- Focused complete-none artifact integration test passed: 1 passed, 197
  filtered.
- Full `native_engine` integration suite passed: 198 passed, 0 failed.
- Feature-enabled library suite passed: 967 passed, 0 failed, 1 ignored.
- Strict affected-package Clippy (`--all-targets --all-features -- -D
  warnings`) passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- Locked `glass-dev` check/build passed in the isolated target; both debug
  binaries were present: `glass` and `glass-browser`.
- The direct repository workspace validator first reproduced the pre-existing
  default-stack overflow in `cli::args::tests::agent_readiness_commands_are_explicit`;
  the identical `bash scripts/check-rust-workspace.sh` validator then passed
  with `RUST_MIN_STACK=33554432` and output redirected to a bounded temporary
  log. This is a test-environment stack requirement, not a native-engine
  failure.
- Static truth passed: version sync at 0.3.14; feature parity 14 capabilities
  across 4 targets; TUI 15 implementation keys/63 documentation markers;
  documentation depth 93 current guides/19 substantive contracts; reliability
  6 scenarios across 4 targets; public read-only adapters 5; Web IR 8
  fixtures/8 scenarios/11 categories; release documentation passed at 574
  Markdown documents, 83 current documents, 57 previous-version hits, 665
  semantic audit hits, and 0 current-claim failures; documentation coverage
  passed at 574 Markdown files, 345 full-product MCP tools (100 browser-only),
  17 examples, and 22 public modules.
- No remote CI, push, release, registry publication, browser parity, or
  security-boundary claim is made.

Pre-cleanup inventory after all validation: `/tmp/glass-160-focused` was an
exact real directory of 5,429,440,512 bytes across 9,070 files and 1,185
directories; repository `target/` was an exact real directory of 5,039,251,456
bytes across 6,215 files and 694 directories. No active Cargo/compiler process
or open handle targeted either directory. Both are regenerable build output;
only these exact paths may be removed. Both directories were removed with
bounded `find -P <exact-path> -xdev -depth -delete`; the paths are absent and
free space increased from 72,378,933,248 to 82,847,617,024 bytes
(10,468,683,776 bytes reclaimed). The temporary workspace log and process
inventory created for this certification were also removed by exact filename.
