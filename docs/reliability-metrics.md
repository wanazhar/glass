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

## Metric definitions

| Metric | Numerator or observation | Denominator or boundary |
|---|---|---|
| action success rate | actions with verified `succeeded` status | all dispatched actions of that kind |
| verification success rate | dispatched actions whose declared postcondition passed | actions that declared that verification |
| bounded timeout rate | typed deadline expirations | all operations with the same operation/deadline class |
| wrong-action count | effects on a target other than the declared unique target | absolute count; acceptance requires zero |
| stale-revision rejection | requests rejected before dispatch for stale evidence | all intentionally stale adversarial requests |
| recovery attempts | explicit recovery operations | report by recovery class and original phase |
| action latency | monotonic start through completed verification/result | p50/p95/p99 from one comparable warm/cold class |
| RSS | Glass process resident set measured by the declared sampler | peak and sampling interval; exclude browser unless stated |
| response size | serialized bytes at the public interface boundary | report response mode and operation |

## Separate populations

Do not combine cold startup with warm-session operation latency. Do not combine
structured observation with screenshot capture, local transport with SSH, or
human pointer mode with fast mode. Report empty, partial, challenge,
policy-denied, and browser-failure page states separately.

Requested, acquired, and presented frame rates are different measurements.
Also record frame age, dropped/replaced/stale frames, bytes, scale, capture and
presentation latency, selected policy, and the reasons for that policy.

## Recovery and retry accounting

Record the original execution ID, failure phase, whether dispatch was proven,
recovery mode, and final result. Caller retries and Glass automatic retries are
separate counters. A retry after an indeterminate dispatch cannot be counted as
ordinary first-attempt success.

## Report acceptance

A comparative report must include raw machine-readable results, the exact
command, workload/fixture revision, warm-up policy, sample count, process
boundary, and failures. Use median and tail percentiles only with enough
samples to make them meaningful. Do not select the best run or omit partial
results.

See [Browser automation measurements](category-metric.md), [Reliability
laboratory](reliability.md), and the [benchmark methodology](../benchmarks/README.md).
