# Actions and revisions

Glass actions can be used as ordinary commands or tied to a specific
observation. The revision-safe form is useful when a page may change between
inspection and execution.

## Observation to action

1. Call `observe` and retain its page revision and the target reference.
2. Pass both values to `expectedRevision` (MCP), `--expected-revision` (CLI),
   or the corresponding Rust method.
3. Use the returned status and verification fields.
4. If the revision is stale, observe again before retrying.

Example CLI flow inside the terminal UI:

```text
navigate https://example.com
observe
click r7:b42 --expected-revision 7
```

The same guard is available on the MCP `navigate`, `click`, `type`, and
`fillForm` tools. The Rust library exposes `click_with_revision`,
`type_text_with_expected_revision`, `fill_form_with_expected_revision`, and
`navigate_with_revision`.

## Successful action result

Input actions return the existing action fields plus the common contract fields:

```json
{
  "status": "succeeded",
  "action": "click",
  "revision": 8,
  "previousRevision": 7,
  "currentRevision": 8,
  "target": {"label": "Save", "reference": "r7:b42"},
  "target_id": "page-target",
  "frame_id": "main-frame",
  "verification": {
    "revisionDelta": 1,
    "urlChanged": false,
    "titleChanged": false,
    "targetChanged": false,
    "frameChanged": false
  }
}
```

`verification` is deliberately bounded. It may report URL/title changes,
target or frame transitions, revision delta, and count-only accessibility
changes. Popup actions include their popup witness. Navigation results include
the resulting page metadata. Older unguarded methods remain available for
callers that do not need an expected revision.

## Failure kinds

Clients should branch on structured fields rather than matching display text.

| Kind | Meaning | Recovery |
|---|---|---|
| `stale_revision` | The expected page revision is no longer current | Observe again and retry with the new revision |
| `ambiguous_target` | More than one target matched | Refine the locator; never choose a candidate implicitly |
| `target_not_found` | No target matched | Check the locator or observe the current page |
| `denied` | The active policy disallows the operation | Change policy configuration or the operation |
| `confirmation_required` | The operation needs an explicit capability approval | Supply the configured confirmation token |
| `transport` | The CDP connection or protocol failed | Check browser health and retry when appropriate |
| `verification_failed` | The action ran but its postcondition was not satisfied | Observe the result and decide whether a retry is safe |

Legacy target errors retain their original `kind` field and also expose the
normalized `failureKind` field. Candidate lists and verification metadata are
bounded; page text, selectors, typed values, and raw CDP details are not copied
into the normal action envelope.

## Related references

- [CLI reference](cli.md)
- [MCP integration](mcp.md)
- [Architecture: browser data plane](architecture/browser.md)
- [Security policy](../SECURITY.md)
