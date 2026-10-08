---
id: index-001
scope: extensions/index
status: done
depends-on: []
---

## Objective

Repo-map-adjacent token saving (Aider's repomap is frozen; Maki's `index`
tool is the current reference with published accounting: +59 tok/turn for
the tool, −224 tok/turn on reads). Delivered as an out-of-process extension:
no tree-sitter, no core change.

## Context

- `index {path}` outlines one file; `index {query}` finds files containing
  the literal (ripgrep when present, bounded stdlib scan otherwise) and
  outlines those.
- Line-anchored regex scanners for rust/python/go/js/ts/jsx/tsx; doc-comment
  lines skipped so documentation cannot masquerade as symbols; unknown
  extensions explained rather than guessed.
- Caps: 60 outline lines/file, 200/call, 256 KB scanned/file; paths resolve
  inside the workspace, `../` refused.
- Deliberate trade-off recorded in the README: a regex that never lies about
  line numbers beats a parser that occasionally does, and the core stays
  dependency-free.

## Paths

- `extensions/index/index.py` + README; `addons.rs` bundle entry.
- `scripts/smoke-index.py`.

## Verification

- Selftest: python/rust line numbers, comment skipping, unknown ext, cap.
- `scripts/smoke-index.py`: protocol-level — exact lines, query search,
  containment refusal, cap enforcement.
