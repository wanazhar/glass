# verify

Run the project's verifier after commands that write files, and append
failures to the tool result the model is about to read — before it builds
more work on a broken file.

- **Trigger**: a `tool_result` for `bash` whose body contains the host's
  `[wrote …]` note (heredoc, redirect, tee, `sed -i`), at most once per
  `debounce_s` window.
- **Execution**: through the host's own `bash` tool as a nested `call_tool`
  request, so the sandbox policy, timeout, and output caps apply. No shell
  of its own, no bypass.
- **Feedback**: failures append as `[verify exit N] <command>\n<head+tail,
  capped 4 KiB>`; after a failure, the first success appends `[verify ok]`
  so "back to green" is visible. Quiet otherwise.

Config at `~/.config/glass-core/verify.json` (or `GC_VERIFY_CONFIG`):

```json
{
  "command": "cargo test --quiet",
  "timeout_s": 90,
  "debounce_s": 8
}
```

Without a config, detection is deliberately small: `Cargo.toml` →
`cargo check --quiet`; `package.json` with `scripts.typecheck` →
`npm run --silent typecheck`. Anything else stays inactive — a wrong guess
about how to test a project is worse than silence. `debounce_s: 0` verifies
after every detected write.

`/verify` shows the active command and the last result. Offline checks:
`python3 verify.py --selftest` and `scripts/smoke-verify.py`.
