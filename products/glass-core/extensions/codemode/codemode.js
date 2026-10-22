#!/usr/bin/env node
/**
 * codemode extension for glass.
 *
 * Lets the model write one small JS program that drives many tools, instead of
 * round-tripping through the model for every action. Only the value the script
 * returns reaches the model, so intermediate output never enters context.
 *
 * Disabled by default. Enable in ~/.config/glass/config.toml:
 *   extensions = ["node /path/to/glass-core/extensions/codemode/codemode.js"]
 *
 * Node standard library only: nothing to install, and nothing enters the
 * glass binary.
 */

const fs = require("fs");
const vm = require("vm");

const PROTOCOL = 2;
const MAX_LINE = 8 * 1024 * 1024;
// Output budget (parity with pi codemode): scripts that produce more than
// this spill the FULL text to a temp file and the model sees a bounded
// head+tail preview with the path. PI parity constants: estimated tokens,
// 4 chars/token, default 10k tokens.
const CHARS_PER_TOKEN = 4;
const DEFAULT_MAX_OUTPUT_TOKENS = 10_000;
const MAX_OUTPUT_TOKENS = 100_000;
const DEFAULT_TIMEOUT_MS = 10_000;
const MAX_TIMEOUT_MS = 120_000;

/** Parse the optional first line: `// @options: {json}`. Unknown keys are
 * reported, not ignored, so a typo does not silently change nothing. */
function parseOptions(code) {
  const first = code.split("\n", 1)[0];
  const match = /^\/\/\s*@options:\s*(\{.*\})\s*$/.exec(first);
  if (!match) return { code, warnings: [] };
  let parsed;
  try {
    parsed = JSON.parse(match[1]);
  } catch (e) {
    return { code, warnings: ["@options line is not valid JSON: " + e.message] };
  }
  const warnings = [];
  const options = {};
  for (const key of Object.keys(parsed)) {
    if (key === "max_output_tokens") {
      if (typeof parsed[key] !== "number" || !Number.isInteger(parsed[key]) || parsed[key] <= 0) {
        warnings.push("@options max_output_tokens must be a positive integer");
      } else if (parsed[key] > MAX_OUTPUT_TOKENS) {
        warnings.push(`@options max_output_tokens capped at ${MAX_OUTPUT_TOKENS}`);
        options.max_output_tokens = MAX_OUTPUT_TOKENS;
      } else {
        options.max_output_tokens = parsed[key];
      }
    } else if (key === "timeout_ms") {
      if (typeof parsed[key] !== "number" || !Number.isInteger(parsed[key]) || parsed[key] <= 0) {
        warnings.push("@options timeout_ms must be a positive integer");
      } else if (parsed[key] > MAX_TIMEOUT_MS) {
        warnings.push(`@options timeout_ms capped at ${MAX_TIMEOUT_MS}`);
        options.timeout_ms = MAX_TIMEOUT_MS;
      } else {
        options.timeout_ms = parsed[key];
      }
    } else {
      warnings.push(`unknown @options key "${key}" ignored`);
    }    
  }
  return { code, options, warnings };
}

/** Overflow spill: full text to a temp file, like bash truncation does.
 * The path is named so a later `bash` call can read it with offset/limit. */
function spillOutput(text) {
  try {
    const dir = "/tmp";
    const path = `${dir}/gc-codemode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}.txt`;
    // 0600: same protection as the host's own spill files.
    const fd = fs.openSync(path, "wx", 0o600);
    try {
      fs.writeSync(fd, text);
    } finally {
      fs.closeSync(fd);
    }
    return { path };
  } catch (error) {
    return { error: error && error.message ? error.message : String(error) };
  }
}

/**
 * The output item protocol (pi contract, task six-wave-e-001):
 * more than one text item → each item after the first starts with
 * `==> text N/M <==`; console lines gather in one trailing block.
 * The `total` counts text items only, image items are not applicable here
 * (no image support in this sandbox).
 */
function formatOutput(items, consoleLines) {
  const total = items.length;
  let out = "";
  if (total > 1) {
    items.forEach((item, i) => {
      out += `==> text ${i + 1}/${total} <==\n${item}\n`;
    });
  } else if (total === 1) {
    out = items[0];
  }
  if (consoleLines.length > 0) {
    if (out && !out.endsWith("\n")) out += "\n";
    out += `<console_output>\n${consoleLines.join("\n")}\n</console_output>`;
  }
  return out;
}

function send(obj) {
  fs.writeSync(1, JSON.stringify(obj) + "\n");
}

/**
 * Read one line from stdin, synchronously.
 *
 * `fs.readSync` on fd 0 blocks until bytes arrive, which is what lets the tool
 * API look synchronous to the script. The protocol is request/response, so
 * without this the whole thing would need a promise pump and every script would
 * have to be written as async.
 *
 * The buffer is module-level on purpose: a single read can return several
 * lines at once, and anything after the first newline has to survive until the
 * next call. Discarding it silently loses requests.
 */
let inbuf = "";

