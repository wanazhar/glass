# Release checklist

Use this checklist for each public release.

## Release status

The release checkout is `glass-browser` and `glass-dev` version `0.3.12`.
Linux x86-64, Linux arm64, macOS x86-64, and macOS arm64 remain declared
targets. Target support claims remain bounded by the machine-readable
feature-parity matrix and native evidence recorded for each environment.
Windows receives browser-free source checks and native named-pipe daemon
certification; native browser/PTY support is not certified.

## 0.3.12 release

- [x] Re-audit issue #36 scenarios A-J, gates 1-15, and every forbidden
      outcome against the current source and executable tests.
- [x] Pass the complete local workspace, docs, package, security, fuzz, live
      browser, PTY, clean-install, and publish dry-run gates.
- [x] Verify the signed exact tag, ordered registry publication, clean registry
      installs, native CI, fuzz, GitHub Release, and release record.

The 0.3.12 release contains the managed Pi runtime refresh path, in-TUI Agent
setup/login/update and conversation recovery, responsive Glass Dev surfaces,
browser-target selection and recovery, bounded target archiving, and
docs.rs-discoverable package metadata.

Exact source CI, native certification, fuzz, publication, and GitHub Release
records are recorded in [`release-evidence.md`](release-evidence.md). The
published 0.3.11 record remains historical and is not reused as evidence for
this release.

The signed `v0.3.10` tag is retained as an immutable failed candidate. Its
workflow stopped before publication because the tagged release notes contained
forbidden pre-publication wording; no registry or GitHub Release record exists.


## Canonical release procedure (mandatory)

This is the only supported release path. Copy these gates into the next
versioned release section before starting. Do not mark a gate complete from
intent: every checked box must have command output, a workflow URL, or a
registry/GitHub record in `docs/release-evidence.md`. A pushed commit, pushed
tag, dry run, or green partial job is not publication proof.

### Gate 0 — approval and one candidate version

- [ ] Obtain explicit approval for the exact `VERSION`, source push, signed tag,
      crates.io publication, GitHub Release, and issue update.
- [ ] Choose one stable `VERSION` and release date. Do not reuse a failed
      candidate tag or silently change the version during the run.
- [ ] Update the workspace/package versions, `Cargo.lock`, Rust client metadata,
      changelog, release notes, release evidence, feature-parity metadata, and
      every current-version assertion in the validation scripts.
- [ ] Confirm the release notes contain the required headings and no
      pre-publication wording such as “unpublished”, “candidate”, “not ready”,
      or “do not publish”. Run the release-note generator and validator before
      tagging.

### Gate 1 — validate the candidate source before any tag

Run the complete local gate set, not a narrowed substitute:

```console
cargo fmt --all -- --check
python3 scripts/check-version-sync.py
python3 scripts/check-feature-parity.py
python3 scripts/check-release-documentation.py
python3 scripts/check-documentation-coverage.py
python3 scripts/check-documentation-depth.py
python3 scripts/check-reliability-matrix.py
python3 scripts/check-public-readonly-adapters.py
python3 scripts/check-web-ir-corpus.py --baseline benchmarks/results/web-ir-v1.json
scripts/check-rust-workspace.sh test
scripts/check-rust-workspace.sh clippy
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --locked --no-deps
cargo deny check
cargo audit
cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets
scripts/release-validate.sh
```

- [ ] Run the recorded live-browser and PTY suites where the release evidence
      requires them; record the target, architecture, browser, Rust toolchain,
      and exact commands.
- [ ] Package both crates and inspect their file lists:

```console
cargo package --package glass-browser --locked
cargo package --package glass-dev --locked --no-verify \
  --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'
```

- [ ] Run both crates.io publish dry runs without uploading:

```console
cargo publish --package glass-browser --locked --dry-run --no-verify
cargo publish --package glass-dev --locked --dry-run --no-verify \
  --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'
```

- [ ] Run the clean-install/upgrade smoke test from the previous published
      version. Do not treat a local `cargo install --path` as a registry test.
- [ ] Confirm the working tree contains no generated profiles, screenshots,
      logs, package archives, or unrelated changes.

### Gate 2 — commit and prove the exact source

- [ ] Commit the candidate with the release metadata and documentation only
      after Gate 1 passes.
- [ ] Push the release commit to `main`; capture `SOURCE_SHA` and verify
      `origin/main` resolves to that exact SHA.
- [ ] Wait for the complete main CI workflow for `SOURCE_SHA`. Query the run's
      `headSha` and `conclusion`; do not accept “the latest CI is green” when it
      tested a different commit.
- [ ] If CI fails and is rerun, record both the original failed run and the
      successful rerun. A successful rerun does not erase the original failure.

### Gate 3 — generate, sign, and push exactly one tag

- [ ] Generate the tagged release notes from `SOURCE_SHA`:

```console
python3 scripts/generate-release-notes.py \
  --version "$VERSION" \
  --tag "v$VERSION" \
  --commit "$SOURCE_SHA" \
  --repository wanazhar/glass \
  --run-url "https://github.com/wanazhar/glass/actions/runs/$CI_RUN_ID" \
  --output "/tmp/glass-release-notes-$VERSION.md"
```

- [ ] Re-run the release-documentation and release-note validators after
      generating the notes.
