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
}