function readLine() {
  const buf = Buffer.alloc(65536);
  for (;;) {
    const nl = inbuf.indexOf("\n");
    if (nl !== -1) {
      const line = inbuf.slice(0, nl);
      inbuf = inbuf.slice(nl + 1);
      return line;
    }
    let n;
    try {
      n = fs.readSync(0, buf, 0, buf.length, null);
    } catch (e) {
      // EAGAIN surfaces on a non-blocking stdin; treat it as no data yet.
      if (e && (e.code === "EAGAIN" || e.code === "EWOULDBLOCK")) continue;
      throw e;
    }
    if (n === 0) {
      // EOF: hand back whatever is left without a trailing newline.
      if (inbuf.length > 0) {
        const line = inbuf;
        inbuf = "";
        return line;
      }
      return null; // host closed
    }
    inbuf += buf.toString("utf8", 0, n);
    if (inbuf.length > MAX_LINE) throw new Error("input exceeded " + MAX_LINE + " bytes");
  }
}

/**
 * Call a tool on the host and wait for its output.
 *
 * Only `bash` and `state` are reachable, and that restriction is enforced by
 * the host rather than here, so the capability set cannot be widened by
 * editing this file.
 */
let callSeq = 0;
// Output items and console lines collected during the current script.
// Module-level so the sandbox closures and the run_tool handler share them;
// reset before every run.
let scriptOutput = [];
let scriptConsole = [];

/** Strings as-is, other values as compact JSON (pi valueText). */
function valueText(value) {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function callHostTool(name, args) {
  const callId = "cm-" + process.pid + "-" + ++callSeq;
  send({ type: "call_tool", call_id: callId, name, arguments: JSON.stringify(args || {}) });
  for (;;) {
    const line = readLine();
    if (line === null) throw new Error("host closed the connection");
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      continue;
    }
    if (msg.type === "call_tool_result" && msg.call_id === callId) {
      return msg.body === undefined ? "" : String(msg.body);
    }
    // Anything else belongs to the outer request/response conversation and is
    // not ours to answer.
  }
}

/**
 * The sandbox.
 *
 * No `require`, no `process`, no timers. This is not the security boundary —
 * the boundary is that the host only forwards `bash` and `state` — it exists
 * so a runaway script cannot keep this process alive.
 */
function runScript(code, timeoutMs) {
  const context = vm.createContext({
    tools: {
      bash: (command) => callHostTool("bash", { command }),
      state: (items) => callHostTool("state", items === undefined ? {} : { items }),
      lines: (s) => String(s).split("\n"),
      grep: (s, re) => String(s).split("\n").filter((l) => re.test(l)),
      unique: (a) => Array.from(new Set(a)),
      count: (a) => a.length,
      join: (a, sep) => a.join(sep === undefined ? "\n" : sep),
      text: (value) => {
        scriptOutput.push(valueText(value));
      },
    },
    // console now collects (pi parity): the lines surface in one trailing
    // <console_output> block instead of vanishing.
    console: {
      log: (...parts) => scriptConsole.push(parts.map(String).join(" ")),
      error: (...parts) => scriptConsole.push("ERROR: " + parts.map(String).join(" ")),
    },
  });

  const script = new vm.Script("(async () => {\n" + code + "\n})()", {
    filename: "codemode",
  });
  // `runInContext` returns the promise the async wrapper created. Every tool
  // call inside the script blocks on the host synchronously, so the promise
  // has already settled by the time we get it; awaiting it is what recovers
  // the script's actual return value. The timeout is the @options timeout,
  // never above MAX_TIMEOUT_MS.
  //
  // A vm timeout is NOT a promise rejection: it throws synchronously inside
  // runInContext and unwinds through the caller's stack. Thrown inside the
  // extension's main loop that would kill the process and the model would
  // get nothing, so with a timeout set we catch here and return a rejected
  // promise — the same shape every other script failure takes.
  try {
    return script.runInContext(context, { timeout: timeoutMs });
  } catch (error) {
    if (error && /Script execution timed out/i.test(error.message || "")) {
      return Promise.reject(error);
    }
    throw error;
  }
}

