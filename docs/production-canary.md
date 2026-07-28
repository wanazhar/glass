# Production canary runbook

A production canary is optional. It must be read-only, bounded, and monitored.

Use an approved URL. Use the `hardened` policy and an exact host allowlist.
Do not use a logged-in profile. Do not evaluate JavaScript. Do not click, type,
upload, download, or change browser emulation.

Set the URL and run:

```console
export CANARY_URL=https://approved.example/health

cargo run --release --locked -- \
  --policy hardened \
  --policy-allow-host approved.example \
  navigate "$CANARY_URL" --timeout-ms 20000

cargo run --release --locked -- \
  --policy hardened \
  --policy-allow-host approved.example \
  observe
```

Record the commit, Glass version, Chrome version, host operating system and
architecture, URL host, policy, result, elapsed time, and typed failure.

Stop on an outside redirect, unexpected dialog, transport error, or stale
topology. Do not relax policy. Do not retry a mutation.

The deterministic fixture and scorecard remain the release gate. Canary data is
supplementary evidence.
