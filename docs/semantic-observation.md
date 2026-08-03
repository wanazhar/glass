# Semantic observations

A semantic observation is a bounded, versioned view of the current page. Glass
builds it from browser accessibility data.

A classification is evidence. It does not authorize an action.

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

## Privacy and limits

Semantic data may contain references, roles, names, input types, labels, visible
text, regions, evidence, and change vectors.

Glass does not include form values, cookies, credentials, evaluated source, or
screenshot pixels in semantic data.

Glass bounds all semantic data before serialization. The schema is
[semantic-observation-v1.schema.json](schema/semantic-observation-v1.schema.json).
