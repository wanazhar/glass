# Shared Glass and GC chat resumption

Status: authorized for implementation; not yet shipped.

## Contract

A closed conversation can be discovered and resumed from Glass, GC TUI, or GC
Connect WebUI for its canonical project. All clients continue the same append-only
Core log and conversation ID. They must not copy a log into a second writable
conversation or silently fork it. The existing canonical-path writer lock remains
authoritative. An active conversation is visible but cannot acquire a second
writer; show a clear active-in-another-app message and preserve the draft.

Creation origin is stable metadata: `glass`, `gc`, or unknown for a legacy record
whose origin cannot be established. Pickers show `Created in Glass`, `Created in
GC`, or `Unknown origin`. Switching clients must not overwrite origin. Legacy
inference is allowed only for known native GC and project Glass session stores.

Glass attaches its governed integration tools when adopting a native GC session.
GC uses its native tool capabilities when resuming a Glass session and must not
pretend that unavailable Glass host services are attached. Returning to Glass
refreshes its bounded host catalog. Credentials remain owned by Core. Workspace,
provider/model settings, messages, tool results, attachments, and origin survive
handoff. Existing client-specific drafts must not be deleted or overwritten.

## Modules and integrations

- Core session discovery and metadata: enumerate the current GC store and the
  project's `.glass/agent-sessions` store, deduplicate canonical paths, retain a
  stable source reference and origin, and validate workspace identity.
- Core runtime adoption: open native GC logs under the existing writer lock and
  bind mixed Glass tool ownership without copying history. Keep existing strict
  resume behavior for unrelated ownership mismatches. Validate all host schemas.
- Glass registry and picker: include resumable GC conversations alongside durable
  Glass threads, map selected references to the same canonical Core log, and
  retain cross-store references across restart.
- GC TUI and Connect backend: share discovery/reference resolution and origin;
  resume external Glass logs safely through existing dispatch paths.
- Web assets: show origin labels and active-owner errors and resume through the
  actual backend, without duplicating a thread in the sidebar.

The real chains to verify are GC → Glass → GC and Glass → WebUI → Glass, including
process restart, existing history, a new response, tool ownership, and simultaneous
open rejection. Filesystem-only list tests are insufficient.

## Picker layout

```text
Resume conversation
  Parser failure        Created in GC      /project
  Browser investigation Created in Glass   /project
  Older conversation    Unknown origin     /project

Enter resume · Esc back
Active elsewhere: close it in the other app, then retry. Draft retained.
```

Use the existing picker typography and responsive layouts. Labels must remain
readable at compact widths and in WebUI. Empty, loading, error, and active states
must be distinct; never replace a failed adoption with a fresh conversation.
