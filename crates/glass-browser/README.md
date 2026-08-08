# glass-browser

`glass-browser` is Glass's standalone browser intelligence runtime and Rust
library. It drives local Chrome or Chromium through a transport-neutral
contract with CDP as the production backend. It does not bundle a browser,
host a remote browser service, or infer an autonomous action plan.

The package provides:

- the `glass-browser` command for browser-only CLI, TUI, MCP, daemon, semantic,
  workflow, policy, and reliability operations; and
- the `glass_browser` Rust crate for embedding the same runtime.

The full terminal development environment and `glass` command are distributed
separately by [`glass-dev`](https://crates.io/crates/glass-dev).

## Install

```console
cargo install glass-browser --locked
glass-browser doctor
glass-browser --help
```

Chrome, Chromium, or Chrome for Testing is required for browser-backed
operations. `doctor`, Task Protocol validation/compilation, Web IR operations,
policy checks, and several scorecards are browser-free.

For Rust:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Observe, act, verify

```console
glass-browser navigate https://example.com
glass-browser observe --level interactive
glass-browser click r7:b42 --expected-revision 7
```

Observation is structured-first. Screenshots, full DOM, PDFs, evaluated
JavaScript, and form values are explicit operations and may require policy
capabilities. Locators must resolve exactly one current target; stale revisions
fail before browser input.

Use a disposable session for untrusted browsing:

```console
glass-browser --policy hardened --incognito \
  --policy-allow-host example.com \
  navigate https://example.com
```

## Rust quick start

```rust,no_run
use glass_browser::{BrowserSession, SessionOptions};

#[tokio::main]
async fn main() -> glass_browser::BrowserResult<()> {
    let options = SessionOptions::builder().incognito(true).build()?;
    let session = BrowserSession::start(&options).await?;
    let page = session.navigate("https://example.com").await?;
    let observation = session.observe().await?;
    println!("{} revision={}", page.url, observation.accessibility.revision);
    session.close().await
}
```

Call `close` on an owned session so Chrome can flush profile state. Attach mode
does not own or close the external browser.

## Major contracts

| Contract | Rust entry point | Purpose |
|---|---|---|
| Browser session | `BrowserSession`, `SessionOptions` | Chrome lifecycle, targets, frames, interaction, storage, evidence |
| Semantic observation | `browser::session::SemanticObservation` | Bounded page, regions, targets, records, revision, route |
| Evidence extraction | `ExtractionRequest`, `ExtractionEvidence` | Strict source-labelled non-mutating evidence |
| Glass Web IR v1 | `GlassWebIrV1` | Stable reconciled entities, relationships, details, coverage |
| Task Protocol | `GlassTask`, `compile_task` | Browser-free deterministic intent-to-plan compilation |
| Workflows | `WorkflowDefinition`, `WorkflowCheckpoint` | Typed bounded execution, proof, resume, and recovery |
| Knowledge | `KnowledgeStore` | Scoped advisory persistence and freshness assessment |
| Backend interface | `browser_backend` | Capability-evidenced semantic backend dispatch |
| Surfaces | `surfaces` | Multi-surface evidence, coverage, provenance, and bridge grants |
| Presentation | `presentation`, `terminal_graphics` | Bounded latest-frame metadata and terminal adapters |
| MCP/protocol | `mcp`, `protocol` | Negotiated stdio server and canonical request envelopes |
| Reliability | `reliability`, `reliability_runner` | Scenarios, fixtures, replay evidence, and certification gates |
| Development Runtime | `development` | Optional project files, PTYs, events, graph, harness, and replay |

## Cargo features

| Feature | Default | Purpose |
|---|---:|---|
| `development-runtime` | no | Real PTY process manager and `glass.toml` support |
| `visual-compare` | no | Explicit PNG comparison helpers |
| `fuzzing` | no | Test-only fuzz hooks |

## MCP

```console
glass-browser mcp-config --client generic
glass-browser --policy hardened --incognito \
  --policy-allow-host example.com --mcp
```

MCP uses stdio. Keep stdout reserved for protocol frames. Clients must complete
initialization and the initialized notification before tools. The negotiated
agreement reports exact schema and capability status.

## Safety and support

- Keep the CDP port on a trusted local interface.
- Use `--incognito` when persistence is unnecessary.
- Treat profiles, cookies, storage, screenshots, DOM, PDFs, evaluated results,
  and diagnostic logs as sensitive.
- Linux and macOS targets are declared; native certification is tracked
  separately. Windows, Firefox, WebKit, and Safari are unsupported.
- WebDriver BiDi is experimental and bounded. An unavailable capability fails
  closed rather than falling back to raw transport.
- Native extensions require explicit opt-in and a platform sandbox gate.

## Documentation

- [Getting started](https://github.com/wanazhar/glass/blob/main/docs/getting-started.md)
- [Complete feature reference](https://github.com/wanazhar/glass/blob/main/docs/features.md)
- [Rust SDK](https://github.com/wanazhar/glass/blob/main/docs/rust-sdk.md)
- [Runnable examples](https://github.com/wanazhar/glass/blob/main/docs/examples.md)
- [CLI reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md)
- [MCP integration](https://github.com/wanazhar/glass/blob/main/docs/mcp.md)
- [Security policy](https://github.com/wanazhar/glass/blob/main/SECURITY.md)
- [API documentation](https://docs.rs/glass-browser)

License: MIT.
