# Issue #31 migration

Existing `knowledge` commands remain available for compatibility. New clients
should use capability-oriented `memory` commands and consume the common result
envelope. Existing `certify replay` remains valid; `replay inspect|diff|attach`
provides the same validation as a general experience surface.

Clients must not treat remembered locators as executable handles. Re-observe
and revalidate against the current Web IR revision before mutation. Clients
must also handle `partial`, `restricted`, and `unavailable` backend/surface
states as non-executable rather than retrying with guessed coordinates.

The deterministic reliability and ProofBackend paths provide contract evidence
only. They must not be described as real browser parity or performance
benchmarks. Use CDP-backed scenarios for real browser claims and report their
policy, provenance, and verification evidence.


## Cross-interface cutover

The CLI, MCP, and daemon now project the same bounded experience envelope.
Consumers should read `schemaVersion`, `provenance`, and typed `resourceRefs`
before using a result. Resource references are scope/ownership identifiers;
they are not browser locators and cannot authorize an action.

`replay inspect`, `replay diff`, and `replay attach` are browser-free. Inputs
are size-bounded, redacted, and validated against the exact scenario hash.
`attach` records an evidence relationship only; it never attaches to or takes
over Chrome. A daemon mutation still requires the single current actor lease
and matching revision. On lease expiry, ownership change, or stale revision,
clients must re-observe and reconcile rather than retrying blindly.