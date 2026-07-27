# Semantic observations

Semantic observations are a bounded, versioned view of the current page. They
classify page and landmark regions from the browser's accessibility evidence;
the classifications are advisory and do not authorize an action.

Every observation includes a schema version, route identity, page revision,
page classification, region summaries, and omission limits. Region expansion
handles carry the same target, frame, URL, and revision so a handle cannot be
silently reused after the page changes.

## Levels

Choose the smallest level that answers the question:

| Level | Adds |
|---|---|
| `summary` | Page classification and bounded region summaries. |
| `interactive` | Revision-scoped action references grouped by region. |
| `structured` | Bounded visible page text. |
| `detailed` | The bounded compact accessibility projection. |
| `raw` | The same bounded fields plus an explicitly named raw accessibility projection. |

Form values, screenshots, and deep DOM are separate explicit capabilities;
they are not smuggled into semantic levels. An omitted or truncated field is
reported through `limits` rather than silently treated as complete.

## CLI

```console
glass observe --level summary
glass observe --level interactive
glass observe --level structured --region region_main_1
```

Semantic options cannot be combined with `--deep-dom`, `--screenshot`, or
`--form-values`. In the TUI, use `semantic LEVEL [REGION_ID]`.

## MCP

Call the existing `observe` tool with `level` set to one of the five values.
Add `region` for a revision-checked scoped expansion. Calls without `level`
retain the compact `PageContext` response for compatibility.

## Revisions and diffs

`SemanticObservation::diff_from` accepts observations only when their target,
frame, URL, and level match and the revision does not move backward. It
reports bounded region and target additions, removals, and updates. A
continuity entry is emitted only for a unique role/name/input-type match; it is
an advisory link between two references, not a replacement reference and not
permission to perform an action.

## Privacy and limits

Semantic targets contain references, roles, names, and input types. They do
not contain form values, cookies, credentials, evaluated source, or screenshot
pixels. Labels, roles, visible text, regions, targets, evidence, and change
vectors are bounded before serialization. The JSON schema is published at
[`docs/schema/semantic-observation-v1.schema.json`](schema/semantic-observation-v1.schema.json).
