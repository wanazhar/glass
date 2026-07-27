# Workflow 019-010 — thin-client helpers

## Status

Completed locally.

## Scope

Expose workflow execution through the maintained TypeScript and Python MCP
clients. The helpers pass the definition and typed input map to the MCP
`workflow` tool; contract validation remains centralized in the Glass runtime.

## Acceptance criteria

- [x] The TypeScript client exposes a generic `workflow` helper.
- [x] The Python client exposes a `workflow` helper using standard-library types.
- [x] Both client READMEs include a truthful workflow example.
- [x] No client duplicates or weakens runtime workflow validation.

## Validation

```text
npm run typecheck --prefix clients/typescript
python -m py_compile clients/python/glass_client.py
git diff --check
```
