---
id: sandbox-egress-001
scope: crates/glass sandbox module, config, bash/jobs spawn sites
status: done
depends-on: [sandbox is part of wave 1 of the first port batch]
---
# Sandbox egress

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

Sandboxing moved from filesystem to network and credentials across the field
(Claude per-command domain review + credential masking, Copilot MXC GA);
this closes the two cheapest gaps in glass-core's existing write-jail:
unbounded egress and readable API keys.

## Context

- **Egress** (Linux, Landlock ABI 4 / kernel 6.7+): `handled_access_net =
  CONNECT_TCP` plus one `LANDLOCK_RULE_NET_PORT` rule per allowed port;
  ruleset attribute size is 8 bytes when net is not handled, 16 when it is
  (older kernels reject larger attrs). Binds stay unrestricted (local test
  servers), UDP/DNS is unaffected, and no rule means denied. Ports come from
  `sandbox_ports` (default `[443, 80]`, empty = deny all outbound TCP).
  ABI < 4: one warning, network stays open — never silent.
- **Credentials**: `sandbox_env` ("scrub" default when the sandbox is on,
  "inherit" to opt out) strips credential-shaped names from `bash` and
  `/bg` children via `sandbox::scrub_env`, which both spawn sites call after
  `harden`. Matching errs toward stripping.
- macOS gets the env scrub and a warning that network restrictions are not
  implemented there yet; extensions and MCP servers remain unsandboxed by
  design (documented), since they are programs the user installed.

## Paths

- `crates/glass/src/sandbox.rs` (net rules, is_sensitive/scrub_env, State),
  `config.rs` (`sandbox_ports`, `sandbox_env`), `tools.rs`, `jobs.rs`.
- `scripts/smoke-sandbox.py` (network + env cases).

## Verification

- Unit tests (ABI-gated): allowed port connects, another is denied; credential
  name classification; config validation/defaults.
- `scripts/smoke-sandbox.py`: through the binary — port-limited egress,
  scrub default, `inherit` escape hatch, plus the existing write/read jails.
- Kernel here is 6.17 (ABI ≥ 4), so the network test ran.
