---
id: plugins-001
scope: crates/glass plugins loader, gc plugin commands, mcp-bridge plugin config
status: done
depends-on: []
---
# Plugins

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

Agent Plugins 1.0 support: load the cross-vendor packaging standard
(published Aug 6, GA Aug 12, backed by GitHub/AWS/OpenAI/Vercel/Cursor) and
install from a path or git URL. Spec pinned from `agent-plugins-spec` 1.0.0.

## Context

- Components: `skills/` (immediate child directories with `SKILL.md`) join
  the skill catalogue; root `mcp.json` servers load through mcp-bridge
  (stdio variant, per-server failure isolation, `${PLUGIN_ROOT}` /
  `${PLUGIN_DATA}` expansion, `./command` and `cwd` containment).
- Failure boundaries are implemented verbatim: unknown manifest fields are
  reported and ignored; wrong `$schema`, invalid or missing `name`, or a
  path that escapes the plugin root reject/skip exactly that component.
- User-level only (`~/.config/glass-core/plugins/`): a repository cannot add
  a plugin, so this stays outside the trust-gate surface.
- `gc plugin add` stages, validates, then moves into place so an interrupted
  install leaves nothing visible; `lock.json` records the source plus an
  FNV metadata hash, and `plugin list` reports post-install drift. Removing
  keeps `plugin_data_dir` (reported) because it may hold user state.

## Paths

- `crates/glass/src/plugins.rs` (new): manifest/name validation, skill
  loading via `skills::parse` (now `pub(crate)`), containment checks, tree
  hash, add/remove/list.
- `crates/glass/src/main.rs`: `mod plugins`, `gc plugin` dispatch, plugin
  skills merged into the catalogue.
- `extensions/mcp-bridge/mcp_bridge.py`: `plugin_servers()` + cwd support.
- `scripts/smoke-plugin.py`.

## Verification

- Unit tests: name rules, valid load, unknown-field warning, schema/name
  rejection, symlink escape skipped, tree-hash drift.
- `scripts/smoke-plugin.py`: add/list/remove through the binary, duplicate
  refusal, skill in the prompt, plugin MCP server callable end-to-end,
  `${PLUGIN_DATA}` created.
- `cargo test -p glass-core` (382 tests) and clippy.
