# Glass Pi runtime

This package is the Glass-owned process boundary around Pi's `AgentSession`
SDK. It never invokes Pi's legacy CLI RPC mode.

The runtime uses 4-byte big-endian length-prefixed JSON frames over private
stdio, loads Pi with built-in/project tools and resources disabled, and exposes
one `glass_tool` capability whose calls return to the authoritative Glass Dev
router. Session files remain compatible JSONL under `.glass/pi-sessions`;
incompatible files fail explicitly during resume/switch.

The canonical runtime source embedded by `glass-dev` is
`crates/glass-dev/assets/pi-runtime.mjs`. This package pins and exercises the
SDK dependency during repository development.
