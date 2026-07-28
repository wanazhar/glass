# Scorecard adapter contract

An adapter runs the versioned fixture scenarios and writes one JSON report that
matches `benchmarks/report-schema.json`.

Each adapter must use the same:

- Chrome executable;
- viewport;
- corpus version;
- iteration count; and
- warm-profile metadata.

A scenario passes only when the exact expected state is observed. A different
actionable element is `wrong_action`. It is not a timeout or partial success.

Install adapter dependencies in a temporary directory. Do not add them to
`Cargo.toml` or this repository.

Report resource scope for the runner process separately from Chrome. Set a
metric to `null` when the adapter cannot collect it. Do not estimate a
missing value.

The acceptance runner supplies bounded deadlines, fixture revision, run ID, and
checkpoint paths. A partial checkpoint cannot pass the acceptance gate.

The reference adapters and their exact versions are documented in
[benchmarks/README.md](../README.md). The current comparison includes Glass,
Playwright, and other external adapters when their commands and evidence are
available. Unsupported adapter operations remain visible in the report.
