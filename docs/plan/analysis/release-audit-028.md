# 0.2.0 release audit and 0.2.1 evidence baseline

Status: published-release audit, 2026-07-29.

## Release truth

Glass `0.2.0` is published. The public release is tagged as `v0.2.0` and
points to source commit `f8874cab93b05d5ce5e7e965d29e5485e26156f6`. The
package is available as [`glass-browser 0.2.0`](https://crates.io/crates/glass-browser/0.2.0),
and the GitHub release is [v0.2.0](https://github.com/wanazhar/glass/releases/tag/v0.2.0),
published on 2026-07-28.

The release contains these platform artifacts. These hashes identify the
published files and are a release baseline; they do not mean that every
artifact has completed the post-release certification matrix.

| Target | Artifact | SHA-256 |
|---|---|---|
| Linux x86-64 | `glass-linux-x86_64` | `7d6c86cce2f7b05e8c63aa752219540d79ea106c9701b38246e5be38622b4fe8` |
| Linux arm64 | `glass-linux-arm64` | `3e35b89918aa425f1ce332181a793f1692aee974575ddbd4782464e584495ced` |
| macOS x86-64 | `glass-macos-x86_64` | `679abb57021ad58611d6726df3c99a6e932dbda407d9a41887281a1e86b8ca4b` |
| macOS arm64 | `glass-macos-aarch64` | `d7140a047ee535b9fdc4917e5ce157417ff7587dbe1c7fe348a8ee213a80b223` |

The next small release is `0.2.1`. Its work remains local until the
corresponding release validation, publication, and artifact certification
steps are complete.

The machine-readable [feature parity inventory](../../feature-parity.json)
records the current implementation and target status for this release
baseline. Its contract is [feature-parity-v1.schema.json](../../schema/feature-parity-v1.schema.json).

This audit compares the seven open remote epics with the current checkout. An
open remote issue is not treated as completed only because local code exists.

## Summary

| Issue | Local state | Release state |
|---|---|---|
| #21 Transactional Workflow Runtime | Core workflow, retry, checkpoint, trace, resume, and authoring paths are implemented and tested locally. | Partial. The full public workflow fixture matrix and public adapters are not certified. |
| #22 Semantic Observation Engine | Versioned levels, regions, revisions, diffs, schemas, and cross-interface tests exist locally. | Partial. Cross-platform and public evidence are not complete. |
| #23 Intent Resolution Engine | Versioned requests, evidence, policies, stale checks, guarded execution, workflow use, and TUI review exist locally. | Partial. Full benchmark and public adapter evidence are not complete. |
| #24 Persistent Browser Knowledge | Scoped records, lifecycle, redaction, CLI/MCP operations, schemas, scorecard fixtures, and a four-target packaged migration gate exist locally. | Partial. Native release-runner execution remains incomplete. |
| #25 Workflow Authoring System | YAML/JSON compilation, diagnostics, preview, diff, semantic recording, client smoke paths, and exact-artifact client evidence exist locally. | Partial. Native release-runner execution remains incomplete. |
| #26 Reliability Laboratory | Scenario, fixture, replay, forbidden-outcome, and fail-closed release gate foundations exist locally. | Blocked. The complete deterministic matrix, platform artifacts, and public scorecard are not certified. |
| #27 Stable Runtime Platform | Protocol v1, capability negotiation, isolated daemon sessions, leases, recovery, SDK guards, TUI inventory, extension host foundations, lifecycle tests, and executable client/transport conformance exist locally. | Blocked. Extension capability is disabled until the native sandbox gate passes on the release environments. |

## Evidence that passes locally

The current local checkout has:

- synchronized 0.2.1 package versions;
- passing Rust tests, Clippy, rustdoc, audit, and dependency checks on Linux;
- a successful verified package check with `cargo package --locked`;
- Python and TypeScript client build and smoke checks against the local binary;
- protocol golden fixtures;
- daemon recovery and lease-owner tests;
- bounded extension host and native-sandbox fail-closed tests; and
- documentation that labels local-only features and unsupported Windows use.

Linux ARM64 evidence is now available on the current host:

- host target: `aarch64-unknown-linux-gnu`;
- browser: system Chromium `150.0.7871.46`;
- target check: `cargo check --target aarch64-unknown-linux-gnu --locked`;
- browser smoke: `GLASS_E2E=1 GLASS_DISABLE_CHROME_SANDBOX=1 cargo test
  --locked --test browser_smoke -- --nocapture --test-threads=1`;
- result: 10 browser smoke tests passed on commit `b6a88cb`.

This is local Linux ARM64 evidence. It does not replace the Linux x86-64 or
macOS release runners, and it does not prove a clean published artifact.

These results prove local behavior. They do not complete the post-release
certification matrix for the published artifacts.

Additional Linux ARM64 release-candidate evidence is now available:

- the six-scenario reliability capability suite passes with only `passed` or
  `safe_refusal` classifications and validates every replay bundle;
- the release binary smoke reports the CLI/MCP contract and the complete MCP
  tool inventory;
- TypeScript, Python, and npm package checks pass against the release binary,
  with machine-readable evidence bound to each target artifact;
- the local `glass-browser` `0.2.1` crate packages successfully with 168
  entries and the declared exclusion set;
- a temporary Cargo home installed published `0.1.18`, installed the packaged
  `0.2.0` crate, and upgraded the same installation root successfully; and
- Linux bubblewrap extension sandbox, redaction, permission, and lifecycle
  tests pass on this host.

The release workflow now runs client compatibility and clean-install checks,
verifies artifact checksums, and creates and verifies a keyless Sigstore bundle
for the checksum manifest. These workflow checks still require a tagged GitHub
run with OIDC signing enabled.

## Release blockers

The 0.2.1 evidence plan carries these remaining items:

1. Run the real-browser matrix on Linux x86-64/arm64 and macOS x86-64/arm64.
   Linux ARM64 uses a system Chromium binary because Chrome for Testing does
   not publish a Linux ARM64 archive.
2. Run the release binary and packaged browser/screenshot/TUI smoke on all
   release targets. The workflow contains these gates; their native GitHub
   execution remains pending.
3. Run full package inspection and repeat the clean verified package-install
   test on each release runner; Linux ARM64 now passes this locally.
4. Generate and verify artifact checksums and Sigstore provenance.
5. Attach dependency, license, and vulnerability reports.
6. Complete clean-machine install and upgrade checks on the release
   environments. Linux ARM64 passes locally.
7. Complete the deterministic reliability certification matrix on every
   release platform and publish the scorecard evidence.
8. Execute and publish the client compatibility matrix against the four
   native release artifacts. The local workflow and evidence merge are ready;
   Linux ARM64 client checks pass locally.
9. Complete extension redaction and native-sandbox certification on the
   release environments. Linux ARM64 passes locally; macOS remains unverified.

## Publication boundary

The `0.2.0` publication boundary has been crossed. Do not publish `0.2.1`,
upload new binaries, or close issue #28 while a required certification or
evidence gate is open.

The correct current labels are: `0.2.0 published; post-release certification
incomplete` and `0.2.1 local development; not ready for public release`.
