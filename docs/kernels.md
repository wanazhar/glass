# Persistent kernels and governed Glass bindings

Glass Dev provides named Python, JavaScript, shell, and in-memory SQL kernels.
State persists between executions until the kernel is reset, cancelled, stopped,
or its workspace owner exits. Code, output, message queues, session count,
execution time, and nested Glass calls are bounded.

## Capability binding

A kernel starts with an explicit allowlist. An empty list grants no Glass tool
access:

```json
{
  "name": "analysis-python",
  "kind": "python",
  "capabilities": ["glass.browser.observe", "glass.test.results", "glass.graph.path"],
  "mutationAuthority": false
}
```

Bindings use the same `DevelopmentToolRouter` as MCP, Pi, the daemon, and TUI.
The router rechecks workspace generation and current project revision, trust,
tool availability, mutation authority, confirmation, result size, hooks, graph
links, and replay events. Kernel code cannot grant itself a capability or
mutation authority. `glass.eval.*` is never a valid nested capability, and one
execution is limited to 32 nested calls, preventing recursive kernel dispatch.

Each nested call records both actors:

```text
initiator: agent-0003
executor:  kernel:analysis-python
```

The executor retains the initiator's authority class; entering a kernel never
upgrades permissions.

## Language handles

Python and JavaScript use JSON values:

```python
tests = glass.call("glass.test.results", {})
```

```javascript
const tests = await glass.call("glass.test.results", {});
```

Shell uses a line-safe helper and JSON arguments:

```sh
glass_call glass.graph.path '{"from":"source:checkout","to":"entity:submit"}'
```

SQL uses a protocol statement whose payload is JSON:

```sql
GLASS CALL {"id":"query-1","tool":"glass.test.results","arguments":{}}
```

The SQL form is a Glass kernel protocol statement, not portable SQL syntax.

## Lifecycle and failure recovery

`glass.eval.list` reports kind, state, stable executor, revision, allowlist,
mutation policy, and execution count. `glass.eval.cancel` terminates the owned
worker and marks the session failed; `glass.eval.reset` recreates its backend
with the same capability policy; `glass.eval.stop` removes it. An execution
timeout also terminates the worker and requires reset. Malformed worker frames,
closed streams, oversized responses, invalid tool calls, missing capabilities,
stale revisions, and denied mutations fail closed.

## Security boundary

Kernels are not operating-system sandboxes. Python executes arbitrary Python
under the current user. JavaScript `vm` provides a persistent language context,
not a security boundary. Shell code executes as the current user. SQL is
in-memory but a Glass binding can exercise only its explicitly granted router
capabilities. Workspace trust and Glass authority govern access; they do not
isolate hostile code at the OS level. Use a container, VM, or restricted OS
account when stronger isolation is required.
