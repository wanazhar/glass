# Verification extension

Run a configured project check after detected shell writes and append relevant
feedback to the tool result. Native file validation remains a separate mechanism.

## Trigger and execution

The extension recognizes the host's `[wrote …]` result markers for supported shell
write patterns, subject to a debounce interval. It invokes the host's `bash` tool
through a nested call, retaining the applicable policy, timeout, and output limits.
Unsupported shell patterns may not produce a marker.

Failures append the check's exit status and a bounded 4 KiB head/tail preview.
The first success after a failure appends a recovery notice. Other successful checks
remain quiet.

## Configuration

Use `~/.config/glass-core/verify.json`, or `GC_VERIFY_CONFIG`:

```json
{"command":"cargo test --quiet","timeout_s":90,"debounce_s":8}
```

Without a command, detection supports `Cargo.toml` (`cargo check --quiet`) and a
`package.json` typecheck script (`npm run --silent typecheck`). Other projects
remain inactive until configured. A zero debounce interval checks every detected
write. `/verify` shows the command and last result.

Checks: `python3 verify.py --selftest` from this directory, or
`python3 scripts/smoke-verify.py` from the repository root.
