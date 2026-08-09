# Semantic observations

A semantic observation is a bounded, versioned view of the current page. Glass
builds it from browser accessibility data.

A classification is evidence. It does not authorize an action.

Use semantic observation as the default page-reading path for humans and
agents. It is smaller and safer than a full DOM or screenshot, preserves
accessibility relationships, and produces revision-bound references for
guarded actions.

Each observation contains:

- a schema version;
- route identity;
- a page revision;
- a page classification;
- region summaries;
- viewport geometry; and
- omission and truncation limits.

Structured text is a bounded document projection. When `limits.textTruncated`
is true, expand a named region instead of assuming the text is complete. The
observation also reports `limits.textBytes`.

Region expansion is payload-local. It returns the selected region's bounded
text, targets, and (for `detailed` or `raw`) accessibility subtree instead of
repeating the full-page projection. `limits.omittedRegions`,
`limits.omittedTargets`, and `limits.omittedBytes` make any omitted page-level
content explicit.

Viewport geometry reports scroll offsets, viewport dimensions, and document
dimensions. Semantic text remains document-oriented; use the geometry to decide
whether a fresh observation is needed after scrolling.

A region handle contains the target, frame, URL, and revision. Glass rejects the
handle when the page changes.

## Observation levels

Select the smallest level that answers the question:

| Level | Data |
|---|---|
| `summary` | Page classification and region summaries. |
| `interactive` | Revision-scoped action references grouped by region. |
| `structured` | Bounded visible page text. |
| `detailed` | A bounded compact accessibility projection. |
| `raw` | The bounded fields and an explicit raw accessibility projection. |

Selection guidance:

- use `summary` for routing, page-state classification, and cheap monitoring;
- use `interactive` before selecting or acting on controls;
- use `structured` for visible document content and records;
- use `detailed` when hierarchy or accessibility properties matter; and
- use `raw` only for explicit protocol/debug analysis.

Form values, screenshots, and deep DOM are separate operations. Glass does not
add them to a semantic level. Glass reports omitted and truncated fields in
`limits`.

## CLI

Run:

```console
glass observe --level summary
glass observe --level interactive
glass observe --level structured --region region_main_1
```

Do not combine semantic options with `--deep-dom`, `--screenshot`, or
`--form-values`. In the TUI, use `semantic LEVEL [REGION_ID]`.

## MCP

Call the `observe` tool with `level` set to one of the five levels. Add
`region` for a revision-checked region expansion.

If `level` is not present, Glass returns the compact `PageContext` response
for compatibility.

## Observation lifecycle

```text
target/frame selection
        │
        ├─ navigation, reconnect, or selection change -> invalidate old context
        │
        └─ fresh accessibility tree + bounded page metadata
                         │
                         ├─ assign monotonic page revision
                         ├─ build regions/text/targets/limits
                         └─ publish revision-bound references
```

Glass fetches the complete accessibility tree needed for a correct observation
instead of trusting a partial incremental mirror. The in-memory compact context
may satisfy a compatible cached read. Deep DOM, screenshots, network bodies,
and raw page payloads do not enter that cache by default.

Navigation, page replacement, frame change, target change, disconnect, and
reconnect invalidate incompatible references. A successful connection recovery
queues a fresh observation before browser-aware agent tools become current.

## Regions, records, and Web IR

Regions are bounded page-local areas such as forms, dialogs, navigation,
tables, and collections. Region expansion requires the current target, frame,
route, level, and revision. A stale region handle fails rather than expanding a
different page area.

Structured extraction can produce typed table, collection, or field records
with bounded provenance. Glass Web IR is a separate stable reconciliation
contract built from validated evidence. An observation is not automatically a
portable Web IR document, and an old Web IR entity is not a live browser
reference.

## Revisions and diffs

The page revision is persisted in the attached page context when possible, so
separate Glass CLI invocations attached to the same page continue the same
revision sequence. A new document starts a new page context and therefore a
new sequence. The daemon remains the preferred option for long-lived guarded
sessions.

`SemanticObservation::diff_from` accepts observations with the same target,
frame, URL, and level. The new revision must not be lower than the old revision.

The diff reports bounded region and target additions, removals, and changes.

A continuity entry requires one unique role, name, and input-type match. It is
an advisory link. It is not a replacement reference. It does not authorize an
action.

## Failure and recovery

| Failure | Meaning | Recovery |
|---|---|---|
| no active target | browser is detached, recovering, or has no eligible page | launch/attach/select a target, then observe |
| target ambiguity | endpoint has multiple eligible pages | list targets and choose explicitly |
| stale region/revision | page changed after the handle was issued | request a new observation and region handle |
| truncated text/regions/targets | response hit a declared bound | expand a named region or refine the question; do not assume omitted content is absent |
| opaque coverage | browser surface cannot supply semantic evidence | use explicit visual evidence or a supported surface without claiming semantic understanding |
| protocol/timeout | current observation could not be completed | inspect browser health; no partial tree is promoted as complete |

## Consumer rules

- Branch on schema fields and typed limits, not formatted display text.
- Keep the revision with every selected target.
- Treat continuity as advisory evidence, never action authority.
- Request form values only under the dedicated capability and privacy policy.
- Request screenshots only when visual evidence is actually required.
- Re-observe after navigation, target/frame selection, reconnect, or an action
  whose verification reports changed state.

## Privacy and limits

Semantic data may contain references, roles, names, input types, labels, visible
text, regions, evidence, and change vectors.

Glass does not include form values, cookies, credentials, evaluated source, or
screenshot pixels in semantic data.

Glass bounds all semantic data before serialization. The schema is
[semantic-observation-v1.schema.json](schema/semantic-observation-v1.schema.json).
