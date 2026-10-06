---
id: issue60-p0-protocol-001
scope: glass-browser/protocol-identifiers
status: done
depends-on: []
---

# Issue #60 P0: reject control characters in protocol identifiers

## Objective

Resolve Report #57. Reject all Unicode control characters in canonical
protocol identifiers in addition to empty, oversized, and whitespace values.
Keep the bound and error field behavior stable.

## Context

- `docs/protocol.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), Report #57
- [Source report #57](https://github.com/wanazhar/glass/issues/57)

## Contract

An identifier is invalid when it is empty, exceeds the existing UTF-8 byte
limit, contains whitespace, or contains a character for which Rust reports
`char::is_control`. All canonical request fields using this validator share
the same rule.

## Path

- `crates/glass-browser/src/protocol.rs`
- `docs/protocol.md` only if the public contract is not already explicit

## Verification

- Add focused boundary coverage for C0 and C1 control ranges, plus valid
  printable non-ASCII identifiers and existing empty/length/whitespace cases.
- Verify every protocol identifier field uses the shared validator.
