# Workbench contracts sweep

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: implementation in progress; final build, tests, release PTYs and Luna medium review pending.

Part of the full roadmap sweep, not a claim that all roadmap items are complete.

## Implemented contracts awaiting final verification

- Workspace review findings have typed severity, location, title, evidence and disposition. Imports validate as a batch; writes use the existing locked atomic workspace store. Searchable native finding cards and disposition controls share the workspace modal.
- Registered artifacts support bounded UTF-8 byte ranges with revision validation and continuation offsets. Source truncation and preview truncation are separate fields.
- Diagnostic previews allowlist event kinds/counts, numeric request metadata and prefix hashes. They omit prompts, reasoning, commands, paths, credentials and endpoints. Saving uses exclusive creation with private permissions; no upload occurs.
- Opt-in project memory records explicit facts, sources and review dates. Retrieval is bounded to 16 facts / 8 KiB, disabled by default, and appears as an attributed source in request fingerprints. Updates are serialized and atomic; forget removes a fact from future context.
- Attachment composition supports bounded line excerpts with source revisions, removable snapshots and token estimates. Whole-file reads reject special files and oversized inputs before unbounded allocation.
- Slash palette navigation during active turns is pumped alongside model work. Queue, agent, reasoning and tool-output views are available; state-changing actions are labeled for after the turn. Esc clears the slash query and preserves saved drafts.

## Final acceptance cases

Desktop and 40×24/20×8 PTYs: active-turn slash open/filter/Esc, turn completion while palette is open, Ctrl-C, reasoning view, queue promotion, reader restore, attachment range selection and removal, findings evidence/disposition.

Headless: invalid finding imports cannot partially apply; stale/Unicode artifact ranges fail safely; private diagnostic save cannot overwrite; bundles omit secret fixtures; disabled memory injects nothing, enabled retrieval is bounded and attributable, concurrent writes cannot silently replace facts.

## Result contracts

`gc contract SCHEMA.json RESULT.json` validates bounded local contracts. `gc run --output-schema FILE` validates the final answer and permits at most one tool-free formatting correction. Original outputs remain in the transcript; invalid corrected outputs fail visibly. `gc agent create ... --schema FILE` lets the parent define a persistent teammate's deliverable; invalid output blocks the worker with a precise validation path while retaining its transcript. Valid output has a durable artifact reference; receipts include the schema fingerprint rather than resending the schema.

The native schema vocabulary covers types, properties, required/additional properties, arrays, prefix items, uniqueness, lengths, numeric bounds, enum/const, combinators and local JSON Pointer references. Regex, formats, remote references and other unsupported keywords fail explicitly. This is a bounded vocabulary, not a claim of general JSON Schema conformance. Numeric arithmetic uses JSON floating point semantics.
