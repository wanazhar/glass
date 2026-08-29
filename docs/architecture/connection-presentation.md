# Connection-aware browser presentation

Status: Current 0.3.13 source behavior

The source retains profile names from the 0.3.3 design as API labels; that
historical naming does not make the design-era behavior part of the published
0.3.13 release record.

The [Glass Dev TUI guide](development-tui.md) covers surface interaction and
the [Development Runtime guide](../development-runtime.md) covers resident
jobs and cleanup; this document defines the reusable policy and visual
contracts.

## Purpose and boundary

Presentation policy must be honest about evidence. Terminal geometry selects a
layout; shell/transport evidence and measured link quality select a transport
profile; an explicit capability or active probe selects graphics; activity and
writer feedback select pacing. Width and `TERM` alone do not prove a transport
or image protocol. This module does not own browser authority, semantic
observations, or Remote View.

The generic policy types in `glass-browser::connection` are source contracts.
The current TUIs additionally use `tui-live`/`tui-live-backend` to choose a
concrete visual path. Those are related but not one automatic negotiation
pipeline: current TUI live selection is implemented in `tui/live_view.rs` and
uses Herdr availability plus explicit flags.

```text
terminal geometry ───────────────> LayoutClass ────────────────┐
SSH/Mosh/multiplexer signals ────> shell + transport evidence  │
RTT/throughput/write measurements ────────────────────────────├> ConnectionEnvironment
explicit transport/graphics overrides ────────────────────────┘       │
                                                                       v
                                                          PresentationPolicy
                                                                       │
                                      activity ───────────> pacing/scale/reasons
                                      tui-live flags ─────> concrete TUI path
                                                                       │
                                      Herdr | Kitty | ANSI | semantic-only
```

## Source dimensions

| Dimension | Source variants | Rule |
|---|---|---|
| `LayoutClass` | `Phone`, `Compact`, `Wide` | Derived from columns (`≤72`, `≤109`, otherwise wide), or explicitly overridden; rows are validated but do not classify transport. |
| `TransportClass` | `Local`, `RemoteFast`, `RemoteConstrained`, `Mosh`, `UnknownRemote` | Local/SSH/Mosh metadata plus optional measurements or override. Unknown stays unknown. |
| `GraphicsClass` | `Kitty`, `Sixel`, `ITermInline`, `Herdr`, `Ansi`, `SemanticOnly`, `Unknown` | Generic vocabulary; native claims require probe/explicit evidence. Not every enum has a renderer. |
| `ShellKind` | `Local`, `Ssh`, `Mosh`, `UnknownRemote` | Derived from process metadata. |
| `MultiplexerKind` | `None`, `Tmux`, `Screen`, `Herdr`, `Nested`, `Unknown` | Retains nested evidence when multiple signals are present. |
| `ActivityClass` | `Interactive`, `Settled`, `Idle`, `Background` | Independent of layout and transport. |
| `ConnectionMeasurements` | optional RTT, throughput, terminal-write latency, cell pixel size | Invalid, negative, non-finite, or over-bounded measurements fail; absent values remain unknown. |

Diagnostics evidence is bounded (at most 16 entries and 160 characters per
entry) and contains no host, username, command, URL, cookie, frame, or source
content. `ConnectionEnvironment::remote()` means any non-Local transport,
including unknown remote.

## Policy matrix

These are the current `PresentationPolicy` source labels and values. They are
not a claim that every TUI path automatically consumes the matrix.

| Effective profile | Interactive target | Default scale | Semantic primary | Pixel behavior |
|---|---:|---:|---:|---|
| `LocalSmooth` | 60 fps (floor 30) | 1.0 | no | continuous pixels |
| `LocalBalanced` | 30 fps | 1.0 | no | continuous pixels |
| `LocalDegraded` (`quality=data`) | 20 fps | 0.5 | no | continuous pixels |
| `RemoteInteractive` | 20–30 fps | 0.85 | no | continuous pixels |
| `RemoteConstrained` | 3/6/12 fps by quality | 0.5 (0.65 smooth) | yes | assistive/limited pixels |
| `MobileRemote` | 0 fps | 0.5 | yes | semantic default for remote phone + auto pixels |
| `MoshSemantic` | 0 fps | 0.5 | yes | semantic only |
| `SemanticOnly` | 0 fps | 0.5 | yes | no continuous pixels |

`PixelIntent::Off` always returns `SemanticOnly`. Mosh always returns
`MoshSemantic`. Unknown/SemanticOnly graphics return semantic-only when pixel
intent is `Auto`; explicit `On` can request a transport profile, subject to the
concrete renderer being available. A remote phone in `Auto` uses
`MobileRemote` when graphics are otherwise proven. Settled activity limits the
requested rate to 5 fps and disables continuous mode; idle limits it to 3 fps;
background sets requested fps to zero. The 3/6/12 tiers describe constrained
profiles, not a healthy local link.

## Current TUI renderer selection

