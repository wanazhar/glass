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

## Connect integration contract

Thread rows retain their existing fields and add `origin`, `originLabel`, and a
trusted `sourceRef`. Origin is `glass`, `gc`, or null. `sourceRef` is a backend-issued
canonical reference, not permission to open an arbitrary caller-supplied file.
Every reference must resolve within an authorized shared store and match the
recorded project and conversation ID. Ambiguous IDs must not resolve arbitrarily.

`POST /api/open` accepts `{id, sourceRef}` for shared rows; existing native rows
remain compatible. The response retains the existing open payload and supplies the
resolved session/reference and origin. Writer contention is returned distinctly as
`session_active_elsewhere`. The frontend must acquire successfully before replacing
its current chat, stream, selection, and composer, or restore all of those on failure.

Connect intentionally keeps workers alive after a tab disconnects. Therefore this
feature also needs an explicit idle-only release/handoff operation in WebUI. It
releases the writer without deleting history or interrupting a running turn. A busy
conversation must refuse release clearly. The browser journey must release its
conversation and then resume the same log in Glass; stopping the entire gateway is
not an acceptable handoff test.

Existing per-session provider/model metadata is authoritative. For a legacy native
GC conversation that never recorded those settings, use Core's current settings
explicitly rather than claiming that missing historical configuration was recovered.
