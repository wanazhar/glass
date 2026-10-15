# Approvals and user questions

GC distinguishes permission decisions from ordinary questions. Both pause the
current turn. The model makes no further provider request while a native dialog
waits for input.

## Execution modes

```sh
gc --restricted
gc --restricted run --json "Study this project"
gc --restricted resume SESSION_ID
gc --yolo
```

Put a mode flag before `run` or `resume`. Mode flags apply to conversation
startup; other CLI subcommands use the saved profile. Existing saved settings
remain the default when no mode flag is supplied.

`--restricted` uses workspace writes and disables shell networking. A saved
`permission_profile = "restricted"` is an alias for `workspace-edit`; its network
ports default to none unless explicitly configured. Native reads may inspect
regular files outside the workspace; outside writes need an approved exception.
Read-only and strict profiles keep their narrower read policy.

`--yolo` and `--unrestricted` disable local sandboxing and permission prompts.
Administrator requirements and explicit tool-policy denials still apply.
Revision checks, file-format bounds, symlink refusal and read-before-write checks
remain data-integrity rules, separate from permission approval.

## Requesting an exception

The model adds these fields to the actual `bash`, `edit` or `write` operation:

```json
{
  "command": "a concrete command",
  "permission": "require_escalated",
  "justification": "Why this exact operation needs wider access"
}
```

The host shows the exact arguments in a scrollable review, together with the
permission decision. Press **a** to allow once; **d**, Enter or Esc denies. No
operation executes before approval. The grant belongs to that call and is
removed when the call finishes. It does not change the profile or approve an
identical later call. Elevated shell commands must use the session workspace as
their working directory. Native file grants are scoped to the worker handling
that operation and reset afterwards.

File revisions and read-before-write rules still apply after approval. An
exception cannot override an administrator-forced sandbox or an explicit shell
policy denial. Requests larger than 64 KiB are refused instead of showing a
truncated approval. Approval decisions are recorded in the session; grants are
not persisted or inherited by delegated agents.

A sandbox rejection is not automatically replayed with broader rights: the
model must request the exception explicitly. This avoids replaying a command
that may already have performed part of its work.

Headless and delegated agents cannot approve themselves. An elevation request
returns a blocker without execution when no interactive human is connected.

## Asking a question

Interactive models receive `ask_user_question`. It accepts one to four questions:

```json
{
  "questions": [{
    "header": "Storage",
    "question": "Which storage should we use?",
    "options": [
      {"label": "SQLite", "description": "A local database"},
      {"label": "Files", "description": "Plain files"}
    ],
    "multiSelect": false
  }]
}
```

Free text is always available. Compact menus show the question preview; F2 opens
the full question. Omit options for a written answer. Multi-select
questions let the user toggle choices and submit them together. Cancellation is
reported explicitly and stops the current turn; it is never treated as an
answer or a permission grant. Other calls in the same batch are deferred, even
if listed before the question, until the model processes the answer. The tool is advertised only when an agent has a connected interactive input
host. Headless runs, delegated agents and worker-only planning do not advertise
a connected human and never auto-select an answer. Models should ask in their final response when interaction is unavailable.

The workflow follows [Codex's documented approval and sandbox
controls](https://learn.chatgpt.com/docs/developer-commands?surface=cli) and
CommandCode's installed `ask_user_question` tool contract. GC deliberately does
not copy CommandCode's headless first-option fallback.
