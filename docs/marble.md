# Glass Marble

[Documentation index](INDEX.md) · [Glass Core resident harness](glass-core-harness.md) ·
[Host runtime protocol](../products/glass-core/docs/reference/runtime.md)

Glass Marble is a self-hosted durable collaborator built on Glass Core. It is a
separate product from the Glass workspace and from the `gc` terminal: Marble owns
the conversation surface, the approvals, and the durable product document, while
Core owns the model loop, its native coding tools, and the journal.

The product ships in the workspace as the `glass-marble` crate.

## Ownership boundary

Marble uses protocol v2 mixed tool ownership:

- Core executes its own coding tools inside the configured workspace and under
  Core's policy.
- Only `glass.*` integrations registered by Marble come back to Marble over
  `tool/execute`. Those are the tools a human approves.
- A runtime that reports no `mixedTools` capability is rejected at startup
  instead of silently degrading to a host-only session.

The governed host catalog is deliberately small:

| Tool | Effect |
| --- | --- |
| `glass.workspace.list` | List one workspace directory |
| `glass.workspace.read` | Read a UTF-8 workspace file, bounded and truncated |
| `glass.workspace.write` | Write a workspace file, creating parents |

Every path is resolved against one canonical workspace root. Absolute paths,
parent traversal, and symlinks that escape the root are refused before any
filesystem call.

## Build and run

```text
cargo build -p glass-marble --locked
cargo test -p glass-marble --locked
```

One-shot message, one durable turn:

```text
glass-marble "summarise the workspace" --cwd .
```

The one-shot path denies governed tools unless `--approve` is passed, because no
surface is present to answer an approval.

Serve the product:

```text
glass-marble serve --cwd /path/to/project --port 4783
```

Startup prints the listen address, the workspace, the data directory, and the
machine's Tailscale IPv4 address when `tailscale` is installed.

## Self-hosting over Tailscale

Marble binds loopback by default. To share it on your own tailnet:

```text
printf '%s' "$(openssl rand -hex 32)" > ~/.marble-token
chmod 600 ~/.marble-token
glass-marble serve --host "$(tailscale ip -4)" --token-file ~/.marble-token
```

Rules the server enforces:

- Serving off loopback without a token is refused at startup.
- Every `/api/*` request needs the token, as a session cookie set by
  `POST /api/login`, an `Authorization: Bearer` header, an `X-Marble-Token`
  header, or a `token` query parameter for scripted access.
- Mutating requests with an `Origin` header must match the `Host` that served
  the page.
- Request heads are bounded and bodies over 256 KiB are rejected before reading.
- Token comparisons run in constant time.

The data directory holds the runtime sessions and the product document
(`product-state.json`). Override it with `--data-dir` or
`GLASS_MARBLE_DATA_DIR`; the default is `$XDG_DATA_HOME/glass-marble`, then
`~/.local/share/glass-marble`.

If you need TLS, put Tailscale in front of the loopback listener instead of
terminating TLS in Marble:

```text
glass-marble serve --port 4783
tailscale serve --bg 4783
```

## HTTP surface

| Route | Method | Behavior |
| --- | --- | --- |
| `/` | GET | Product UI |
| `/healthz` | GET | Liveness, state revision, event sequence, token requirement |
| `/api/login` | POST | Exchange the access token for an HTTP-only session cookie |
| `/api/state` | GET | Revisioned product document plus live pending approvals |
| `/api/events?since=N` | GET | Server-sent events with bounded replay from a cursor |
| `/api/turns` | POST | Admit a durable turn; answers `202` with the task id |
| `/api/tasks/{id}/cancel` | POST | Request cancellation of a running task |
| `/api/approvals/{id}/allow` and `/deny` | POST | Answer one pending governed tool |

A turn is answered as soon as the durable task id exists, so a long model turn
never blocks the HTTP surface. Text, reasoning, tool activity, usage, and
settlement stream over `/api/events`; the transcript itself stays authoritative
in the product document.

Approvals fail closed. When nobody answers within the approval window the tool
is denied and the model receives an error result.

## Web UI

`glass-marble serve` serves one page that adapts from a phone to a wide desktop:
a collapsible conversation rail, a transcript that renders model text as text
rather than HTML, live tool activity, approval cards, task cancellation, and an
unlock dialog for token-protected hosts. Safe-area insets keep the composer
above the home indicator, and the layout honours reduced-motion preferences.

## iOS client

The iOS app lives in `apps/glass-marble-ios` and is built with Xcode, not Cargo.

- SwiftUI surfaces use Liquid Glass (`.glassEffect`) on iOS 26 and later, and
  fall back to `ultraThinMaterial` on earlier systems.
- `Main.storyboard` hosts the first-launch introduction scene through a
  `UIViewControllerRepresentable`; `LaunchScreen.storyboard` is the launch
  screen. Both are compiled from the synchronized `GlassMarble` source folder.
- The host address is stored in `UserDefaults`; the access token is stored in the
  keychain and sent as a bearer token.
- The client reads `/api/state` and consumes `/api/events`, so approvals and
  cancellations work from the phone exactly as they do in the browser.
- `NSAllowsArbitraryLoads` is enabled because a self-hosted tailnet address is a
  user-supplied plain-HTTP origin. Prefer `tailscale serve` if you want TLS.

Build and run:

```text
open apps/glass-marble-ios/GlassMarble.xcodeproj
```

Select a development team, choose a simulator or device, and run. The first
launch shows the storyboard introduction; open host settings and enter the
address printed by `glass-marble serve`.

## Verification

Local checks for this product:

```text
cargo check -p glass-marble --all-targets --locked
cargo test -p glass-marble --locked
cargo clippy -p glass-marble --all-targets -- -D warnings
```

The adapter tests use `tests/fixtures/fake_glass_core_runtime.py`, so they need
`python3` and no provider account. They cover mixed-session negotiation,
tool execution and denial, per-message durable task identity, live approvals,
token authentication, cross-origin rejection, body limits, event replay, and the
HTTP turn route end to end.

Not covered by those checks: a live provider turn against a real `glass-core`
binary, and an iOS build. The iOS target compiles in Xcode only; the Swift
sources and storyboards here were reviewed but not built in this environment.
