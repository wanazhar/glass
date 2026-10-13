# Consistent CLI presentation and resume picker

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

2026-10-10. All human CLI reports now share the native help palette and a small
buffered renderer. Wide screens align labels and values; narrow screens stack
and wrap them. Reports use an accent heading, a border divider, secondary labels,
muted context and semantic success, notice and error colors. Offline doctor
renders separate diagnostic cards. Explicit JSON, agent JSON on pipes, ACP,
Unix-socket protocol frames and exported Markdown bypass presentation.

## Command coverage

Status, doctor/probe, sessions/list/search, context/review/details, profiles,
extensions, init/write, export confirmations, agent output, file read/check/edit/
write, plugin operations, worktree operations, send, serve/stop/respawn/logs and
headless model results use the shared presentation. Completed model output uses
the native Markdown renderer. File and tool content preserve whitespace and
line numbers, with tabs displayed as four spaces. Pipes keep original data.

Every registered subcommand has static `--help`/`-h` and `gc help COMMAND`
guides that work before configuration or provider initialization. A literal
`--help` after a run prompt or end-of-options marker remains prompt content.

## Resume

Bare `gc resume` opens the existing searchable modal for current-project threads,
newest first, with titles, timestamps and IDs. F1 shows the full label. Search
matches titles and IDs; Enter resumes directly, Esc/Ctrl+C cancel. Empty projects
show a helpful message without creating a session. Noninteractive use gets
`gc sessions --json` / `gc run --resume ID` guidance. Explicit IDs still resume
directly. Extra resume arguments are rejected.

The index reads metadata workspace/title fields before attachment payloads and
caps fallback first-message reads at 64 KiB. Full logs are loaded only after
selection. The modal caches normalized search text once per invocation. The
input thread stops before terminal modes are restored, including setup failure.

## Verification

Two Luna medium reviewers checked CLI/protocol boundaries, help accuracy,
resume filtering, terminal cleanup and cost. Their init-write confirmation and
partial terminal-setup cleanup findings were fixed. Tests also caught and fixed
an early-stop metadata parser issue and whitespace loss in literal output.

- Strict workspace Clippy passed with warnings denied.
- All 625 workspace tests passed; one existing test remains ignored.
- Actual release CLI matrix passed at 80×30, 40×24 and 20×8: 25 command
  guides, 16 report paths, JSON on terminals and pipes, NO_COLOR, raw export,
  file whitespace, errors, picker search/no matches/selection/cancellation,
  resumed context, empty projects, non-Git startup and init-write confirmation.
- Help guides work with invalid configuration. Picker cancellation makes no
  provider request and does not modify session logs. Terminal input modes are
  restored after cancellation and normal exit.
- Existing help PTY smoke passed in all three sizes with and without color.
- Release performance smoke passed: local median help startup 4.02 ms,
  worst median key visibility 2.49 ms; long-history reader frame 81.12 ms.
- Release SHA-256:
  `bb40d61451942501285d2bcae0f542b0f5068cf1ffa515b8be4763e28f75ffb8`.
- `target/release/gc` remains a symlink to `glass-core`.

- All 15 existing release regression cases passed: polish, reader, inline
  scrolling, scroll submission, native files/jobs, context, blocks, motion,
  live turns, recovery, permission PTYs, security, onboarding, mobile and
  features. Release hash was unchanged throughout the audit.
