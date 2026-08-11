# Full-suite performance and reliability

Glass tests the resident development suite under simultaneous subsystem load,
not only as isolated feature calls. The deterministic local gate combines task
DAG pressure, multi-workspace daemon activity, browser control-plane reads,
LSP, DAP, test discovery, process inspection, and bounded event delivery.

## Queue and retention bounds

| Resource | Bound | Overflow behavior |
|---|---:|---|
| daemon workspace registry | 8 workspaces | open fails explicitly |
| daemon clients | 16 | connection is rejected |
| workspace actor commands | 64 | sender backpressure is explicit |
| workspace event history | newest 512 | `droppedEvents` and `lostBefore` report loss |
| daemon event batch | 256 | larger/empty request is rejected |
| Pi commands per worker | 32 | caller receives queue-full conflict |
| Pi worker events | 256 shared | per-agent `droppedEventCount` increments |
| Pi retained history | newest 512 | older history is not presented as retained |

The TUI displays each agent's retained and dropped event counts. Daemon clients
resume an exclusive sequence cursor and can distinguish “no new events” from
“history was lost.” No queue silently grows with session duration.

## Deterministic stress scenarios

The library suite proves:

1. Eight independent ready tasks are scheduled, followed by an integration
   task depending on all eight. The dependent wakes only after its prerequisites
   settle and verify.
2. Workspace A runs a one-second persistent-kernel command. During that call,
   workspace B concurrently services its workspace inspection plus browser
   state, LSP list, a real framed fixture-DAP event poll, test discovery, and
   process-list requests. The complete B batch must finish within 300 ms, so A
   cannot retain the daemon registry or serialize unrelated workspaces.
3. A fresh daemon client handle resumes the same workspace actor after its
   saved event cursor.
4. Daemon event-ring and Pi worker-channel overflow are bounded and reported.

The fixture DAP test exercises actual Content-Length framing and an owned
adapter process, but adapter-family certification remains in the separate real
debugpy/LLDB/Delve matrix.

## Live browser and presentation gate

The deterministic stress test uses `glass.browser.state`, which proves the
resident browser control plane remains responsive without requiring Chrome.
It is not live-browser evidence. The release candidate must additionally run
the opt-in Chromium smoke and presentation suites for local active 30 FPS,
supported 60 FPS, remote adaptive/mobile semantic-first behavior,
newest-frame-wins, and background throttling. A missing Chromium environment
is reported as unavailable, never converted into a passing live-browser claim.

Run the deterministic portion with:

```console
cargo test -p glass-dev --lib --all-features
cargo test -p glass-dev --test development_runtime --all-features
```

Run live browser evidence with the pinned managed/browser environment described
in [browser reliability](reliability.md).
