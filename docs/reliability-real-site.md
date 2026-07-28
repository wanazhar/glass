# Read-only real-site certification

A real-site check is supplementary release evidence. It does not replace the
deterministic fixture suite.

Run a check only on an approved, read-only route.

## Conditions

- Use `--incognito` and the `hardened` policy.
- Allow only the exact host with `--policy-allow-host`.
- Do not provide credentials, cookies, API keys, or personal data.
- Use navigation, observation, text, bounded waits, and verification only.
- Do not submit, purchase, delete, upload, download, or dismiss consent.
- Set explicit duration and action limits.
- Store only redacted replay events and read-only oracle results.

Stop and classify the run as `unsupported` or `indeterminate` when the site
requires authentication, shows an unexpected destructive control, redirects
outside the allowlist, or does not provide complete oracle evidence.

These classifications cannot certify a release.

## Example

`console
glass --incognito --policy hardened \
  --policy-allow-host example.com \
  navigate https://example.com/approved-read-only-route

glass --incognito --policy hardened \
  --policy-allow-host example.com \
  observe --level summary

glass --incognito --policy hardened \
  --policy-allow-host example.com \
  verify '{"textContains":"Expected marker"}'
`

This example does not certify `example.com`. The current checkout does not
discover approved sites or publish real-site evidence.
