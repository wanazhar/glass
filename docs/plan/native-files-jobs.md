# Native files, diagnostics, and owned jobs

## Contract

Keep bash + state as the default schemas. Native `glass-core file` helpers run through bash, using `$GC_BIN` supplied by the host; help is loaded on demand. File helpers are available outside Git and without provider configuration.

Reads return bounded, numbered text and a SHA-256 revision. Edits support exact replacements and original-file line ranges, validated as a batch; writes require an expected revision or an explicit create-only request. Mutations reject symlink traversal, special files, hard-linked targets, stale revisions and overlaps, preserve ordinary rwx permission bits (drop special bits), and replace atomically using a same-directory temporary file. No implicit directory creation. Cooperative writers lock the parent directory. Uncooperative external writers cannot be fully serialized; revision is checked again before commit.

Native previews summarize changed lines and structured diagnostics. JSON validation is in process; optional syntax checking is explicit, bounded, and reports unavailable/unsupported honestly. Preview bytes, file bytes and operation counts have hard bounds.

Bash gains owned job controls rather than another tool: background start, poll, stdin, and graceful stop. UI /bg and /jobs share the registry with model calls. Job output uses bounded memory and a disk cap, output cursors support incremental reads, and application exit stops owned groups; switching sessions retains jobs under their original owner. Background processes do not implicitly bypass shell permissions or read-before-write checks. No detached jobs.

## Presentation

    ✓ bash · 0.1s
      $ "$GC_BIN" file edit ...
    └ file · src/main.rs · +2 -1
      - old line
      + new line
      … /details

Diagnostics use severity, path, line, column and message. All provider/tool text is terminal-sanitized; source logs remain exact.

## Verification

Root implements; one or two GPT-6 Luna medium agents review. Unit checks cover stale/overlapping edits, EOF/CRLF/BOM, filesystem safety, bounded payloads, job I/O and cleanup. Offline real PTYs cover helpers, previews/diagnostics, job controls, headless/non-Git, and 20x8. Compare startup, typing, redraw, RSS, long-history and file throughput; record prompt/schema growth separately. Do not claim comparative model task gains without matched real-model trials.
