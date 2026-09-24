---
id: native-engine-browser-702
scope: native-engine/html/initial-script-mime-classification
status: complete
depends-on: [native-engine-browser-701]
---

# Objective

Use one JavaScript MIME essence policy for initial-document script discovery
and external script response validation, matching the already-aligned dynamic
inline path.

## Contract

- Recognize the same five JavaScript MIME essences for initial classic script
  classification as the external response validator.
- Ignore ASCII case and MIME parameters consistently.
- Preserve absent/empty-type classic handling and module classification.
- Keep data blocks such as `application/json` out of the executable script
  source list.
- Do not claim the full historical JavaScript MIME type registry or complete
  parser/scheduling semantics from this bounded alignment.
- Keep issue #40 open until the full browser profile and native-only gates
  pass.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- [`native-engine-browser-701`](native-engine-browser-701.md)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-702.md`

## Verification and result

- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before
  tests.
- Run the affected MIME-policy tests with
  `cargo test -p glass-browser --lib mime_policy --locked --quiet`.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut checks. Run inventory/link coverage only if its debug
  binaries are already available; do not build solely for this gate.
- Record remote CI separately; do not claim issue #40 completion.

Implementation and focused tests passed locally:

- `cargo fmt --all` and `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --tests --locked --quiet`
- The paired MIME-policy tests passed 2/2, covering dynamic inline execution
  and initial-document source discovery.
- Release-documentation truth validated 1,330 Markdown documents (83 current;
  63 previous-version hits; 1,464 semantic hits; zero current-claim failures).
- Documentation depth validated 93 guides/19 contracts; shortcut inventory
  validated 15 keys/63 markers; formatting and whitespace checks passed.
- Documentation inventory/link coverage was skipped because no executable
  `glass` or `glass-browser` debug binary exists at `target/debug`; no binary
  was built solely for this gate.
- Remote CI, complete script-type conformance, and issue #40 completion are
  not claimed.
