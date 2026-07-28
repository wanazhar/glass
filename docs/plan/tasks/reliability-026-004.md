# Capability corpus and replay evidence

Status: in progress locally.

The checked-in capability corpus covers replacement, duplicate-target
ambiguity, cross-region movement, overlays, and delayed effects. Replay v1
stores redacted ordered events and binds observations to scenario and fixture
digests. `glass certify replay` validates one bundle without starting Chrome.
`glass certify replay-diff` compares two validated bundles, and
`glass certify release --replays` cross-checks replay observations against the
release evidence set. Validated scenarios also expand into an ordered,
manifest-bound execution plan for a future browser runner.

Remaining work includes browser-run scenario orchestration, fault execution for
renderer and browser disconnects, and a real-site read-only certification
procedure.
