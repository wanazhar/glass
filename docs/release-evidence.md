# Release evidence

The 0.3.0 release follows the crates-only distribution boundary.
The `glass-browser` 0.3.0 release has a source-only GitHub Release with
generated notes. The project does not publish native GitHub release binaries,
installers, or updater infrastructure.

The 0.3.0 release retains bounded MCP response-cost measurements in
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
python3 scripts/check-web-ir-corpus.py --baseline benchmarks/results/web-ir-v1.json
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

## 0.3.0 release validation

The tagged checkout passed the complete revision-bound release suite before
publication:

- Stable Glass Web IR v1 and Task Protocol v1 conformance passed across Rust,
  CLI, MCP, daemon, capability, and golden protocol surfaces.
- Web IR corpus validation passed with 8 fixtures, 8 scenarios, and 11
  categories.
- All-target tests, formatting, Clippy, rustdoc, release build, package,
  publish dry-run, dependency-policy, vulnerability, and fuzz checks passed.
- Semantic-contract fuzzing completed 512 runs without failure.
- The recorded Linux ARM64 browser suite passed all 17 scenarios.
- The live semantic fixture reduced estimated agent task context by 71%.
- `cargo package --locked --no-verify` packaged 211 files.

## 0.2.9 release validation

The tagged checkout `9d13ea0` passed the release workflow
(`31096629636`) before publication:

- Web IR corpus validation passed with 8 fixtures and 11 categories.
- `cargo test --all-targets --locked`, formatting, clippy, documentation,
  release build, package, advisory, audit, and fuzz checks passed.
- `cargo package --locked --no-verify` packaged 210 files.
- `GLASS_E2E=1 cargo test --test browser_smoke --locked -- --nocapture`:
  16 passed in the recorded Linux ARM64 environment.
- `glass-browser 0.2.9` is available from crates.io.
- GitHub Release `v0.2.9` is published, non-draft, non-prerelease, and
  contains no native binary assets.

## 0.2.7 release validation

The tagged checkout passed the complete pre-publication validation suite:

- `cargo test --all-targets --locked`: 553 passed, 1 ignored.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`:
  passed.
- `cargo doc --all-features --locked --no-deps`: passed.
- `cargo deny check`, `cargo audit`, and the fuzz binary check: passed.
- `cargo package --locked --no-verify` packaged 209 files.
- `cargo publish --locked --dry-run --no-verify` passed; the dry run performed
  no upload.
- `GLASS_E2E=1 cargo test --test browser_smoke --locked -- --nocapture
  --test-threads=1`: 16 passed in the recorded Linux ARM64 environment.
- `GLASS_PREVIOUS_VERSION=0.2.6 scripts/smoke-clean-install.sh`: passed for
  the candidate package and local upgrade simulation from 0.2.6 to 0.2.7.
- Published-crate install smoke for `glass-browser 0.2.7`: passed.
- Version, release-documentation, feature-parity, reliability-matrix,
  public-read-only-adapter, and GitHub release-record checks: passed.

Publication verification:

- crates.io contains `glass-browser 0.2.7`.
- Signed tag `v0.2.7` is present.
- GitHub Release `v0.2.7` is published, non-draft, non-prerelease, and marked
  latest.
- Release workflow `30979367498` completed successfully.
- GitHub release coverage now validates 27 version tags.

The native evidence remains limited to the recorded Linux ARM64 environment;
other declared targets are not certified by this release.

The `glass-browser` `0.3.0` crate is the current release, and `v0.3.0` has a
matching published GitHub Release entry. No native binary assets are expected.
