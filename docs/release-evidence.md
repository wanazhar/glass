# Release evidence

## 0.3.4 local release evidence

This checkout is a local `0.3.4` release candidate implementing authoritative
issue #34. No push, tag, crates.io publication, issue mutation, or GitHub
Release has been performed.

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

Final workspace, documentation, package, and clean-install measurements are
recorded below after the convergence commands complete.

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
