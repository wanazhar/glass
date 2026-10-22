# Source index extension

Return symbol names and line numbers before reading a complete source file.
The extension supports Rust, Python, Go, JavaScript, and TypeScript variants.

- `index {"path":"src/agent.rs"}` outlines one file.
- `index {"query":"retry_backoff"}` finds files containing a literal query and
  returns their outlines. It uses ripgrep when available, with a bounded fallback.

Output is limited to 60 outline lines per file and 200 per call. Scanned files
are limited to 256 KiB. Paths must remain within the workspace. Unsupported file
types return an explanation.

The implementation uses regular expressions and skips comment lines. It reports
source line numbers but does not provide complete parsing, type analysis, or all
language constructs. Read the relevant source range before editing it.

Checks: `python3 index.py --selftest` from this directory, or
`python3 scripts/smoke-index.py` from the repository root.
