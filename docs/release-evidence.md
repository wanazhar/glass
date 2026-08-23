# Release evidence

## 0.3.12 release evidence

This record tracks the exact signed source and publication gates for the
0.3.12 release. Fields below come from command output, workflow records,
registry responses, or GitHub records; prior-release evidence is not reused.

- Source commits: release metadata was prepared in
  [`33ff290f601adea114df210a2def4173307ad052`](https://github.com/wanazhar/glass/commit/33ff290f601adea114df210a2def4173307ad052);
  the cross-platform CI fixes landed in the final release source
  [`2cf23a66625c535bd6257a2fcee45db567db2aa2`](https://github.com/wanazhar/glass/commit/2cf23a66625c535bd6257a2fcee45db567db2aa2).
  At release closure, `origin/main` and `v0.3.12` both resolved to the final
  SHA. This is historical closure evidence, not a claim about the current
  `origin/main` ref.
- Signed tag: local `git tag -v v0.3.12` passed with EDDSA key
  `C7102B6A568EABDE023F818528E01A5852DB1559`; GitHub reports the annotated
  tag object as `verified: true`, reason `valid`, and the tag commit is the
  final SHA.
- Main CI: the first source run
  [`32569739176`](https://github.com/wanazhar/glass/actions/runs/32569739176)
  tested `33ff290f601adea114df210a2def4173307ad052` and failed: Clippy
  rejected `chunks_exact(2)` under the current toolchain and browser-free
  macOS/Linux/Windows contract tests assumed a ready Pi runtime. The fix was
  pushed as a new commit; final CI
  [`32570431822`](https://github.com/wanazhar/glass/actions/runs/32570431822)
  completed successfully with `headSha` equal to the final SHA.
- Fuzz: exact-source Parser fuzz smoke run
  [`32570431823`](https://github.com/wanazhar/glass/actions/runs/32570431823)
  completed successfully with the final SHA.
- Release workflow:
  [`32571020731`](https://github.com/wanazhar/glass/actions/runs/32571020731)
  completed successfully with `headSha` equal to the final SHA. Validation job
  `97026373388`, ordered publication job `97027722571`, and GitHub Release job
  `97028855346` all passed. The publish job completed the crate publication and
  exact registry-install help smoke tests.
- Registry: crates.io returned HTTP 200, exact version `0.3.12`, and
  `yanked: false` for both crates. `glass-browser` was published at
  `2026-08-22T11:57:17.224521Z`; `glass-dev` was published at
  `2026-08-22T11:58:16.453206Z`.
- Native certification: exact-tag run
  [`32572181403`](https://github.com/wanazhar/glass/actions/runs/32572181403)
  passed with the final SHA. Native Pi, experiments, and Chromium job
  `97029129743` and Windows named-pipe lifecycle job `97029129786` both
  passed.
- GitHub Release:
  [`v0.3.12`](https://github.com/wanazhar/glass/releases/tag/v0.3.12) is
  non-draft, non-prerelease, latest, source-only, has no assets, and is
  attached to the verified signed tag. It was published at
  `2026-08-22T12:06:26Z`.
- Closure: `python3 scripts/check-github-releases.py` passed with 38 published
  tags and 4 retained failed candidates. Final documentation and repository
  synchronization checks are recorded in the closing commit.

## 0.3.11 release evidence

The candidate source commit `66d760959213aa4b7eea8d05093f8a424061fb8e` carries
the post-0.3.9 TUI responsiveness, browser feedback, agent-composer,
projection, and documentation hardening changes. Signed annotated tag `v0.3.11`
points to commit `37507fd8c72cb2703a6ff48f4af56ad896fbeba6`; GitHub reports its
signature as verified.

Exact-tag release workflow run `32039662208` passed tag verification, all
validation, package, clean-install, and publish-dry-run gates. Its ordered
publication job `95419228613` published `glass-browser 0.3.11` at
`2026-08-17T14:56:00.295932Z` and `glass-dev 0.3.11` at
`2026-08-17T14:56:58.989380Z`; both are unyanked. The source-only GitHub
Release is non-draft, non-prerelease, and has no assets.

The first run's final record step exposed a collection-endpoint inconsistency
and omitted the newly retained failed `v0.3.10` tag. Commit `9d985ef` changes
the checker to verify each tag endpoint and records `v0.3.10` as failed.
Manual verification run `32041501320` passed the complete validation job
`95421519847` and the corrected release-record job `95423411306`.

Native certification run `32042409142` passed the pinned Pi SDK, automatic
experiments, all live Chromium scenarios, and native Windows named-pipe
lifecycle. Fuzz run `32042326767` passed `mcp_frame`, `cdp_message`, `ax_dom`,
`locator`, `url_policy`, and `semantic_contracts`.

Post-publication main CI run `32044348297` initially failed: the macOS
browser-free package check exposed the `openpty` pointer ABI mismatch, the
Windows browser-free suite had one scheduler-sensitive daemon operation
failure, and GitHub action downloads also returned transient 429/502 errors.
The PTY portability fixes landed in commit `5d16cb8`; the failed-job rerun
completed successfully for the exact `5d16cb8` source (`headSha` verified).
This is post-publication CI evidence and does not change the immutable
`v0.3.11` tag or published crates.

## 0.3.10 failed candidate

Signed tag `v0.3.10` points to commit
`c4334d4ae6a039bb426efc4c764803f09d1c1bd7`; local signature verification
passed. Release workflow run `32038818476` stopped in validation because the
tagged release notes contained forbidden pre-publication wording. No crates.io
publication or GitHub Release was created. The tag remains immutable audit
evidence.

## 0.3.9 release evidence

Issue #36 candidate evidence is recorded in
[`plan/reviews/release-036-gates.md`](plan/reviews/release-036-gates.md).
Signed tag `v0.3.9` points to
`f9c4d5f507e1a14905442487aa5a54ac7f2e42ce`; GitHub reports its signature as
verified. Exact-tag CI run 31594125510 and fuzz run 31594125431 passed. Release
run 31594125413 passed validation, ordered publication, and clean registry
installs. Its final record check exposed a stale validator assumption; commit
`9105682` now distinguishes the three immutable failed candidates and the
corrected validator passes with 36 published records.

crates.io published `glass-browser 0.3.9` at
`2026-08-12T12:11:46.815942Z` and `glass-dev 0.3.9` at
`2026-08-12T12:12:47.073595Z`; neither is yanked. The source-only GitHub Release
is non-draft, non-prerelease, and has no assets. Native certification run
31596213668 passed pinned Pi, automatic experiments, all 18 live Chromium
scenarios, and native Windows named-pipe reconnect/process-tree lifecycle.

## 0.3.8 release evidence

Signed tag `v0.3.8` points to
`86fa51bcf6b02b7f0a0ba03fe2b91b68957210f9`. Exact-source Windows CI run
31591017832 passed the repaired all-target package check, then exposed two
scheduler-sensitive daemon assertions under parallel native load. Release run
31591019273 was cancelled before either crate was published and before a GitHub
Release was created; both crates.io version endpoints returned 404 after
cancellation. The immutable failed tag was superseded by 0.3.9.

## 0.3.7 release evidence

Signed tag `v0.3.7` points to
`0e67e581d0398181cbe3c99abe22d5f6ea3b6393`. Exact-tag CI run 31589287135
proved that the Windows-only native daemon integration fixture omitted the new
optional `operation_id` field, so the all-target package check failed before
native execution. The release workflow did not publish crates or create a
GitHub Release. The immutable failed tag was superseded by later repairs.

## 0.3.6 release evidence

Signed tag `v0.3.6` points to
`3c1ba0397db32afdf2c34f4ce2e1481c797d9e28`. Exact-tag CI run 31587950535
exposed that automatic Chrome startup blocked the TUI input loop on a clean
runner, causing the five-second PTY quit contract to fail. The release workflow
did not publish crates or create a GitHub Release. The tag remains immutable as
failed-candidate evidence; it was superseded by the later repairs.

## 0.3.5 release evidence

This section records the immutable and public evidence for the signed
`v0.3.5` release implementing authoritative issue #35.

### Published 0.3.5 records

- Signed annotated tag `v0.3.5` points to
  `3c528689b70396ac5f30367ed89f4d13e3d0ee78`. Local verification succeeded
  with EdDSA key `C7102B6A568EABDE023F818528E01A5852DB1559`; GitHub reports the
  signature as verified and valid.
- [Release workflow run 31547613725](https://github.com/wanazhar/glass/actions/runs/31547613725)
  passed exact-tag signature/version checks, the complete test and
  documentation gates, package verification, clean source-package installs,
  publication dry-runs, ordered crates.io publication, clean registry installs,
  and GitHub Release creation.
- [Exact-tag fuzz run 31547613703](https://github.com/wanazhar/glass/actions/runs/31547613703)
  passed all six bounded sanitizer targets. Exact-tag
  [CI run 31547613719](https://github.com/wanazhar/glass/actions/runs/31547613719)
  passed every Linux, macOS, Windows, client, dependency/security, and real
  debugpy/LLDB/Delve job on attempt 4. Earlier Windows attempts exposed
  teardown and scheduling races that were hardened on `main` before closure.
- [Exact-source native certification run 31549718984](https://github.com/wanazhar/glass/actions/runs/31549718984)
  verified the signed tag and commit before running the pinned Pi SDK session,
  three-candidate automatic experiment, all 18 live Chromium scenarios, and
  native Windows named-pipe reconnect/process-tree lifecycle.
- crates.io published `glass-browser 0.3.5` at
  `2026-08-11T23:59:29.613432Z` and `glass-dev 0.3.5` at
  `2026-08-12T00:00:17.312955Z`; neither version is yanked.
- [GitHub Release v0.3.5](https://github.com/wanazhar/glass/releases/tag/v0.3.5)
  was published at `2026-08-12T00:06:41Z`. It is non-draft, non-prerelease,
  source-only, and has no attached binary assets.

### Post-tag hardening

- `test(debugger): accept raced adapter crash errors` recognizes both typed
  protocol EOF and OS broken-pipe outcomes from a deliberately crashing DAP
  adapter.
- `fix(agents): join owned workers on shutdown` retains and joins agent worker
  handles on completion, cancellation, and registry drop. The focused agent
  suite passed, followed by 20 repeated teardown runs.
- The reusable native certification workflow binds evidence to an explicit
  signed tag and expected commit SHA; it cannot silently certify the moving
  default branch in place of the release source.

## 0.3.4 release evidence

This section records the local and public evidence for the signed `v0.3.4`
release implementing authoritative issue #34.

- The full Development Workspace is owned and consumed by `glass-dev`; the
  dependency remains one-way to independently buildable `glass-browser`.
- Pi 0.84.1 completed real RPC startup with the packaged coding extension, and
  tests exercised multiple independent session processes, dependencies,
  cancellation, model/thinking state, and durable brokerage.
- Installed rust-analyzer served two actors through one resident service. A
  disposable debugpy 1.8.21 environment proved initialize, deferred launch,
  verified breakpoint, stop, threads, stack, scopes, variables, evaluation,
  continue, termination, and adapter cleanup.
- Snap Chromium executed the revision-safe local fixture, accessibility
  snapshot, semantic observation, observation diff, typing, clicking, workflow
  authority, and clean child/port shutdown. Durable-daemon evidence retained a
  Pi agent, PTY process, SQL kernel, and browser identity across fresh clients.
- Native Git, structured tests, persistent kernels, isolated worktrees,
  evidence ranking, graph paths, replay diffs, desktop/phone TUI snapshots, and
  a real PTY TUI smoke are covered by executable tests.
- `glass --mcp` advertises both the browser and resident development catalogs;
  a JSON-RPC integration test calls both with clean stdout framing. Direct CLI
  mutations remain authority/confirmation gated outside `--yolo`.

### Published 0.3.4 records

- Signed annotated tag `v0.3.4` points to
  `739b2e6a461cf17d5a776a3d6c2cf98b83c2e83f`; local signature verification
  succeeded with EdDSA key `C7102B6A568EABDE023F818528E01A5852DB1559`.
- [Release workflow run 31442780359](https://github.com/wanazhar/glass/actions/runs/31442780359)
  passed tag validation, package and publication dry runs, ordered publication,
  crates.io propagation, clean registry installs, executable help smokes, and
  GitHub Release creation. Exact-tag
  [CI run 31442780356](https://github.com/wanazhar/glass/actions/runs/31442780356)
  and [fuzz run 31442780360](https://github.com/wanazhar/glass/actions/runs/31442780360)
  also passed.
- crates.io published `glass-browser 0.3.4` at
  `2026-08-10T23:48:44.961163Z` and `glass-dev 0.3.4` at
  `2026-08-10T23:49:44.508902Z`; neither version is yanked.
- [GitHub Release v0.3.4](https://github.com/wanazhar/glass/releases/tag/v0.3.4)
  was published at `2026-08-10T23:57:57Z`. It is non-draft, non-prerelease,
  source-only, and has no attached binary assets.

### Completed 0.3.4 local validation

- `scripts/check-rust-workspace.sh test` passed for both products: 822
  `glass-browser` library tests passed with one intentionally ignored test, all
  binary/integration/example/real-PTY targets passed, and all 36 `glass-dev`
  library plus four full-product integration tests passed. The run included
  available real Pi, rust-analyzer, debugpy, Chromium, durable-daemon, Git,
  worktree, kernel, experiment, and MCP paths.
- `cargo fmt --all -- --check`, warnings-denied all-target/all-feature Clippy,
  warnings-denied workspace rustdoc, and the independent
  `glass-browser --no-default-features` build passed. All release/documentation,
  feature-parity, reliability, public-adapter, and eight-fixture Web IR gates
  passed.
- The full `glass` MCP inventory is pinned at 284 tools and 129,444 serialized
  UTF-8 bytes; `glass-browser` retains its independently checked 133-tool
  catalog. TypeScript build/typecheck/package/handshake and Python
  compile/wheel/handshake smokes passed against `glass 0.3.4`.
- `glass-browser` packages 188 files (4.4 MiB; 855.9 KiB compressed) and verifies
  from normalized package contents. `glass-dev` packages 28 files (500.5 KiB;
  108.0 KiB compressed), verifies with the packaged browser crate, and retains
  the exact `glass-browser =0.3.4` dependency. The locked fuzz all-target build,
  `cargo deny check`, and the current 1,200-advisory RustSec audit passed.
- `GLASS_PREVIOUS_VERSION=0.3.3 scripts/smoke-clean-install.sh` passed isolated
  core-only and full-suite installs, all same-version command-ownership
  transitions, and a real published `0.3.3` to local `0.3.4` upgrade. Installed
  version, help, and capability commands were executed before the temporary
  Cargo root was removed.
- The stripped `release-size` binaries execute as version `0.3.4`:
  `glass` is 9,592,528 bytes and `glass-browser` is 7,742,560 bytes on
  `aarch64-unknown-linux-gnu`.

## 0.3.3 release evidence

This section records the evidence used for the signed `v0.3.3` release on
2026-08-10. Publication is performed by the ordered tag workflow and verified
against crates.io and the matching source-only GitHub Release.

- Workspace, TypeScript, Python, and exact inter-crate versions are `0.3.3`.
- `glass-dev` packages both `glass` and `glass-browser`; the core
  `glass-browser` package remains independently installable. Clean-install
  validation must execute `--help` for both full-suite binaries and the core
  binary in separate Cargo roots.
- Issue #33 maps its 15 pillars, scenarios A–K, and Gates 1–10 to
  [the release analysis](plan/analysis/release-033.md) and focused task files.
- Connection/presentation tests cover independent dimensions, conservative
  unknowns, active graphics evidence, local 30/60 FPS, remote profiles,
  adaptive scale/rate, idle/background suspension, and bounded observability.
- Recovery evidence covers free, verified CDP, unrelated, and unknown
  endpoints; attach always requires a user decision and target ambiguity is
  explicit. Remote View is loopback-only, tokenized, revocable, newest-frame,
  and revision-guarded.
- Runtime evidence covers explicit tree truncation, checked PTY state,
  process-group termination, persistent LSP lifecycle, real Neovim embed RPC,
  attached-agent revision context, and browser-free platform CI.
- The iOS and Android concept JPEGs decode successfully; the iOS source SVG
  is retained alongside the rendered asset.

### Published 0.3.3 records

- Signed annotated tag `v0.3.3` points to
  `f5951f40c0c2fbb0c8cae60f44e7a07840c6ced3`; local signature verification
  succeeded with EdDSA key `C7102B6A568EABDE023F818528E01A5852DB1559`.
- [Release workflow run 31373242351](https://github.com/wanazhar/glass/actions/runs/31373242351)
  passed tag validation, package and publication dry runs, ordered publication,
  crates.io propagation, clean registry installs, executable help smokes, and
  GitHub Release creation. Exact-tag
  [CI run 31373242354](https://github.com/wanazhar/glass/actions/runs/31373242354)
  and [fuzz run 31373242362](https://github.com/wanazhar/glass/actions/runs/31373242362)
  also passed.
- crates.io published `glass-browser 0.3.3` at
  `2026-08-10T09:23:18.508627Z` and `glass-dev 0.3.3` at
  `2026-08-10T09:24:14.018121Z`; neither version is yanked.
- [GitHub Release v0.3.3](https://github.com/wanazhar/glass/releases/tag/v0.3.3)
  was published at `2026-08-10T09:32:07Z`. It is non-draft,
  non-prerelease, source-only, and has no attached binary assets.

### Completed 0.3.3 validation

- `scripts/check-rust-workspace.sh test`: the complete split-package library
  tests passed, one installed-tool availability test was intentionally ignored,
  and every binary, integration test, example and 40x20 real-PTY target passed.
- `GLASS_E2E=1 cargo test -p glass-browser --all-features --test
  browser_smoke --locked -- --nocapture --test-threads=1`: all 19 Chromium
  scenarios passed on Linux 6.17 ARM64 with Chromium 150.0.7871.128.
- Formatting, warnings-denied Clippy, warnings-denied workspace rustdoc,
  browser-core no-default-feature compilation, and separate optimized builds
  of both packages passed on Rust 1.97.0 (`aarch64-unknown-linux-gnu`).
- The installed Pi 0.84.0 binary completed a real offline RPC state handshake
  with the packaged Glass system prompt and expanded coding extension; protocol
  classification tests cover prompt acknowledgement, authoritative assistant
  messages, hidden user-message echoes, and final `agent_settled` handling.
- Documentation validation covers 351 Markdown files, the exact top-level and
  nested CLI inventories, 133 MCP tools, 17 examples and 22 public modules.
  Sixteen substantive guide contracts plus complete current-guide routing are
  depth-checked. Version, feature-parity, reliability, read-only-adapter and
  eight-fixture Web IR corpus checks passed.
- TypeScript build/typecheck/smoke and Python compile/smoke passed against the
  checkout's 0.3.3 executable. `cargo deny`, `cargo audit`, and the locked fuzz
  all-target build passed.
- `glass-browser` packages 188 files (4.4 MiB; 844.0 KiB compressed) and
  `glass-dev` packages 8 files (94.4 KiB; 27.2 KiB compressed). Both package
  verification and publication dry runs passed without upload; the normalized
  development package retains the exact `glass-browser =0.3.3` dependency.
- `scripts/smoke-clean-install.sh` passed isolated core-only and full-suite
  installs plus core-to-full, full-to-full reinstall, and full-to-core command
  ownership transitions. Installed `--version` and `--help` behavior was
  checked for both commands.
- [The issue #33 gate review](plan/reviews/release-033-gates.md) records 53/53
  mandatory checkboxes with their implementation evidence.

This is host-specific native evidence, not a native-platform claim for macOS
or Windows. Those platforms have browser-free CI definitions only in this
release.

## 0.3.2 publication evidence

- Signed annotated tag `v0.3.2` points to
  `4e548421abb6ed27ef1c91024379f7eb7abf3f90`.
- [Release workflow run 31254928934](https://github.com/wanazhar/glass/actions/runs/31254928934)
  passed validation, ordered publication, registry propagation, clean installs,
  executable help smokes, and GitHub Release creation.
- crates.io published `glass-browser 0.3.2` at
  `2026-08-08T11:34:05.365045Z` and `glass-dev 0.3.2` at
  `2026-08-08T11:34:48.438905Z`; neither version is yanked.
- [GitHub Release v0.3.2](https://github.com/wanazhar/glass/releases/tag/v0.3.2)
  is published, non-draft, non-prerelease, marked latest, and source-only.
- The release contains no native binaries, checksum manifests, Sigstore
  bundles, or npm/PyPI client packages.

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
python3 scripts/check-documentation-coverage.py
python3 scripts/check-feature-parity.py
python3 scripts/check-reliability-matrix.py
python3 scripts/check-public-readonly-adapters.py
python3 scripts/check-version-sync.py
python3 scripts/check-web-ir-corpus.py --baseline benchmarks/results/web-ir-v1.json
python3 scripts/check-github-releases.py
cargo package --package glass-browser --locked
cargo publish --package glass-browser --locked --dry-run --no-verify
cargo package --package glass-browser --locked --list
cargo package --package glass-dev --locked --no-verify --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'
cargo publish --package glass-dev --locked --dry-run --no-verify --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'
python3 scripts/check-packaged-dependency.py target/package/glass-dev-0.3.3.crate --version 0.3.3
cargo package --package glass-dev --locked --list
scripts/smoke-clean-install.sh
```

Run the native browser check in the target environment:

```console
GLASS_E2E=1 cargo test -p glass-browser --all-features --test browser_smoke --locked -- --nocapture --test-threads=1
```

Record the target environment and browser details with the result. Do not
convert source inventory or cross-compilation into a support claim for another
OS. The current recorded result is in
[Recorded platform evidence](local-platform.md).

## 0.3.0 release validation

The signed tag `v0.3.0` targets checkout
`2efcb3d1649d84b415d63bf25f9bb5dd713f2dfb`. It passed the complete
revision-bound release suite before publication:

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
- `cargo publish --locked` published `glass-browser 0.3.0` to crates.io.
- GitHub Release `v0.3.0` is published, non-draft, non-prerelease, marked
  `Latest`, and contains no native binary assets.
- A clean registry install reported `glass 0.3.0`; its CLI/MCP smoke passed
  with 85 advertised MCP tools.

GitHub Actions run
[`31118833163`](https://github.com/wanazhar/glass/actions/runs/31118833163)
did not reach repository code: the hosted runner failed during `Set up job`
while resolving action downloads with `Service Unavailable`. The approved
direct `cargo publish --locked` and `gh release create` paths completed the
same crates-only publication boundary, and the published package and release
records were verified independently.

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

The `glass-browser` `0.3.0` crate remains the latest published registry version
until `v0.3.1` is approved and pushed. Its matching GitHub Release entry has no
native binary assets, as expected for the crates-only distribution boundary.
