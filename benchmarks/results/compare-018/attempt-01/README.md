# Compare-018 evidence: 2026-07-13

This directory retains the first required 100-iteration acceptance execution
at Git revision `251c8ab37746594ef159f1b70abec71deea2cd9f`.

- Start: `2026-07-13T23:20:01Z`
- Aggregate generated: `2026-07-13T23:23:06Z`
- Host: Linux arm64
- Chromium: `/snap/bin/chromium`, `Chromium 150.0.7871.46 snap`
- Profile semantics: fresh ephemeral browser state, one warm session per adapter
- Viewport: 1280x720

Command:

```sh
CHROME_PATH=/snap/bin/chromium \
  GLASS_SCORECARD_ITERATIONS=100 \
  GLASS_ACCEPTANCE_COMMAND_TIMEOUT_MS=600000 \
  GLASS_ACCEPTANCE_ALLOW_FAILURE=1 \
  node benchmarks/run-acceptance.mjs
```

The aggregate correctly sets `best_in_class_eligible` to `false`. No
best-in-class claim is supported by this evidence.

## Adapter results

| Adapter | Aggregate status | Raw outcome | Diagnosis |
|---|---|---|---|
| Glass 0.1.0 | failed validation | 600 success, 100 failure, 400 unsupported, 0 wrong actions | The raw report is complete, but the runner compares serialized viewport objects by key order. Glass emits `height,width` while the contract contains `width,height`, so equivalent controls are rejected. Delayed content failed all iterations; popup, frame, dialog, and download were reported unsupported. |
| Playwright 1.61.1 | completed | 600 success, 500 failure, 0 unsupported, 0 wrong actions | Five scenarios fail because the adapter calls `inputValue()` on the fixture's `<output id="result">`; Playwright reports that the node is not an input, textarea, or select. This is an adapter defect, not product evidence for those scenarios. |
| `@playwright/mcp` 0.0.78 | failed | no scenario report | The adapter exits before initialization with `ReferenceError: Cannot access 'McpClient' before initialization` at `playwright-mcp-scorecard.mjs:16`. The zero-byte raw file and full stderr are retained. |
| Codex browser | unsupported | no report | No callable, versioned black-box harness contract is available. |

Glass's complete raw report records a 5,382,320-byte release binary, 8,527,872
bytes peak runner RSS, 12,260 compact-context bytes, and 1,017,036,800 bytes
peak Chrome process-tree RSS. These values are retained observations, but the
aggregate does not treat them as passing competitive gates because the Glass
adapter row failed validation.

## Missing prerequisite evidence

Revision-bound ratified-gate, full release-validation, and real-browser
platform-matrix evidence was not available. Existing benchmark reports do not
meet the versioned evidence contract's revision, producer, raw-reference, and
complete-check/platform requirements. These prerequisites remain explicitly
`missing`; no envelope or metric was fabricated from older reports.

The raw JSON, stderr, build log, npm install log, environment manifest, and
aggregate decision are retained beside this note. External dependencies were
installed only in the runner's temporary directory and removed after the run.
