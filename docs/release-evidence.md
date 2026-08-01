# Release evidence

The 0.2.7 development path publishes the `glass-browser` crate on crates.io
and creates a source-only GitHub Release with generated notes. The project does
not publish native GitHub release binaries, installers, or updater
infrastructure.

The published 0.2.6 release and 0.2.7 development work retain bounded MCP
response-cost measurements in
[`benchmarks/response-cost-v1.json`](../benchmarks/response-cost-v1.json).

## Evidence layers

| Report | Producer | Binding | Certification meaning |
|---|---|---|---|
| `feature-parity.json` | `check-feature-parity.py` | source checkout | Declared implementation inventory; runtime not claimed |
| native browser smoke | `cargo test --test browser_smoke` | target environment | Native behavior observed for the tested environment only |
| crate package | `cargo package` and `cargo publish --dry-run` | source checkout | Package contents and crates.io publication shape are valid |
| source checks | Rust and Python validation scripts | source checkout | Tests, lint, docs, and release truth are consistent |
| GitHub Release | `gh release create` | versioned tag | Release notes are present; no native binary assets are expected |

These checks intentionally do not create an artifact matrix. A successful
Linux ARM64 check does not certify Linux x86-64, macOS, or Windows.

## Local checks

Run the static inventory checks from the repository root:

```console
python3 scripts/check-release-documentation.py
python3 scripts/check-feature-parity.py
python3 scripts/check-reliability-matrix.py
python3 scripts/check-public-readonly-adapters.py
python3 scripts/check-version-sync.py
python3 scripts/check-github-releases.py
cargo package --locked --no-verify
cargo publish --locked --dry-run --no-verify
cargo package --locked --list
```

Run the native browser check in the target environment:

```console
GLASS_E2E=1 cargo test --test browser_smoke --locked -- --nocapture --test-threads=1
```

Record the target environment and browser details with the result. Do not
convert source inventory or cross-compilation into a support claim for another
OS. The current recorded result is in
[Recorded platform evidence](local-platform.md).

## Publication boundary

The `glass-browser` `0.2.6` crate is the current published release, and
`v0.2.6` has a matching GitHub Release entry. The 0.2.7 changes are not
published until explicitly approved. Its eventual annotated tag must have both
a crates.io package publication and a matching non-draft GitHub Release before
the release is considered complete. No native binary assets are expected; this
checkout makes no cross-platform support claim beyond the recorded Linux ARM64
validation environment.
