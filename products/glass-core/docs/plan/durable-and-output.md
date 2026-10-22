# Durable execution and readable output

Status: native contract implemented and verified. This supersedes the deliberate
durability omissions in the historical Pi survey; existing CLI behavior remains
compatible. See [verification](reviews/native-durable-output-2026-10-15.md) and
[current API boundaries](../reference/durable.md).

## Delivery contract

Native GC parity covers atomic committed state (entries, versioned documents,
and tasks), checkpointed recovery, replay policy, stable provider identity,
idempotent input admission, ownership/cancellation including background tasks,
child-task waits, task inspection, resumable observation, and integration with
GC's existing provider/tool engine. Platform-specific Pi adapters are not copied.
A compatibility matrix must identify implemented behavior and remaining gaps;
no package-parity claim is made merely because APIs have similar names.

Syntax highlighting runs locally with bounded lexical state and no grammar
server or rendering subprocess. Language aliases select keyword, comment,
string, number, and callable-name colors. Unknown languages remain readable
plain text. Source, code copying, and durable records remain exact.

LaTeX uses native terminal math for inline and display delimiters and math fences.
Unicode operators, Greek symbols, fractions, roots, scripts, and common aligned
expressions are supported within explicit parse bounds. Unsupported syntax keeps
its source; no TeX macros, shell escapes, files, packages, or network operations
are executed. Streaming and replay share rendering.

## Exit modal

Ctrl+D opens a compact centered warning dialog modeled on Glass Dev, retaining
the current chat beneath it. Enter confirms; Esc or Ctrl+C cancels. Resize keeps
it centered. A saved conversation is retained; an unsent draft is not. The modal
must fit 20x8 and preserve the composer when dismissed.

```text
      ╭─ EXIT · confirm ──────────────╮
      │ Quit glass-core?             │
      │ Saved conversation stays.    │
      │ Unsent draft is discarded.   │
      │ Enter quit · Esc stay        │
      ╰──────────────────────────────╯
```

## Evidence gates

Fault-injection tests cover torn/unconfirmed commits, poisoned writes, restart
at execution boundaries, duplicate requests, ownership, waits, and observation.
Release PTYs cover language colors, exact copying, math, resize, scrolling, and
exit/cancel at desktop and phone sizes. Build, review, and performance checks
run once source changes are settled.

## Native contract mapping

| Pi behavior | GC implementation |
| --- | --- |
| Atomic entries, documents and tasks | Checked, sequenced journal commits |
| Per-phase checkpoints and replay policy | Task transitions, claim, recovery and interrupted outcomes |
| Deduplicated inbox and submission reacquisition | Persisted request receipts; input promoted only on execution |
| Stable provider identity | Persisted identity and provider affinity; fresh identity on fork |
| Foreground/background ownership | Task owners, completing state, abort acknowledgement and child waits |
| Versioned definitions and reload | Process-local registry, dispatch checks and explicit migrations |
| Committed live output and resumable observation | Batched live documents and bounded commit cursor reads |
| Historical state and fork policy | Historical document reads, fork boundaries and initial/current/as-of policies |
| Reset and settings changes | Idle reset and validated conversation configuration |
| Coding tools, extensions and compaction | Existing GC engine and host protocol |
| Platform/package adapters | GC Rust/JSON-RPC interface; Pi package APIs are not copied |

The host controls scheduling, including custom task execution. A single runtime
still advertises serial model turns. Bounds and future storage optimizations are
explicit in the API reference; this is not a drop-in Pi TypeScript package.
