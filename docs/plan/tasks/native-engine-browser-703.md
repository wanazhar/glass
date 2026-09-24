---
id: native-engine-browser-703
scope: native-engine/javascript/javascript-mime-essence-matching
status: complete
depends-on: [native-engine-browser-702]
---

# Objective

Correct JavaScript MIME recognition across script `type` attributes and
external response headers, and recognize the complete standard list of
JavaScript MIME type essences.

## Contract

- Recognize all 16 JavaScript MIME type essences from the MIME Sniffing
  Standard, using ASCII case-insensitive comparison.
- A script `type` attribute must match an essence exactly; parameters and
  surrounding whitespace do not match.
- External response `Content-Type` matching ignores parameters while using
  the same JavaScript essence list.
- Initial-document discovery and connected dynamic inline classic-script
  execution use the same strict type-attribute rule.
- Keep `application/json` as a data block and preserve module classification.
- Do not infer complete script scheduling, module conformance, or browser
  completion from this MIME-policy slice. Issue #40 remains open.

## Context

- [HTML Standard: scripting languages](https://html.spec.whatwg.org/multipage/scripting.html#scriptingLanguages)
- [MIME Sniffing Standard: JavaScript MIME types](https://mimesniff.spec.whatwg.org/#javascript-mime-type)
- [`native-engine-browser-701`](native-engine-browser-701.md) and
  [`native-engine-browser-702`](native-engine-browser-702.md) for the interim
  behavior and its correction.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-701.md`
- `docs/plan/tasks/native-engine-browser-702.md`
- `docs/plan/tasks/native-engine-browser-703.md`

## Verification and result

- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before
  targeted tests.
- Run `cargo test -p glass-browser --lib mime_type --locked --quiet` for the
  shared list, dynamic insertion, and parsed-document behavior.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut checks. Run inventory/link coverage only if its debug
  binaries are already available; do not build solely for this gate.
- Record remote CI separately; do not claim issue #40 completion.

Implementation and focused tests passed locally:

- `cargo fmt --all` and `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --tests --locked --quiet`
- The focused `mime_type` batch passed 4/4.
- Release-documentation truth validated 1,331 Markdown documents (83 current;
  63 previous-version hits; 1,466 semantic hits; zero current-claim failures).
- Documentation depth validated 93 guides/19 contracts; shortcut inventory
  validated 15 keys/63 markers; formatting and whitespace checks passed.
- Documentation inventory/link coverage was skipped because no executable
  `glass` or `glass-browser` debug binary exists at `target/debug`; no binary
  was built solely for this gate.
- Remote CI, full script scheduling, and issue #40 completion are not claimed.
