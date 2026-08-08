# Semantic resource and correctness audit

Status: Completed and verified locally on 2026-08-08

## Confirmed audit targets

| Boundary | Current cost or defect | Required outcome |
|---|---|---|
| Task compiler | Repeats graph BFS for each form field and submitter | One reachability set per selected form scope |
| Live binding | Allocates a candidate vector for every key | Allocation-free unique-match scan |
| Agent gateway | Rebuilds the complete descriptor vector during calls | Descriptor catalog retained by the gateway |
| Tool envelope | Call ID is not explicitly bounded | Reject empty or oversized IDs and names before audit |
| Pi request file | Metadata is checked before an unbounded `read_to_string` | Handle-bound, capped read with one-use cleanup |
| Project source reads | Several metadata checks precede unbounded path-based reads | One shared handle-bound reader for project, editor, graph, and LSP paths |
| Tool JSON evidence | Arguments and successful results are copied into temporary byte vectors | One streaming argument digest and allocation-free result sizing |
| Form name matching | Normalizes every entity name for every authored input | One normalized scoped-field index per compilation |

## Integration chains

```text
Task Protocol -> compiler -> scoped graph index -> live binding -> CDP action
Pi extension -> private request file -> CLI broker -> gateway -> semantic tool
Local harness -------------------------------> gateway -> project event audit
```

Each chain must use the real implementation in tests. No optimization may
introduce a fallback target, simulated browser result, raw value persistence,
or screenshot capture.

## Verification evidence

- all-feature workspace suite: 762 library tests passed, one ignored, with all
  integration and doc tests passing;
- native Chromium suite: all 19 scenarios passed, including Web IR corpus,
  scoped form execution, development edits, MCP lifecycle, and reliability;
- workspace Clippy, strict rustdoc, and all-feature release build passed;
- `cargo deny check`, `cargo audit`, fuzz-target check, Web IR corpus validator,
  release-documentation validator, and TypeScript/Python client smokes passed;
- `glass-browser` package verification and publish dry-run passed with 183
  packaged files and no upload;
- the pinned 1000-iteration benchmark reports 784.361 microseconds median,
  910.041 microseconds p95, and 2268 KiB process peak resident memory.
