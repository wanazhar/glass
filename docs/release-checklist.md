# Release checklist

Use this checklist for each public release.

## Release status

The release checkout is `glass-browser` and `glass-dev` version `0.3.6`.
Linux x86-64, Linux arm64, macOS x86-64, and macOS arm64 remain declared
targets. Target support claims remain bounded by the machine-readable
feature-parity matrix and native evidence recorded for each environment.
Windows receives browser-free source checks and native named-pipe daemon
certification; native browser/PTY support is not certified.

## 0.3.6 release candidate

- [ ] Certify issue #36 scenarios A-J, gates 1-15, and every forbidden outcome.
- [x] Pass the complete local workspace, docs, package, security, fuzz, live
      browser, PTY, clean-install, and publish dry-run gates.
- [ ] Verify the signed exact tag, ordered registry publication, clean registry
      installs, native CI, fuzz, GitHub Release, and issue closure.

Until the public records above exist, the immutable 0.3.5 section below is the
latest published-release evidence.

## 0.3.5 release record

Signed annotated tag `v0.3.5` points to commit
`3c528689b70396ac5f30367ed89f4d13e3d0ee78` and GitHub reports its signature
as verified. The
[ordered release workflow](https://github.com/wanazhar/glass/actions/runs/31547613725)
published `glass-browser 0.3.5`, then `glass-dev 0.3.5`, clean-installed both
registry packages, and created the source-only
[GitHub Release](https://github.com/wanazhar/glass/releases/tag/v0.3.5) on
2026-08-12. Exact-source
[native certification](https://github.com/wanazhar/glass/actions/runs/31549718984)
passed pinned Pi SDK, automatic experiments, all 18 live Chromium scenarios,
and the native Windows named-pipe lifecycle.

- [x] Verify exact-tag/version/signature, documentation, packages, dry-runs,
      clean package installs, and publication state before upload.
- [x] Publish both unyanked crates in dependency order and clean-install both
      from crates.io.
- [x] Retain exact-tag parser fuzz, security, client, real
      debugpy/LLDB/Delve, native Pi, Chromium, and Windows named-pipe evidence.
- [x] Publish substantive, non-draft, non-prerelease, source-only GitHub notes.
- [x] Audit every issue #35 gate and forbidden outcome before closing the epic.

## 0.3.4 release record

Signed annotated tag `v0.3.4` points to commit
`739b2e6a461cf17d5a776a3d6c2cf98b83c2e83f`. The
[ordered release workflow](https://github.com/wanazhar/glass/actions/runs/31442780359)
published `glass-browser 0.3.4`, then `glass-dev 0.3.4`, clean-installed both
registry packages, and created the source-only
[GitHub Release](https://github.com/wanazhar/glass/releases/tag/v0.3.4) on
2026-08-10.

- [x] Pi is the sole embedded runtime and independent Pi sessions expose native
      session, model, thinking, steering, follow-up, compaction, and cancellation.
- [x] Resident file/editor/process/browser/workflow/LSP/DAP/Git/test/kernel,
      memory, graph, replay, and external-client tools share governed state.
- [x] Real rust-analyzer, debugpy, Chromium, Pi, durable reconnect, isolated
      worktree, PTY TUI, and process/browser cleanup scenarios pass locally.
- [x] `glass --mcp` preserves the browser catalog and adds the live Glass Dev
      catalog; direct CLI callers are not forced through Pi.
- [x] `glass-dev` owns the decomposed full development shell while
      `glass-browser` remains independently buildable with no default features.
- [x] Complete the final workspace, rustdoc, package, and clean-install gates
      and record exact results in `docs/release-evidence.md`.
- [x] Obtain explicit approval before any push, tag, publication, issue update,
      or GitHub Release operation; approval was given on 2026-08-10.
- [x] Verify exact-tag CI and parser fuzz, ordered crates.io publication,
      unyanked registry records, clean registry installs, and the non-draft,
      non-prerelease, source-only GitHub Release.

## 0.3.3 release record

Signed annotated tag `v0.3.3` points to commit
`f5951f40c0c2fbb0c8cae60f44e7a07840c6ced3`. The
[ordered release workflow](https://github.com/wanazhar/glass/actions/runs/31373242351)
published `glass-browser 0.3.3`, then `glass-dev 0.3.3`, clean-installed both
registry packages, and created the source-only
[GitHub Release](https://github.com/wanazhar/glass/releases/tag/v0.3.3) on
2026-08-10.

- [x] Map all 53 mandatory issue #33 checkboxes, including the authoritative
      amendment, to integrated evidence.
- [x] Verify the responsive phone/compact/wide TUI, real 40x20 PTY behavior,
      browser recovery, target selection, same-session Remote View and dynamic
      agent context.
- [x] Verify process-tree cleanup, bounded project snapshots, persistent LSP,
      and real embedded Neovim RPC evidence.
- [x] Run formatting, all-feature workspace tests, 19 serial Chromium
      scenarios, warnings-denied Clippy/rustdoc, minimal-core compilation,
      dependency/security/fuzz checks, and separate release builds.
- [x] Package and dry-run both crates without upload; validate the exact
      normalized `glass-browser =0.3.3` dependency.
- [x] Clean-install core and full packages and exercise core-to-full,
      full-to-full and full-to-core ownership transitions.
- [x] Document complete uninstallation for both package owners, custom Cargo
      roots, retained state, external MCP entries, and experiment worktrees.
- [x] Keep macOS and Windows claims bounded to browser-free CI definitions.
- [x] Obtain explicit approval before pushing, tagging, publishing, closing
      issue #33, or creating the GitHub Release; approval was given on
      2026-08-10.
- [x] Verify exact-tag CI and parser fuzz, ordered crates.io publication,
      unyanked registry records, clean registry installs, and the non-draft,
      non-prerelease, source-only GitHub Release.

## 0.3.2 release record

Signed tag `v0.3.2` points to commit `4e548421abb6ed27ef1c91024379f7eb7abf3f90`.
The ordered release workflow published `glass-browser 0.3.2`, then
`glass-dev 0.3.2`, clean-installed both registry packages, and created the
source-only GitHub Release on 2026-08-08.

- [x] Synchronize Rust, Python, and TypeScript package metadata at `0.3.2`.
- [x] Validate both publishable Rust packages: `glass-browser` and `glass-dev`.
- [x] Run version-sync, feature-parity, release-documentation, and complete
      documentation inventory/link validators.
- [x] Run formatting and all-target workspace tests.
- [x] Run the opt-in 19-scenario Chromium smoke suite in the recorded
      validation environment.
- [x] Run Clippy, rustdoc, dependency-policy, vulnerability, and fuzz-build
      gates.
- [x] Inspect both package file lists; validate the exact normalized dev
      dependency with a local patch source; complete dry-runs without upload.
- [x] Record bounded issue #32 implementation evidence and target boundaries.
- [x] Obtain explicit approval before any stable tag, push, crates.io
      publication, or GitHub Release.

Every version tag must have a matching, published, non-draft GitHub Release
entry. The release entry contains generated notes and does not imply native
binary distribution.
The newest published release must be explicitly marked `Latest`; older
release records must not carry that marker.

Versioning and annotated `vX.Y.Z` tags remain part of the process. Each release
publishes the crate to crates.io and creates a source-only GitHub Release.
GitHub release binaries, checksum manifests, Sigstore bundles, and the npm
native launcher are not release deliverables.

Pushing a `vX.Y.Z` tag runs `.github/workflows/crates-release.yml`. The action
checks the tag and package version, runs the release validation suite, performs
a crates.io dry run, publishes the crate when needed, and creates the matching
GitHub Release with generated notes. Existing crate or release records are
detected idempotently; native binary artifacts are never uploaded.

Issue #30 / 0.3.0 exit acceptance additionally requires stable Glass Web IR
and Task Protocol v1 contracts, deterministic compilation, mandatory guarded
runtime compilation, revision and confirmation gates, generated verification,
cross-interface conformance, malformed-input and fuzz coverage, live browser
scenarios, measured payload reduction, migration guidance, and a release audit.

## Prepare

- [x] Confirm release version `0.3.0`; release date `2026-08-06`.
- [x] Check the package name, description, license, README, and repository
      metadata in `Cargo.toml`.
- [x] Record the 0.2.0 release date in the changelog.
- [x] Record the 0.2.1 release date in the changelog.
- [x] Record the 0.2.2 release date in the changelog.
- [x] Record the 0.2.3 release date in the changelog.
- [x] Check the README installation commands against `glass --help`.
- [x] Review dependency and browser-facing security changes.
- [x] Check that the working tree has no profiles, screenshots, logs, or other
      generated data.

## Validate the checkout

Run:

```console
cargo fmt --all -- --check
python3 scripts/check-version-sync.py
python3 scripts/check-documentation-depth.py
scripts/check-rust-workspace.sh test
scripts/check-rust-workspace.sh clippy
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --locked --no-deps
python3 scripts/check-web-ir-corpus.py --baseline benchmarks/results/web-ir-v1.json
cargo package --package glass-browser --locked
cargo publish --package glass-browser --locked --dry-run --no-verify
cargo package --package glass-dev --locked --no-verify --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'
cargo publish --package glass-dev --locked --dry-run --no-verify
cargo deny check
cargo audit
cargo check --manifest-path fuzz/Cargo.toml --bins
GLASS_PREVIOUS_VERSION=0.3.2 scripts/smoke-clean-install.sh
```

The split-package test and Clippy script is required: the core-only and full
packages both intentionally publish `glass-browser`, so one workspace-wide
all-target invocation asks Cargo to produce two same-named artifacts. Package
validation preserves both install contracts without emitting that collision
warning.

Then complete these release checks:

- [x] Run the real-browser smoke test in the recorded Linux ARM64 validation
      environment.
- [x] Record the target environment, architecture, Rust target, browser
      version, and commands used for the platform check.
- [x] Keep other declared targets labeled uncertified unless their own native
      environments are tested separately.
- [x] Inspect `cargo package --list` and the unpacked package.
- [x] Review dependency, license, and vulnerability JSON reports.
- [x] Run a clean-machine crates.io install and upgrade test after publication.

## Verify GitHub release records

Run after the tagged release workflow or with authenticated `gh` access:

```console
python3 scripts/check-github-releases.py
```

This check compares every `vX.Y.Z` tag with published, non-draft,
non-prerelease GitHub Release records and fails if any tag is missing.

The browser and package checks are evidence for the tested environment only.

## Publish

- [x] Publish and verify the `0.2.6` crates.io package.
- [x] Create and verify the matching published GitHub Release for `v0.2.6`.
- [x] Create the signed annotated tag `v0.2.6`.
- [x] Publish `glass-browser` from the tagged commit with `cargo publish
      --locked` after the package checks passed and publication was approved.
- [x] Include the Linux ARM64 validation boundary and known
      limitations in the 0.2.6 release notes or changelog.
- [x] Verify installation and upgrade smoke checks for the published release.
- [x] Restore an empty `Unreleased` changelog section after publication.
- [x] Run the full 0.2.6 release validation suite and package dry runs.
- [x] Push the release commit and tag after explicit approval.
- [x] Update issue #30 with final verified 0.2.6 release evidence.
- [x] Run the full 0.2.8 release validation suite and package dry runs.
- [x] Create the signed annotated `v0.2.8` tag after publication approval.
- [x] Publish `glass-browser` from the tagged commit after explicit approval.
- [x] Create and verify the matching published GitHub Release for `v0.2.8`.
- [x] Run the full 0.2.9 release validation suite and package dry runs.
- [x] Create the signed annotated `v0.2.9` tag after publication approval.
- [x] Publish `glass-browser` from the tagged commit after explicit approval.
- [x] Create and verify the matching published GitHub Release for `v0.2.9`.

## 0.3.0 release

- [x] Synchronize Rust, TypeScript, and Python package metadata at `0.3.0`.
- [x] Promote Web IR and Task Protocol v1 without public draft aliases.
- [x] Route every browser-backed Task Protocol family through the shared
      compiler, revision checks, policy confirmation, and verification runtime.
- [x] Cover Rust, CLI, MCP, daemon, capability, and golden protocol surfaces.
- [x] Add strict malformed-input and semantic-contract fuzz coverage.
- [x] Exercise the 17-scenario Linux ARM64 browser suite, including live Web IR
      extraction, compilation, runtime safety, and fixture payload metrics.
- [x] Run the complete formatting, test, Clippy, rustdoc, corpus, package,
      publish-dry-run, dependency-policy, vulnerability, and fuzz gates against
      the final candidate.
- [x] Obtain explicit publication approval.
- [x] Create and push the signed annotated `v0.3.0` tag.
- [x] Verify crates.io publication and the source-only GitHub Release.

A release is not complete while any required checkbox is open.
