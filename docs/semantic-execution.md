# Semantic execution

Glass `0.3.0` uses a deterministic pipeline for high-level browser tasks:

```text
bounded PageContext -> extraction evidence -> Glass Web IR v1
Glass Task Protocol v1 + Web IR v1 -> guarded task plan -> verified runtime result
```

The extractor and compiler are deterministic Rust code. They do not call an
LLM, invent goals, broaden scope, carry CSS or XPath selectors, or treat
revision-local entity IDs as browser authority.

## Stable contracts

`GlassWebIrV1` is the bounded source document. It records the page revision,
semantic entities, relationships, execution details, evidence provenance,
quality, coverage, opacity, diagnostics, and truncation limits. Canonical JSON
uses schema version `1` and rejects unknown fields.

`GlassTask` is the authored intent. Its fixed task families cover forms, field
reads, navigation, tables, collections, regions, dialogs, and pagination. Every
task declares scope, action/item/time limits, risk, ambiguity policy, revision
policy, and optional postconditions.

`compile_task(task, ir)` validates both documents and emits a
`TaskExecutionPlan`. The plan records compiler and source versions, source
revision, a redacted task fingerprint, selected entity IDs, evidence
requirements for every selected entity, value-free semantic binding keys,
runtime capability requirements, revision and action preconditions, risk,
confirmation gates, bounded operations, and generated or authored
postconditions. Compilation is browser-free and side-effect-free.

Compiler version 2 adds entity-scoped evidence, semantic binding keys, and
runtime capability requirements. Persisted version-1 plans must be recompiled
from their authored task against a current Web IR revision before execution.

The same contracts are exposed through the Rust crate, `glass task` and
`glass ir`, MCP `task.*` and `webIr.*` operations, and daemon-isolated MCP child
sessions. The checked-in protocol golden fixture is the cross-interface wire
contract.

## Runtime safety

Browser-backed task execution extracts fresh Web IR, compiles once, then uses
the existing guarded session actions and verification paths. Actionable
entities are resolved once into ephemeral browser references from the exact
compiled revision. Those references are never serialized or accepted from a
caller. Zero, multiple, or stale binding candidates fail before dispatch. A
successful result means the operation-specific effect and compiled
postconditions were observed. Post-dispatch uncertainty is `indeterminate`,
never success.

Every execution result includes a compact, value-free receipt containing
selected and binding-candidate entity IDs, per-entity evidence requirements, runtime
capabilities, the confirmation decision, and postcondition outcomes.

Revision policies are fail-closed:

- `exact`: the compiled revision must equal the caller-observed revision;
- `compatible`: read-only work may bind to a newer uniquely resolved revision,
  while mutating drift is rejected without prior continuity evidence;
- `reextract`: the caller explicitly permits one bounded fresh compilation,
  and every mutating re-extraction requires confirmation.

A revision regression is always rejected. Mutating actions check the compiled
revision immediately before dispatch. Risk can only be raised by the compiler,
not lowered. Remote irreversible, authentication, disclosure, ambiguity-gated,
and mutating re-extraction plans require confirmation. Unsafe blind retry is
reported as `unsafeUntilReconciled` with a bounded recovery operation.

## Evidence, opacity, and limits

Extraction is non-mutating and limited by node count, depth, text bytes, output
bytes, and wall-clock duration. Compilation is limited by Task Protocol action,
input, item, and timeout bounds. Web IR limits cap entities, relationships,
diagnostics, text, and serialized output.

Requested sources that could not be observed appear in `limits.missingSources`.
Observed absence is not reported as missing evidence. Incomplete frames, shadow
roots, inaccessible controls, and unsupported regions reduce coverage or become
explicit opaque entities. Mutating compilation rejects truncated required
evidence; selected entities must meet the evidence quality floor, include every
required source, and advertise the compiled action.

Checked and disabled state flow from live form evidence. Disabled or read-only
entities do not advertise mutation actions. `entityState` postconditions use
the bounded `<entity-name>.<state>=<true|false>` syntax and require one unique
fresh semantic target. Supported states are `disabled`, `readOnly`, `required`,
`checked`, and `empty`.

Sensitivity is conservative and token-aware. Password and one-time-code fields
are secret; payment tokens are financial; email, telephone, address, identity,
and file fields are personal. Unknown free-text fields remain unknown instead
of defaulting to public.

Large MCP diagnostics use bounded summaries or artifact references. Stable Task
and Web IR contracts never contain cookies, credentials, form values, raw DOM,
CDP node IDs, evaluated source, or screenshots.

`webIr.inspect` is the compact agent projection. It includes entity-kind counts
and at most 16 actionable entity summaries with sensitivity and supported
actions; requesting full canonical Web IR remains explicit.

## Executable corpus

`tests/fixtures/web-ir/corpus-v1.json` contains pinned live-extraction goldens
for every fixture. `browser_session_executes_the_versioned_web_ir_corpus` loads
all eight pages in Chromium and checks entity multisets, relationship kinds,
schema validity, and opacity. `adversarial-v1.json` records duplicate-label,
unrelated-evidence, reordering, continuity, privacy, and state mutations covered
by deterministic compiler and reconciliation tests. Static inventory numbers
are fixture metadata, not runtime evidence.

## Examples and failure behavior

Success:

```console
glass task validate task.json
glass ir validate web-ir.json
glass task compile task.json web-ir.json --output plan.json
```

Ambiguity: two compatible entities with `ambiguity: "fail"` produce a
`taskCompilation` error at `scope`. `requireConfirmation` keeps the bounded
candidate set but confirmation-gates the operation.

Stale state: an `exact` plan compiled at revision `7` cannot dispatch for caller
revision `8`. A compatible mutating task also fails on drift. Use an explicit
`reextract` policy and confirmation only when rebinding to current evidence is
acceptable.

Opaque or insufficient evidence: missing required accessibility/form evidence,
quality below `strong`, truncation on a mutating task, absent execution details,
or an unsupported action produces a typed compilation failure before mutation.

Policy block: daemon mutation leases and browser policy still apply after
compilation. A valid plan does not bypass host allowlists, raw-CDP restrictions,
confirmation requirements, or lease ownership.

## Measured acceptance evidence

The 0.3.0 Linux ARM64 acceptance run on 2026-08-06 used Chromium
`150.0.7871.128` and:

```sh
GLASS_E2E=1 CHROME_PATH=/snap/bin/chromium \
  cargo test --locked --test browser_smoke -- --nocapture --test-threads=1
```

All 17 live scenarios passed. The semantic execution fixture measured 40,756
microseconds for extraction and 7,733 microseconds to compile three tasks. Its
pre-Web-IR agent payload was 12,441 bytes, the full canonical Web IR was 22,120
bytes, and the compiled agent payload was 3,632 bytes: a 71% estimated token
reduction from the pre-Web-IR task context. These single-host measurements are
release acceptance evidence, not a cross-platform latency claim or SLA.

## Migration and retained APIs

High-level Task Protocol execution uses live Web IR by default. Existing
low-level navigation, action, observation, locator, raw-CDP, and workflow APIs
remain explicit debugging or integration primitives. Legacy semantic
observations remain the compact human/agent inspection surface and supply
caller revisions; they are not a parallel task compiler.

Offline `ir validate`, `ir inspect`, `ir diff`, `ir continuity`, and
`task compile` use the exact stable contracts used by live execution. There are
no public draft aliases or browser-free/live compiler variants.
