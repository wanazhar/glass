# Runtime storage and request overhead

[Documentation index](../INDEX.md)

## What GC writes

Configuration uses the `glass-core` XDG config directory. Runtime data uses the
`glass-core` XDG data directory, normally `~/.local/share/glass-core`. GC does not
create a Cargo build tree or install dependencies when you start a conversation.
Those files come from explicit development, installation, or model commands.

| Data | When written | Growth and retention |
| --- | --- | --- |
| Conversation logs and request reports | During conversations and model requests | Append as work continues; retained for resume and inspection |
| Context manifests and latest-request metadata | When context or requests change | Latest snapshots are replaced; reports remain in the request log |
| Partial response sidecar | During a turn, by default at 250 ms intervals and phase boundaries | Text tail capped at 64 KiB before JSON escaping; removed after a normal turn end |
| Foreground shell overflow | When a stream exceeds 16 KiB of inline output | At most 4 MiB per stream; moved into content-addressed blobs without a duplicate file |
| Background job output | When a job is started | At most 4 MiB per job; retained logs are not reattached on restart |
| Prepared model images | When an image is explicitly attached or viewed | At most 1 MiB encoded per prepared image; content-addressed artifacts remain on disk |
| Prompt history | After interactive prompt submission | Last 200 entries; this is an entry bound, not a total byte quota |
| Neuralnets and reviewed facts | When learning or facts are saved | Combined native store capped at 1,088 KiB; automatic retrieval has smaller bounds |
| Provider pacing records | At request starts and cooldown changes | Small per-endpoint/account timing records; no credential values |
| Checkpoints, worker sessions, and workflow artifacts | When the corresponding workflow is used | Retained according to that workflow; can include project snapshots |

No full model request-body or packet trace is written by default. Request
diagnostics retain metadata and fingerprints. Conversation logs still contain
prompts, model output, tool arguments, and bounded tool results; treat them as
private data. Optional extensions may have separate stores and retention rules.

Overlay output uses the existing 1 MiB in-memory display cache. Opening a modal
or reasoning reader does not create a UI spool file. The durable conversation
log remains the source for full transcript inspection.

The partial sidecar uses atomic replacement and syncing for crash recovery.
`live_flush_ms = 0` disables it. This reduces writes but gives up in-flight
partial recovery. `log_fsync = "always"` increases session-log durability and
write cost; the default is `"off"`. Neither setting removes saved conversations.

## Shell capture limits

Each foreground stdout/stderr stream retains 16 KiB inline and up to 4 MiB of
overflow. Output beyond that bound is drained so the child cannot block on a
full pipe. The result discloses discarded bytes and includes the final 2 KiB.
A saved overflow is not a promise of a complete log after the bound is reached.
Storage failures are also disclosed rather than silently treated as complete.

The same overflow bytes are stored once, addressed by their content hash. A model
can inspect the returned path with a bounded native read or shell command. For a
complete larger log, explicitly redirect the command to a task-owned file. That
file is outside the capture quota and may consume additional space.

## Inspecting and cleaning up

GC has no global disk quota or automatic expiration for saved conversations,
images, checkpoints, or referenced blobs. Per-file limits prevent one capture
from growing indefinitely; many sessions can still accumulate substantial data.
GC does not automatically delete conversation history to meet a space target.

On a Unix host, inspect your actual XDG data directory with `du`. Close affected
sessions before manually removing their data. Keep credentials and project files
separate from disposable build output.

`gc blobs sweep --json` explicitly removes old blobs that no foreground or managed
worker log references. It retains blobs modified in the last five minutes to
protect in-flight recording, streams log scans in bounded buffers, and refuses
to delete when a required reference scan fails. It is not a general session,
image, checkpoint, or old output-file cleanup command. No sweep runs at startup.

## Model request overhead

The default system instructions, workflow protocol, and permission context are
compact. Detailed workflow recipes load through `state` on demand. Resuming a
conversation replaces older generated workflow and permission paragraphs rather
than accumulating versions. Custom instructions and project facts are preserved.

Shell inline bounds reduce repeated tool-result text in later requests. They do
not reduce the model's own reasoning or answer allowance. Source instructions,
tool schemas, history, images, and configured extensions still consume context.
Byte counts are reproducible; actual tokens and cache behavior depend on the
provider. Use `/context`, `/requests`, and `/usage` to inspect reported costs.

## Measuring changes

```sh
export GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core"
GC_PERF_REPORT=/tmp/gc-performance.json python3 scripts/check-performance.py
python3 scripts/check-native-performance.py
python3 scripts/check-runtime-footprint.py --bounded
```

The footprint fixture uses a local provider and disposable directories. It
reports binary bytes, system/schema bytes, request sizes, tool-result bytes, and
automatic storage for a greeting and a verbose command. These are controlled
measurements, not a prediction of live-model token use or provider latency.
