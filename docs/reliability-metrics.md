# Reliability metrics

A reliability report is evidence. It is not a product guarantee.

Record these fields for every report:

- Glass commit;
- crate version;
- host operating system and architecture;
- Chrome version;
- fixture revision;
- policy preset;
- interaction mode;
- warm-up count;
- iteration count; and
- cold or warm browser state.

Record these measurements:

- action success rate by action kind;
- verification success rate;
- bounded timeout rate;
- wrong-action count;
- stale-revision rejection count;
- recovery attempts;
- automatic retries and caller retries;
- p50, p95, and p99 action latency;
- trace size;
- resident set size (RSS); and
- serialized response size.

A report is valid only when the deterministic fixture matrix passes and all
forbidden outcomes are zero. Missing measurements fail the acceptance gate.
Do not treat a missing measurement as zero.
