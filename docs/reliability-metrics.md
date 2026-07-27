# Reliability metrics

Issue #20 uses versioned evidence rather than unqualified performance claims.
Every report records the Glass commit, crate version, host OS/architecture,
Chrome version, fixture revision, policy preset, interaction mode, warm-up
count, iteration count, and whether the run was cold or warm.

The reliability report should include:

- action success rate, split by action kind;
- verification satisfaction rate and bounded timeout rate;
- wrong-action count and stale-revision rejection count;
- recovery attempts, with automatic retries counted separately from explicit
  caller retries;
- p50/p95/p99 action latency and the bounded trace size;
- RSS and serialized response size.

These are measurement categories, not product guarantees. A release report is
valid only when its deterministic fixture matrix completes, its forbidden
outcomes are zero, and its metadata identifies the exact build and browser.
Missing or partial measurements fail the acceptance gate instead of being
treated as zero.
