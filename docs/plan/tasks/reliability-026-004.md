# Capability corpus and replay evidence

Status: in progress locally.

The checked-in capability corpus covers replacement, duplicate-target
ambiguity, cross-region movement, overlays, and delayed effects. Replay v1
stores redacted ordered events and binds observations to scenario and fixture
digests. `glass certify replay` validates one bundle without starting Chrome.

Remaining work includes browser-run scenario orchestration, fault execution for
renderer and browser disconnects, replay comparison across runs, and a
generated scorecard suitable for release evidence.
