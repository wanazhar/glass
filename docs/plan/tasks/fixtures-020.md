id: fixtures-020
scope: deterministic fixture verification coverage
status: done
depends-on: [verify-020, effects-020]

## objective

Extend the opt-in local browser smoke path to exercise the bounded verification
predicate contract against the deterministic fixture.

## path

- `tests/browser_smoke.rs`
- `tests/fixtures/basic.html`
- GitHub issue #20

## verification

- The local fixture checks composed title and URL predicates.
- The existing smoke suite remains opt-in through `GLASS_E2E=1`.
- No remote push, tag, or publication occurs.
