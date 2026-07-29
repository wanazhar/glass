# Capability corpus and replay evidence

Status: included in published 0.2.0; complete artifact and cross-platform
certification remains tracked by issue #28.

The checked-in capability corpus covers replacement, duplicate-target
ambiguity, cross-region movement, overlays, and delayed effects. Replay v1
stores redacted ordered events and binds observations to scenario and fixture
digests. `glass certify replay` validates one bundle without starting Chrome.
`glass certify replay-diff` compares two validated bundles, and
`glass certify release --replays` cross-checks replay observations against the
release evidence set. Validated scenarios expand into an ordered,
manifest-bound execution plan for the `certify run` browser runner.

The runner executes checked-in fixture controls, bounds workflow source paths,
emits redacted replay evidence, and dispatches renderer/browser disconnect
probes through CDP. Those transport fault runs remain explicitly
non-certifying because they cannot provide a complete post-fault oracle. The
real-site read-only certification procedure is documented separately and
remains a manual operator workflow.
