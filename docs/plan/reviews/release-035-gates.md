# Glass v0.3.5 issue #35 gate review

Status: local candidate verified on 2026-08-11; exact-tag and native remote
evidence pending

This review maps every release gate, integrated scenario, and forbidden outcome
in issue #35 to executable evidence. A checked item means the implementation
and host-local proof exist. Native Windows, exact-tag CI, GitHub signature
verification, publication, and registry propagation remain unchecked until
those environments have produced their own evidence.

## Integrated scenarios

### A. Untrusted repository

- [x] `workspace::tests::untrusted_open_never_executes_or_privileges_project_configuration`
      opens a fixture containing project commands, hooks, tools, and skills
      without executing or elevating them, while static project inspection
      remains available.
- [x] `workspace::tests::trust_once_activates_without_persisting_and_project_trust_persists`
      proves explicit activation and the difference between process-local and
      persisted trust.
- [x] `development_runtime::yolo_does_not_bypass_workspace_trust_and_normal_mode_stays_gated`
      proves the public CLI cannot use `--yolo` to bypass this boundary.

### B. Native Pi session integration

- [x] `pi_runtime::tests::runtime_asset_never_invokes_pi_rpc_cli` and
      `request_mapping_uses_native_sdk_operations` prove the embedded runtime
      imports the Pi SDK and maps prompt, steer, follow-up, compact, fork,
      model, thinking, and session operations without `pi --mode rpc`.
- [x] `native_sdk_starts_and_reports_capabilities_when_installed` is the
      host-native SDK smoke test; Pi lifecycle, tool dispatch, and reconnect
      are additionally covered by agent and daemon tests.
- [ ] Repeat the exact-tag native Pi smoke in release CI and retain its log.

### C. Autonomous task DAG

- [x] `tasks_dispatch_prompts_verify_and_wake_dag_dependents` proves automatic
      dispatch, external verification, completion, and dependent wake-up.
- [x] `eight_ready_tasks_dispatch_before_integration_dependency_wakes` proves
      concurrent leaves and gated integration.
- [x] `verification_failure_retries_then_blocks_descendants` and
      `external_verification_evidence_is_proof_not_agent_claim` prove that an
      agent settling is not sufficient evidence of success.

### D. Multi-workspace daemon concurrency

- [x] `workspace_actors_do_not_serialize_unrelated_long_operations` runs a
      one-second kernel operation in workspace A while browser, LSP, DAP, test,
      process, and inspect requests in workspace B complete in under 300 ms.
      Workspace actors own long work; the registry lock does not.

### E. Windows durable workspace

- [x] The Windows implementation uses local Glass-owned named pipes and
      job-object process-tree ownership; pipe validation is host-tested.
- [ ] Run
      `windows_named_pipe_daemon_lifecycle_reconnect_and_permissions` on native
      Windows exact-tag CI. Compile-only or Unix evidence does not certify this
      scenario.

### F. Automated experiments

- [x] `isolated_worktrees_are_ranked_and_selected_from_evidence` creates three
      isolated implementations, assigns three native Pi candidates when Pi is
      installed, collects measured build/test/process/browser evidence with
      provenance for each, ranks all three automatically, and permits selection
      only of the evidence-derived recommendation.
- [x] Missing environment evidence is represented as unavailable provenance,
      never as an invented passing measurement.

### G. Debugger matrix

- [x] DAP framing, bounded reverse requests, `runInTerminal` through the Glass
      process owner, adapter crash, timeout, TCP transport, and cleanup have
      deterministic executable tests.
- [x] The installed debugpy adapter has a real breakpoint/continue smoke path.
- [ ] Run real LLDB and Delve breakpoint/stack/continue smokes with the pinned
      adapters in exact-tag native CI. Their tests intentionally do not turn an
      absent adapter into a certification claim.

### H. Kernel to Glass capabilities

- [x] `governed_python_queries_browser_tests_and_graph_before_approved_mutation`
      executes the complete scenario through a persistent Python kernel and the
      same `DevelopmentToolRouter`: browser state, test results, graph path,
      rejected unapproved write, approved write, and dual initiator/executor
      replay provenance.
