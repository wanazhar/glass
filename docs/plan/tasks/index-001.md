---
id: index-001
scope: extensions/index
status: done
depends-on: []
---
# Index

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

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
- The regular-expression scanner reports source line numbers without adding a
  parser dependency. It does not provide complete language parsing.

## Paths

- `extensions/index/index.py` + README; `addons.rs` bundle entry.
- `scripts/smoke-index.py`.

## Verification

- Selftest: python/rust line numbers, comment skipping, unknown ext, cap.
- `scripts/smoke-index.py`: protocol-level — exact lines, query search,
  containment refusal, cap enforcement.
