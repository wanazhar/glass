# Workflow authoring

Workflow authoring converts YAML or JSON into the validated workflow contract.
The commands are offline. They do not start Chrome.

## Commands

Use these commands with a source file:

| Command | Result |
|---|---|
| `glass workflow compile FILE` | Write canonical workflow JSON. |
| `glass workflow format FILE` | Write deterministic YAML. |
| `glass workflow validate FILE` | Print the compiled document and diagnostics. |
| `glass workflow lint FILE` | Print diagnostics and fail on error findings. |
| `glass workflow preview FILE` | Print a value-free execution shape. |
| `glass workflow diff BEFORE AFTER` | Compare validated definitions. |
| `glass workflow record` | Import semantic events and write a reviewable draft. |

Add `--warnings-as-errors` to `lint` when warnings must fail the command.
The `record` command reads the event envelope from stdin when `--input` is
not present.

The existing `glass workflow FILE` command remains the browser execution
command. Authoring commands run before browser startup.

## Source and validation

YAML is an authoring format. Canonical JSON is the runtime format. Both use the
same strict schema and validation rules.

The compiler reports diagnostics for:

- missing postconditions;
- unknown transaction classes;
- unsafe retries;
- non-idempotent steps without an effect marker;
- invalid `${inputs.name}` references;
- literal values in type and select actions;
- fragile CSS, ordinal, coordinate, and revision targets; and
- sensitive-looking input names.

Glass marks an input as sensitive when its name contains terms such as
`password`, `secret`, `token`, `api_key`, or `cookie` and no
declaration exists. Setting `sensitive: false` for such an input is an error.

Input values do not enter source or preview output. Callers provide them at
execution time.

## Semantic recording

`WorkflowRecorder` creates a local draft from semantic resolution evidence.
A draft may contain:

- the intent phrase;
- the resolution state;
- the revision;
- the semantic region kind;
- the route without query or fragment;
- digest-only target and frame evidence;
- a target fingerprint digest;
- the transaction class; and
- the postcondition.

A current target reference is not a replay selector. An ambiguous or unselected
result remains review-required. It does not become executable.

Typed values use `${inputs.name}` placeholders. `inferred_inputs()` creates
value-free string declarations and marks sensitive names.

The record command imports an explicit event envelope. It does not attach to
Chrome. It does not intercept user input. It does not observe a session without
an explicit evidence input.

Before execution, review the draft. Add budgets, a terminal condition, outputs,
and required postconditions.

## Preview and diff

Preview reports:

- action names;
- semantic or batch shape;
- transaction class;
- retry and repeat limits;
- postcondition presence; and
- input names.

Preview omits URLs, selectors, expressions, and values.

Diff reports additions, removals, order changes, transaction changes, retry
changes, postcondition changes, input changes, and terminal-condition changes.
It marks breaking changes for review. It does not authorize or migrate a
workflow.
