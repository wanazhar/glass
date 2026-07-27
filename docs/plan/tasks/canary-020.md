id: canary-020
scope: monitored non-destructive production canary procedure
status: done
depends-on: [metrics-020]

## objective

Document a hardened, host-allowlisted, read-only production canary procedure
with bounded evidence and explicit stop conditions.

## path

- `docs/production-canary.md`
- `docs/INDEX.md`
- `docs/reliability-metrics.md`
- GitHub issue #20

## verification

- The procedure performs only bounded navigation and observation.
- It requires an approved host and hardened policy.
- Canary output is supplementary evidence, not a product guarantee.
- No remote push, tag, or publication occurs.
