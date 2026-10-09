# Twenty-item final verification

Date: 2026-10-09
Source checkpoint: `1890e59` (subsequent changes are verification documentation only).

## Delivery

All 20 accepted deliverables in [the shipping plan](../ship-next-20.md) are implemented.
The root implemented integration and fixes directly; two independent GPT-6 Luna
medium reviewers reviewed the terminal and security/MCP boundaries.

User accounts, credentials and installed executables were preserved. Verification
uses disposable configurations, homes, sessions, extension children and local mock
providers. No remote push, tag or publication was performed.

## Review fixes

- Correlate attach startup failures to their request IDs, persist failed status,
  and permit the same connection to submit after recovery without replaying effects.
- Reject hidden controls and oversized values in every supported MCP enum source,
  defaults, field names and returned form values.
- Keep exact command approvals scoped to interactive session workers and explicit
  background jobs; clear them on session changes. Delegates and extension host
  callbacks receive no inherited approval snapshot.
- Redact provider errors before display, durable logging and terminal status.
- Rehash tracked snapshot content despite Git's copied-index stat cache, preserving
  the user's index. Equal-size/mtime regression and rename/directory cases pass.
- Update PTY fixtures for the actual file/hunk browser and pre-enable capability
  review, including clipped headings at 20x8.

## Evidence

- `cargo test --locked --workspace`: 485 core + 11 extension API + 75 TUI
  tests pass (571 total; one ignored doctest).
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: pass.
- `cargo build --locked --release`: pass.
- Final binary SHA-256:
  `8d81bae65e14f8aeb6cf131f41cd299dbde9dbc3ca49906fa82ee03132d3aa93`.
- Workspace artifact `target/release/gc` is a relative symlink to `glass-core`;
  `gc --version` reports `glass-core 0.1.0`.
- All 40 `scripts/smoke-*.py` suites pass on the final release, including
  real clean-install/upgrade preservation, serve factory failure recovery, reader
  VT-cell checks, security deny paths and MCP form privacy. PTY coverage includes
  40x24 and 20x8; workflow coverage also includes 80x24, 40x12 and 24x10.
- `scripts/check-performance.py`: pass, 12,000-record history.

| Measurement | 40x24 | 20x8 |
|---|---:|---:|
| Interactive prompt startup, median | 6.33 ms | 8.01 ms |
| Key visible, median | 1.06 ms | 0.18 ms |
| Frame redraw, median | 0.24 ms | 0.25 ms |
| Long-history reader frame, median | 84.87 ms | 76.75 ms |

Long-history search was 1.75 ms and measured child peak RSS was 3,364 KiB on
this Linux ARM64 host. These are local fixture measurements, not provider or
network latency guarantees.

Independent reviews: [terminal](ship-terminal-001-02.md),
[security and MCP](ship-security-input-001-02.md), and
[earlier reliability/reader/plugin verification](ship-wave1-001-01.md).

## Boundaries

Permission profiles and extension confinement are opt-in; the unrestricted default
and existing motion/theme remain. Strict confinement refuses unsupported policies.
Network declarations bound outbound TCP ports, not hosts, UDP or Unix sockets;
process creation itself is not denied, and children inherit the applicable jail.
See [the enforcement contract](../../security-confinement.md).

## Complete smoke inventory

`smoke-acp.py`, `smoke-chat.py`, `smoke-doctor.py`, `smoke-editcontext.py`, `smoke-elicitation-pty.py`, `smoke-features.py`, `smoke-image.py`, `smoke-inbox.py`, `smoke-index.py`, `smoke-init.py`, `smoke-inline-scroll.py`, `smoke-install.py`, `smoke-live.py`, `smoke-loop.py`, `smoke-lsp.py`, `smoke-mcp.py`, `smoke-memory.py`, `smoke-mobile.py`, `smoke-motion.py`, `smoke-navigation.py`, `smoke-onboarding.py`, `smoke-oracle.py`, `smoke-permissions-pty.py`, `smoke-plugin-integrity.py`, `smoke-plugin.py`, `smoke-polish.py`, `smoke-reader.py`, `smoke-recovery.py`, `smoke-reliability.py`, `smoke-round3.py`, `smoke-sandbox.py`, `smoke-security.py`, `smoke-serve.py`, `smoke-status.py`, `smoke-trust.py`, `smoke-upgrade.py`, `smoke-verify.py`, `smoke-web.py`, `smoke-workflows.py`, `smoke-worktree.py`.
