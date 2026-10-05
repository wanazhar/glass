# Workflow & Engineering

- **Research before recommending.** When a design decision depends on what other tools actually do, survey the field first and bring back data (real numbers, cited sources) rather than answering from memory. Confidence: 0.8
- Expects claims to be backed by **measurements**, not estimates: wants concrete binary size, RSS, and startup numbers reported, and compares against competitor baselines. Confidence: 0.8
- **Names a specific rival as the benchmark and scopes work as a gap analysis against it** ("pi is our benchmark. lets beat pi. what can we ship next?"). Rank proposed work against that rival; "where we already win, where we lag, what I'd skip" is the expected shape of the answer. Confidence: 0.7
- When a ranked proposal is accepted, the directive is "do all of those" — treat a green-light on a list as a full commitment to every item on it, executed to completion with tests and a commit each, not a quiet renegotiation over a subset. Confidence: 0.7
- Rust is the implementation language of choice. Confidence: 0.85
- Wants a **test suite as part of the deliverable** — every module shipped with unit tests, all green before calling it done. Confidence: 0.85
- Test the parts that fail silently. Real bugs found: process-group kill semantics, JSON number vs string args, write-only file handle reads, CRLF in a streaming parser. Confidence: 0.7
- Verify the whole thing actually runs end-to-end (pty-driven interactive test against a mock server), not just that it compiles. Confidence: 0.8
- Debug by **isolating a hypothesis into a minimal standalone probe** rather than reasoning about the real code, and A/B test the suspect variable. Confidence: 0.75
- Zero-warning, clippy-clean builds are expected. Silence the lints by fixing the code, not by adding `allow`. Confidence: 0.7
- Commit working increments to git as soon as a layer is green, rather than one giant commit at the end. Confidence: 0.65
- Values a written README that explains the *why* behind each decision with numbers, not just usage instructions. Confidence: 0.7
