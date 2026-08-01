# 0.2.0 release audit and 0.2.1 evidence baseline

Status: published-release audit, 2026-07-29.

## Release truth

Glass `0.2.0` is published. The public release is tagged as `v0.2.0` and
points to source commit `f8874cab93b05d5ce5e7e965d29e5485e26156f6`. The
package is available as [`glass-browser 0.2.0`](https://crates.io/crates/glass-browser/0.2.0),
and the GitHub release is [v0.2.0](https://github.com/wanazhar/glass/releases/tag/v0.2.0),
published on 2026-07-28.

The 0.2.0 release also had historical platform artifacts. Those files are
retained here only as an audit record; they are not part of the 0.2.1 release
process.

| Target | Artifact | SHA-256 |
|---|---|---|
| Linux x86-64 | `glass-linux-x86_64` | `7d6c86cce2f7b05e8c63aa752219540d79ea106c9701b38246e5be38622b4fe8` |
| Linux arm64 | `glass-linux-arm64` | `3e35b89918aa425f1ce332181a793f1692aee974575ddbd4782464e584495ced` |
| macOS x86-64 | `glass-macos-x86_64` | `679abb57021ad58611d6726df3c99a6e932dbda407d9a41887281a1e86b8ca4b` |
| macOS arm64 | `glass-macos-aarch64` | `d7140a047ee535b9fdc4917e5ce157417ff7587dbe1c7fe348a8ee213a80b223` |

The next small release was `0.2.1`. From this release onward, versioning,
annotated tags, crates.io publication, and source-only GitHub Release records
remain in scope; native GitHub release binaries, checksum manifests, and
Sigstore bundles do not.

The machine-readable [feature parity inventory](../../feature-parity.json)
records the current implementation and target status for this release
baseline. Its contract is [feature-parity-v1.schema.json](../../schema/feature-parity-v1.schema.json).

This audit compares the seven open remote epics with the current checkout. An
open remote issue is not treated as completed only because local code exists.

## Summary

| Issue | Repository state | Release state |
|---|---|---|
| #21 Transactional Workflow Runtime | Core workflow, retry, checkpoint, trace, resume, and authoring paths are implemented and tested in the repository. | Partial. The full public workflow fixture matrix and public adapters are not certified. |
| #22 Semantic Observation Engine | Versioned levels, regions, revisions, diffs, schemas, and cross-interface tests exist in the repository. | Partial. Cross-platform and public evidence are not complete. |
| #23 Intent Resolution Engine | Versioned requests, evidence, policies, stale checks, guarded execution, workflow use, and TUI review exist in the repository. | Partial. Full benchmark and public adapter evidence are not complete. |
| #24 Persistent Browser Knowledge | Scoped records, lifecycle, redaction, CLI/MCP operations, schemas, and scorecard fixtures exist in the repository. | Partial. Other-target runtime execution remains uncertified. |
| #25 Workflow Authoring System | YAML/JSON compilation, diagnostics, preview, diff, semantic recording, and client smoke paths exist in the repository. | Partial. Other-target runtime execution remains uncertified. |
| #26 Reliability Laboratory | Scenario, fixture, replay, forbidden-outcome, and fail-closed release gate foundations exist in the repository. | Partial. The public scorecard is not published. |
| #27 Stable Runtime Platform | Protocol v1, capability negotiation, isolated daemon sessions, leases, recovery, SDK guards, TUI inventory, extension host foundations, lifecycle tests, and executable client/transport conformance exist in the repository. | Blocked. Extension capability is disabled until the native sandbox gate passes on the release environments. |

## Recorded evidence

The repository contains:

- synchronized 0.2.1 package versions;
- passing Rust tests, Clippy, rustdoc, audit, and dependency checks on Linux;
- a successful verified package check with `cargo package --locked`;
- Python and TypeScript client build and smoke checks against the local binary;
- protocol golden fixtures;
- daemon recovery and lease-owner tests;
- bounded extension host and native-sandbox fail-closed tests; and
- documentation that labels local-only features and unsupported Windows use.

Linux ARM64 evidence is available for one recorded validation environment:

- target: `aarch64-unknown-linux-gnu`;
- browser: system Chromium `150.0.7871.128`;
- environment: Linux `aarch64`, Rust `aarch64-unknown-linux-gnu`;
- browser smoke: `GLASS_E2E=1 GLASS_DISABLE_CHROME_SANDBOX=1 cargo test
  --test browser_smoke --locked -- --nocapture --test-threads=1`;
- result: 11 browser smoke tests passed on 2026-07-30.

This evidence does not replace the Linux x86-64 or macOS release runners, and
it does not prove a clean published artifact.

These results describe the recorded Linux ARM64 environment only. They do not
certify another operating system or a future published crate.

Additional Linux ARM64 source-checkout evidence is available:

- the six-scenario reliability capability suite passes with only `passed` or
  `safe_refusal` classifications and validates every replay bundle;
- TypeScript and Python client checks pass against the local debug binary;
- the local `glass-browser` `0.2.1` crate packages successfully with 168
  entries and the declared exclusion set;
- a temporary Cargo home installed published `0.1.18`, installed the packaged
  `0.2.0` crate, and upgraded the same installation root successfully; and
- Linux bubblewrap extension sandbox, redaction, permission, and lifecycle
  tests pass in the recorded Linux ARM64 validation environment.

The release workflow validates the source checkout and crates.io package shape,
then creates a source-only GitHub Release with generated notes. It does not
create or upload native binaries.

## Release blockers

The 0.2.1 release plan carries these remaining items:

1. Complete the final 0.2.1 package, documentation, dependency, and audit
   checks on the tagged commit.
2. Publish `glass-browser` with `cargo publish --locked`.
3. Verify a clean crates.io installation and upgrade after publication.
4. Keep Linux x86-64 and macOS support language conservative until those
   environments receive their own native verification.
5. Complete extension redaction and native-sandbox certification for any
   target before describing that extension surface as available there.

## Publication boundary

The `0.2.0` publication boundary has been crossed, followed by the
`0.2.1` crates.io package and source-only GitHub Release. Do not upload
binaries. This historical audit does not close issue #28; its remote definition
still needs to reflect the release policy before its status can be reconsidered.

The correct current labels are: `0.2.5 published; source-only GitHub Release
record and local certification` and `0.2.6 local development; not ready for
public release`.