The focused browser TUI defaults to semantic-only (`--tui-live off`), while an
explicit screenshot remains available. `tui-live` selection currently follows:

| Flags/environment | Path | Boundary and failure |
|---|---|---|
| `off` | SemanticOnly | No continuous pixel capture. |
| `auto`, backend `auto`, valid `HERDR_ENV=1` plus bounded socket/pane variables | Herdr | Experimental Unix local-socket stream; a failed stream disables live view. |
| `auto`, backend `auto`, no Herdr | SemanticOnly | Current auto mode does not actively probe or select Kitty. |
| `on`, backend `auto` | ANSI | True-color Unicode half-block pane; bounded capture. |
| any live mode, backend `kitty` | Kitty | Explicit Kitty protocol output; renderer initialization may fail. |
| any live mode, backend `herdr` | Herdr | Requires Herdr environment; unavailable environment becomes semantic-only. |
| any live mode, backend `ansi` | ANSI | Explicit bounded ANSI sampling. |

Current `GraphicsMode` has only explicit `Kitty` and `Semantic` variants.
`TerminalGraphics::negotiate` treats environment variables as hints and does
not enable Kitty from `TERM` or `TERM_PROGRAM` alone. `GraphicsClass::Sixel`
and `ITermInline` are retained vocabulary but have no current TUI renderer;
there is no Sixel or iTerm output path. ANSI is a separate half-block renderer,
not a native image protocol. Herdr is a separate experimental pane stream,
not `GraphicsMode::Kitty`.

Use the installed syntax and defaults in [CLI reference](../cli.md). The
embedded development App can toggle its current bounded browser visual view;
its semantic inspector and workflow projection remain usable when visual output
is disabled. Standalone Remote View is unsupported; embedded Remote View is a
same-session Glass Dev service described in [browser connection](browser-connection.md).

## Capture, mailbox, and adaptation

The browser presentation contract retains metadata in a
`LatestFrameMailbox` with at most one current frame and one newest pending
replacement. Older browser or geometry revisions are rejected; replacing a
pending frame drops it eagerly. Terminal payload adapters apply the same bound
to encoded bytes. A frame is tied to:

- browser revision (page/semantic freshness),
- geometry revision (pane/viewport mapping),
- frame generation (monotonic presentation order), and
- target resource identity.

Visual input must provide matching browser and geometry revisions and must land
inside the displayed image rectangle. Letterboxed/outside-pane coordinates are
rejected rather than mapped into browser content. Pixels never authorize a
browser action; semantic references and governed mutation authority remain the
control plane.

The generic adaptation order is:

```text
replace obsolete pending frame
  → apply producer backpressure
  → suppress unchanged capture
  → reduce capture scale (0.5 minimum)
  → reduce rate toward the profile floor
  → expose an explicit degraded reason
```

Settled/idle presentation is paint-driven or rate-limited; background pauses.
Scheduling uses bounded current/pending storage and skips missed ticks. It does
not retain historical frames for later display.

ANSI quality profiles use bounded pane budgets: Data ≈40 columns/20 rows,
Balanced ≈80/40, Smooth ≈120/60 (clamped to available area), with capture
intervals of 333/160/80 ms. ANSI fit is `contain` (letterbox), `cover` (crop),
or `actual` (one source pixel per sample, crop overflow). Native Kitty output
uses contain geometry.

Herdr reads `HERDR_SOCKET_PATH` and `HERDR_PANE_ID`, bounds each value, connects
to a Unix socket, requests `pane.graphics.stream`, and sends raw PNG frames with
placement metadata. Its queue capacity is one; failed connection or writes
produce a bounded failure event and stop the worker. Non-Unix hosts cannot use
this stream.

## Metrics and privacy

`PresentationObservatory` and terminal diagnostics distinguish requested,
acquired, and presented rates; capture scale; frame age; capture/encode/decode/
write/input latency where measurable; encoded bytes; producer/mailbox/writer
drops; stale rejections; browser/geometry/frame revisions; active policy and
degradation reasons. Counters saturate and rolling windows are bounded; metric
collection does not block input/render loops. Unknown RTT/throughput remain
unknown instead of being imputed from width.

Frame metadata contracts do not serialize payload bytes, and default storage
policy forbids persistence. Concrete TUI code necessarily holds a transient
screenshot while rendering; it does not promise to erase terminal scrollback.
Incognito/private screenshots therefore require the same local handling as any
sensitive browser view.

## Contract tests

Source tests cover independent width/transport/graphics decisions, local/SSH/
Mosh/tmux/Herdr combinations, unknown measurements, policy activity transitions,
latest-frame replacement, stale browser/geometry rejection, pane coordinate
mapping, Kitty payload limits, ANSI fit/quality bounds, Herdr handshake and
one-frame backpressure, and diagnostics unknowns. They do not certify Sixel,
iTerm, touch-specific input, unrestricted remote graphics, or standalone
Remote View.