- [x] Kernel recursion, capability count, output, code, message, timeout, and
      cancellation limits fail closed. Documentation states that Python,
      JavaScript `vm`, SQL, and shell contexts are not security sandboxes.

### I. Full-stack repair

- [x] The flagship flow is an executable evidence suite, not a mocked product
      path: scenario A supplies trust; C supplies the repair DAG; D supplies
      concurrent LSP/browser/DAP/test/process evidence; F supplies competing
      fixes and recommendation; daemon reconnect tests preserve resident
      agent/process/kernel/browser state; TUI tests exercise desktop, compact,
      and mobile recovery surfaces against the same workspace state.
- [x] `observable_projection_populates_every_issue_35_graph_resource_kind`
      joins repository, editor, Git, LSP, DAP, browser, Web IR, workflow, test,
      kernel, agent, task, experiment, and tool evidence in graph/replay.
- [ ] Retain the exact-tag release-CI logs for the native Pi, browser, DAP, and
      Windows portions before calling the combined demonstration certified on
      every supported platform.

## Release gates

### Gate 1. Workspace Trust

- [x] Repository-controlled hooks, commands, tools, tests, LSP/DAP processes,
      and privileged project skills are inert before explicit trust.
- [x] Trust is identity-bound, external to the repository, visible in the TUI,
      and cannot be elevated by daemon reconnect or `--yolo`.

### Gate 2. Native Pi SDK Runtime

- [x] `glass-dev` owns a Glass-specific Pi SDK host protocol; the old CLI RPC
      harness is not its primary embedded architecture.
- [x] There is no generic harness adapter or OMP embedding.

### Gate 3. Product Boundary Completion

- [x] `glass-dev` owns all development runtime contracts. `glass-browser` has
      no `development-runtime` feature, development module, compatibility CLI,
      compatibility MCP tools, or development TUI state.
- [x] The dependency remains one-way: `glass-dev -> glass-browser`; the browser
      crate checks independently with `--no-default-features`.

### Gate 4. Autonomous Task DAG

- [x] Tasks dispatch, prompt workers, consume evidence, verify, settle,
      complete/fail, wake dependencies, retry, enforce budgets, and propagate
      blocked state automatically.

### Gate 5. Daemon Concurrency

- [x] Per-workspace actors and bounded queues keep unrelated workspace work
      concurrent; long work does not retain the global workspace-map lock.

### Gate 6. Cross-Platform Durability

- [x] Unix authenticated local IPC, reconnect, resource persistence, and
      process-tree cleanup are host-certified.
- [ ] Native Windows named-pipe reconnect and job-object cleanup require the
      exact-tag Windows job.

### Gate 7. Automatic Experiments

- [x] Primary build, test, workflow, semantic, visual, performance,
      diagnostics, debugger, diff, health, and crash dimensions are collected
      from resident services with per-dimension provenance and availability.

### Gate 8. DAP Breadth

- [x] The client supports stdio/TCP adapters and supervised reverse requests.
- [ ] Exact-tag native logs must certify debugpy, LLDB, and Delve; only debugpy
      is locally installed and certified here.

### Gate 9. Kernel Bindings

- [x] Persistent Python, JavaScript, shell, and SQL kernels invoke the governed
      router with capability allowlists, trust/revision/mutation enforcement,
      dual actor attribution, replay evidence, and bounded recursion.

### Gate 10. TUI Parity

- [x] Trust, tasks, daemon/recovery, debugger, experiments, kernels, tests,
      browser, process, Git, and agent lifecycle actions have palette routes.
- [x] Desktop, compact, and purpose-built mobile layouts have drill-down and
      trust-prompt snapshot coverage in focused TUI modules.

### Gate 11. Performance and Reliability

- [x] Full-suite cross-service concurrency has a hard responsiveness bound;
      queues, events, schemas, code, output, logs, and sessions are bounded.
- [x] The browser's structured-first observation, explicit screenshot,
      interaction, cleanup, and live Chromium contracts remain independently
      tested.

### Gate 12. Release Quality

- [x] Rust, Python, TypeScript, documentation, packages, and release notes are
      synchronized at 0.3.5; release automation rejects thin tag notes.
