# Experience commands

Status: Current 0.3.13 source behavior (including current-source work in this checkout)

Experience commands are browser-free inspection and orchestration surfaces. They
return a bounded JSON `ExperienceResult` envelope and do not start, attach to,
or adopt Chrome. The envelope is non-authoritative CLI evidence: it carries an
operation, status, result, provenance, and (when applicable) typed
`resourceRefs`.

```console
glass workspace list
glass memory status
glass memory explain RECORD_ID
glass surfaces coverage SURFACES.json
glass backend test BACKEND.json
glass replay diff SCENARIO.json A.json B.json
```

Use `glass COMMAND --help` for the installed Clap syntax. The complete command
inventory and binary boundary are in [the CLI reference](../cli.md); this guide
specifies the common envelope and ownership rules rather than duplicating the
TUI. For resident UI routes and output placement, see [the development TUI
architecture](../architecture/development-tui.md) and [the runtime
guide](../development-runtime.md).

## Supported command families

| Family | Public routes | Browser use | Output contract |
|---|---|---|---|
| `workspace` | `list`, `inspect ID`, `suspend ID`, `resume ID`, `delete ID` | None | `ExperienceResult` with workspace-scoped `resourceRefs` where an identity exists |
| `memory` | `status`, `inspect RECORD_ID`, `explain RECORD_ID`, `forget RECORD_ID`, `export [PATH]`, `prune`, `reindex` | None | `ExperienceResult`; local knowledge snapshot is advisory |
| `surfaces` | `inspect FILE`, `coverage FILE` | None | `ExperienceResult` after strict surface-set validation |
| `backend` | `status FILE`, `capabilities FILE`, `test FILE` | None | `ExperienceResult`; `test` may run the built-in semantic-proof backend only |
| `replay` | `inspect SCENARIO INPUT`, `diff SCENARIO BEFORE AFTER`, `attach SCENARIO INPUT` | None | `ExperienceResult` after strict scenario/replay binding |

`knowledge` is a separate top-level command family. It reads and mutates the
same profile-scoped store but prints its raw snapshot/record contracts rather
than an `ExperienceResult`; use it when the canonical knowledge JSON is needed.
`memory` is the experience projection of bounded local memory. Neither family
can authorize a browser or project action.

## Envelope and ownership

A successful response has the following shape (fields are versioned by the
Rust contract; do not infer authority from display text):

```json
{
  "schemaVersion": 1,
  "operation": "workspace.inspect",
  "status": "ok",
  "result": {},
  "provenance": {
    "source": "cli",
    "authoritative": false,
    "resourceRef": null,
    "revision": null,
    "observedAt": "..."
  },
  "resourceRefs": []
}
```

`provenance.source` is `cli` and `authoritative` is false for these commands.
`resourceRefs` are typed workspace references, not executable locators or
permission grants. A workspace mutation updates the persisted registry and
returns the resulting identity reference; `workspace delete` returns a
reference to the deleted identity. Workspace ownership and lifecycle revisions
remain enforced by the store. A stale revision, active owner, or unknown ID
fails closed; the CLI does not silently retry or adopt another workspace.

The experience command itself owns only its local read/write operation. It does
not claim ownership of Chrome, a terminal pane, a Pi session, a development
process, or a live browser target. The interactive TUI owns those UI resources;
its command-palette result is a compact status string and updates the selected
surface, editor projection, or terminal pane rather than printing an experience
envelope.

## Memory, surfaces, and backends

`memory` opens the profile-scoped knowledge snapshot (or the path supplied by
`--knowledge-store`). `status` reports bounded statistics; `inspect` returns a
record; `explain` includes scope, source, confidence, history, and a content
hash; `forget` removes one record; `prune` removes stale, contradicted, and
quarantined records; `reindex` reloads and validates the snapshot. `export`
prints canonical JSON unless an output path is supplied. A supplied path is
written locally and the envelope reports the path and record count. The
persistent-profile policy is required.

`surfaces inspect|coverage FILE` parses and validates the complete surface-set
JSON before producing a result. Coverage reports evidence and declared
capabilities; detection is not an input grant. `backend status|capabilities|test
FILE` validates a declared backend profile first. `backend test` proves only the
capabilities exercised by that profile. A semantic-proof result is deterministic
proof, not real-browser parity.

## Replay and failure behavior

Replay commands validate the scenario JSON and the redacted replay bundle
before producing a result. `inspect` reports scenario/fixture identity, event
count, and content hash; `attach` reports the same bounded metadata with
`attached: true`; `diff` compares two validated bundles. Attachment does not
replay browser input and does not grant a live target.

Malformed JSON, unknown records/workspaces, invalid schemas, scope mismatch,
expired or incompatible evidence, and denied profile capability are errors.
The command exits unsuccessfully and writes the diagnostic error to stderr; a
successful structured response is compact JSON on stdout. Correct the input or
refresh the referenced state instead of stripping provenance and retrying under
another scope. No experience command has an interactive prompt.

With stdin/stdout redirected, these commands remain promptless. In an
interactive terminal, `glass` and `glass tui` launch the development workspace
only when the command is omitted or explicitly `tui`; an experience command
never changes that startup rule. `glass-browser` exposes the browser product's
experience routes, but not the development `project`, `agent`, or `harness`
routes.

MCP consumers receive the same canonical result payload inside JSON-RPC and
must negotiate the corresponding capability first. The generated schemas and
Rust types are the contract authority; this page is a usage guide.
