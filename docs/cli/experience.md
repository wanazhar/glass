# Experience commands

Experience commands expose workspace, memory, surfaces, backend, and replay
state through a consistent browser-free envelope. They are intended for
diagnostics and orchestration that must not accidentally launch or adopt a
browser.

All commands emit bounded JSON with an `experience` envelope for the
browser-free integration surfaces. They do not start Chrome unless a command
explicitly requires a live browser.

```text
glass workspace list
glass memory status
glass memory explain RECORD_ID
glass surfaces coverage SURFACES.json
glass backend test BACKEND.json
glass replay diff --scenario SCENARIO.json --before A.json --after B.json
```

`memory` is an alias-style management surface for the persistent knowledge
store. `forget`, `prune`, and `reindex` are bounded local mutations; stored
records never authorize an action. `surfaces` and `backend` validate strict
contracts before reporting coverage. `replay inspect`, `replay diff`, and
`replay attach` validate exact scenario binding, bounded redacted recordings,
and scope before returning an experience result; none starts Chrome.

Experience results include `schemaVersion`, `provenance`, and `resourceRefs`.
`resourceRefs` are typed workspace-scoped references, not executable locators.
Mutation commands still require a current revision and explicit capability;
lease expiry is reported as a typed failure rather than silently retried.

## State and mutation boundaries

| Family | Reads | Mutations | Persistent state |
|---|---|---|---|
| `workspace` | registered resource summaries | explicit workspace operations only | bounded workspace registry |
| `memory` | status, record explanation | forget, prune, reindex | advisory knowledge store |
| `surfaces` | schema and coverage evidence | none | input artifact only |
| `backend` | backend declarations/test evidence | bounded test dispatch | no implicit browser state |
| `replay` | scenario/recording inspection and diff | scoped attachment | bounded redacted recordings |

Knowledge is advisory: a stored record never becomes policy authority, a live
locator, or proof that current page/project state still matches it. Replay
attachment validates scenario identity and scope; it does not replay a browser
mutation merely because an earlier result succeeded.

## Failure handling

Invalid schemas, unknown workspace references, expired cursors, scope mismatch,
and denied capability return typed failures. Correct the artifact or refresh
the referenced state. Do not strip provenance and retry the same payload under
a different workspace. A backend conformance result establishes only the
capabilities evidenced by that run; the deterministic semantic-proof backend
is not proof of real-browser parity.

With stdin/stdout redirected, explicit commands remain promptless and print
JSON. In a terminal, both `glass` and `glass tui` start the interactive
workspace. The TUI owns its browser lazily: opening the workspace alone does
not start Chrome; the first browser operation or explicit browser launch does.

Use `glass COMMAND --help` for exact arguments and [the CLI reference](../cli.md)
for the complete command tree. MCP consumers receive the same canonical result
inside JSON-RPC and must negotiate the corresponding capability first.
