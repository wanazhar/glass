---
id: native-engine-browser-700
scope: native-engine/javascript/connected-dynamic-inline-scripts
status: complete
depends-on: [native-engine-browser-699]
---

# Objective

Make insertion of dynamically created inline classic scripts into a connected
HTML subtree finish without a parser-state error, while preserving the
existing execute-once and content-policy behavior.

## Contract

- An eligible nonempty classic inline script executes once when it becomes
  connected through `appendChild` or `insertBefore`.
- Creating or editing a detached script does not execute it. Removing and
  reinserting a script that already started does not execute it again.
- A script insertion must not enter HTML parser active-formatting bookkeeping;
  that state belongs to fragment parsing, not DOM mutation.
- Keep existing CSP checks, `error` dispatch, and the start-script command.
- Do not claim dynamic external-script, module-script, async/defer, or general
  script-execution conformance from this bounded fix.
- Keep issue #40 open until its full browser profile and native-only gates pass.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [`native-engine-browser-699`](native-engine-browser-699.md)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/tasks/native-engine-browser-700.md`

## Verification and result

- Exercise detached creation, `appendChild`, `insertBefore`, execute-once
  behavior after removal/reinsertion, start-script commands, and Rust command
  commit. Confirm failures are not swallowed as successful insertion.
- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before the
  focused dynamic-script test. Exclude the user-owned selector draft test.
- Run formatting, whitespace, release-documentation truth, documentation
  depth, and shortcut checks. Run inventory/link coverage only when its debug
  binaries are already available; do not build solely for this non-final gate.
- Record remote CI separately. Do not claim broader script conformance or
  issue #40 completion.

Implementation and focused checks passed locally:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --tests --locked --quiet`
- `cargo test -p glass-browser --lib javascript_dynamic_inline_script_insertion_runs_once --locked --quiet` (1 passed)
- Release-documentation truth, documentation-depth, TUI-shortcut, and
  whitespace checks passed.
- Documentation inventory/link coverage was skipped because no executable
  `glass` or `glass-browser` debug binary exists at `target/debug`.

The misplaced parser-only active-formatting block is removed from the
connected DOM insertion path. The regression verifies detached preparation,
`appendChild`, `insertBefore`, one-time inline classic execution, removal and
reinsertion, start-script markers, and Rust command commit. Dynamic external
and module-script loading, general script conformance, remote CI, and issue
#40 completion are not claimed.
