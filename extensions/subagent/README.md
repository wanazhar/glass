# subagent

Delegate one to eight self-contained tasks and get back their reports. Each
task runs in a fresh context with the `bash` and `state` tools; nothing from
the parent conversation reaches it.

- **Parallel by batch.** Independent tasks in one call run up to four at a
  time. The host owns the fan-out — one extension is one serial stdio pipe,
  so a batch arrives as a single `run_subagents` request and the reports come
  back in request order. The extension declares `concurrency: "serial"` so
  two batches cannot overlap.
- **Failure contained.** A task that fails, or whose worker panics, gets an
  error string in its own slot; siblings finish normally.
- **Agent definitions.** `~/.config/glass-core/agents/*.md`:

  ```markdown
  ---
  name: security-review
  description: Reviews a diff for injection and secret-handling bugs.
  model: claude-opus-4-5
  ---

  You review code changes for security defects. Be specific: file, line,
  and the concrete failure mode.
  ```

  The body becomes the sub-agent's prompt; `model` (optional) routes the task
  to that model on the active connection. `/agents` lists what is loaded.
- **Worktree isolation.** `"worktree": true` on a task runs it in a detached
  git worktree at HEAD under the data directory. The report includes the
  path; nothing is merged back. Uncommitted parent changes are absent by
  design.
- **One tool schema.** The tool takes a `tasks` array (1–8). Text after a
  colon/frontmatter is passed through; the host composes the final prompt, so
  a custom agent body cannot lose its assignment.

Protocol surface: `system_prompt`, `authorize` (always allows), `run_tool`
(forwarding `run_subagents`), and the `agents` slash command. The host must
have a subagent runner installed; without one, the request is refused with a
reason rather than left hanging.