- [x] Both crates package and verify; clean core/full installs, ownership
      transitions, reinstallation, and published 0.3.4 to candidate 0.3.5
      upgrade pass in isolated roots.
- [ ] Create the final signed `v0.3.5` tag only after all checks pass, make its
      signing key/email verifiable by GitHub, then retain exact-tag CI,
      security/fuzz, ordered publication, registry install, and GitHub Release
      evidence. This is intentionally not satisfiable by a moving worktree.

## Forbidden-outcome audit

| # | Outcome excluded by implementation/evidence |
|---:|---|
| 1 | Trust is enforced in code and tested; it is not a warning in documentation. |
| 2 | Project hooks remain inert until explicit trust activation. |
| 3 | Untrusted project skills are metadata only and never privileged instructions. |
| 4 | Project tool declarations cannot weaken the central trust policy. |
| 5 | The Pi SDK host imports native SDK modules and never invokes `pi --mode rpc`. |
| 6 | No generic multi-harness abstraction was introduced. |
| 7 | OMP is not embedded. |
| 8 | The browser-owned `development-runtime` bridge and code were removed. |
| 9 | The task scheduler dispatches and drives workers; it is not an idle-session list. |
| 10 | Ready tasks prompt workers automatically. |
| 11 | Independent verification, not `agent_settled`, decides task success. |
| 12 | Long operations run in per-workspace actors outside the registry lock. |
| 13 | Windows remains explicitly uncertified until the native job passes. |
| 14 | Experiment metrics come from resident collectors with measured provenance. |
| 15 | LLDB and Delve remain explicitly uncertified until real adapter jobs pass. |
| 16 | Kernel documentation explicitly denies OS sandbox guarantees. |
| 17 | Kernel calls re-enter `DevelopmentToolRouter`; no side channel exists. |
| 18 | Browser performance/mobile contracts retain their independent tests and defaults. |
| 19 | Development TUI state, commands, rendering, and shell control are decomposed. |
| 20 | Release automation requires substantive verified-tag notes for the GitHub Release. |

## Candidate validation record

The final direct local run on Linux ARM64 recorded:

- `scripts/check-rust-workspace.sh test`: browser 734 passed/1 ignored plus all
  binary, integration, example, PTY, protocol, and package targets; Glass Dev
  118 passed plus its 4 black-box integration scenarios.
- `cargo fmt --all -- --check`, strict all-target/all-feature Clippy for both
  crates, browser `--no-default-features`, and warnings-denied workspace
  rustdoc all passed.
- The full 18-test live Chromium suite passed with explicit sandbox opt-out;
  the semantic execution sample measured a 69% estimated token reduction.
- The pinned native Pi SDK smoke, three native Pi experiment workers, and an
  isolated real debugpy 1.8.21 breakpoint/continue session passed.
- Both crates packaged and verified: browser 170 files/714.4 KiB compressed;
  Dev 52 files/236.9 KiB compressed. The packaged Dev dependency is exactly
  `glass-browser =0.3.5`. Both publish dry-runs reached the aborted-upload
  boundary; Dev used the local patch expected before browser publication.
- Isolated core/full installs, core-to-full, full reinstall, full-to-core
  ownership, and published 0.3.4 to candidate 0.3.5 upgrade all passed.
- Version, feature, release-truth, documentation coverage/depth, reliability,
  public read-only adapter, and Web IR corpus validators passed. Coverage
  measured 383 Markdown files, 292 full-product MCP tools, 100 browser-only
  tools, 17 examples, and 21 public modules.
- TypeScript build/typecheck/package/handshake and Python wheel/handshake
  passed against the current catalog, including untrusted static inspection
  and executable-service denial.
- `cargo deny check` passed. `cargo audit` found no denied vulnerability and
  retained the configured `lru 0.18.1` warning. The locked fuzz workspace built
  and all six fuzz targets completed 512 sanitizer runs without a crash.
- Free space was checked throughout. Only validated disposable incremental
  compiler caches and generated fuzz corpus files were removed; source and
  the reusable compiled target were retained.
