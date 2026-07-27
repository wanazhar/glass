# Production canary runbook

Production canaries are optional, monitored, and non-destructive. They must
run against an approved read-only URL with a host allowlist and the hardened
policy. Do not use a logged-in profile, evaluate JavaScript, click controls,
type, upload, download, or change browser emulation during a canary.

Set an approved URL and run a bounded navigation/observation probe:

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

Record the commit, Glass version, Chrome version, host OS/architecture, URL
host, policy preset, result status, elapsed time, and any typed failure. Stop
the canary on redirects outside the allowlist, unexpected dialogs, transport
errors, or stale topology. Never treat a canary failure as permission to relax
the policy or retry a mutation.

The deterministic fixture and scorecard remain the release gate; canary data
is supplementary evidence and must not be presented as a product guarantee.
