# index

A file's skeleton before the file itself: symbol names with exact line
numbers, so the model reads a range instead of a whole file. Reads dominate
a bash-first harness's token spend; Maki's published accounting for this
pattern is +59 tokens/turn for the tool and −224 tokens/turn on reads.

- `index {path: "src/agent.rs"}` — outline one file (`rust`, `python`, `go`,
  `js/ts/jsx/tsx`, unknown types explain themselves instead of guessing).
- `index {query: "retry_backoff"}` — files containing the literal (ripgrep
  when available, a bounded stdlib scan otherwise) with outlines.
- Caps: 60 outline lines per file, 200 per call, 256 KB per scanned file.
- Containment: paths resolve inside the workspace; `../` is refused.

Deliberately regex, not tree-sitter: a simple scanner that never lies about
line numbers beats a parser that occasionally does, and the core stays
dependency-free. Doc-comment lines are skipped so documentation does not
masquerade as symbols.

Offline checks: `python3 index.py --selftest` and `scripts/smoke-index.py`.
