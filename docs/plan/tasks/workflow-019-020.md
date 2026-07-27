# Workflow 019-020 — client checkpoint helpers

## Status

Completed locally.

## Scope

Add optional checkpoint arguments to the maintained TypeScript and Python
workflow helpers. The clients pass opaque JSON through MCP; Glass remains the
authority for reconciliation and resume safety.

## Acceptance criteria

- [x] TypeScript exposes a typed checkpoint map on `workflow`.
- [x] Python accepts an optional checkpoint map on `workflow`.
- [x] Omitted checkpoints preserve normal workflow execution.
- [x] Supplied checkpoints are passed unchanged to MCP.
- [x] Client documentation describes suffix resume without claiming local
  reconciliation.

## Validation

```text
npm run typecheck --prefix clients/typescript
python3 -m py_compile clients/python/glass_client.py
git diff --check
```
