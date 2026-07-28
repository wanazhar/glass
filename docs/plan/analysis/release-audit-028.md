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
| #27 Stable Runtime Platform | Protocol v1, capability negotiation, isolated daemon sessions, leases, recovery, SDK guards, TUI inventory, extension host foundations, lifecycle tests, and executable client/transport conformance exist locally. | Blocked. Extension capability is disabled until the native sandbox gate passes on the release environments. |

## Evidence that passes locally

The current local checkout has:

- synchronized 0.2.0 package versions;
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

These results prove local behavior. They do not prove a published release.

## Release blockers

The 0.2.0 release checklist remains open for these items:

1. Run the real-browser matrix on Linux x86-64/arm64 and macOS x86-64/arm64.
   Linux ARM64 uses a system Chromium binary because Chrome for Testing does
   not publish a Linux ARM64 archive.
2. Run the release binary contract smoke on all release targets. The workflow
   now checks CLI help, the capability manifest, MCP initialization, and the
   complete tool inventory. Browser, screenshot, and TUI startup evidence is
   still external release evidence.
3. Run full package inspection and a clean verified package-install test.
4. Generate and verify artifact checksums and Sigstore provenance.
5. Attach dependency, license, and vulnerability reports.
6. Complete clean-machine install and upgrade checks.
7. Complete the deterministic reliability certification matrix.
8. Publish the client compatibility matrix and test clients against released
   binaries.
9. Complete extension redaction and native-sandbox certification on the
   release environments. Lifecycle and cross-transport conformance now pass
   locally.

## Publication boundary

Do not create the public tag, publish crates.io or client packages, upload
binaries, or close the remote epics while a release blocker is open.

The correct current label is: `0.2.0 local release candidate; not ready for
public release`.
