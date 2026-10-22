# GC Connect

Status: implemented and release verified.

## Contract

`gc connect` starts an optional native web gateway, reuses Tailscale Serve for
private HTTPS, and prints a stable installable PWA URL. It never resets existing
Serve routes or enables Funnel. The gateway binds loopback only and accepts the
host user's Tailscale identity. A local development mode uses a private access
link. Browser origins and hosts are validated on every API request.

GC's normal session logs remain canonical. Each active thread has one served
writer, shared by browser and terminal clients. Closed ordinary TUI threads can
be resumed in the web interface. An independently active ordinary TUI retains
its writer lock; the web interface explains how to hand off instead of stealing
it. `gc resume ID` attaches when a served owner exists. Browser disconnect does
not cancel work. Input ownership is explicit; approvals and questions pause the
turn and cannot be answered by an observer.

## Interface

```text
┌ threads ─────────┬ workspace / thread ────────────┬ details ───────┐
│ Glass  +         │ connected · model · activity   │ activity      │
│ workspace       │                                │ reasoning     │
│   thread        │ user request                   │ context       │
│   thread        │ answer                         │               │
│                 │ ▸ read / edit / command group  │               │
│                 │ answer with rich content       │               │
│                 │ approval / question when asked │               │
│                 │ composer · send / stop         │               │
└─────────────────┴────────────────────────────────┴───────────────┘
```

Phone screens show one reading surface with thread and details drawers. Use
restrained warm neutrals, a green accent, compact activity rows, and readable
prose. Keep code and tool output distinct. T3 Code's WorkLog and incremental
Markdown approach inform the design; no upstream application code is copied.

## Performance and lifecycle

Static assets are installed separately from the core binary; there is no Node runtime. Building the optional asset bundle requires Node.
History loads only after thread selection and is paginated. Live events have
bounded replay buffers and use a persistent SSE connection. Stream updates are
batched per frame. Cached PWA assets contain no transcripts or credentials.
Opt-in gateway and served workers are idle when unused; normal CLI startup does
not launch services. `connect stop` removes only GC's route and gateway; session
workers are preserved so stopping remote access never cancels a task.

## Verification

Check host/origin/identity rejection, bounded HTTP bodies and clients, route
collision/preservation, reconnect without command replay, writer locks, stream
and replay parity, question/custom answer, explicit approvals, mobile layout,
PWA assets and service worker caching, release binary footprint and latency.