function main() {
  for (;;) {
    const line = readLine();
    if (line === null) break;
    if (!line.trim()) continue;

    let req;
    try {
      req = JSON.parse(line);
    } catch {
      continue;
    }

    const id = req.id;
    switch (req.type) {
      case "hello":
        send({
          id,
          protocol: PROTOCOL,
          name: "codemode",
          version: "1.0.0",
          system_prompt:
            "A `codemode` tool is available. It runs a JavaScript program that can call " +
            "`tools.bash(command)` and `tools.state(items)`, plus helpers `tools.lines`, " +
            "`tools.grep(s, /re/)`, `tools.unique`, `tools.count`, `tools.join`, " +
            "`tools.text(value)`. Only the " +
            "value you return reaches you, so intermediate tool output never enters " +
            "context. Optional first line `// @options: {\"max_output_tokens\": 10000, " +
            "\"timeout_ms\": 60000}` tunes the output budget (default 10000 tokens; over " +
            "budget, the full output is spilled to a file whose path is named) and the " +
            "script timeout (default 10s). Several returned items arrive each preceded " +
            "by a `==> text N/M <==` line; console.log/error surface in one trailing " +
            "<console_output> block. Use it when a task would otherwise need many bash " +
            "calls or would dump large output into context. Do not use it for a single command.",
          tools: [
            {
              name: "codemode",
              description:
                "Run a JS program that orchestrates tools. `await tools.bash(cmd)` returns " +
                "the output as a string. Helpers: tools.lines, tools.grep, tools.unique, " +
                "tools.count, tools.join, tools.text(value). Optional first line " +
                "`// @options: {\"max_output_tokens\": 10000, \"timeout_ms\": 60000}`. " +
                "Multiple items come back each after a `==> text N/M <==` line; over the output " +
                "budget the full output spills to a named file. Return the summary worth " +
                "keeping. Use instead of many sequential bash calls.",
              schema: {
                type: "object",
                properties: {
                  code: { type: "string", description: "JS program. Top-level await allowed." },
                },
                required: ["code"],
              },
            },
          ],
          commands: [{ name: "codemode", description: "report codemode availability" }],
        });
        break;

      case "command":
        send({ id, output: "codemode 2.0.0 (node " + process.version + ", items+budget+@options)" });
        break;

      case "run_tool": {
        if (req.name !== "codemode") {
          send({ id, output: "unknown tool: " + req.name });
          break;
        }
        let args = {};
        try {
          args = JSON.parse(req.arguments || "{}");
        } catch {}
        const code = args.code;
        if (typeof code !== "string" || !code.trim()) {
          send({ id, output: "codemode: `code` is required and must be a non-empty string" });
          break;
        }

        // @options (pi contract): optional first `// @options: {...}` line
        // tunes max_output_tokens and timeout_ms; unknown keys warn instead
        // of silently doing nothing.
        const { code: body, options = {}, warnings = [] } = parseOptions(code);
        const timeoutMs = options.timeout_ms || DEFAULT_TIMEOUT_MS;
        const maxTokens = options.max_output_tokens || DEFAULT_MAX_OUTPUT_TOKENS;

        // Settle synchronously. The main loop is about to block in readLine(),
        // and a pending promise would never get its turn, so the microtask
        // queue has to be drained here with Node's internal tick hook.
        scriptOutput = [];
        scriptConsole = [];
        let settled = false;
        let value;
        let failure = null;
        runScript(body, timeoutMs).then(
          (v) => {
            settled = true;
            value = v;
          },
          (e) => {
            settled = true;
            failure = e;
          }
        );
        for (let i = 0; i < 1000 && !settled; i++) {
          if (typeof process._tickCallback === "function") process._tickCallback();
        }

        let output;
        if (!settled) {
          output = "codemode error: script did not settle (await something the sandbox cannot provide?)";
        } else if (failure) {
          const timedOut = /Script execution timed out|timed out/i.test(
            failure.message || String(failure)
          );
          output = timedOut
            ? `codemode error: script timed out after ${timeoutMs}ms`
            : "codemode error: " + (failure.message || String(failure));
        } else {
          // Every produced text value is an item (pi contract): text() calls
          // in order, then the returned value last.
          const items = scriptOutput.map(valueText);
          if (value !== undefined) items.push(valueText(value));
          let text = formatOutput(items, scriptConsole);
          if (warnings.length > 0) text = `${warnings.join("\n")}\n${text}`;

          // Overflow budget (pi parity): over budget → bounded head+tail
          // preview, full text spilled to a temp file whose path is named;
          // spill failure must not discard the script's own work.
          const budget = maxTokens * CHARS_PER_TOKEN;
          if (text.length > budget) {
            const lines = text.split("\n").length;
            const headChars = Math.floor(budget / 2);
            const tailChars = budget - headChars;
            const head = text.slice(0, headChars);
            const tail = tailChars > 0 ? text.slice(-tailChars) : "";
            const spilled = spillOutput(text);
            const spillNote =
              "path" in spilled
                ? `[full output saved to ${spilled.path} — read it with \`head\`, \`tail\`, or \`rg\`]`
                : `[Could not save the full output: ${spilled.error}]`;
            text =
              `Warning: truncated output (original token estimate: ${Math.ceil(text.length / CHARS_PER_TOKEN)}, budget: ${maxTokens})\n` +
              `Total output lines: ${lines}\n\n${head}…${Math.ceil((text.length - headChars - tailChars) / CHARS_PER_TOKEN)} tokens truncated…${tail}\n\n${spillNote}`;
          }
          output = text.length > 0 ? text : "(script returned nothing)";
        }
        send({ id, output });
        break;
      }

      case "system_prompt":
      case "authorize":
      case "tool_result":
        send({ id, allow: true, text: "", body: req.body });
        break;

      case "turn_end":
        break;

      case "goodbye":
        return;

      default:
        break;
    }
  }
}

main();
