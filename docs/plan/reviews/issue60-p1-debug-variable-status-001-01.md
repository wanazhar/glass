# Issue #60 P1 F20 debugger variable status review 01

## Scope

Reviewed implementation commit `7a647c717f41ed357abd1531da0a4a11c7769f59`
at its exact tip. Read the F20 task contract, Development TUI contract, and
changed state/render code and tests. Focused review covered preview ownership
across snapshot invalidation, active and queued stale results, terminal
variable-reference outcomes, Code source-jump status, and stale projection
rejection. No implementation files were changed.

## Findings

**P1/P2 blocking: none.**

- Snapshot reconciliation clears debugger projections when the selected
  session disappears or changes, then checks pending and active Debug scope or
  variable preview keys (`crates/glass-dev/src/tui/state.rs:8373-8409,
  2249-2274`). It resolves a `Loading` or `Queued` message only while the
  status still equals that preview's expected message; otherwise newer status
  remains untouched (`state.rs:2214-2239`).
- Tool results recheck the captured selection key before dispatching the result
  into projection updates. Stale scope/variable results return without
  repopulating `debug_scopes` or `debug_variables` (`state.rs:4230-4245`). The
  regressions cover active stale scope and variable results after snapshot
  session removal, including preservation of a newer user status
  (`state.rs:13459-13519, 13521-13588`). A stale queued variable request is
  discarded at flush, resolves its owned queued status, and is not submitted
  (`state.rs:13590-13639`).
- Variable reference zero and negative values produce separate visible notices
  and return before tool submission. `debug_variable_lines` renders the notice
  in the VARIABLES panel (`state.rs:5461-5508`,
  `crates/glass-dev/src/tui/render.rs:3634-3654`). The regression verifies
  terminal status, no active/running job, and Code status preservation
  (`state.rs:13641-13696`). Existing Code source-jump coverage remains in the
  focused suite.

## Verification

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib --locked debug_` — passed, 13 passed and 0 failed; build completed in 3m26s. It emitted the recorded 72 `glass-browser` dead-code warnings.
- `git diff --check 7a647c71^ 7a647c71` — passed.

## Conclusion

**PASS.** The stale preview paths resolve only their still-owned status,
invalid references terminate without a variables request, Code status remains
protected, and stale results do not repopulate projections.
