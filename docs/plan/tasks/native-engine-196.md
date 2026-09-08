---
id: native-engine-196
scope: glass-browser/native-engine/dimension-inheritance
status: complete
depends-on: [native-engine-195]
---

# Native dimension explicit inheritance

## Objective

Add bounded explicit `inherit` handling to the existing local dimension
cascade owners without introducing implicit dimension inheritance, unresolved
used-value state, or a general CSS-wide keyword engine.

## Contract

- `width`, `height`, `min-width`, `max-width`, `min-height`, and
  `max-height` accept standalone, case-insensitive `inherit`.
- An explicit dimension `inherit` copies the parent computed optional pixel
  value. A parent with no local value passes the existing `None`/auto
  fallback; a root with no parent also resolves to `None`.
- Inherited values participate in the existing important-over-normal,
  reversed named-layer, inline-important, invalid-later, source-order, and
  `revert-layer` behavior. Omission remains local and does not inherit.
- `inherit` cannot be mixed with lengths, reset keywords, percentages, or
  other shorthand tokens. Existing standalone `initial`, `unset`, `revert`,
  and `revert-layer` semantics remain distinct.
- The value is retained in the existing optional pixel dimension owner and
  continues through the current content-box/border-box, min/max, normal-flow,
  flex, overflow, display-list, raster, point-hit, and semantic consumers.

## Boundary and tradeoffs

- This slice does not add implicit inheritance, percentages, negative values,
  intrinsic sizing, aspect ratio, margin collapsing, positioning, replaced
  element sizing, vertical writing modes, multiple origins, transitions,
  animations, or browser-wide CSS sizing conformance.
- Parent optional dimensions are carried only through the existing private
  style walk. No public computed-style accessor, transport schema, renderer
  owner, dependency, feature default, crate boundary, or security boundary
  changes.
- Copying the parent’s optional computed value preserves the engine’s current
  `None`/auto distinction; no new used-value or containing-block state is
  introduced.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive physical `inherit`,
  parent/child optional values, root/none fallback, shorthand-independent
  longhands, terminal `!important`, `revert-layer` rollback, invalid-later
  preservation, and mixed-token rejection.
- Run one integration fixture through parent/child geometry, min/max
  constraints, content-box/border-box sizing, normal-flow/flex consumers,
  display-list/raster/PNG capture, point-hit, and semantic/source-order
  outputs.
- Near issue completion retain the full native integration, feature library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Implementation is `fd6ee415`; design is `9231d17f`. The implementation adds
optional width/height/min-width/max-width/min-height/max-height inheritance to
the existing private DOM style walk. Standalone case-insensitive `inherit`
copies the parent computed optional pixel value, while omitted dimensions still
fall back locally to `None`; root and parent-`None` inheritance therefore stay
explicitly bounded. The existing important/source-order, invalid-later,
`revert-layer`, box-sizing, min/max, normal-flow, flex, display-list, raster,
PNG, point-hit, and semantic consumers remain unchanged.

Local evidence:

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Locked native-feature check passed in `/tmp/glass-196-focused`.
- Focused parser/cascade unit passed: `1 passed; 0 failed`.
- Focused public integration passed: `1 passed; 0 failed`.
- Full native integration passed: `234 passed; 0 failed; 0 ignored`.
- Strict native-feature Clippy passed with `-D warnings`.
- Warning-denied native-feature rustdoc passed.
- Release-documentation truth passed: `610` Markdown documents, `83` current
  documents, `57` previous-version hits, `710` semantic audit hits, and `0`
  current-claim failures.
- Documentation coverage passed: `610` Markdown files, `345` full-product MCP
  tools (`100` browser-only), `17` examples, and `22` public modules.
- Documentation depth passed: `93` current guides routed/audited and `19`
  substantive contracts.
- Feature parity passed: `14` capabilities across `4` targets; baseline
  `0.3.0`, next `0.3.14`, checkout `0.3.14`.
- TUI shortcut inventory passed: `15` implementation help keys and `63`
  documentation markers.
- Version synchronization passed at `0.3.14`.
- Before cleanup, the exact isolated target measured `5,977,537,254` bytes,
  `9,152` files, and `1,072` directories; the report measured `195,034`
  bytes. No Cargo/Rust process or open handle was present. Exact bounded
  deletion removed both paths and free space rose from `71G` to `77G`.

Paired-crate, package, workspace, security/fuzz, issue synchronization, and
remote-CI gates remain deferred to the final issue #40 certification boundary.
No remote CI, push, release, tag, registry publication, browser-parity,
security-boundary, or promotion claim is made.
