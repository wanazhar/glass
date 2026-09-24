---
id: native-engine-browser-701
scope: native-engine/javascript/dynamic-inline-script-mime-policy
status: complete
depends-on: [native-engine-browser-700]
---

# Objective

Align MIME-type recognition for connected dynamically inserted inline classic
scripts with the existing external-script response policy.

## Contract

- Accept an empty type and the five JavaScript MIME essences already allowed
  by the external-script response validator.
- Match MIME essence case-insensitively and ignore parameters, consistent with
  the existing loader policy.
- Keep non-script data types such as `application/json` inert and do not emit a
  start-script command for them.
- Preserve the connected-only execution path, CSP checks, error dispatch, and
  execute-once ledger.
- Do not change external/module loading or infer full script/task scheduling
  conformance from this bounded alignment.
- Keep issue #40 open until the full browser profile and native-only gates
  pass.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [`native-engine-browser-700`](native-engine-browser-700.md)
- [`native-engine-browser-279`](native-engine-browser-279.md) through
  [`native-engine-browser-281`](native-engine-browser-281.md) for the bounded
  process-backed dynamic external/module path.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-701.md`

## Verification and result

- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before the
  focused MIME-policy test.
- Run
  `cargo test -p glass-browser --lib javascript_dynamic_inline_scripts_match_external_script_mime_policy --locked --quiet`.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut checks. Run inventory/link coverage only if its debug
  binaries are already available; do not build solely for this gate.
- Record remote CI separately; do not claim issue #40 completion.

Implementation and focused test passed locally:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --tests --locked --quiet`
- The focused MIME-policy regression passed 1/1.
- Release-documentation truth validated 1,329 Markdown documents (83 current;
  63 previous-version hits; 1,462 semantic hits; zero current-claim failures).
- Documentation depth validated 93 guides/19 contracts; shortcut inventory
  validated 15 keys/63 markers; formatting and whitespace checks passed.
- Documentation inventory/link coverage was skipped because no executable
  `glass` or `glass-browser` debug binary exists at `target/debug`; no binary
  was built solely for this gate.
- Remote CI, general script scheduling, and issue #40 completion are not
  claimed.