- [ ] Create a signed annotated tag on `SOURCE_SHA`, verify it locally with
      `git tag -v`, and verify GitHub reports the tag signature as valid.
- [ ] Push the tag only after the source CI and signature checks pass. Never
      move, delete, or force-update a published or failed release tag.

### Gate 4 — let the exact-tag workflow publish

- [ ] Confirm that `v$VERSION` triggered
      `.github/workflows/crates-release.yml`; capture its exact run ID.
- [ ] Watch the entire run to a terminal `success` conclusion:

```console
gh run watch "$RELEASE_RUN_ID" --exit-status --interval 15
gh run view "$RELEASE_RUN_ID" --json status,conclusion,headSha,jobs
```

- [ ] Confirm the workflow completed validation, package checks, dry runs,
      registry-state checks, ordered publication (`glass-browser` first,
      `glass-dev` second), clean registry installs, and GitHub Release creation.
- [ ] Do not run a separate `cargo publish`, `gh release create`, or manual
      registry upload while the exact-tag workflow is running. If the workflow
      fails, stop and use the recovery rules below.

### Gate 5 — verify public publication and native evidence

- [ ] Query the exact crates.io version endpoint for both crates with an
      explicit user agent. Require HTTP 200, the exact version, and
      `yanked: false`.
- [ ] Search for and clean-install both exact registry versions, then run the
      installed `glass`, `glass-browser`, and `glass-dev` help smoke checks.
- [ ] Verify the GitHub Release for `v$VERSION` is non-draft, non-prerelease,
      marked latest, source-only, and attached to the exact signed tag.
- [ ] Dispatch native certification for the exact tag and exact
      `EXPECTED_SHA`; record the run ID and every job result.
- [ ] Record the fuzz workflow run that tested the release source. If it tested
      a different commit, label it as non-exact-source evidence rather than
      claiming exact-tag fuzz coverage.

### Gate 6 — record and close

- [ ] Add the source SHA, signed tag, CI run, release run, publication
      timestamps, crate states, clean-install result, native run, fuzz run, and
      GitHub Release URL to `docs/release-evidence.md`.
- [ ] Mark the versioned checklist section complete only after every Gate 0–5
      proof is recorded.
- [ ] Run `python3 scripts/check-github-releases.py` and the documentation
      validators against the final tree.
- [ ] Confirm `git status --short` is clean and `main` is synchronized with
      `origin/main`.
- [ ] Only now announce the release, close the issue, or start the next
      version.

### Mandatory failure recovery

- **Candidate validation or source CI failure before publication:** stop.
  Fix the source in a new commit and use a new version/tag. Retain the failed
  tag as immutable audit evidence, record the failed workflow, add it to the
  explicit failed-candidate allowlist, and verify that neither crates.io nor
  GitHub has a public release for it.
- **Exact-tag validation failure before publication:** never delete or move the
  tag and never claim the version released. The next candidate must use a new
  version; update the changelog, release evidence, and checklist before
  restarting Gate 0.
- **Partial publication or registry/API ambiguity:** do not bump the version
  merely to hide the state and do not issue an independent duplicate publish.
  Query both registry endpoints and the GitHub Release, then rerun the
  idempotent release/record workflow with explicit approval.

For a failed final-record step after publication, use the idempotent verifier
instead of manually publishing again:

```console
gh workflow run crates-release.yml --ref main \
  -f publish=false -f verify_release_records=true
```

- **Transient CI/API failure:** record the original failure and the rerun.
  “Rerun passed” is valid evidence only when the full rerun concludes
  successfully and its exact SHA is recorded.

### Required release record

Every versioned section must contain these non-empty fields:

| Field | Required value |
|---|---|
| Version/date | Stable `X.Y.Z` and publication date |
| Source | Candidate commit SHA and exact tag commit SHA |
| Signature | Local and GitHub tag-verification result |
| CI | Main CI run ID, exact `headSha`, and terminal conclusion |
| Release workflow | Exact-tag run ID and terminal conclusion |
| Registry | Both crate version endpoints, timestamps, and yanked state |
| Installation | Clean exact-version registry install result |
| Native/fuzz | Exact source run IDs or explicit bounded non-exact evidence |
| GitHub | Non-draft, non-prerelease source-only Release URL |
| Closure | Final clean tree and issue/update record |

The release is incomplete while any required field or gate is blank.

## 0.3.9 release candidate

- [x] Certify issue #36 scenarios A-J, gates 1-15, and every forbidden outcome.
- [x] Pass the complete local workspace, docs, package, security, fuzz, live
      browser, PTY, clean-install, and publish dry-run gates.
- [x] Verify the signed exact tag, ordered registry publication, clean registry
      installs, native CI, fuzz, GitHub Release, and issue closure.

The signed `v0.3.6`, `v0.3.7`, and `v0.3.8` tags are retained as failed,
unpublished candidates. They exposed blocking TUI startup, an unsynchronized
Windows-only daemon request fixture, and Windows scheduler-sensitive daemon
tests respectively. `v0.3.9` contained all three repairs and was the only
candidate eligible for publication.

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

This check requires published, non-draft, non-prerelease GitHub Release records
for every release tag except the explicitly enumerated immutable failed
candidates. Those failed tags must exist and must not have public Release
records.

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
