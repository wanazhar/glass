id: failure-020
scope: typed action failure phases and correlation
status: done
depends-on: [contract-020]

## objective

Add a bounded failure phase and execution identity to revision and
postcondition errors without changing legacy constructors or exposing page
contents.

## context

- `docs/action-contract.md`
- GitHub issue #20

## path

- `src/browser/session/types.rs`
- `src/browser/session/action.rs`
- `src/browser/session/tests.rs`

## verification

- Stale revision errors report the `preflight` phase and session execution ID.
- Action verification errors report the `verification` phase.
- Existing typed error recovery and serialization tests pass.
- No remote push, tag, or publication occurs.
