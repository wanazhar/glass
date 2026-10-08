---
id: sandbox-001
scope: crates/glass sandbox module, tools bash spawn, jobs spawn, config
status: done
depends-on: []
---

## Objective

Optional OS-level confinement for shell commands, ported from Codex's sandbox
modes (research/harness-features-2026.md): `sandbox = "off" | "workspace" |
"strict"` in config. `workspace` jails writes to the workspace, temp, caches,
and glass data dirs; `strict` also narrows reads and execution to an
allowlist. No prompts, no permission system — kernel enforcement only.

## Context

- Linux: Landlock through raw syscalls (444/445/446), no dependency beyond
  `libc`. Rights are chosen per ABI (REFER is ABI ≥2, TRUNCATE ≥3) because
  handling a right the kernel does not know fails ruleset creation. The
  ruleset is built once in the parent and applied per child in `pre_exec`.
- `/dev/null` is a file rule: only file rights are valid there, and only
  rights the ruleset handles — hence READ_FILE only under strict.
- The harness's own git/checkpoint/verification subprocesses are not
  sandboxed; the policy covers what the model runs (`bash`, `/bg` jobs).
- Unsupported kernels/filesystems warn once; `strict` refuses commands instead
  of silently running them bare.

## Paths

- `crates/glass/src/sandbox.rs` (new): policies, Landlock build/enter,
  macOS SBPL wrap, `shell()`/`harden()`/`refusal()`, tests.
- `crates/glass/src/tools.rs`, `jobs.rs`: build commands through
  `sandbox::shell` and `harden`; strict refusal preflight.
- `crates/glass/src/config.rs`: `sandbox` key; `main.rs`: init + `/security`
  state line; `scripts/smoke-sandbox.py`.

## Verification

- Unit tests (skipped on kernels without Landlock): inside writes succeed,
  outside writes denied, reads unrestricted under workspace; strict denies an
  outside read while allowing the workspace and system reads; missing
  allowlist entries are not fatal.
- `scripts/smoke-sandbox.py`: end-to-end `gc run` with a mock endpoint whose
  tool call writes inside and outside; strict variant probes a denied read and
  an allowed `/etc` read.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
