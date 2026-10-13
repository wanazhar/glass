# Cache-warmer extension

The optional extension schedules prefix refresh requests through the host provider.
It is disabled by default. Enable it through `/extensions` when the measured
latency benefit justifies additional model requests.

## Behavior and limits

Defaults are 240 seconds after a turn and a 4,000-byte minimum prefix.
`GLASS_CACHE_WARMER_REFRESH_S` and `GLASS_CACHE_WARMER_MIN_PREFIX_BYTES` override
these scheduling values. Refreshes consume quota and use session accounting;
provider cache lifetime and billing are separate from the extension schedule.

See the [user reference](../../docs/configuration.md#optional-cache-warming) and the implementation in this directory.
