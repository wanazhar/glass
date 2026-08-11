# Native Pi SDK runtime

Glass Dev embeds Pi as its only coding-agent runtime through Pi's
`AgentSession` SDK. Resident agents do not start the Pi CLI or use its RPC
mode. A small Glass-owned Node process loads the pinned SDK and communicates
with Rust over private, 4-byte length-prefixed JSON frames.

```text
AgentRegistry
  -> GlassPiRuntime (Rust lifecycle and bounded IPC)
  -> @glass-dev/pi-runtime
  -> Pi AgentSession and SessionManager SDKs
  -> glass_tool
  -> glassd workspace actor and DevelopmentToolRouter
```

## Authority and tools

Pi starts with its built-in tools, extensions, context files, project skills,
prompt templates, and themes disabled. Its only executable capability is the
Glass-owned `glass_tool`. Every call returns to the durable workspace through
the same router used by CLI, MCP, and daemon clients, preserving workspace
trust, confinement, actor attribution, generation and project-revision guards,
confirmation policy, browser leases, and resident service ownership.

The process protocol is bounded to 16 MiB per frame and a 256-frame Rust
channel. Each agent owns one child process; dropping the owner terminates and
reaps that child. Runtime source is embedded in the Rust crate and materialized
in the user's Glass cache, so a repository cannot replace it.

## Session contract

The native interface supports create, resume, list, select, clone, fork,
branch tree, entries, messages, model selection, thinking level, compaction,
steering, follow-up, abort, naming, statistics, tool events, assistant events,
settled/error state, and persistent sessions. Session paths are confined to
the explicit `.glass/pi-sessions` directory.

Pi-compatible JSONL sessions in that directory remain directly readable.
Selecting a missing, out-of-directory, empty, or SDK-incompatible session
returns an explicit operation error; Glass never rewrites or silently
"repairs" an incompatible file. Clone and fork require a valid persisted
conversation, so an empty newly created session correctly reports that it
cannot yet be cloned.

For repository development, `packages/pi-runtime/package.json` pins the exact
SDK version and its lockfile. Installed Cargo binaries may resolve the same SDK
from a local package installation or from the installed `pi` package; operators
can set `GLASS_PI_SDK_ENTRY` to an explicit SDK `dist/index.js` file.

## Local verification

```bash
node --check crates/glass-dev/assets/pi-runtime.mjs
npm --prefix packages/pi-runtime run check
cargo test -p glass-dev pi_runtime::tests -- --nocapture
cargo clippy -p glass-dev --all-targets -- -D warnings
```

The native test creates a real `AgentSession`, exercises capability discovery,
naming, statistics, tree/messages, new/list operations, and proves fail-closed
session selection without requiring a paid model request.
