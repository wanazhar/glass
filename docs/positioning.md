# Positioning: Where Glass Fits

Glass is a **safe, tiny local agent control plane** for Chrome. It is not a
planner, not a QA framework, and not a cloud browser. This page explains the
categories and when to use each tool.

## The Landscape

Browser automation for agents has split into four categories:

| Category | Examples | Glass? |
|----------|----------|--------|
| **Local control plane** | Glass, Playwright MCP, agent-browser | ✅ This is Glass |
| **QA / test framework** | Playwright, Puppeteer, Selenium | ❌ |
| **Autonomous agent** | Browser Use, Stagehand, Codex CLI | ❌ |
| **Cloud browser** | Browserbase, Browserless, Steel. | ❌ |

## Control Plane vs Planner vs Cloud Browser

### Control Plane (Glass)

A control plane turns deterministic commands into browser actions. The agent
(your LLM) decides *what* to click; Glass decides *whether* it's safe, resolves
the target, and executes. Glass never plans — it only controls.

**When to use Glass:**
- You have an LLM or agent that already decides what to do
- You need deterministic, fail-closed targeting (no wrong actions)
- You care about runtime memory (Glass runs in ~9 MB)
- You want typed errors with recovery hints that your agent can act on
- You need bounded observations (compact accessibility, not full screenshots/HTML)

### Planner / Autonomous Agent (Browser Use, Stagehand)

These tools embed an LLM planner that decides navigation, targeting, and
recovery autonomously. They are higher-level and often easier to start with,
but sacrifice determinism and resource control.

**When to use a planner instead of Glass:**
- You want a single "do this task" command without writing agent logic
- Your task is exploratory (the correct sequence of actions is unknown)
- You are willing to trade determinism for autonomy
- Your agent cannot emit structured tool calls

### Cloud Browser (Browserbase, Browserless)

Cloud browsers run Chrome on remote infrastructure and stream the viewport.
They solve cross-platform access and scaling, but add latency, cost, and
network dependency.

**When to use a cloud browser instead of Glass:**
- You need to run on a platform where Chrome can't run locally
- You need many concurrent browser sessions
- You want to isolate browsing from the agent's host
- Network latency and per-session cost are acceptable

## Glass vs Other Local Tools

### Glass vs Playwright MCP

| Concern | Glass | Playwright MCP |
|---------|-------|---------------|
| **Runtime** | Rust (~9 MB RSS) | Node.js + Playwright (~196 MB RSS) |
| **Targeting** | Fail-closed: ambiguous = error | Best-effort heuristic matching |
| **Observations** | Compact accessibility tree (bounded) | Screenshot + DOM by default |
| **Policy** | Typed presets (dev, ci, hardened) | No policy engine |
| **Recovery** | Typed errors with recovery hints | Raw text errors |
| **Cross-browser** | Chrome/CDP only | Chrome, Firefox, WebKit |
| **Mature QA surface** | Not a goal | Trace viewer, codegen, assertions |

**Use Glass when** correctness-per-byte and resource efficiency matter most.
**Use Playwright MCP when** you need cross-browser support, trace debugging,
or a mature QA ecosystem.

### Glass vs agent-browser (Vercel)

| Concern | Glass | agent-browser |
|---------|-------|--------------|
| **Model** | Library/CLI/MCP | CLI with daemon |
| **Targeting** | Revisioned refs, locator chains | Accessibility refs + AI selectors |
| **MCP surface** | Typed tools with stable contracts | Broad typed surface (all CLI parity) |
| **Install** | Rust toolchain or brew | npm + managed Chrome |
| **Popup verification** | Causal witness chain | Not published as typed primitive |
| **Determinism** | Zero-wrong-action gate | AI-assisted selection may vary |

agent-browser is a strong, fast native CLI. Glass differentiates on fail-closed
targeting, revisioned references, bounded token budgets, and a published
zero-wrong-action benchmark. If your agent drives both, Glass wins on
determinism; agent-browser wins on install speed and ecosystem breadth.

## Explicit Non-Goals

Glass will **never**:

- Embed an LLM planner or compete with Browser Use / Stagehand on autonomy
- Implement anti-bot evasion, stealth, fingerprint spoofing, or CAPTCHA solving
- Support Firefox, WebKit, or Safari as release-blocking targets
- Provide a trace viewer, codegen, or QA-specific UI
- Run as a cloud service or managed browser farm
- Adopt an occupied CDP port silently

These exclusions are intentional. Glass is the narrowest, most deterministic
slice of the browser-automation stack. It composes with your agent, your
planner, or your cloud provider — it does not replace them.

## When NOT to Use Glass

- You need cross-browser testing → use Playwright
- You want a single "do this" command → use Browser Use or Stagehand
- You need cloud-hosted browsers → use Browserbase or Browserless
- You need to bypass bot detection → Glass explicitly does not support this
- You need visual trace debugging → use Playwright with trace viewer

## Further Reading

- [Bot-protection & consent walls](bot-protection.md) — legitimate access paths
- [Detection-surface report](detection-surface.md) — what CDP exposes
- [Competitive acceptance guide](../benchmarks/README.md#competitive-acceptance)
- [Architecture](architecture/README.md)
