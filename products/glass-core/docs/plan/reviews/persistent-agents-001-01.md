# Persistent teammates verification

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

2026-10-09, Linux ARM64. Root implemented; GPT-6 Luna medium independently
reviewed runtime and UI. Local delivery only.

## Verified artifact

`target/release/glass-core`, SHA256
`32038f9f97534a888a4a79f040103602b4a0782cf1ef6902080be1c015c3bd09`.
`target/release/gc` remains a symlink to `glass-core`.

- Strict workspace Clippy: passed.
- Workspace unit tests: 615 passed (517 core, 13 mod API, 85 TUI);
  one ignored doctest.
- New provider-wire integration: headless/non-Git reuse, exact/unknown pricing,
  two default main tools, read-only workers, concise parent notifications.
- Release PTYs: 80x24, 40x12, 20x8. Live modal, interactions/transcripts,
  quiet unchanged frames, Esc, follow-up, slow-header interruption, restart,
  concurrency limits, private permissions, offline review and busy-main access.
- Fifteen release regression suites: polish, reader, inline-scroll, scroll-submit,
  native-files-jobs, context, blocks, motion, live, recovery, permissions-pty,
  security, onboarding, mobile, features. All passed, artifact hash unchanged.
- Additional release suites: ACP, serve, trust, sandbox, Unicode jobs.
- Tool repair: all 46 provider-wire/headless/PTY cases passed across Chat
  Completions, Responses and Anthropic, including cancellation and follow-up.
- Saved Kilo/SW connection: real read-only teammate inspected a subtraction bug
  in a temporary non-Git workspace; completed with transcript visible, original
  file unchanged. Provider reported 875 input / 224 output tokens.

## Performance

Release performance gate passed at 40x24 and 20x8, with 12,000 history records:
worst median startup 7.70ms, key visibility 0.104ms, redraw 0.345ms,
reader frame 87.05ms, search 2.13ms, measured child peak RSS 3364KiB.
Native throughput gate passed: 8MB read median 17.93ms, 1MB edit 26.48ms;
ordinary Python writes spawn no checker.

Unchanged teammate transcript ticks skip snapshot/file/page work. Active roster
animation is capped at 8Hz; changed transcript pages remain bounded to 48KiB.
Review noted a minor bounded allocation on changed transcript frames, with no
functional blocker.

## Review fixes and limits

Fixed mismatched transcript record keys, narrow title/help layout, repeated idle
page work, wrong-model construction, shutdown ownership races, invalid-budget
resource creation, immediate-error reservation leaks, and reservation persistence
failure before transport launch. Per-agent cost uses the actual provider rate
card and remains unavailable for unpriced or incomplete usage.

The repair mock now detects follow-up only in user messages; scanning the system
prompt falsely treated orchestration instructions as user follow-up.

Tool-capable workers join on shutdown. Already accepted blocking HTTP requests
may drain without tools until the transport deadline; bounded transport leases
prevent unbounded accumulation. Uncertain mailboxes remain reviewable after
restart and are never silently replayed. Token limits are checked before each
request and can overshoot by one accepted request. Native interactive/run Unix
paths are integrated; ACP/serve retain their previous delegation interfaces.
