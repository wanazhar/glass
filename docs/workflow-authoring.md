# Workflow authoring

Workflow authoring is a local, unreleased 0.2.0 development surface. It turns
human-readable YAML or JSON into the validated workflow contract used by the
runtime. The authoring tools do not start Chrome.

## CLI

Use the following commands against a workflow source file:

- `glass workflow compile FILE` writes canonical workflow JSON.
- `glass workflow format FILE` renders deterministic YAML.
- `glass workflow validate FILE` prints the compiled document and diagnostics.
- `glass workflow lint FILE` prints diagnostics and returns failure for error
  findings; add `--warnings-as-errors` for stricter CI.
- `glass workflow preview FILE` prints a value-free execution shape.
- `glass workflow diff BEFORE AFTER` compares two validated definitions and
  prints stable hashes, risk levels, and migration guidance.
- `glass workflow record [--input EVENTS_JSON] [--output DRAFT_JSON]` imports
  an explicit semantic-event envelope and emits a reviewable draft. Without
  `--input`, the event envelope is read from stdin.

The existing `glass workflow FILE` form remains the browser execution command.
Authoring subcommands are selected before browser startup, so a malformed
source cannot cause a browser action.

## Source and safety checks

YAML is an authoring format; canonical JSON is the runtime interchange format.
Both use the same strict schema and validation rules. Parse failures report a
bounded diagnostic with a source line and column when the parser provides one.

The compiler currently reports stable diagnostics for:

- missing postconditions and unknown transaction classes;
- unsafe retries and non-idempotent steps without an effect marker;
- undefined or malformed `${inputs.name}` references;
- literal values in type/select actions and value-bearing semantic intents;
- fragile CSS, ordinal, coordinate, and revision-reference targets; and
- sensitive-looking input names.

Names containing terms such as `password`, `secret`, `token`, `api_key`, or
`cookie` are inferred as sensitive when no declaration is present. Explicitly
setting `sensitive: false` for such an input is an error. Input values are
never part of authoring source or preview output; callers provide them at
execution time.

## Semantic recording boundary

`WorkflowRecorder` is a local draft builder for semantic resolution results. A
draft can retain the intent phrase, resolution state, revision, semantic
region kind, route URL without query or fragment, digest-only target/frame
evidence, target fingerprint digest, transaction class, and postcondition.
Current target references are not replay selectors. Ambiguous or unselected
results remain review-required and do not become executable targets.

Typed values are represented only by `${inputs.name}` placeholders. The
`inferred_inputs()` helper creates value-free string declarations and marks
sensitive names. Callers must review the draft and provide budgets, terminal
conditions, outputs, and any required postconditions before execution.

The record command is an offline evidence importer, not browser event
interception. Integrations must supply semantic resolution evidence explicitly
or use the library recorder API; the command does not attach to Chrome or
silently observe user input.

## Preview and diff

Preview output describes action names, semantic-vs-batch shape, transaction
classes, retry/repeat bounds, postcondition presence, and referenced input
names. It omits URLs, selectors, expressions, and values.

Diff output is keyed by stable step IDs and reports additions, removals,
reordering, changed effect/retry/postcondition declarations, input declaration
changes, and terminal-condition changes. Breaking changes are marked for
review; the diff does not authorize or migrate a workflow automatically.
