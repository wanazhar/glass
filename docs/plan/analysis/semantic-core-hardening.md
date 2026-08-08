# Semantic core hardening delivery analysis

Status: Completed direct implementation

This post-issue-32 improvement closes correctness gaps discovered while
reviewing existing Web IR, Task Protocol, and development-agent integration.
It deliberately retains the stable structured-first and explicit-screenshot
product contracts.

## Dependency order

1. Publish the architecture and acceptance contract.
2. Turn the Web IR corpus into executable runtime evidence and add adversarial
   and metamorphic coverage.
3. Scope compilation and evidence to selected graph entities.
4. Introduce revision-bound live bindings and capability-aware interpretation.
5. Propagate semantic state and conservative sensitivity; implement
   `entityState` verification.
6. Route local and Pi agent tools through one bounded authorization gateway.
7. Add compact semantic tools, projections, and explanation receipts.
8. Run the complete repository and browser validation gates and commit local
   checkpoints with conventional messages.

## Compatibility decisions

- Additive serialized fields use defaults and omission for empty values.
- Stable Web IR remains free of browser handles and user-entered values.
- Offline compilation emits capability requirements but does not claim a
  runtime supports them.
- Existing low-level actions remain available; high-level task execution gains
  stricter preflight and does not silently fall back when bindings are absent.
- Pi integration may advertise only the tools the installed RPC protocol can
  broker safely. Unsupported attachment-dependent tools remain explicitly
  unavailable rather than simulated.
