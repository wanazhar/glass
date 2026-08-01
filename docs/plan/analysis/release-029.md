# Glass 0.2.2 Issue #29 Delivery Outline

## Scope

Issue #29 defines the complete 0.2.2 release goal: make safe browser execution easier and cheaper for agents while preserving explicit dispatch, revision safety, policy gates, verification, crates.io-only distribution, and the existing cross-interface contract.

This plan treats the issue as six dependency-ordered phases. Each phase ends with focused verification and one or more local conventional commits. No phase publishes to crates.io or pushes to a remote.

## Current baseline

Observed in the source checkout before this plan:

- Effective capability negotiation is partially implemented in `src/capabilities.rs` and is the first 0.2.2 increment already committed as `2ca2b85`.
- `glass doctor` exists as a partial offline diagnostic in `src/cli/runner.rs`; it lacks the complete stable finding model, JSON mode distinction, MCP configuration generation, and installation/path remediation required by the issue.
- Low-level browser primitives, revision guards, semantic observation, workflows, knowledge, diagnostics, checkpoints, and reliability fixtures already exist.
- No canonical cross-interface error envelope covers all public failure paths.
- No canonical `minimal` / `normal` / `diagnostic` response mode or bounded result artifact store exists.
- No task-oriented `inspect_page`, `find_target`, `act_and_verify`, `extract_structured`, or `recover_run` contract is exposed across Rust, CLI JSON, and MCP.
- Checkpoint export is a bounded browser-state artifact, but it is not the issue's redacted, versioned session snapshot contract or snapshot CLI family.
- Workflow authoring exists, but the five requested starter templates and template commands are not exposed.
- Large responsibility clusters remain in `src/browser/session/types.rs`, `action.rs`, `authoring.rs`, and `workflow.rs`, and public exports require an ownership audit.
- The crates.io package boundary is documented; TypeScript and Python clients are
  repository clients and are not published as part of the crate.

## Phase plan

| Phase | Issue sections | Primary outcome | Atomic commit boundary |
|---|---|---|---|
| 1. Contracts | A, B, C, E, F | Canonical errors, retry semantics, result modes/store, tolerant protocol fixtures, and effective negotiation completion | One commit per contract family; phase gate requires cross-interface serialization fixtures |
| 2. Installation | 1, 3 | Complete doctor diagnostics, deterministic MCP config generation, local result commands, and bounded redacted artifact lifecycle | One commit for doctor/config, one for result store/CLI |
| 3. Agent operations | 2, 6 | Bounded inspect/find/act/verify/recover/extract operations wired through the shared guarded runtime | One commit per operation family plus one real MCP/CLI integration commit |
| 4. State and authoring | 7, 8 | Versioned redacted snapshots, deterministic snapshot diff/purge, and five workflow starter templates | One commit for snapshots, one for templates |
| 5. Maintainability and distribution | 9, 10, H, I | Compile-tested examples, stable/experimental docs, client status, module decomposition, ownership docs, and preserved golden outputs | One commit per documentation/example/decomposition slice |
| 6. Release gates | 11 and release-wide criteria | Full deterministic, package, docs, client, and local ARM64 evidence gates; no publication | Verification-only commit(s) only when evidence/docs change |

## Contract decisions

1. Rust types are the source contract. CLI JSON and MCP serialize the same typed values; clients consume the same field names and stable codes.
2. Every public failure has a namespace code, phase, mutation possibility, retry classification, and recommended bounded operation. Subsystem detail remains nested and bounded.
3. `minimal` is the default agent response. `normal` adds bounded target/effect/policy context. `diagnostic` adds references to local bounded evidence rather than embedding unbounded traces.
4. Result and snapshot stores are local, atomic, size-bounded, redacted, and prunable. They never store cookies, authorization headers, passwords, payment data, typed form values, screenshots, or full DOM by default.
5. High-level operations call the existing guarded executor and never bypass policy, fresh observation, revision checks, effect handling, or verification.
6. Ambiguity, stale revisions, and potentially completed mutations fail closed. Retry guidance never authorizes blind replay of a non-idempotent mutation.
7. Discovery manifests remain separate from negotiated agreements. Additive response fields are tolerated; incompatible schema versions fail explicitly.
8. TypeScript and Python remain experimental repository clients for 0.2.2. Cargo remains the only release installation path.
9. Module moves preserve public behavior and golden serialized output. New ownership docs explain responsibility boundaries rather than adding abstractions without callers.

## Integration map

- CLI command parsing -> runner dispatch -> policy/session construction -> typed result/error serialization.
- MCP initialize -> capability negotiation -> immutable agreement -> tool schemas/results/errors.
- High-level operations -> fresh observation -> intent/target resolution -> guarded action -> effect witness -> verification -> result mode projection.
- Result store -> CLI `result show` / `result purge` and diagnostic references in CLI/MCP responses.
- Session snapshot -> redaction/bounds validator -> local snapshot store -> inspect/diff/purge commands.
- Workflow templates -> authoring parser/compiler -> deterministic fixture compilation.
- Rust public contracts -> TypeScript/Python protocol types and conformance fixtures.
- Package metadata/docs/examples -> cargo package/docs.rs and crates.io package
  release checks.

## Non-goals

No native binary releases, installers, hosted browsers, Windows support, cloud execution, hidden planner, marketplace, remote extension registry, npm/PyPI publication pipeline, additional browser engine, or crates.io publication is part of this development run.

## Phase gate

A phase is complete only when its focused tests and relevant existing tests pass, its documentation matches the implementation, its commit is local and conventional, and the next phase can consume the committed contract without a stub, mock, or compatibility alias.
