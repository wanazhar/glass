# Experience commands

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

With stdin/stdout redirected, Glass remains promptless and prints JSON; the
interactive TUI remains explicit via `glass tui` in a terminal. A bare
`glass` invocation prints the concise start-here message and never starts
Chrome.
