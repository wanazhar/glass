# Release evidence

The 0.2.4 development path publishes one thing: the `glass-browser` crate on
crates.io. Versioning and `vX.Y.Z` tags remain required, but the project does no
native GitHub release binaries, installers, or updater infrastructure.

The published 0.2.3 release and local 0.2.4 work also retain bounded MCP
response-cost measurements in
[`benchmarks/response-cost-v1.json`](../benchmarks/response-cost-v1.json).

## Evidence layers

| Report | Producer | Binding | Certification meaning |
|---|---|---|---|
| `feature-parity.json` | `check-feature-parity.py` | source checkout | Declared implementation inventory; runtime not claimed |
| local browser smoke | `cargo test --test browser_smoke` | current host | Browser behavior verified on the current machine only |
| crate package | `cargo package` and `cargo publish --dry-run` | source checkout | Package contents and crates.io publication shape are valid |
| source checks | Rust and Python validation scripts | source checkout | Tests, lint, docs, and release truth are consistent |

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
cargo package --locked --no-verify
cargo publish --locked --dry-run --no-verify
cargo package --locked --list
```

Run the native browser check on the current machine:

```console
GLASS_E2E=1 cargo test --test browser_smoke --locked -- --nocapture --test-threads=1
```

Record the host and browser details with the result. Do not convert source
inventory or cross-compilation into a support claim for another OS.
The current recorded result is in [Local platform evidence](local-platform.md).

## Publication boundary

The `glass-browser` `0.2.3` crate is the current published release.
The 0.2.4 changes are local and must not be published until explicitly
approved. Its eventual annotated tag will remain the versioned release
boundary; the tagged workflow validates and publishes the crate only. No GitHub binary release is expected;
this checkout makes no cross-platform support claim beyond the recorded Linux
ARM64 host.
