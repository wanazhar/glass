# Native live Web IR extraction (325)

Status: implemented locally in the native runtime.

This slice gives the Glass-owned native runtime a live Web IR v1 extraction
path. `BrowserRuntimeSession::native_extract_web_ir` captures the native
semantic/layout revision under the session operation lock, honors document,
region, and frame scope, and passes the result through the existing bounded
evidence reconciler and Web IR validator. A frame request selects an exact
currently attached native frame; a region request narrows text and targets to
the named semantic region.

The native adapter projects regions, actionable targets, form controls,
navigation targets, native frame boundaries, viewport metadata, and bounded
visible text. Password values are never part of this projection. Evidence
classes that the native semantic snapshot cannot prove are listed in
`limits.missingSources`, while node, text, depth, output, and observation
truncation remain explicit in the returned coverage and limits.

The CLI adds `extract-web-ir [REQUEST]`; omitted input uses the bounded default
request and `-` reads a strict `ExtractionRequest` from stdin. MCP adds the
read-only `extractWebIr` tool with the same request contract. Both routes use
the native runtime when native is selected and the existing Chromium evidence
adapter when Chromium is explicitly selected. Conformance fixtures and tool
documentation are updated with the new live operation.

Focused validation covers a live native data document, Web IR graph
validation, region/frame scope handling, source omissions, and the shared
MCP/CLI registration paths. The remaining issue #40 work continues with
resident `glass-dev` native-session ownership, full browser-surface parity,
Core Web Profile certification, recovery/cancellation certification, and
cross-platform release evidence.
