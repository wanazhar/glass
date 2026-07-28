# Read-only real-site certification

Real-site checks are release evidence, not a substitute for the deterministic
fixture suite. They must be run only against an approved site and a bounded,
non-destructive route.

## Required conditions

- Use a disposable `--incognito` session and hardened policy.
- Allowlist only the exact approved host with `--policy-allow-host`.
- Do not provide credentials, cookies, API keys, or personal data.
- Use only navigation, observation, text, bounded waits, and verification.
- Do not click submit, purchase, delete, upload, download, or consent controls.
- Keep the duration and action budgets explicit in the scenario evidence.
- Capture only redacted replay events and independent read-only oracle results.

The operator must stop and classify the run as unsupported or indeterminate if
the site requires authentication, presents an unexpected destructive control,
redirects outside the allowlist, or cannot produce complete oracle evidence.
Those classifications cannot certify a release.

## Example probe

```console
glass --incognito --policy hardened \
  --policy-allow-host example.com \
  navigate https://example.com/approved-read-only-route
glass --incognito --policy hardened \
  --policy-allow-host example.com \
  observe --level summary
glass --incognito --policy hardened \
  --policy-allow-host example.com \
  verify '{"textContains":"Expected marker"}'
```

This is an operator procedure, not an assertion that the example site is
certified. The current checkout does not automatically discover approved
sites or publish real-site evidence.
