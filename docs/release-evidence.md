# Release evidence

The 0.2.1 release workflow produces evidence in layers. A report is not a
release claim unless its runtime status is certified and its artifact identity
joins to the contract matrix.

## Evidence layers

| Report | Producer | Binding | Certification meaning |
|---|---|---|---|
| `feature-parity.json` | `check-feature-parity.py` | source checkout | Static four-target inventory; runtime not claimed |
| `platform-matrix.json` | `merge-platform-evidence.py` | packaged artifact SHA-256 | Browser/platform smoke passed on each native runner |
| `contract-matrix.json` | `merge-contract-evidence.py` | packaged artifact SHA-256 | CLI help, capability manifest, and MCP tools agree |
| `reliability-scorecards.json` | `certify-reliability-matrix.py` | packaged artifact SHA-256 | Six deterministic scenarios certified per target |
| `sandbox-matrix.json` | `merge-extension-sandbox-evidence.py` | packaged artifact SHA-256 | Native sandbox gate passed per target |
| `knowledge-migration-matrix.json` | `merge-knowledge-migration-evidence.py` | packaged artifact SHA-256 | v1 round-trip and v2 rejection passed per target |
| `client-compatibility-matrix.json` | `merge-client-compatibility-evidence.py` | packaged artifact SHA-256 | TypeScript, Python, and npm launcher checks passed |

The acceptance job first merges the per-target rows. It then runs
`verify-runtime-artifact-evidence.py`, which compares every certified runtime
and client row with the exact filename, size, and SHA-256 in
`contract-matrix.json`. The final release job repeats this join after
downloading the evidence and verifies the downloaded artifact bytes again.

## Local checks

Run the static inventory checks from the repository root:

```console
python3 scripts/check-release-documentation.py
python3 scripts/check-feature-parity.py
python3 scripts/check-reliability-matrix.py
python3 scripts/check-public-readonly-adapters.py
python3 scripts/check-version-sync.py
```

Packaged-artifact reports are produced by the release matrix. Do not replace
them with source-build reports: the migration validator labels source-build
evidence explicitly, and runtime certification requires the packaged binding.

## Publication boundary

The checked-in 0.2.1 metadata and evidence contracts are local preparation.
Native Linux/macOS matrix execution, checksum signing, publication, and the
GitHub release remain pending until the tagged workflow completes. No report
in this checkout authorizes a push, crate publication, or release creation.
