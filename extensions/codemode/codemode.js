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

const PROTOCOL = 1;
const MAX_LINE = 8 * 1024 * 1024;

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
function runScript(code) {
  const context = vm.createContext({
    tools: {
      bash: (command) => callHostTool("bash", { command }),
      state: (items) => callHostTool("state", items === undefined ? {} : { items }),
      lines: (s) => String(s).split("\n"),
      grep: (s, re) => String(s).split("\n").filter((l) => re.test(l)),
      unique: (a) => Array.from(new Set(a)),
      count: (a) => a.length,
      join: (a, sep) => a.join(sep === undefined ? "\n" : sep),
    },
    console: { log: () => {}, error: () => {} },
  });

  const script = new vm.Script("(async () => {\n" + code + "\n})()", {
    filename: "codemode",
  });
  // `runInContext` returns the promise the async wrapper created. Every tool
  // call inside the script blocks on the host synchronously, so the promise
  // has already settled by the time we get it; awaiting it is what recovers
  // the script's actual return value.
  return script.runInContext(context, { timeout: 10_000 });
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
            "`tools.grep(s, /re/)`, `tools.unique`, `tools.count`, `tools.join`. Only the " +
            "value you return reaches you, so intermediate tool output never enters " +
            "context. Use it when a task would otherwise need many bash calls or would " +
            "dump large output into context. Do not use it for a single command.",
          tools: [
            {
              name: "codemode",
              description:
                "Run a JS program that orchestrates tools. `await tools.bash(cmd)` returns " +
                "the output as a string. Helpers: tools.lines, tools.grep, tools.unique, " +
                "tools.count, tools.join. Return the summary worth keeping. Use instead of " +
                "many sequential bash calls.",
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
        send({ id, output: "codemode 1.0.0 (node " + process.version + ")" });
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

        // Settle synchronously. The main loop is about to block in readLine(),
        // and a pending promise would never get its turn, so the microtask
        // queue has to be drained here with Node's internal tick hook.
        let settled = false;
        let value;
        let failure = null;
        runScript(code).then(
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

        if (!settled) {
          send({ id, output: "codemode error: script did not settle (await something the sandbox cannot provide?)" });
        } else if (failure) {
          send({ id, output: "codemode error: " + (failure.message || String(failure)) });
        } else {
          const out =
            value === undefined
              ? "(script returned nothing)"
              : typeof value === "string"
                ? value
                : JSON.stringify(value, null, 2);
          send({ id, output: out });
        }
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
