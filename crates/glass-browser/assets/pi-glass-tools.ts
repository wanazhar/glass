import { Type } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { createHash, randomUUID } from "node:crypto";
import { writeFile, unlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

export default function (pi: ExtensionAPI) {
  const broker = process.env.GLASS_PI_BROKER_BIN;
  if (!broker) return;

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
    const revision = `Project revision: ${String(params.expectedRevision ?? "missing")}`;
    if (glassName === "glass.file.patch") {
      const search = String(params.search ?? "");
      const replace = String(params.replace ?? "");
      return [
        `File: ${bounded(String(params.path ?? ""))}`,
        `Match: ${search.length} bytes · sha256 ${digest(search)}`,
        `Replacement: ${replace.length} bytes · sha256 ${digest(replace)}`,
        revision,
        "This approval is valid for this exact serialized call once.",
      ].join("\n");
    }
    if (glassName === "glass.process.start" || glassName === "glass.test.run") {
      const command = String(params.command ?? "");
      return [
        `Name: ${bounded(String(params.name ?? ""))}`,
        `Command: ${redactCommand(command)}`,
        `Command evidence: ${command.length} bytes · sha256 ${digest(command)}`,
        revision,
        "This approval is valid for this exact serialized call once.",
      ].join("\n");
    }
    return [
      `Process: ${bounded(String(params.name ?? ""))}`,
      revision,
      "This approval is valid for this exact serialized call once.",
    ].join("\n");
  };

  const register = (
    name: string,
    glassName: string,
    description: string,
    parameters: any,
    mutating = false,
  ) => {
    pi.registerTool({
      name,
      label: glassName,
      description,
      promptSnippet: description,
      parameters,
      async execute(toolCallId, params, signal, _onUpdate, ctx) {
        const call = JSON.stringify({ id: toolCallId, name: glassName, arguments: params });
        if (mutating) {
          const confirmed = await ctx.ui.confirm(
            `Approve ${glassName}?`,
            approvalSummary(glassName, params as Record<string, unknown>),
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
            { cwd: ctx.cwd, signal, timeout: 15000 },
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
    "glass_file_read",
    "glass.file.read",
    "Read one bounded project file through Glass root confinement",
    Type.Object({ path: Type.String() }),
  );
  register(
    "glass_file_list",
    "glass.file.list",
    "List the bounded Glass project tree",
    Type.Object({}),
  );
  register(
    "glass_file_search",
    "glass.file.search",
    "Search bounded project files and semantic project state",
    Type.Object({ query: Type.String() }),
  );
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
    "glass_file_patch",
    "glass.file.patch",
    "Replace one exact bounded text occurrence after per-call human approval",
    Type.Object({
      path: Type.String(), search: Type.String(), replace: Type.String(),
      expectedRevision: Type.Integer({ minimum: 0 }),
    }),
    true,
  );
  register(
    "glass_process_start",
    "glass.process.start",
    "Start one named PTY process after per-call human approval",
    Type.Object({
      name: Type.String(), command: Type.String(), expectedRevision: Type.Integer({ minimum: 0 }),
    }),
    true,
  );
  register(
    "glass_process_stop",
    "glass.process.stop",
    "Stop one named managed process after per-call human approval",
    Type.Object({ name: Type.String(), expectedRevision: Type.Integer({ minimum: 0 }) }),
    true,
  );
  register(
    "glass_test_run",
    "glass.test.run",
    "Run one named verification command after per-call human approval",
    Type.Object({
      name: Type.String(), command: Type.String(), expectedRevision: Type.Integer({ minimum: 0 }),
    }),
    true,
  );
  register(
    "glass_process_logs",
    "glass.process.logs",
    "Read a bounded output tail from one Glass-managed process",
    Type.Object({ name: Type.String() }),
  );
  register(
    "glass_process_list",
    "glass.process.list",
    "List Glass-managed processes and their checked state",
    Type.Object({}),
  );
  register(
    "glass_runtime_inspect",
    "glass.runtime.inspect",
    "Inspect bounded project, process, actor, diagnostic, and revision state",
    Type.Object({}),
  );
}
