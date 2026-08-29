# glass-browser

`glass-browser` is Glass's standalone browser intelligence runtime and Rust
library. It drives local Chrome or Chromium through a transport-neutral
contract with CDP as the production backend. It does not bundle a browser,
host a remote browser service, or infer an autonomous action plan.

**Status: Current 0.3.14 source behavior.** This is the browser-only package;
the complete development TUI, project runtime, Pi Agent, editor, PTYs, and
Remote View belong to `glass-dev` and are not exported here.


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

For subsequent Cargo registry releases, run `glass-browser update --dry-run`
to inspect the resolved package, source, root, and Cargo arguments, then run
`glass-browser update`. The command updates this `glass-browser` package and
does not switch to `glass-dev`. Use `--version VERSION` to pin a release and
`--force` only for an intentional reinstall.

This package installs only `glass-browser`. Install `glass-dev` instead when
you want both `glass` and `glass-browser`. Installing both packages into the
same Cargo home can make the last installation replace the shared
`glass-browser` executable; use one package as the owner of that command.

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

This package owns browser lifecycle, semantic observation, guarded actions,
workflows, MCP, daemon, and its standalone browser TUI. It does not provide
the `glass` development command, project/agent/harness routes, native code
editor, PTY dev-suite launcher, or development-TUI Remote View.

```console
glass-browser navigate https://example.com
glass-browser observe --level interactive
glass-browser click r7:b42 --expected-revision 7
```

Observation is structured-first. Screenshots, full DOM, PDFs, evaluated
JavaScript, and form values are explicit operations and may require policy
capabilities. Locators must resolve exactly one current target; stale revisions
fail before browser input.

The standalone browser TUI starts structured-only by default. Its first screen
offers `l` to launch a local browser, `a` to attach a verified DevTools port,
`n` to navigate, `t` to type, `j`/`k` to select semantic entities, and `Enter`
to activate. Use `live on`/`live off` in that TUI; these are not `glass-dev`
development-TUI commands. For disposable state:

```console
glass-browser --policy hardened --incognito \
  --policy-allow-host example.com navigate https://example.com
```

### Standalone terminal presentation

The standalone browser TUI is structured-first and responsive: terminals up to
72 columns use phone composition, up to 109 columns use compact composition,
and wider terminals use desktop composition. Continuous browser pixels are off
by default. With `--tui-live auto` and backend `auto`, Herdr is used only when
detected; otherwise the TUI remains semantic-only. `--tui-live on` with
backend `auto` uses the bounded ANSI fallback. Explicit `kitty` emits Kitty
graphics and explicit `ansi` uses the portable cell renderer; an unavailable
explicit Herdr backend remains semantic-only. `--tui-live-quality
data|balanced|smooth` targets approximately 3/6/12 FPS, and
`--tui-live-fit contain|cover|actual` controls ANSI fitting (`contain` is the
default; native image paths use contain). If a requested backend cannot
initialize, the TUI reports the failure and remains semantic-only.

```console
glass-browser --tui-live on --tui-live-backend kitty tui
glass-browser --tui-live on --tui-live-backend ansi --tui-live-quality data tui
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

## Cargo features

| Feature | Default | Purpose |
|---|---:|---|
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
  separately. Windows receives browser-free source checks but has no certified
  native browser runtime. Firefox, WebKit, and Safari are unsupported.
- WebDriver BiDi is experimental and bounded. An unavailable capability fails
  closed rather than falling back to raw transport.
- Native extensions require explicit opt-in and a platform sandbox gate.

## Documentation

- [Glass product overview](https://github.com/wanazhar/glass/blob/main/README.md)
- [Getting started](https://github.com/wanazhar/glass/blob/main/docs/getting-started.md)
- [Complete feature reference](https://github.com/wanazhar/glass/blob/main/docs/features.md)
- [Rust SDK](https://github.com/wanazhar/glass/blob/main/docs/rust-sdk.md)
- [Runnable examples](https://github.com/wanazhar/glass/blob/main/docs/examples.md)
- [CLI reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md)
- [MCP integration](https://github.com/wanazhar/glass/blob/main/docs/mcp.md)
- [Security policy](https://github.com/wanazhar/glass/blob/main/SECURITY.md)
- [Complete uninstall and retained state](https://github.com/wanazhar/glass/blob/main/docs/installation.md#fully-uninstall-glass)
- [API documentation](https://docs.rs/glass-browser)

The docs.rs page documents the Rust library; use the [CLI
reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md) for
installed command behavior.

License: MIT.
