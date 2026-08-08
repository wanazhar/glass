# Semantic core hardening

Status: Accepted for the local `0.3.2` candidate

## Goal

Make Web IR an executable, privacy-aware source of browser authority rather
than an inventory or an advisory description. A task may execute only when the
compiler, live binding layer, and runtime all agree on the same revision,
entities, evidence, and capabilities.

## Invariants

1. Corpus fixtures are exercised through live extraction. Static manifest
   validation remains useful, but never counts as runtime evidence.
2. Selection is relationship-scoped. Form fields and submitters must belong to
   the selected form through `contains`, `owns`, or `submits`; evidence on an
   unrelated entity cannot satisfy a selected operation.
3. Browser handles are ephemeral execution bindings, not Web IR. A binding is
   created from the same page revision as the IR, is keyed by entity ID, and is
   rejected after revision drift or when semantic resolution is ambiguous.
4. State is observed conservatively. Checked and disabled state flow from live
   evidence; unknown state remains unknown. Sensitive inputs are never
   classified public merely because their label is unfamiliar.
5. Plans declare runtime capabilities. The interpreter rejects unsupported
   operations and postconditions before dispatch. `entityState` is verified
   from one unique target in a fresh, revisioned structured observation.
6. Continuity is scoped by graph context. Duplicate labels in different forms
   or regions do not rebound across those boundaries.
7. Agent tools share one bounded gateway. Schemas are validated, mutating calls
   require explicit authority, results are capped, and persisted events contain
   metadata rather than values or page secrets.
8. Execution returns a compact explanation receipt: selected entities,
   evidence decisions, capability checks, and postcondition outcomes. Compact
   projections are the default agent context; full Web IR remains explicit.

## Data flow

```text
one bounded PageContext
  -> evidence + execution origins
  -> Web IR v1 + revision-bound bindings
  -> relationship-scoped compiler
  -> capability preflight
  -> guarded plan interpreter
  -> fresh postcondition extraction
  -> compact execution receipt
```

Stable Web IR JSON never contains CSS, XPath, backend node IDs, CDP object IDs,
form values, screenshots, or credentials. Bindings live only in memory and are
not accepted from callers.

## Verification contract

- Every versioned corpus fixture has live-Chromium runtime assertions for the
  exact entity multiset, relationship-kind set, opaque-region count, and Web
  IR schema validity.
- Adversarial fixtures cover duplicate labels, unrelated evidence, reordered
  markup, state changes, and privacy-sensitive controls.
- Compiler tests prove that out-of-scope fields and submitters are rejected.
- Runtime tests prove capability preflight and entity-state evaluation.
- Gateway tests prove schema, authority, output-limit, and redaction behavior.
- Formatting, unit/integration tests, browser smoke, Clippy, rustdoc, package,
  and release validators must pass before the checkpoint is complete.
