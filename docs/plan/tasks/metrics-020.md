id: metrics-020
scope: reliability benchmark metrics and release gates
status: done
depends-on: [fixtures-020]

## objective

Document versioned reliability metrics and strict release acceptance rules for
the deterministic fixture and benchmark reports.

## path

- `docs/reliability-metrics.md`
- `docs/category-metric.md`
- `benchmarks/report-schema.json`
- `examples/scorecard.rs`
- GitHub issue #20

## verification

- Reports identify build, browser, host, fixture, policy, and run mode.
- Missing or partial metrics are invalid rather than silently successful.
- No remote push, tag, or publication occurs.
