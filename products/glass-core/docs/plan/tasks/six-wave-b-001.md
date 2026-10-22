---
id: six-wave-b-001
scope: wave B — process self-hardening, admin requirements layer, egress policy proxy
status: implemented
source: research/six-codebases-2026-10-10.md candidates #10, #7, #9
---

# Wave B: security hardening

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

1. **Process self-hardening** (codex `process-hardening`): before REPL
   startup, when running as the main binary — not for `gc run` child
   processes — disable core dumps (`RLIMIT_CORE=0`), and strip
   `LD_PRELOAD`/`LD_LIBRARY_PATH` (`LD_*`, `DYLD_*`) from the environment
   the agent and its children inherit. macOS equivalents only where libc
   allows portably; never fail the launch when a setrlimit is refused —
   warn once and continue.
2. **Admin requirements layer** (codex `allow_managed_hooks_only` +
   `requirements.toml`): a `requirements.toml` in the config directory that
   user/project/runtime config cannot override, read for a small
   allow-list of forced settings: `sandbox = off|workspace|strict`,
   `sandbox_env = on|off`, and future keys only through an explicit enum.
   Settings attribution already tracks layers; add the requirements layer
   above runtime, and make `/settings` show "admin requirement" provenance
   with the strongest-wins rule stated. Corrupt file fails closed: forced
   entries are skipped with a warning, not confused with user choices.
3. **Egress policy proxy** (codex `network-proxy`): when the sandbox is on
   and `sandbox.egress = "policy"`, spawn an in-process HTTP CONNECT proxy
   on a 127.0.0.1 port that enforces an allow/deny host list read from
   config (`[sandbox.egress_allow] hosts = [...]`, wildcard suffix
   patterns). Sandboxed bash children get `http_proxy`/`https_proxy`/
   `ALL_PROXY` pointing at it; non-allowlisted CONNECT is refused with a
   diagnostic naming the host. No TLS interception and no SOCKS5 in this
   slice — document that limitation explicitly. Proxy
   lifetime is per-session, stops with the process, and adds no startup
   cost when the sandbox is off.

## Path

Own these paths only.

- `crates/glass/src/main.rs` — hardening prelude call site only
- new `crates/glass/src/hardening.rs`
- `crates/glass/src/config.rs` — requirements layer + egress keys
- `crates/glass/src/settings.rs` — provenance slot
- `crates/glass/src/sandbox.rs` — env injection + proxy wiring
- new `crates/glass/src/egress.rs` — CONNECT proxy
- new `scripts/smoke-egress.py`
- `docs/plan/tasks/six-wave-b-001.md`

Do not edit `agent.rs`, `session.rs`, `tools.rs`, `ext/**`.

## Constraints

- No new dependencies: proxy is hand-rolled over `std::net::TcpListener`
  with the existing `ureq`/`ring` TLS posture untouched. Blocking I/O on a
  dedicated thread, matching repo style.
- Proxy must be bounded: max command header size, per-connection timeout,
  and a connection cap. Malformed CONNECT never panics.
- Requirements layer cannot be widened by project config; tests prove
  override attempts are refused.
- Hardening is best-effort; a refused setrlimit downgrades to a warning.
- env strip never removes variables the user's shell needs for the
  *harness itself* — strip happens for child command environments, and
  the harness env only for LD_PRELOAD-family variables known dangerous.

## Verification

- Unit tests: rlimit refusal tolerated; LD_* stripped from child env;
  requirements take precedence over every other layer, including runtime overrides;
  wildcard matching exact semantics; CONNECT to denied host refused with
  host named; allowlisted host passes through.
- `scripts/smoke-egress.py`: sandbox on, curl to denied host fails with
  the diagnostic, allowlisted host succeeds; proxy absent when sandbox
  off.
- clippy + full tests green; startup measured unchanged when sandbox off.
- PTY: `/settings` shows admin provenance rows at 40×24.

## Verification

- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --locked --workspace` — 641 + 13 + 91 pass, 0 fail,
  including new hardening (rlimit refused tolerated, keep-list spares
  real library paths), requirements (missing file / force / no-change /
  corrupt / invalid-value / recognized-but-unused keys), HostPolicy
  (exact + wildcard + deny-wins + default-deny), and egress parser
  (control chars, IPv6 brackets, non-numeric port).
- `scripts/smoke-egress.py` — PASS: denied CONNECT → 403 naming the host
  and the allowlist; allowlisted hosts leave the policy layer; non-CONNECT
  → 405; proxy absent when the sandbox is off; `sandbox = off` user choice
  forced to `strict` by requirements with the startup line naming the
  force; corrupt and invalid-value requirements files fail closed with
  named reasons.
- Startup unchanged when the proxy is off: the proxy exists only under
  `sandbox_egress = "policy"` with a nonzero sandbox; benefits: hardening
  runs two setrlimit/env scans at startup (microseconds).
- Recorded limitations in the task text: blind CONNECT relay (no
  TLS interception), no SOCKS5 in this slice, no `egress_deny` key yet.
