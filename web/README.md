# Glass web workspace

This optional frontend connects to GC's native gateway. It contains no model
loop, provider credentials, or tool executor. GC owns threads and execution.

Build with `npm ci --ignore-scripts && npm run build` in this directory. Output
in `dist` is regenerable and is distributed as a sidecar asset bundle, not linked
into the core executable. Node 22.16 or newer is needed only to build it.

`src/app.js` owns the timeline and interaction state. `style.css` defines the
responsive layout. `launch.css` is inlined into the generated HTML so startup and
retry remain styled even when the main stylesheet fails. Syntax, math, Mermaid, and PDF modules are separate lazy
imports. `sw.js` caches application files and excludes all API requests.
`build.mjs` bundles modules, emits deterministic app icons and compressed assets,
and includes third-party license notices. Dependency versions are pinned in the
lockfile.

See [Remote workspace](../docs/connect.md) for operation, ownership, security,
preview support, and resource limits. The layout takes inspiration from T3 Code's
work logs and conversation surface. No T3 Code application code is copied.

Run `node test/startup.mjs` after building for isolated browser checks of launch
geometry, delayed/failed assets, parallel startup reads, cached-shell recovery,
and saved TUI history when live attachment fails. `python3 scripts/smoke-connect.py`
from the repository root covers the native gateway and full browser workflows;
`python3 scripts/smoke-connect-upgrade.py` reproduces installation while a gateway
is running. These tests create isolated gateways and browser profiles.
