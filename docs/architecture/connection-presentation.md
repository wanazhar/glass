# Connection-aware browser presentation

Status: Accepted for 0.3.3

## Purpose and boundary

This contract selects an honest presentation strategy without changing browser
authority. Terminal size chooses layout. Connection/transport evidence chooses
delivery budgets. Graphics probing chooses a renderer. Activity chooses pacing.
Unknown measurements remain unknown.

## Overall flow

```text
terminal cells/pixels ───────> LayoutClass ───────────────┐
explicit config/environment ─> TransportClass             │
active graphics probe ───────> GraphicsClass              ├─> PresentationPolicy
shell/multiplexer evidence ──> ShellEnvironment           │      │
measured writer feedback ────> ConnectionMeasurements ────┘      ├─ capture scheduler
browser/activity state ──────> ActivityClass                     ├─ terminal renderer
                                                                  └─ diagnostics
```

No arrow exists from terminal width to transport class or graphics class.

## Types

| Type | Variants / fields | Invariant |
|---|---|---|
| `LayoutClass` | `Phone`, `Compact`, `Wide` | geometry/override only |
| `TransportClass` | `Local`, `RemoteFast`, `RemoteConstrained`, `Mosh`, `UnknownRemote` | never inferred from width |
| `GraphicsClass` | `Kitty`, `Sixel`, `ITermInline`, `Ansi`, `SemanticOnly`, `Unknown` | native protocols require a probe or explicit override |
| `ShellKind` | `Local`, `Ssh`, `Mosh`, `UnknownRemote` | metadata evidence only |
| `MultiplexerKind` | `None`, `Tmux`, `Screen`, `Herdr`, `Nested`, `Unknown` | nested evidence is retained |
| `ActivityClass` | `Interactive`, `Settled`, `Idle`, `Background` | selected independently of environment |
| `ConnectionMeasurements` | optional RTT/throughput/write latency/cell pixels | absent values serialize as absent/unknown |

`ConnectionEnvironment` includes the terminal row/column geometry, the five
typed dimensions, measurements, evidence strings and explicit overrides. It is
bounded and safe for diagnostic output; it contains no remote host, username,
command, URL, cookie, frame or source content.

## Policy matrix

| Effective profile | Active target | Scale ladder | Pixel default | Intended use |
|---|---:|---|---|---|
| `LocalSmooth` | 60 FPS | 1.00, .75, .65, .50 | continuous when changing | capable local graphics |
| `LocalBalanced` | 30 FPS | 1.00, .75, .50 | continuous when changing | local fallback |
| `RemoteInteractive` | 30 FPS, floor 20 | .85, .65, .50 | paint/latest-state | proven remote path |
| `RemoteConstrained` | 12/6/3 FPS | .65, .50 | assistive | constrained remote path |
| `MobileRemote` | snapshot/burst | .50 | off by default | phone control plane |
| `MoshSemantic` | 0 terminal FPS | n/a | semantic only | state-synchronized shell |
| `SemanticOnly` | 0 terminal FPS | n/a | off | unavailable/disabled graphics |

The 3/6/12 tiers never represent a healthy local profile. A wide SSH terminal
can use `RemoteInteractive`; a narrow local terminal can use phone layout with
`LocalSmooth` policy when explicitly displaying pixels.

## Adaptation order

```text
replace obsolete pending frame
  -> producer backpressure
  -> suppress unchanged capture
  -> lower capture scale
  -> lower active rate toward the profile floor
  -> explicit degraded state and reason
```

Background presentation pauses. Idle/settled presentation is paint-driven or
1–5 FPS. Scheduling uses missed-tick skip behavior and bounded current/pending
storage; historical frames are never queued for eventual display.

## Metrics

The observatory distinguishes:

- requested, acquisition and presentation FPS;
- capture scale and encoded bytes per second;
- frame age and input-to-present latency;
- capture, encode/decode and terminal-write latency where measurable;
- producer/mailbox/writer drops and stale rejections;
- current browser/geometry/frame revisions;
- policy profile and every active degradation reason;
- Remote View lifecycle;
- unknown RTT/throughput from measured values.

Metrics use bounded rolling windows and saturating counters. Measurement must
not block the render/input loop.

## Interaction and configuration

Explicit CLI/TUI overrides are authoritative and diagnostic-visible. Auto mode
uses conservative evidence. A failed/absent graphics probe chooses ANSI only
when the user allowed it, otherwise semantic-only. Mosh never auto-enables
continuous terminal pixels.

## Tests

- Matrix fixtures cover local/SSH/tmux/Mosh/phone/unknown combinations.
- Tests prove width does not change transport and `TERM` alone does not prove
  a native image protocol.
- Scheduler tests prove latest-frame replacement, idle/background behavior and
  the quality-before-frame-rate adaptation order.
- Metrics tests distinguish requested/acquired/presented values and unknowns.
