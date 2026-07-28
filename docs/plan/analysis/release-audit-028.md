# 0.2.0 release audit

Status: local audit, 2026-07-28.

This audit compares the seven open remote epics with the current checkout. An
open remote issue is not treated as completed only because local code exists.

## Summary

| Issue | Local state | Release state |
|---|---|---|
| #21 Transactional Workflow Runtime | Core workflow, retry, checkpoint, trace, resume, and authoring paths are implemented and tested locally. | Partial. The full public workflow fixture matrix and public adapters are not certified. |
| #22 Semantic Observation Engine | Versioned levels, regions, revisions, diffs, schemas, and cross-interface tests exist locally. | Partial. Cross-platform and public evidence are not complete. |
| #23 Intent Resolution Engine | Versioned requests, evidence, policies, stale checks, guarded execution, workflow use, and TUI review exist locally. | Partial. Full benchmark and public adapter evidence are not complete. |
| #24 Persistent Browser Knowledge | Scoped records, lifecycle, redaction, CLI/MCP operations, schemas, and scorecard fixtures exist locally. | Partial. Persistence migration and release evidence remain incomplete. |
| #25 Workflow Authoring System | YAML/JSON compilation, diagnostics, preview, diff, semantic recording, and client smoke paths exist locally. | Partial. Full recorder/compiler certification and packaged client evidence remain incomplete. |
| #26 Reliability Laboratory | Scenario, fixture, replay, forbidden-outcome, and fail-closed release gate foundations exist locally. | Blocked. The complete deterministic matrix, platform artifacts, and public scorecard are not certified. |
| #27 Stable Runtime Platform | Protocol v1, capability negotiation, daemon lifecycle, leases, recovery, SDK guards, TUI inventory, and extension host foundations exist locally. | Blocked. Extension capability is disabled. Extension lifecycle certification and executable client/transport conformance remain. |

## Evidence that passes locally

The current local checkout has:

- synchronized 0.2.0 package versions;
- passing Rust tests, Clippy, rustdoc, audit, and dependency checks on Linux;
- a successful package assembly check with `cargo package --no-verify`;
- Python and TypeScript client build and smoke checks against the local binary;
- protocol golden fixtures;
- daemon recovery and lease-owner tests;
- bounded extension host and native-sandbox fail-closed tests; and
- documentation that labels local-only features and unsupported Windows use.

These results prove local behavior. They do not prove a published release.

## Release blockers

The 0.2.0 release checklist remains open for these items:

1. Run the real-browser matrix on Linux x86-64 and macOS x86-64/arm64.
2. Run release-binary smoke checks for CLI, navigation, observation,
   screenshots, TUI startup, and MCP initialization.
3. Run full package inspection and a verified package-install test.
4. Generate and verify artifact checksums and Sigstore provenance.
5. Attach dependency, license, and vulnerability reports.
6. Complete clean-machine install and upgrade checks.
7. Complete the deterministic reliability certification matrix.
8. Publish the client compatibility matrix and test clients against released
   binaries.
9. Complete extension lifecycle, redaction, sandbox, and cross-transport
   certification.
10. Decide whether the daemon contract remains one shared session namespace or
    implements independent browser sessions. Document the decision in the
    stable contract before release.

## Publication boundary

Do not create the public tag, publish crates.io or client packages, upload
binaries, or close the remote epics while a release blocker is open.

The correct current label is: `0.2.0 local release candidate; not ready for
public release`.

