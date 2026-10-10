# CLI help presentation

2026-10-10. Replaced the flat, outdated CLI help text with a small native
renderer. `gc help` shows a quick start, focused guides and copyable examples.
`gc help commands`, `run`, `keys`, and `all` expose detailed guides. `agent` and
`file` forward to their existing command help.

Wide terminals align commands and descriptions; narrow terminals stack them.
Headings use the accent color, commands use the secondary color, and supporting
text uses a muted tone. Output uses one buffered write with no screen ownership,
cursor movement, pager, provider initialization or new dependencies. Pipes and
`NO_COLOR` use plain text. The old claim that GC has no permission system was
removed. Unknown help topics return exit code 2.

## Verification

- Strict workspace Clippy passed with warnings denied.
- All 622 workspace tests passed; one existing test remains ignored.
- Renderer tests cover five pages at widths 8, 20, 40, 80 and 100.
- `scripts/smoke-help.py` checks the release binary through real PTYs at
  80×30, 40×24 and 20×8, with and without color, plus aliases, focused guides,
  invalid topics, piped output and startup outside Git with empty XDG paths.
- A Luna medium reviewer checked commands, flags and shortcuts against the
  implementations; status and attachment descriptions were corrected.
- Release performance smoke passed: median help startup 1.87 ms; narrow PTY
  input visibility below 0.61 ms. These are local smoke measurements.
- Verified release SHA-256:
  `4f101375ebd949a0d19748b6c6e59ec3debaccd51643c960ca85b73ef818011d`.
- All 15 existing release regression cases passed: polish, reader, inline
  scrolling, scroll submission, native files/jobs, context, blocks, motion,
  live turns, recovery, permission PTYs, security, onboarding, mobile and
  features. The release hash remained unchanged during the audit.
