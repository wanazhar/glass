# governance-035-008 — visible customization authority and evidence

Status: Complete locally on 2026-08-11. No remote mutation performed.

Customization inspection now classifies Glass built-in, user-global, trusted
project, untrusted project, and external-client boundaries. Skill context and
the Trust TUI retain exact authority/source provenance.

Hooks expose event, command, timeout, source, trust, failure policy, and latest
actor-attributed duration/result evidence. Custom tools expose their schema,
declared mutation behavior, effective mutation policy, timeout, command, source,
trust requirement, and latest evidence.

All shell-backed project tools are effectively mutating even when a repository
declares `mutating = false`; mutation authority and confirmation can no longer
be waived by project metadata. TUI help and results label project commands
explicitly as `PROJECT` actions.

Focused tests prove built-in/external authority visibility, hook/tool evidence,
and the effective-mutation policy.
