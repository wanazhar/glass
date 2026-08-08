id: remote-agent-033-003
scope: Glass v0.3.3 same-session visual and agent integration
status: completed
depends-on: [tui-recovery-033-002]

## objective

Implement loopback/token/revocable Remote View v1 against the authoritative
session and dynamic embedded-agent browser/workflow/memory context with fresh
revision, policy and mutation-lease enforcement.

## context

- `docs/plan/analysis/release-033.md`
- `docs/architecture/browser-connection.md`
- `docs/architecture/semantic-core-hardening.md`

## path

- browser/TUI Remote View modules
- development agent gateway/context
- TUI controller integration
- integration tests and user/security docs

## verification

- loopback, token scope/revocation and no-persistence tests
- same target/revision and stale-input rejection
- detached/attached/recovering dynamic capability tests
- takeover/reconciliation regression coverage

## result

Completed locally on 2026-08-08. Remote View is loopback-only, token-scoped,
revocable, bounded to four clients and an 8 MiB newest-PNG mailbox, while the
agent gateway derives fail-closed tools and revision context from the same
authoritative browser workspace.
