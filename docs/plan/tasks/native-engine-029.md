---
id: native-engine-029
scope: glass-browser/native-engine/pixel-golden-capture
status: done
depends-on: [native-engine-028]
---

# Native bounded PNG pixel-golden certification

## Objective

Turn the existing bounded PNG capture path into a checked-in pixel-golden
acceptance gate. The native engine already has a deterministic display list,
logical RGBA surface, and explicit PNG backend operation, but the prior tests
proved only selected pixels, dimensions, and PNG framing. A complete small
fixture golden must protect the clear/fill/paint-to-PNG path from drift.

## Contract

One fixed 8x6 local fixture has an explicit expected RGBA pixel matrix. The
test compares both the direct Rust surface and the decoded bytes returned by
the native engine capture path against that matrix. The test also verifies
that capture remains read-only and uses no network, filesystem, platform
window, font, image, or GPU capability.

This is a certification slice for the existing bounded renderer, not a claim
of screenshot compatibility, physical-pixel fidelity, font fidelity, image
support, color management, or browser parity. It does not change the stable
evidence schema or add a new backend capability.

## Tradeoffs

- A tiny full-frame golden catches coordinate, clear, fill, clipping, and PNG
  regressions more reliably than a handful of probes, while remaining fast and
  readable in source.
- A hard-coded logical-pixel fixture is deterministic and dependency-free, but
  it intentionally does not cover fonts, images, anti-aliasing, or platform
  rendering.
- Keeping the golden in the native integration test avoids a binary asset and
  keeps the acceptance artifact inside the two-crate repository boundary.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- direct logical surface matches the checked-in full-frame golden;
- decoded bounded PNG capture matches the same golden;
- capture does not mutate the native revision;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented in the `test(native-engine): certify PNG pixel golden` checkpoint.
The native integration contract now compares a fixed 8x6 logical RGBA matrix
through both the direct software surface and decoded bounded PNG capture while
asserting capture leaves the document revision unchanged.

- Focused logical-pixel golden: 1 passed.
- Native integration suite: 42 passed.
- Native unit suite: 35 passed.
- Strict default-feature and `native-engine` all-target Clippy gates pass with
  warnings denied.
- Full locked `glass-browser` all-target/all-feature matrix: 817 unit tests
  passed, 1 ignored; all integration and example targets passed, including 42
  native integration tests.
- Native-feature doctests: 4 passed.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Documentation coverage: 443 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures; feature parity remains green
  for the 0.3.14 checkout.
- No new dependency, third crate, stable transport capability, screenshot
  evidence, or browser-parity claim was introduced.
