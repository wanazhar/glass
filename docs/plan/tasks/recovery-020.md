id: recovery-020
scope: explicit action recovery policy
status: done
depends-on: [failure-020]

## objective

Expose an explicit typed recovery strategy on guarded revision and verification
failures, keeping automatic retries disabled by default.

## path

- `src/browser/session/types.rs`
- `src/browser/session/action.rs`
- `src/browser/session/wait.rs`
- `src/browser/session/tests.rs`
- `docs/action-contract.md`
- GitHub issue #20

## verification

- Guarded failures serialize `recoveryStrategy: "report"`.
- No failure silently relocates a stale reference or retries a dispatched action.
- No remote push, tag, or publication occurs.
