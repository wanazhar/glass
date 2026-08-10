import { Type } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { createHash, randomUUID } from "node:crypto";
import { writeFile, unlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

export default function (pi: ExtensionAPI) {
  const broker = process.env.GLASS_PI_BROKER_BIN;
  if (!broker) return;
  const unrestricted = process.env.GLASS_PI_YOLO === "1";

  const digest = (value: string) => createHash("sha256").update(value).digest("hex").slice(0, 12);
  const bounded = (value: string, limit = 160) =>
    value.length <= limit ? value : `${value.slice(0, limit)}…`;
  const redactCommand = (command: string) => bounded(command
    .replace(
      /\b([A-Z_][A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|PASSWD|API_?KEY)[A-Z0-9_]*)=([^\s]+)/giu,
      "$1=<redacted>",
    )
    .replace(
      /(--(?:token|secret|password|passwd|api[_-]?key))(?:=|\s+)([^\s]+)/giu,
      "$1=<redacted>",
    )
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s/@]+:[^\s/@]+@/giu, "$1<redacted>@"));
  const approvalSummary = (glassName: string, params: Record<string, unknown>) => {
    if (glassName === "glass.file.edit") {
      const edits = Array.isArray(params.edits) ? params.edits : [];
      return [
        `File: ${bounded(String(params.path ?? ""))}`,
        `Exact replacements: ${edits.length}`,
        `Edit evidence: sha256 ${digest(JSON.stringify(edits))}`,
        "This approval is valid for this exact serialized call once.",
      ].join("\n");
    }
    if (glassName === "glass.file.write") {
      const content = String(params.content ?? "");
      return [
        `File: ${bounded(String(params.path ?? ""))}`,
        `Content: ${content.length} bytes · sha256 ${digest(content)}`,
        "This approval is valid for this exact serialized call once.",
      ].join("\n");
    }
    if (glassName === "glass.command.run" || glassName === "glass.test.run") {
      const command = String(params.command ?? "");
      return [
        `Name: ${bounded(String(params.name ?? ""))}`,
        `Command: ${redactCommand(command)}`,
        `Command evidence: ${command.length} bytes · sha256 ${digest(command)}`,
        "This approval is valid for this exact serialized call once.",
      ].join("\n");
    }
    if (glassName === "glass.file.rename") {
      return [
        `From: ${bounded(String(params.from ?? ""))}`,
        `To: ${bounded(String(params.to ?? ""))}`,
        "This approval is valid for this exact serialized call once.",
      ].join("\n");
    }
    return [
      `Target: ${bounded(String(params.path ?? params.name ?? ""))}`,
      "This approval is valid for this exact serialized call once.",
    ].join("\n");
  };

  const register = (
    name: string,
    glassName: string,
    description: string,
    parameters: any,
    mutating = false,
    mapArguments: (params: any, toolCallId: string) => Record<string, unknown> = (params) => params,
  ) => {
    pi.registerTool({
      name,
      label: glassName,
      description,
      promptSnippet: description,
      parameters,
      async execute(toolCallId, params, signal, _onUpdate, ctx) {
        const arguments_ = mapArguments(params, toolCallId);
        const call = JSON.stringify({ id: toolCallId, name: glassName, arguments: arguments_ });
        if (mutating && !unrestricted) {
          const confirmed = await ctx.ui.confirm(
            `Approve ${glassName}?`,
            approvalSummary(glassName, arguments_),
            { timeout: 120000 },
          );
          if (!confirmed) throw new Error(`Glass denied ${glassName}`);
        }
        const requestPath = join(tmpdir(), `glass-pi-call-${randomUUID()}.json`);
        await writeFile(requestPath, call, { encoding: "utf8", mode: 0o600, flag: "wx" });
        let result;
        try {
          result = await pi.exec(
            broker,
            [
              "agent", "tool-file", requestPath, "--root", ctx.cwd,
              ...(mutating ? ["--allow-mutation", "--yes"] : []),
            ],
            {
              cwd: ctx.cwd,
              signal,
              timeout: glassName === "glass.command.run"
                ? (Number(arguments_.timeoutSeconds ?? 120) + 5) * 1000
                : glassName === "glass.test.run" ? 125000 : 30000,
            },
          );
        } finally {
          await unlink(requestPath).catch(() => {});
        }
        if (result.code !== 0) {
          throw new Error(result.stderr.trim() || `Glass broker exited ${result.code}`);
        }
        const value = JSON.parse(result.stdout);
        return {
          content: [{ type: "text", text: JSON.stringify(value) }],
          details: { broker: "glass", bytes: result.stdout.length },
        };
      },
    });
  };

  register(
    "glass_git_status",
    "glass.git.status",
    "Inspect bounded code and runtime impact without modifying Git",
    Type.Object({}),
  );
  register(
    "glass_semantic_inspect",
    "glass.semantic.inspect",
    "Inspect source and runtime graph links for one entity",
    Type.Object({ entity: Type.String() }),
  );

  register(
    "glass_web_ir_inspect",
    "glass.web_ir.inspect",
    "Validate and inspect bounded Glass Web IR without requesting a screenshot",
    Type.Object({ ir: Type.Object({}, { additionalProperties: true }) }),
  );
  register(
    "glass_web_ir_diff",
    "glass.web_ir.diff",
    "Summarize a bounded Web IR revision transition",
    Type.Object({
      before: Type.Object({}, { additionalProperties: true }),
      after: Type.Object({}, { additionalProperties: true }),
    }),
  );
  register(
    "glass_web_ir_continuity",
    "glass.web_ir.continuity",
    "Classify graph-scoped entity continuity across Web IR revisions",
    Type.Object({
      before: Type.Object({}, { additionalProperties: true }),
      after: Type.Object({}, { additionalProperties: true }),
      entityId: Type.String(),
    }),
  );
  register(
    "glass_task_plan",
    "glass.task.plan",
    "Compile a Task Protocol request into a value-free compact explanation",
    Type.Object({
      task: Type.Object({}, { additionalProperties: true }),
      ir: Type.Object({}, { additionalProperties: true }),
    }),
  );
  register(
    "glass_file_mkdir",
    "glass.file.mkdir",
    "Create one workspace-confined directory after per-call human approval",
    Type.Object({ path: Type.String() }),
    true,
  );
  register(
    "glass_file_rename",
    "glass.file.rename",
    "Rename one workspace-confined path after per-call human approval",
    Type.Object({ from: Type.String(), to: Type.String() }),
    true,
  );
  register(
    "glass_file_delete",
    "glass.file.delete",
    "Delete one file or empty directory after per-call human approval",
    Type.Object({ path: Type.String() }),
    true,
  );
  register(
    "glass_diagnostics_run",
    "glass.diagnostics.run",
    "Run bounded rust-analyzer diagnostics for one project file",
    Type.Object({ path: Type.String() }),
  );
  register(
    "glass_test_run",
    "glass.test.run",
    "Run one named verification command after per-call human approval",
    Type.Object({ name: Type.String(), command: Type.String() }),
    true,
  );
  register(
    "glass_runtime_inspect",
    "glass.runtime.inspect",
    "Inspect bounded project, actor, diagnostic, and current broker state",
    Type.Object({}),
  );
  register(
    "glass_capabilities",
    "glass.capabilities.inspect",
    "Inspect available Glass tools and explicit unavailable reasons",
    Type.Object({}),
  );

  // Override Pi's familiar coding-tool names with Glass-confined operations.
  // Models retain standard coding-harness affordances without receiving a
  // second, untracked filesystem or shell authority path.
  register(
    "read", "glass.file.read", "Read a workspace-confined text file",
    Type.Object({
      path: Type.String(),
      offset: Type.Optional(Type.Integer({ minimum: 1 })),
      limit: Type.Optional(Type.Integer({ minimum: 1 })),
    }),
    false,
    (params) => ({ path: params.path, offset: params.offset, limit: params.limit }),
  );
  register(
    "write", "glass.file.write", "Create or replace a workspace-confined text file",
    Type.Object({ path: Type.String(), content: Type.String() }),
    true,
  );
  register(
    "edit", "glass.file.edit", "Apply exact non-overlapping edits atomically",
    Type.Object({
      path: Type.String(),
      edits: Type.Array(Type.Object({ oldText: Type.String(), newText: Type.String() }), {
        minItems: 1, maxItems: 64,
      }),
    }),
    true,
  );
  register(
    "bash", "glass.command.run", "Run a command to completion through Glass",
    Type.Object({
      command: Type.String(),
      timeout: Type.Optional(Type.Number({ minimum: 1, maximum: 300 })),
    }),
    true,
    (params, toolCallId) => ({
      name: `pi-${digest(toolCallId)}`,
      command: params.command,
      timeoutSeconds: Math.max(1, Math.min(300, Math.ceil(Number(params.timeout ?? 120)))),
    }),
  );
  register(
    "grep", "glass.file.grep", "Search workspace-confined UTF-8 files for literal text",
    Type.Object({
      pattern: Type.String(),
      path: Type.Optional(Type.String()),
      glob: Type.Optional(Type.String()),
      ignoreCase: Type.Optional(Type.Boolean()),
      context: Type.Optional(Type.Integer({ minimum: 0, maximum: 10 })),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 500 })),
    }),
    false,
    (params) => ({
      pattern: params.pattern,
      path: params.path,
      glob: params.glob,
      ignoreCase: params.ignoreCase,
      context: params.context,
      limit: params.limit,
    }),
  );
  register(
    "find", "glass.file.find", "Find workspace-confined paths with star and question-mark matching",
    Type.Object({
      pattern: Type.String(),
      path: Type.Optional(Type.String()),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 2000 })),
    }),
    false,
    (params) => ({ pattern: params.pattern, path: params.path, limit: params.limit }),
  );
  register(
    "ls", "glass.file.list", "List the bounded Glass project tree",
    Type.Object({
      path: Type.Optional(Type.String()),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 2000 })),
    }),
    false,
    (params) => ({ path: params.path, limit: params.limit }),
  );

  register("glass_process_list", "glass.process.list", "List resident managed processes", Type.Object({}));
  register("glass_process_start", "glass.process.start", "Start a resident named process", Type.Object({ name: Type.String(), command: Type.String() }), true);
  register("glass_process_stop", "glass.process.stop", "Stop a resident named process", Type.Object({ name: Type.String() }), true);
  register("glass_process_logs", "glass.process.logs", "Read bounded resident process logs", Type.Object({ name: Type.String() }));
  register("glass_process_restart", "glass.process.restart", "Restart a resident named process", Type.Object({ name: Type.String() }), true);
  register("glass_process_input", "glass.process.input", "Send bounded input to a resident PTY", Type.Object({ name: Type.String(), input: Type.String() }), true);
  register("glass_process_resize", "glass.process.resize", "Resize a resident PTY", Type.Object({ name: Type.String(), cols: Type.Integer({ minimum: 1 }), rows: Type.Integer({ minimum: 1 }) }), true);
  register("glass_process_health", "glass.process.health", "Inspect resident process health", Type.Object({ name: Type.String() }));
  register("glass_process_ports", "glass.process.ports", "Inspect process-owned detected URLs and ports", Type.Object({}));

  register("glass_editor_open", "glass.editor.open", "Open a shared resident editor buffer", Type.Object({ path: Type.String() }), true);
  register("glass_editor_selection", "glass.editor.selection", "Inspect a shared editor cursor and selection", Type.Object({ path: Type.String() }));
  register("glass_editor_replace", "glass.editor.replace", "Replace text in a shared conflict-safe buffer", Type.Object({ path: Type.String(), oldText: Type.String(), newText: Type.String() }), true);
  register("glass_editor_save", "glass.editor.save", "Save a shared conflict-safe editor buffer", Type.Object({ path: Type.String() }), true);
  register("glass_editor_diff", "glass.editor.diff", "Inspect the current project/editor diff", Type.Object({}));
  register("glass_editor_buffers", "glass.editor.buffers", "List shared resident editor buffers", Type.Object({}));

  register("glass_browser_state", "glass.browser.state", "Inspect the authoritative resident browser state", Type.Object({}));
  register("glass_browser_start", "glass.browser.start", "Start or attach the resident browser", Type.Object({ port: Type.Optional(Type.Integer()), attach: Type.Optional(Type.Boolean()), incognito: Type.Optional(Type.Boolean()), headed: Type.Optional(Type.Boolean()), profile: Type.Optional(Type.String()), chromePath: Type.Optional(Type.String()) }), true);
  register("glass_browser_attach", "glass.browser.attach", "Attach the resident browser to an existing CDP endpoint", Type.Object({ port: Type.Optional(Type.Integer()), incognito: Type.Optional(Type.Boolean()), headed: Type.Optional(Type.Boolean()), profile: Type.Optional(Type.String()), chromePath: Type.Optional(Type.String()) }), true);
  register("glass_browser_reconnect", "glass.browser.reconnect", "Reconnect using the last authoritative browser configuration", Type.Object({}), true);
  register("glass_browser_stop", "glass.browser.stop", "Stop the resident browser", Type.Object({}), true);
  register("glass_browser_observe", "glass.browser.observe", "Create a fresh structured browser observation", Type.Object({}));
  register("glass_browser_snapshot", "glass.browser.snapshot", "Capture a fresh accessibility snapshot", Type.Object({}));
  register("glass_browser_semantic", "glass.browser.semantic", "Inspect fresh semantic browser evidence", Type.Object({ level: Type.Optional(Type.Union([Type.Literal("summary"), Type.Literal("interactive"), Type.Literal("structured"), Type.Literal("detailed"), Type.Literal("raw")])) }));
  register("glass_browser_diff", "glass.browser.diff", "Diff the cached and fresh browser observations", Type.Object({}));
  register("glass_browser_targets", "glass.browser.targets", "List authoritative browser targets", Type.Object({}));
  register("glass_browser_select_target", "glass.browser.target.select", "Select an authoritative browser target", Type.Object({ targetId: Type.String() }), true);
  register("glass_browser_navigate", "glass.browser.navigate", "Navigate with a browser revision guard", Type.Object({ url: Type.String(), browserRevision: Type.Integer({ minimum: 1 }), timeoutSeconds: Type.Optional(Type.Integer()) }), true);
  register("glass_browser_act", "glass.browser.act", "Execute a revision-safe browser click, type, or scroll", Type.Object({ action: Type.Union([Type.Literal("click"), Type.Literal("type"), Type.Literal("scroll")]), browserRevision: Type.Integer({ minimum: 1 }), target: Type.Optional(Type.String()), text: Type.Optional(Type.String()), dx: Type.Optional(Type.Number()), dy: Type.Optional(Type.Number()) }), true);
  register("glass_browser_screenshot", "glass.browser.screenshot", "Capture an explicit bounded browser screenshot", Type.Object({}));
  register("glass_workflow_run", "glass.workflow.run", "Run a validated browser workflow", Type.Object({ definition: Type.Any(), inputs: Type.Optional(Type.Record(Type.String(), Type.Any())) }), true);
  register("glass_workflow_list", "glass.workflow.list", "List resident workflow state and evidence", Type.Object({}));
  register("glass_workflow_pause", "glass.workflow.pause", "Export a redacted workflow checkpoint", Type.Object({}), true);
  register("glass_workflow_resume", "glass.workflow.resume", "Resume a reconciled workflow checkpoint", Type.Object({ definition: Type.Any(), inputs: Type.Optional(Type.Record(Type.String(), Type.Any())), checkpoint: Type.Any() }), true);
  register("glass_workflow_cancel", "glass.workflow.cancel", "Cancel a checkpointed resident workflow", Type.Object({}), true);
  register("glass_workflow_verify", "glass.workflow.verify", "Verify the latest resident workflow result", Type.Object({}));
  register("glass_workflow_record", "glass.workflow.record", "Compile semantic interaction evidence into a workflow draft", Type.Object({ session: Type.Any() }));
  register("glass_memory_retrieve", "glass.memory.retrieve", "Retrieve bounded advisory browser memory", Type.Object({ recordId: Type.Optional(Type.String()) }));
  register("glass_memory_explain", "glass.memory.explain", "Explain one advisory memory record", Type.Object({ recordId: Type.String() }));
  register("glass_memory_forget", "glass.memory.forget", "Forget one advisory memory record", Type.Object({ recordId: Type.String() }), true);
  register("glass_semantic_inspect", "glass.semantic.inspect", "Inspect fresh authoritative browser semantics", Type.Object({}));
  register("glass_semantic_diff", "glass.semantic.diff", "Diff fresh authoritative browser semantics", Type.Object({}));
  register("glass_semantic_links", "glass.semantic.links", "Read source/runtime/browser semantic links", Type.Object({}));

  register("glass_git_diff", "glass.git.diff", "Read a native Git diff", Type.Object({ staged: Type.Optional(Type.Boolean()), path: Type.Optional(Type.String()) }));
  register("glass_git_stage", "glass.git.stage", "Stage repository paths", Type.Object({ paths: Type.Array(Type.String(), { minItems: 1, maxItems: 256 }) }), true);
  register("glass_git_unstage", "glass.git.unstage", "Unstage repository paths", Type.Object({ paths: Type.Array(Type.String(), { minItems: 1, maxItems: 256 }) }), true);
  register("glass_git_commit", "glass.git.commit", "Create an attributed Git commit", Type.Object({ message: Type.String() }), true);
  register("glass_git_branches", "glass.git.branches", "List repository branches", Type.Object({}));
  register("glass_git_branch_create", "glass.git.branch.create", "Create a repository branch", Type.Object({ name: Type.String(), startPoint: Type.Optional(Type.String()) }), true);
  register("glass_git_branch_switch", "glass.git.branch.switch", "Switch repository branches", Type.Object({ name: Type.String(), create: Type.Optional(Type.Boolean()) }), true);
  register("glass_git_blame", "glass.git.blame", "Read bounded Git blame evidence", Type.Object({ path: Type.String(), startLine: Type.Optional(Type.Integer()), endLine: Type.Optional(Type.Integer()) }));
  register("glass_git_worktrees", "glass.git.worktree.list", "List repository worktrees", Type.Object({}));
  register("glass_git_worktree_create", "glass.git.worktree.create", "Create an isolated worktree", Type.Object({ path: Type.String(), branch: Type.String(), createBranch: Type.Optional(Type.Boolean()) }), true);
  register("glass_git_worktree_remove", "glass.git.worktree.remove", "Remove an owned worktree", Type.Object({ path: Type.String(), force: Type.Optional(Type.Boolean()) }), true);

  register("glass_test_discover", "glass.test.discover", "Discover resident test suites", Type.Object({}));
  register("glass_test_run_suite", "glass.test.run", "Run a discovered resident test suite", Type.Object({ runId: Type.String(), suiteId: Type.String(), timeoutSeconds: Type.Optional(Type.Integer()) }), true);
  register("glass_test_results", "glass.test.results", "Inspect structured resident test results", Type.Object({}));
  register("glass_test_cancel", "glass.test.cancel", "Cancel a resident test run", Type.Object({ runId: Type.String() }), true);
  register("glass_test_watch", "glass.test.watch", "Watch a test suite by workspace revision", Type.Object({ suiteId: Type.String() }), true);

  register("glass_eval_start", "glass.eval.start", "Start a persistent execution kernel", Type.Object({ name: Type.String(), kind: Type.Union([Type.Literal("python"), Type.Literal("javascript"), Type.Literal("shell"), Type.Literal("sql")]) }), true);
  register("glass_eval_execute", "glass.eval.execute", "Execute code in a persistent kernel", Type.Object({ name: Type.String(), code: Type.String(), timeoutSeconds: Type.Optional(Type.Integer()) }), true);
  register("glass_eval_list", "glass.eval.list", "List persistent execution kernels", Type.Object({}));
  register("glass_eval_reset", "glass.eval.reset", "Reset a persistent execution kernel", Type.Object({ name: Type.String() }), true);
  register("glass_eval_stop", "glass.eval.stop", "Stop a persistent execution kernel", Type.Object({ name: Type.String() }), true);

  const position = { server: Type.String(), path: Type.String(), line: Type.Integer({ minimum: 1 }), character: Type.Integer({ minimum: 1 }) };
  register("glass_lsp_start", "glass.lsp.start", "Start a shared resident language server", Type.Object({ server: Type.String(), command: Type.String(), arguments: Type.Optional(Type.Array(Type.String())) }), true);
  register("glass_lsp_stop", "glass.lsp.stop", "Stop a shared resident language server", Type.Object({ server: Type.String() }), true);
  register("glass_lsp_list", "glass.lsp.list", "List shared resident language servers", Type.Object({}));
  register("glass_lsp_events", "glass.lsp.events", "Read language-service attribution events", Type.Object({ since: Type.Optional(Type.Integer()) }));
  register("glass_lsp_raw", "glass.lsp.raw", "Send one bounded advanced language-server request", Type.Object({ server: Type.String(), method: Type.String(), params: Type.Optional(Type.Any()), timeoutSeconds: Type.Optional(Type.Integer()) }));
  register("glass_lsp_diagnostics", "glass.lsp.diagnostics", "Read diagnostics from the shared resident LSP", Type.Object({ server: Type.String(), path: Type.String() }));
  register("glass_lsp_hover", "glass.lsp.hover", "Read hover information from the shared resident LSP", Type.Object(position));
  register("glass_lsp_completion", "glass.lsp.completion", "Request completion from the shared resident LSP", Type.Object(position));
  register("glass_lsp_definition", "glass.lsp.definition", "Navigate to a shared LSP definition", Type.Object(position));
  register("glass_lsp_declaration", "glass.lsp.declaration", "Navigate to a shared LSP declaration", Type.Object(position));
  register("glass_lsp_implementation", "glass.lsp.implementation", "Navigate to shared LSP implementations", Type.Object(position));
  register("glass_lsp_references", "glass.lsp.references", "Find shared LSP references", Type.Object(position));
  register("glass_lsp_document_symbols", "glass.lsp.document_symbols", "List document symbols", Type.Object({ server: Type.String(), path: Type.String() }));
  register("glass_lsp_workspace_symbols", "glass.lsp.workspace_symbols", "Search workspace symbols", Type.Object({ server: Type.String(), query: Type.String() }));
  register("glass_lsp_signature_help", "glass.lsp.signature_help", "Read signature help", Type.Object(position));
  const range = { start: Type.Object({ line: Type.Integer({ minimum: 0 }), character: Type.Integer({ minimum: 0 }) }), end: Type.Object({ line: Type.Integer({ minimum: 0 }), character: Type.Integer({ minimum: 0 }) }) };
  register("glass_lsp_code_actions", "glass.lsp.code_actions", "Request shared LSP code actions", Type.Object({ server: Type.String(), path: Type.String(), ...range, diagnostics: Type.Optional(Type.Array(Type.Any())) }));
  register("glass_lsp_formatting", "glass.lsp.formatting", "Request document formatting", Type.Object({ server: Type.String(), path: Type.String() }));
  register("glass_lsp_range_formatting", "glass.lsp.range_formatting", "Request range formatting", Type.Object({ server: Type.String(), path: Type.String(), ...range }));
  register("glass_lsp_semantic_tokens", "glass.lsp.semantic_tokens", "Read semantic tokens", Type.Object({ server: Type.String(), path: Type.String() }));
  register("glass_lsp_rename", "glass.lsp.rename", "Request a shared LSP workspace rename edit", Type.Object({ ...position, newName: Type.String() }));

  register("glass_debug_start", "glass.debug.start", "Start and initialize a resident DAP adapter", Type.Object({ session: Type.String(), command: Type.String(), arguments: Type.Optional(Type.Array(Type.String())), timeoutSeconds: Type.Optional(Type.Integer()) }), true);
  register("glass_debug_launch", "glass.debug.launch", "Launch a program through a resident DAP session", Type.Object({ session: Type.String(), configuration: Type.Object({}, { additionalProperties: true }) }), true);
  register("glass_debug_attach", "glass.debug.attach", "Attach a resident DAP session", Type.Object({ session: Type.String(), configuration: Type.Object({}, { additionalProperties: true }) }), true);
  register("glass_debug_configuration_done", "glass.debug.configuration_done", "Finish debugger configuration and start execution", Type.Object({ session: Type.String() }), true);
  register("glass_debug_restart", "glass.debug.restart", "Restart a resident debug session", Type.Object({ session: Type.String(), configuration: Type.Optional(Type.Any()) }), true);
  register("glass_debug_breakpoint_set", "glass.debug.breakpoint.set", "Set source breakpoints", Type.Object({ session: Type.String(), path: Type.String(), lines: Type.Array(Type.Integer({ minimum: 1 })) }), true);
  register("glass_debug_breakpoint_remove", "glass.debug.breakpoint.remove", "Remove all source breakpoints for a file", Type.Object({ session: Type.String(), path: Type.String() }), true);
  register("glass_debug_exception_set", "glass.debug.exception.set", "Configure exception breakpoint filters", Type.Object({ session: Type.String(), filters: Type.Array(Type.String()) }), true);
  register("glass_debug_continue", "glass.debug.continue", "Continue one debugger thread", Type.Object({ session: Type.String(), threadId: Type.Integer() }), true);
  register("glass_debug_pause", "glass.debug.pause", "Pause one debugger thread", Type.Object({ session: Type.String(), threadId: Type.Integer() }), true);
  register("glass_debug_step", "glass.debug.step", "Step a debugger thread", Type.Object({ session: Type.String(), threadId: Type.Integer(), kind: Type.Union([Type.Literal("over"), Type.Literal("in"), Type.Literal("out")]) }), true);
  register("glass_debug_threads", "glass.debug.threads", "List debugger threads", Type.Object({ session: Type.String() }));
  register("glass_debug_stack", "glass.debug.stack", "Read a debugger stack", Type.Object({ session: Type.String(), threadId: Type.Integer() }));
  register("glass_debug_scopes", "glass.debug.scopes", "Read debugger scopes", Type.Object({ session: Type.String(), frameId: Type.Integer() }));
  register("glass_debug_variables", "glass.debug.variables", "Read debugger variables", Type.Object({ session: Type.String(), variablesReference: Type.Integer() }));
  register("glass_debug_evaluate", "glass.debug.evaluate", "Evaluate in a debugger frame", Type.Object({ session: Type.String(), expression: Type.String(), frameId: Type.Optional(Type.Integer()), context: Type.Optional(Type.String()) }));
  register("glass_debug_events", "glass.debug.events", "Read bounded DAP events", Type.Object({ session: Type.String() }));
  register("glass_debug_disconnect", "glass.debug.disconnect", "Disconnect a resident debugger", Type.Object({ session: Type.String(), terminateDebuggee: Type.Optional(Type.Boolean()) }), true);
  register("glass_debug_terminate", "glass.debug.terminate", "Terminate a debuggee", Type.Object({ session: Type.String(), restart: Type.Optional(Type.Boolean()) }), true);
  register("glass_debug_stop", "glass.debug.stop", "Stop a resident debugger session", Type.Object({ session: Type.String() }), true);

  register("glass_agent_list", "glass.agent.list", "Inspect independent Glass Agent sessions", Type.Object({}));
  register("glass_agent_spawn", "glass.agent.spawn", "Spawn an independent Pi-powered Glass Agent", Type.Object({ spec: Type.Object({}, { additionalProperties: true }) }), true);
  register("glass_agent_prompt", "glass.agent.prompt", "Prompt an independent Glass Agent", Type.Object({ agentId: Type.String(), text: Type.String() }), true);
  register("glass_agent_steer", "glass.agent.steer", "Steer a running Glass Agent", Type.Object({ agentId: Type.String(), text: Type.String() }), true);
  register("glass_agent_follow_up", "glass.agent.follow-up", "Queue a Glass Agent follow-up", Type.Object({ agentId: Type.String(), text: Type.String() }), true);
  register("glass_agent_abort", "glass.agent.abort", "Cancel an independent Glass Agent", Type.Object({ agentId: Type.String() }), true);
  register("glass_agent_compact", "glass.agent.compact", "Compact an independent Glass Agent session", Type.Object({ agentId: Type.String(), instructions: Type.Optional(Type.String()) }), true);
  register("glass_agent_model", "glass.agent.model", "Switch an independent Glass Agent model", Type.Object({ agentId: Type.String(), provider: Type.String(), modelId: Type.String() }), true);
  register("glass_agent_thinking", "glass.agent.thinking", "Switch an independent Glass Agent reasoning level", Type.Object({ agentId: Type.String(), level: Type.String() }), true);
  register("glass_agent_new_session", "glass.agent.new-session", "Start a new Pi conversation in an agent", Type.Object({ agentId: Type.String() }), true);
  register("glass_agent_clone_session", "glass.agent.clone-session", "Clone an agent conversation", Type.Object({ agentId: Type.String() }), true);
  register("glass_agent_fork", "glass.agent.fork", "Fork an agent conversation at an entry", Type.Object({ agentId: Type.String(), entryId: Type.String() }), true);
  register("glass_agent_switch_session", "glass.agent.switch-session", "Resume an agent session path", Type.Object({ agentId: Type.String(), path: Type.String() }), true);
  register("glass_agent_messages", "glass.agent.messages", "Read structured agent messages", Type.Object({ agentId: Type.String() }));
  register("glass_agent_entries", "glass.agent.entries", "Read structured agent session entries", Type.Object({ agentId: Type.String(), since: Type.Optional(Type.String()) }));
  register("glass_agent_stats", "glass.agent.stats", "Read agent token and session statistics", Type.Object({ agentId: Type.String() }));
  register("glass_agent_name", "glass.agent.name", "Name an agent session", Type.Object({ agentId: Type.String(), name: Type.String() }), true);

  register("glass_graph_query", "glass.graph.query", "Query one causal development graph node", Type.Object({ id: Type.String() }));
  register("glass_graph_path", "glass.graph.path", "Explain a causal path between development nodes", Type.Object({ from: Type.String(), to: Type.String() }));
  register("glass_replay_list", "glass.replay.list", "List observable development replay events", Type.Object({ since: Type.Optional(Type.Integer()), limit: Type.Optional(Type.Integer()) }));
  register("glass_replay_diff", "glass.replay.diff", "Diff an observable replay sequence range", Type.Object({ from: Type.Integer(), to: Type.Integer() }));
}
