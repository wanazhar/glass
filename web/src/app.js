import { marked } from "marked";
import DOMPurify from "dompurify";
import { draftStore } from "./drafts.js";
import { attention } from "./notifications.js";
const $ = (s) => document.querySelector(s),
  el = (tag, cls, text) => {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  };
const view = {
  id: null,
  threads: [],
  projectFilter: "",
  projectExpanded: new Set(),
  nextThreads: null,
  threadIndex: new Map(),
  projectOptions: [],
  totalThreads: 0,
  before: 0,
  source: null,
  active: false,
  control: false,
  follow: true,
  reasoning: "",
  reasoningBefore: 0,
  tools: new Map(),
  group: null,
  stream: null,
  streamText: "",
  frame: 0,
  interaction: null,
  tab: "activity",
  pending: new Map(),
  generation: 0,
  streamTruncated: false,
  reasoningFollow: true,
  reasoningWindows: [],
  reasoningSources: [],
  directory: null,
  commandTransportReady: false,
};
const notifyAttention = attention($("#notifications"));
let initialOpening = true;
const limits = { message: 262144, reasoning: 262144, preview: 1048576 };
const text = (s, max = limits.message) => String(s ?? "").slice(0, max),
  escape = (s) =>
    String(s).replace(
      /[&<>"']/g,
      (c) =>
        ({
          "&": "&amp;",
          "<": "&lt;",
          ">": "&gt;",
          '"': "&quot;",
          "'": "&#39;",
        })[c],
    );
let noticeTimer;
function notice(message, error = false) {
  clearTimeout(noticeTimer);
  const node = $("#notice");
  node.replaceChildren(el("span", "", message || ""));
  const close = el("button", "notice-close", "×");
  close.type = "button";
  close.setAttribute("aria-label", "Dismiss notice");
  close.onclick = clearNotice;
  node.append(close);
  node.className = "notice" + (error ? " error" : "");
  noticeTimer = setTimeout(clearNotice, error ? 15000 : 6000);
}
function clearNotice() {
  clearTimeout(noticeTimer);
  $("#notice").classList.add("hidden");
}
let draftWarning = false;
const drafts = draftStore(() => {
  if (!draftWarning) {
    draftWarning = true;
    queueMicrotask(() =>
      notice(
        "Draft storage unavailable. Keep this tab open until you send.",
        true,
      ),
    );
  }
});
const draftKey = () => location.host + "/" + (view.id || "new");
function saveDraft() {
  const value = $("#prompt").value;
  drafts.write(draftKey(), value.trimStart().startsWith("/") ? "" : value);
}
function restoreDraft() {
  $("#prompt").value = drafts.read(draftKey());
  $("#prompt").style.height = "";
}
window.addEventListener("pagehide", () => {
  saveDraft();
  drafts.flush();
});
async function api(path, data, signal) {
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal?.addEventListener("abort", abort, { once: true });
  if (signal?.aborted) controller.abort();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, 12000);
  try {
    const r = await fetch("/api/" + path, {
      method: data === undefined ? "GET" : "POST",
      headers: data === undefined ? {} : { "Content-Type": "application/json" },
      body: data === undefined ? undefined : JSON.stringify(data),
      signal: controller.signal,
    });
    let value;
    try {
      value = await r.json();
    } catch {
      throw Error("Host connection unavailable");
    }
    if (!r.ok) throw Error(value.error || "Request rejected");
    return value;
  } catch (error) {
    if (timedOut)
      throw Error("Host timed out. Tap the connection indicator to reconnect.");
    throw error;
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", abort);
  }
}
async function send(message) {
  if (!view.id) throw Error("Choose a thread first");
  return api("send", { id: view.id, message });
}
function connection(label, online = true) {
  $("#connection").className = "connection" + (online ? "" : " offline");
  $("#connection").replaceChildren(el("i"), document.createTextNode(label));
}
let activityKey = "";
function updateActivity(label) {
  const key = `${label || ""}|${view.control}|${view.id}|${view.workerWorkflows}|${view.hostWorkflows}`;
  if (key === activityKey) return;
  activityKey = key;
  view.active = Boolean(label);
  $("#activity").classList.toggle("working", view.active);
  $("#activity-text").textContent = label || "Ready";
  $("#stop").classList.toggle("hidden", !view.active);
  $("#send-mode").classList.toggle("hidden", !view.active);
  $("#ownership").textContent = view.control
    ? "Release control"
    : "Take control";
  $("#send").disabled = !view.control && Boolean(view.id);
}
let viewportFrame = 0,
  idleViewportHeight = 0;
function syncViewport() {
  if (viewportFrame) return;
  viewportFrame = requestAnimationFrame(() => {
    viewportFrame = 0;
    const viewport = window.visualViewport;
    const standalone =
      matchMedia("(display-mode: standalone)").matches || navigator.standalone;
    document.documentElement.classList.toggle(
      "standalone",
      Boolean(standalone),
    );
    // Keyboard geometry needs an explicit height; idle standalone uses fixed viewport edges.
    const focused = document.activeElement?.matches(
      "textarea,input:not([type=radio]):not([type=checkbox]):not([type=file]):not([type=button]):not([type=submit]),[contenteditable=true]",
    );
    const keyboard =
      viewport &&
      viewport.scale === 1 &&
      (focused ||
        document.documentElement.classList.contains("keyboard-open")) &&
      Math.max(
        idleViewportHeight,
        document.documentElement.clientHeight,
        window.innerHeight,
      ) -
        viewport.height >
        120;
    if (!focused && viewport?.scale === 1)
      idleViewportHeight = Math.max(idleViewportHeight, viewport.height);
    if (keyboard) {
      document.documentElement.style.setProperty(
        "--app-height",
        `${viewport.height}px`,
      );
    } else {
      document.documentElement.style.removeProperty("--app-height");
    }
    document.documentElement.style.setProperty(
      "--viewport-top",
      keyboard ? `${viewport.offsetTop}px` : "0px",
    );
    document.documentElement.classList.toggle(
      "keyboard-open",
      Boolean(keyboard),
    );
    follow();
  });
}
window.visualViewport?.addEventListener("resize", syncViewport, {
  passive: true,
});
window.visualViewport?.addEventListener("scroll", syncViewport, {
  passive: true,
});
document.addEventListener("focusin", syncViewport);
document.addEventListener("focusout", () => {
  syncViewport();
  setTimeout(syncViewport, 300);
});
window.addEventListener("orientationchange", () => {
  idleViewportHeight = 0;
  syncViewport();
  setTimeout(syncViewport, 300);
});
let resizeEnd;
window.addEventListener(
  "resize",
  () => {
    $(".app").classList.add("resizing");
    clearTimeout(resizeEnd);
    resizeEnd = setTimeout(() => $(".app").classList.remove("resizing"), 200);
    syncViewport();
    syncPanels();
  },
  { passive: true },
);
new ResizeObserver(([entry]) => {
  document.documentElement.style.setProperty(
    "--compose-height",
    `${entry.target.offsetHeight}px`,
  );
  follow();
}).observe($(".compose-zone"));
syncViewport();
function atBottom() {
  const n = $("#conversation");
  return n.scrollHeight - n.scrollTop - n.clientHeight < 90;
}
function follow() {
  if (view.follow)
    $("#conversation").scrollTop = $("#conversation").scrollHeight;
  $("#latest").classList.toggle("hidden", view.follow);
}
let readingWidth = $("#conversation").clientWidth;
new ResizeObserver(() => {
  readingWidth = $("#conversation").clientWidth;
  if (view.follow) follow();
}).observe($("#conversation"));
$("#conversation").addEventListener(
  "scroll",
  () => {
    if ($("#conversation").clientWidth !== readingWidth) return;
    view.follow = !view.tailPruned && atBottom();
    $("#latest").classList.toggle("hidden", view.follow);
  },
  { passive: true },
);
$("#latest").onclick = async () => {
  if (view.tailPruned) {
    view.tailPruned = false;
    await history();
  }
  view.follow = true;
  follow();
};
let codeId = 0;
const renderer = {
  code({ text: source, lang }) {
    const language = (lang || "text").split(/\s/)[0].toLowerCase();
    const labels = {
      js: "JavaScript",
      javascript: "JavaScript",
      ts: "TypeScript",
      typescript: "TypeScript",
      py: "Python",
      python: "Python",
      sh: "Shell",
      bash: "Shell",
      text: "Plain text",
      json: "JSON",
      html: "HTML",
      css: "CSS",
      sql: "SQL",
      diff: "Changes",
    };
    const label = labels[language] || language;
    if (language === "diff" || language === "patch") {
      const id = "code-" + ++codeId;
      const lines = source
        .split("\n")
        .map(
          (line) =>
            `<span class="diff-line ${/^(diff |index |--- |\+\+\+ )/.test(line) ? "file" : line.startsWith("@@") ? "hunk" : line.startsWith("+") ? "add" : line.startsWith("-") ? "remove" : ""}">${escape(line) || " "}</span>`,
        )
        .join("");
      return `<div class="code-block"><div class="block-head"><span>Changes</span><span><button data-copy="${id}">Copy</button><button data-preview="${id}">Expand</button></span></div><pre><code id="${id}" class="diff-code">${lines}</code></pre></div>`;
    }
    const id = "code-" + ++codeId;
    if (language === "mermaid")
      return `<figure class="diagram-block" data-diagram="${id}"><div class="block-head"><span>◇ Mermaid diagram</span><span><button data-render="${id}">Render diagram</button><button data-preview="${id}">Expand</button></span></div><pre><code id="${id}" data-language="mermaid">${escape(source)}</code></pre><div class="diagram-render hidden"></div></figure>`;
    if (
      ["math", "latex", "tex"].includes(language) &&
      !source.includes("\\documentclass")
    )
      return `<div class="math" data-math="display"><code>${escape(source)}</code></div>`;
    return `<div class="code-block"><div class="block-head"><span>${escape(label)}</span><span><button data-copy="${id}">Copy</button><button data-preview="${id}">Expand</button>${["html", "svg"].includes(language) ? `<button data-html="${id}">Preview</button>` : ""}</span></div><pre><code id="${id}" data-language="${escape(language)}">${escape(source)}</code></pre></div>`;
  },
  table(token) {
    return (
      `<figure class="table-block table-wrapped" style="--table-columns:${Math.max(1, token.header.length)}"><div class="block-head"><span>Table</span><span><button data-table="wrap">Compact</button><button data-table="copy">Copy CSV</button><button data-table="expand">Expand</button></span></div><div class="table-wrap">` +
      marked.Renderer.prototype.table.call(this, token) +
      "</div></figure>"
    );
  },
};
marked.use({
  gfm: true,
  breaks: false,
  renderer,
  extensions: [
    {
      name: "displayMath",
      level: "block",
      start: (s) => {
        const i = s.search(/\$\$|\\\[/);
        return i < 0 ? undefined : i;
      },
      tokenizer(s) {
        const m = /^(?:\$\$([\s\S]+?)\$\$|\\\[([\s\S]+?)\\\])(?:\n|$)/.exec(s);
        if (m) return { type: "displayMath", raw: m[0], value: m[1] || m[2] };
      },
      renderer: (t) =>
        `<div class="math" data-math="display"><code>${escape(t.value)}</code></div>`,
    },
    {
      name: "inlineMath",
      level: "inline",
      start: (s) => {
        const i = s.search(/\$|\\\(/);
        return i < 0 ? undefined : i;
      },
      tokenizer(s) {
        const m = /^(?:\$([^$\n]+?)\$(?!\d)|\\\(([^\n]+?)\\\))/.exec(s);
        if (m && !/^\d+(?:[.,]\d+)?(?:\s*[-–]\s*)?$/.test(m[1] || ""))
          return { type: "inlineMath", raw: m[0], value: m[1] || m[2] };
      },
      renderer: (t) =>
        `<span data-math="inline"><code>${escape(t.value)}</code></span>`,
    },
  ],
});
let syntaxPromise, mathPromise, mermaidPromise;
function loadMermaid() {
  return (mermaidPromise ??= import("./mermaid.js").catch((error) => {
    mermaidPromise = undefined;
    throw error;
  }));
}
async function loadMath() {
  if (!mathPromise) {
    mathPromise = import("./math.js");
    const css = document.createElement("link");
    css.rel = "stylesheet";
    css.href = "/math.css";
    document.head.append(css);
  }
  return mathPromise;
}
const observer = new IntersectionObserver(
  (entries) => {
    for (const entry of entries)
      if (entry.isIntersecting) {
        observer.unobserve(entry.target);
        enhance(entry.target);
      }
  },
  { rootMargin: "250px" },
);
const markdownStates = new WeakMap(),
  markdownContexts = new WeakMap(),
  codeSources = new WeakMap();
function prepareBlock(node, streaming, context) {
  if (context?.workspace) node.dataset.assetWorkspace = context.workspace;
  if (context?.base) node.dataset.assetBase = context.base;
  if (context?.id) node.dataset.assetId = context.id;
  for (const n of node.querySelectorAll("[data-math]")) observer.observe(n);
  if (!streaming)
    for (const n of node.querySelectorAll("code[data-language]"))
      observer.observe(n);
  for (const a of node.querySelectorAll("a")) {
    a.rel = "noopener noreferrer";
    a.target = "_blank";
  }
  for (const img of node.querySelectorAll("img")) {
    if (img.dataset.assetPrepared) continue;
    img.dataset.assetPrepared = "true";
    img.loading = "lazy";
    img.decoding = "async";
    const src = img.getAttribute("src") || "";
    if (src && !src.startsWith("data:") && !src.startsWith("blob:")) {
      if (
        !/^(?:https?:|\/\/)/i.test(src) &&
        (context?.workspace || context?.id || view.id)
      ) {
        const path =
          !src.startsWith("/") && context?.base
            ? context.base + "/" + src
            : src;
        img.src =
          "/api/file?" +
          (context?.workspace
            ? "workspace=" + encodeURIComponent(context.workspace)
            : "id=" + encodeURIComponent(context?.id || view.id)) +
          "&path=" +
          encodeURIComponent(path);
      } else {
        img.removeAttribute("src");
        img.alt = "External image · open its link to view";
      }
    }
  }
}
function markdown(node, source, streaming = false) {
  const value = text(source);
  let state = markdownStates.get(node);
  if (state?.blocks.some((block) => block.node.parentNode !== node))
    state = null;
  if (!state) {
    state = { blocks: [], links: "", source: "" };
    markdownStates.set(node, state);
  }
  const truncated =
    source.length > limits.message || (streaming && view.streamTruncated);
  if (
    state.source === source &&
    state.streaming === streaming &&
    Boolean(state.truncated) === truncated
  )
    return;
  state.source = source;
  state.streaming = streaming;
  const tokens = marked.lexer(value);
  const links = JSON.stringify(tokens.links);
  const blocks = tokens.filter((token) => token.type !== "space");
  for (let i = 0; i < blocks.length; i++) {
    const token = blocks[i],
      cached = state.blocks[i];
    if (
      cached &&
      cached.raw === token.raw &&
      cached.type === token.type &&
      state.links === links
    ) {
      if ((!streaming || i < blocks.length - 1) && !cached.settled) {
        prepareBlock(cached.node, false, markdownContexts.get(node));
        cached.settled = true;
      }
      continue;
    }
    const list = [token];
    list.links = tokens.links;
    const block = el("div", "markdown-block");
    block.innerHTML = DOMPurify.sanitize(marked.parser(list), {
      ADD_ATTR: [
        "data-language",
        "data-copy",
        "data-preview",
        "data-render",
        "data-diagram",
        "data-math",
        "data-html",
        "id",
      ],
      FORBID_TAGS: ["style", "iframe", "object", "embed", "form", "input"],
      ALLOW_DATA_ATTR: true,
    });
    const settled = !streaming || i < blocks.length - 1;
    prepareBlock(block, !settled, markdownContexts.get(node));
    const diff = block.querySelector(".diff-code");
    if (diff) codeSources.set(diff, token.text);
    if (cached) cached.node.replaceWith(block);
    else node.append(block);
    state.blocks[i] = {
      raw: token.raw,
      type: token.type,
      node: block,
      settled,
    };
  }
  for (const stale of state.blocks.splice(blocks.length)) stale.node.remove();
  state.links = links;
  let cap = node.querySelector(":scope > .display-cap");
  state.truncated ||=
    source.length > limits.message || (streaming && view.streamTruncated);
  if (state.truncated) {
    if (!cap)
      node.append(
        el(
          "p",
          "notice display-cap",
          "Preview limit reached · full response saved on host",
        ),
      );
  } else cap?.remove();
}
function finishStream() {
  if (!view.stream) return;
  const node = view.stream;
  view.stream = null;
  markdown(node, view.streamText, false);
  node.closest("article")?.classList.remove("streaming");
}

async function enhance(n) {
  if (n.dataset.enhanced) return;
  n.dataset.enhanced = "true";
  try {
    if (n.dataset.language === "mermaid") {
      if (n.textContent.length <= 16000) {
        const button = n.closest("figure")?.querySelector("[data-render]");
        if (button && !button.disabled) await renderDiagram(button, n.id);
      }
    } else if (n.dataset.math) {
      const source = n.textContent;
      const { math } = await loadMath();
      if (n.isConnected)
        n.innerHTML = DOMPurify.sanitize(
          math(source, n.dataset.math === "display"),
          { USE_PROFILES: { html: true, mathMl: true, svg: true } },
        );
    } else if (n.dataset.language && n.dataset.language !== "mermaid") {
      const source = n.textContent;
      const { highlight } = await (syntaxPromise ??= import("./syntax.js"));
      const html = highlight(source, n.dataset.language);
      if (html && n.isConnected) n.innerHTML = html;
    }
  } catch {
    delete n.dataset.enhanced;
    /* Source stays readable when an optional renderer fails. */
  }
}
function message(role, source, target = $("#timeline")) {
  view.group = null;
  finishStream();
  const retained = view.reuseMessages?.get(role + "\0" + text(source))?.shift();
  if (retained) {
    const body = retained.querySelector(".prose");
    if (role !== "user") {
      const state = markdownStates.get(body);
      if (state && text(state.source) === text(source)) state.source = source;
      else markdown(body, source);
    }
    delete retained.dataset.recordIndex;
    target.append(retained);
    return body;
  }
  const article = el("article", "message " + role);
  article.append(el("div", "message-meta", role === "user" ? "You" : "Glass"));
  const body = el("div", "prose");
  if (role === "user") {
    body.textContent = source;
    body.style.whiteSpace = "pre-wrap";
  } else {
    markdownContexts.set(body, { id: view.id });
    markdown(body, source);
  }
  article.append(body);
  const nativeActions = el("div", "message-actions native-message-actions");
  for (const [label, action] of role === "user"
    ? [
        ["Edit in branch", "edit"],
        ["Branch", "branch"],
        ["Quote", "quote"],
      ]
    : [
        ["Quote", "quote"],
        ["Branch", "branch"],
      ]) {
    const button = el("button", "", label);
    button.type = "button";
    button.onclick = async () => {
      if (!article.dataset.recordIndex)
        return notice("Wait for this message to be saved on the host.");
      if (!view.control)
        return notice("Take control before changing this conversation.");
      if (!view.hostWorkflows || !view.workerWorkflows)
        return notice(
          "This thread runtime is incompatible with these controls. The host backend needs an update.",
          true,
        );
      const id = view.id,
        generation = view.generation;
      if (article.dataset.actionPending) return;
      article.dataset.actionPending = "true";
      button.disabled = true;
      try {
        const location = await api(
          "activity-location?id=" +
            encodeURIComponent(id) +
            "&recordOffset=" +
            article.dataset.recordIndex,
        );
        if (generation !== view.generation) return;
        if (
          action === "edit" &&
          !confirm(
            "Create a branch before this prompt? Workspace files stay as they are. Edit the draft and send when ready.",
          )
        )
          return;
        const result = await commandTransport(
          action === "quote" ? "quote" : "branch",
          {
            args: String(
              action === "quote"
                ? location.record
                : action === "edit"
                  ? Math.max(0, location.turn - 1)
                  : location.turn,
            ),
            source_record: location.record,
            source_expected: location.revision,
            before: action === "edit",
          },
        );
        if (generation !== view.generation) return;
        if (result.body?.open_thread) {
          await loadThreads();
          if (generation !== view.generation) return;
          await openThread(result.body.open_thread);
          if (action === "edit") {
            $("#prompt").value = source;
            saveDraft();
            $("#prompt").focus({ preventScroll: true });
          }
        } else notice("Quoted message attached to future requests.");
      } catch (e) {
        if (generation === view.generation) notice(e.message, true);
      } finally {
        delete article.dataset.actionPending;
        button.disabled = false;
      }
    };
    nativeActions.append(button);
  }
  article.append(nativeActions);
  if (role !== "user") {
    const actions = el("div", "message-actions");
    const copy = el("button", "", "Copy");
    copy.onclick = () => copyText(markdownStates.get(body)?.source || source);
    actions.append(copy);
    article.append(actions);
  }
  target.append(article);
  return body;
}
function group(target = $("#timeline")) {
  if (!view.group) {
    view.group = el("details", "work-group");
    const summary = el("summary", "", "Working");
    view.group.append(summary);
    target.append(view.group);
  }
  return view.group;
}
function tool(id, name, command = "", target = $("#timeline")) {
  if (id && view.tools.has(id)) return view.tools.get(id);
  finishStream();
  const parent = group(target);
  const row = el("details", "tool-row running");
  const summary = el("summary");
  summary.append(
    el("span", "tool-icon", "◇"),
    el("span", "tool-label", name || "Tool"),
    el("span", "tool-description", command),
  );
  row.append(summary);
  const output = el("pre", "tool-output", command || "Waiting for output…");
  row.append(output);
  const actions = el("div", "tool-actions");
  const expand = el("button", "", "Expand output");
  expand.onclick = () =>
    preview(
      name || "Tool output",
      "Tool output",
      text(output.textContent, limits.preview),
      "text",
    );
  const copy = el("button", "", "Copy");
  copy.onclick = () => copyText(output.textContent);
  actions.append(expand, copy);
  row.append(actions);
  parent.append(row);
  parent.firstChild.textContent = `${parent.children.length - 1} tool ${parent.children.length === 2 ? "call" : "calls"}`;
  const item = { row, output, name, command };
  if (id) view.tools.set(id, item);
  return item;
}
function toolStatus(t, failed = false) {
  t.row.classList.remove("running");
  t.row.classList.add("completed");
  t.row.classList.toggle("failed", failed);
  t.row.querySelector(".tool-icon").textContent = failed ? "×" : "✓";
}
function record(r, target) {
  switch (r.k) {
    case "user":
      message("user", r.t || "", target);
      break;
    case "assistant":
      message("assistant", r.t || "", target);
      break;
    case "reasoning":
      view.reasoning = (view.reasoning + (r.t || "")).slice(-limits.reasoning);
      break;
    case "call":
    case "tool_call": {
      let args;
      try {
        args = JSON.parse(r.args || r.arguments || "{}");
      } catch {
        args = {};
      }
      tool(r.id, r.name, args.command || args.path || "", target);
      break;
    }
    case "bash": {
      const t = tool(r.id, "bash", r.cmd, target);
      t.output.textContent = text("$ " + r.cmd + "\n" + r.out, limits.preview);
      toolStatus(t, r.code !== 0 && r.code != null);
      break;
    }
    case "tool": {
      const t = tool(r.id, r.name, "", target);
      t.output.textContent = text(r.t, limits.preview);
      toolStatus(t, /^error:/i.test(r.t || ""));
      break;
    }
    case "meta":
      if (/error:|failed:|stopped:/.test(r.t || "")) {
        const n = el("div", "notice error", text(r.t, 8000));
        target.append(n);
      }
      break;
    case "custom":
      if (r.c === "state/todo") {
        try {
          view.tasks = JSON.parse(r.d || "{}").tasks || [];
        } catch {}
      }
      break;
    case "state":
      view.tasks = r.items || [];
      break;
    case "image": {
      const img = el("img");
      if (/^image\/(png|jpeg|gif|webp)$/.test(r.media_type || r.mime || "")) {
        img.src = "data:" + (r.media_type || r.mime) + ";base64," + r.data;
        img.alt = "Thread attachment";
        img.className = "attachment";
        target.append(img);
      }
      break;
    }
  }
}
let historyQueue = Promise.resolve();
function history(older = false) {
  const generation = view.generation;
  if (older) view.follow = false;
  const next = historyQueue
    .catch(() => {})
    .then(() =>
      generation === view.generation ? historyNow(older) : undefined,
    );
  historyQueue = next;
  return next;
}
async function historyNow(older = false) {
  const id = view.id,
    generation = view.generation;
  if (!id) return;
  const h = await api(
    "history?id=" +
      encodeURIComponent(id) +
      (older ? "&before=" + view.before : ""),
  );
  if (generation !== view.generation) return;
  if (older) view.follow = false;
  const scroller = $("#conversation");
  const previousTop = scroller.scrollTop;
  const viewportTop = scroller.getBoundingClientRect().top;
  const anchor = [...$("#timeline").children].find(
    (n) => n.getBoundingClientRect().bottom > viewportTop,
  );
  const anchorKey = anchor?.dataset.recordIndex;
  const anchorTop = anchor?.getBoundingClientRect().top;
  const expanded = new Set(
    (older ? [] : [...view.tools.entries()])
      .filter(([, t]) => t.row.open)
      .map(([id]) => id),
  );
  const expandedGroups = new Set(
    (older
      ? []
      : [...document.querySelectorAll("#timeline .work-group[open]")]
    ).map((n) => n.dataset.recordIndex),
  );
  if (!older) {
    view.tools.clear();
    view.reasoning = "";
  }
  const fragment = older
    ? document.createDocumentFragment()
    : {
        nodes: [],
        append(node) {
          this.nodes.push(node);
        },
        get lastElementChild() {
          return this.nodes.at(-1);
        },
      };
  if (!older) {
    view.reuseMessages = new Map();
    for (const node of $("#timeline").querySelectorAll(":scope > .message")) {
      const role = node.classList.contains("user") ? "user" : "assistant";
      const body = node.querySelector(".prose");
      const key =
        role +
        "\0" +
        text(
          role === "user"
            ? body.textContent
            : markdownStates.get(body)?.source || "",
        );
      if (!view.reuseMessages.has(key)) view.reuseMessages.set(key, []);
      view.reuseMessages.get(key).push(node);
    }
  }
  const savedGroup = view.group;
  const liveStream = view.stream;
  const savedReasoning = view.reasoning;
  const savedTasks = view.tasks;
  if (older) view.stream = null;
  view.group = null;
  let visibleRecords = 0;
  for (const item of h.records) {
    if (
      older &&
      (item.record.k === "reasoning" ||
        item.record.k === "state" ||
        (item.record.k === "custom" && item.record.c === "state/todo"))
    )
      continue;
    const previousNode = fragment.lastElementChild;
    record(item.record, fragment);
    if (fragment.lastElementChild !== previousNode) visibleRecords++;
    const node = fragment.lastElementChild;
    if (node && !node.dataset.recordIndex)
      node.dataset.recordIndex = String(item.index);
  }
  for (const omitted of h.omitted || []) {
    const card = el("div", "detail-card large-activity");
    card.dataset.recordIndex = String(omitted.recordOffset);
    card.append(
      el("p", "quiet", "Large saved activity · " + bytes(omitted.bytes)),
    );
    const read = el(
      "button",
      "quiet",
      omitted.readable ? "Read large activity" : "Use export for this activity",
    );
    read.disabled = !omitted.readable;
    read.onclick = async () => {
      await openWorkflow("largeRecord", omitted.recordOffset);
    };
    card.append(read);
    fragment.append(card);
    visibleRecords++;
  }
  view.group = null;
  if (!older) {
    view.reuseMessages = null;
    const timeline = $("#timeline"),
      keep = new Set(fragment.nodes);
    for (const node of [...timeline.children])
      if (!keep.has(node)) node.remove();
    for (let i = 0; i < fragment.nodes.length; i++) {
      if (timeline.children[i] !== fragment.nodes[i])
        timeline.insertBefore(fragment.nodes[i], timeline.children[i] || null);
    }
  } else {
    $("#timeline").prepend(fragment);
    view.group = savedGroup;
    view.stream = liveStream;
    view.reasoning = savedReasoning;
    view.tasks = savedTasks;
  }
  const retainedTimeline = $("#timeline");
  if (older && retainedTimeline.children.length > 240) {
    while (retainedTimeline.children.length > 240)
      retainedTimeline.lastElementChild.remove();
    view.tailPruned = true;
    $("#latest").classList.remove("hidden");
    for (const [id, t] of view.tools)
      if (!t.row.isConnected) view.tools.delete(id);
  }
  if (h.omittedLargeRecord)
    notice(
      "A record exceeds the 512 KiB history window. Load earlier activity or use gc export for full source.",
    );
  if (!older)
    for (const [id, t] of view.tools) if (expanded.has(id)) t.row.open = true;
  for (const group of older
    ? []
    : $("#timeline").querySelectorAll(".work-group"))
    if (expandedGroups.has(group.dataset.recordIndex)) group.open = true;
  if (older || !view.follow) {
    const restored = older
      ? anchor
      : anchorKey
        ? [...$("#timeline").children].find(
            (n) => n.dataset.recordIndex === anchorKey,
          )
        : null;
    scroller.scrollTop =
      restored && anchorTop !== undefined
        ? previousTop + restored.getBoundingClientRect().top - anchorTop
        : previousTop;
  }
  view.before = h.before;
  if (!older) {
    view.reasoningBefore = h.before;
    view.reasoningSources = h.records
      .filter((item) => item.record.k === "reasoning")
      .map((item) => item.index)
      .concat(
        (h.omitted || [])
          .filter((item) => item.readable)
          .map((item) => item.recordOffset),
      )
      .slice(-64);
  }
  $("#older").classList.toggle("hidden", h.before === 0);
  details();
  return visibleRecords;
}
let earlierRequestGeneration = null;
$("#older").onclick = async () => {
  const button = $("#older"),
    generation = view.generation;
  if (earlierRequestGeneration === generation) return;
  earlierRequestGeneration = generation;
  button.disabled = true;
  button.textContent = "Loading earlier activity…";
  button.setAttribute("aria-busy", "true");
  // Paint feedback before parsing and laying out the next page.
  await new Promise((resolve) =>
    requestAnimationFrame(() => setTimeout(resolve, 0)),
  );
  try {
    if (generation !== view.generation) return;
    for (let page = 0; page < 4; page++) {
      const rendered = await history(true);
      if (generation !== view.generation || rendered || !view.before) break;
    }
  } catch (e) {
    if (generation === view.generation) notice(e.message, true);
  } finally {
    if (earlierRequestGeneration === generation) {
      earlierRequestGeneration = null;
      button.disabled = false;
      button.textContent = "↑ Load earlier activity";
      button.removeAttribute("aria-busy");
    }
  }
};
function schedule() {
  if (view.frame) return;
  view.frame = setTimeout(
    () => {
      view.frame = 0;
      requestAnimationFrame(() => {
        if (view.stream) markdown(view.stream, view.streamText, true);
        if (view.tab === "reasoning") details();
        follow();
      });
    },
    view.streamText.length > 196608
      ? 250
      : view.streamText.length > 65536
        ? 160
        : 80,
  );
}
function event(m) {
  switch (m.t) {
    case "command_start":
      updateActivity(`Working · /${m.name}`);
      break;
    case "command_done":
      updateActivity(null);
      break;
    case "command_queued":
      if (commandRequests.has(m.request_id))
        $("#command-status").textContent =
          "Waiting for the current turn to finish. This command will run once.";
      break;
    case "command_result": {
      const pending = commandRequests.get(m.request_id);
      if (pending) {
        commandRequests.delete(m.request_id);
        clearTimeout(pending.timer);
        if (m.error) pending.reject(new Error(m.error));
        else {
          pending.resolve(m.result);
          if (pending.name === "todo") history().catch(() => {});
        }
      }
      break;
    }
    case "command_catalogue":
      if (Array.isArray(m.commands)) {
        commands = m.commands;
        view.commandTransportReady = true;
      }
      break;
    case "replay":
      view.workerWorkflows = m.web_workflows === 1;
      if (Array.isArray(m.commands)) {
        commands = m.commands;
        view.commandTransportReady = true;
      }
      view.control = Boolean(m.can_write);
      updateActivity(view.active ? "Working" : null);
      break;
    case "control":
      // Ownership updates need not repeat the runtime capabilities from replay.
      if (Object.hasOwn(m, "web_workflows"))
        view.workerWorkflows = m.web_workflows === 1;
      view.control = Boolean(m.can_write);
      updateActivity(view.active ? "Working" : null);
      if (!view.control)
        notice(
          "Another client owns input. Release control there, then take control here.",
        );
      else clearNotice();
      break;
    case "turn_start":
      view.active = true;
      view.group = null;
      view.stream = null;
      view.streamText = "";
      view.streamTruncated = false;
      if (view.pending.has(m.request_id)) {
        const pending = view.pending.get(m.request_id);
        pending.status = "started";
        pending.row?.classList.remove("pending");
        view.pending.delete(m.request_id);
        if (!pending.shown && !m.already_recorded)
          message("user", pending.text);
      } else if (m.text) message("user", m.text);
      updateActivity("Working");
      break;
    case "queued":
      if (view.pending.has(m.request_id))
        view.pending.get(m.request_id).status = "saved";
      notice(
        "Follow-up saved on the host. It will run after the current turn.",
      );
      break;
    case "duplicate":
      if (m.request_id) view.pending.delete(m.request_id);
      notice(
        m.message || "This request is already recorded; it was not run twice.",
      );
      break;
    case "steering":
      if (view.pending.has(m.request_id))
        view.pending.get(m.request_id).status = "saved";
      notice(
        "Applying your correction before further actions." +
          (view.pending.has(m.request_id)
            ? "\n" + view.pending.get(m.request_id).text.slice(0, 5000)
            : ""),
      );
      break;
    case "waiting":
      updateActivity("Waiting for provider");
      break;
    case "text":
      clearNotice();
      if (!view.stream) {
        view.stream = message("assistant", "");
        view.streamText = "";
        view.streamTruncated = false;
      }
      view.stream.closest("article").classList.add("streaming");
      const appended = text(view.streamText + (m.delta || ""));
      const changed = appended !== view.streamText;
      const capped =
        view.streamText.length + (m.delta || "").length > limits.message;
      const showCap = capped && !view.streamTruncated;
      view.streamTruncated ||= capped;
      view.streamText = appended;
      updateActivity("Writing");
      if (changed || showCap) schedule();
      break;
    case "reasoning":
      {
        if (view.reasoningWindows.length) {
          updateActivity("Reasoning");
          break;
        }
        const next = (view.reasoning + (m.delta || "")).slice(
          -limits.reasoning,
        );
        const changed = next !== view.reasoning;
        view.reasoning = next;
        updateActivity("Reasoning");
        if (changed) schedule();
      }

      break;
    case "tool":
      tool(m.call_id, m.name, m.command || "");
      updateActivity(
        m.name === "bash" ? "Running command" : "Using " + (m.name || "tool"),
      );
      follow();
      break;
    case "tool_end": {
      const t = tool(m.call_id, "tool");
      t.output.textContent =
        text(
          (t.command ? "$ " + t.command + "\n" : "") +
            (m.output || m.summary || ""),
          limits.preview,
        ) +
        (m.truncated
          ? "\nPreview capped · full output remains on the host."
          : "");
      toolStatus(t, /failed|error:|exit [1-9]/i.test(m.summary || ""));
      details();
      follow();
      break;
    }
    case "interaction":
      notifyAttention("Glass needs your answer", view.id);
      view.interaction = m;
      interaction();
      updateActivity("Waiting for your decision");
      break;
    case "interaction_done":
      view.interaction = null;
      interaction();
      break;
    case "provider_error":
    case "error":
      if (m.request_id && commandRequests.has(m.request_id)) {
        const pending = commandRequests.get(m.request_id);
        clearTimeout(pending.timer);
        commandRequests.delete(m.request_id);
        pending.reject(new Error(m.message || "Command rejected"));
        break;
      }
      if (m.settled) {
        notifyAttention("Glass needs attention", view.id);
        view.interaction = null;
        interaction();
        updateActivity(null);
      }
      notice(m.message || "The operation failed", true);
      if (m.t === "provider_error" || m.settled) {
        const retry = el("button", "quiet", "Retry provider request");
        retry.onclick = async () => {
          if (
            !confirm(
              "Retry the failed provider request? This may consume model tokens.",
            )
          )
            return;
          retry.disabled = true;
          try {
            await commandTransport("retry", {});
          } catch (e) {
            notice(e.message, true);
          } finally {
            retry.disabled = false;
          }
        };
        $("#notice").append(retry);
      }
      if (m.request_id && view.pending.has(m.request_id)) {
        const pending = view.pending.get(m.request_id);
        pending.status = "rejected";
        if (pending.row?.classList.contains("pending")) pending.row.remove();
        view.pending.delete(m.request_id);
        if (!$("#prompt").value.trim()) $("#prompt").value = pending.text;
      }
      break;
    case "retrying":
    case "provider_notice":
      notice(m.message || `Provider cooldown · retry ${m.attempt}`);
      break;
    case "disconnected":
      for (const [id, pending] of commandRequests) {
        clearTimeout(pending.timer);
        pending.reject(
          new Error(
            "The host disconnected. The command may have completed; inspect its state before retrying.",
          ),
        );
        commandRequests.delete(id);
      }
      connection("Disconnected", false);
      notice(m.message, true);
      break;
    case "done":
      notifyAttention("Glass finished working", view.id);
      clearNotice();
      finishStream();
      view.streamText = "";
      view.group = null;
      view.interaction = null;
      interaction();
      updateActivity(null);
      loadThreads();
      return history()
        .then(() => follow())
        .catch((e) => notice(e.message, true));
  }
}
async function openThread(id, { resume = false } = {}) {
  if (
    view.id !== id &&
    workflowPromise &&
    !(await workflowPromise).prepareLeave()
  )
    return;
  const previousFollow = view.follow;
  if (view.id !== id) saveDraft();
  view.commandTransportReady = false;
  view.workerWorkflows = false;
  if (view.id !== id) workflowPromise?.then((w) => w.close());
  if (view.source) view.source.close();
  view.source = null;
  const generation = ++view.generation;
  $("#older").disabled = false;
  $("#older").textContent = "↑ Load earlier activity";
  $("#older").removeAttribute("aria-busy");
  if (view.id !== id) {
    ++fileVersion;
    fileAbort?.abort();
    view.directory = null;
    const details = $("#detail-content");
    delete details.dataset.tab;
    details.replaceChildren();
  }
  const switched = view.id !== id;
  view.id = id;
  if (switched) {
    restoreDraft();
    $("#context-chips").replaceChildren();
  }
  view.tools.clear();
  view.tailPruned = false;
  view.group = null;
  view.stream = null;
  view.streamText = "";
  view.reasoning = "";
  view.reasoningBefore = 0;
  view.reasoningWindows = [];
  view.reasoningSources = [];
  view.tasks = [];
  view.streamTruncated = false;
  view.interaction = null;
  view.control = false;
  view.follow = resume ? previousFollow : true;
  view.active = false;
  interaction();
  updateActivity(null);
  clearNotice();
  $("#welcome").classList.add("hidden");
  if (!resume) {
    closePanels();
    $("#timeline").replaceChildren(el("div", "loading-surface"));
  } else syncPanels();
  connection("Connecting");
  let thread = threadInfo(id);
  if (!thread) {
    try {
      const found = await api("threads?id=" + encodeURIComponent(id));
      if (generation !== view.generation) return;
      rememberThreads(found.threads || []);
      thread = threadInfo(id);
    } catch (error) {
      if (generation === view.generation) {
        connection("Unavailable", false);
        notice(error.message, true);
      }
      return { ok: false, message: error.message };
    }
  }
  $("#thread-name").textContent = thread?.title || "Conversation";
  $("#workspace-name").textContent = projectName(thread?.workspace || "");
  $("#workspace-name").title = thread?.workspace || "Project was not recorded";
  $("#thread-name").title =
    `${thread?.title || "Conversation"}\n${thread?.workspace || "Unassigned"}`;
  $("#prompt").placeholder =
    thread?.workspace_assigned === false ||
    thread?.workspace_available === false
      ? "Read-only archive · project unavailable"
      : "Message…";
  $("#model").textContent = thread?.model || "Glass Core";
  document.title = (thread?.title || "Conversation") + " · Glass";
  window.history.replaceState(null, "", "/?thread=" + encodeURIComponent(id));
  renderThreads();
  try {
    if (
      thread?.workspace_assigned === false ||
      thread?.workspace_available === false
    ) {
      await history();
      if (generation !== view.generation) return;
      connection("Read-only archive", true);
      $("#timeline .loading-surface")?.remove();
      follow();
      return { ok: true };
    }
    await api("open", { id });
    if (generation !== view.generation) return;
    refreshContext();
    await history();
    if (generation !== view.generation) return;
    follow();
    const source = new EventSource("/api/events?id=" + encodeURIComponent(id));
    view.source = source;
    let eventChain = Promise.resolve();
    let attached;
    const ready = new Promise((resolve) => {
      attached = resolve;
    });
    const readyTimeout = setTimeout(() => attached(), 12000);
    source.onopen = () => {
      if (generation === view.generation) connection("Connected");
    };
    source.onerror = () => {
      if (generation !== view.generation) return;
      connection("Reconnecting", false);
      if (source.readyState === EventSource.CLOSED)
        notice(
          "Connection closed. Tap the connection indicator to reconnect.",
          true,
        );
    };
    source.onmessage = (e) => {
      eventChain = eventChain
        .then(async () => {
          if (generation !== view.generation) return;
          const data = JSON.parse(e.data);
          if (data.reset) {
            view.control = Boolean(data.control?.can_write);
            view.workerWorkflows = data.control?.web_workflows === 1;
            if (Array.isArray(data.control?.commands)) {
              commands = data.control.commands;
              view.commandTransportReady = true;
            }
            view.interaction = data.interaction;
            await history();
            if (generation !== view.generation) return;
            for (const m of data.live || []) {
              if (m.t === "tool" && view.tools.has(m.call_id)) continue;
              await event(m);
            }
            interaction();
            updateActivity(data.active ? "Working" : null);
          } else for (const m of data.events || []) await event(m);
          follow();
          clearTimeout(readyTimeout);
          attached();
        })
        .catch((error) => {
          clearTimeout(readyTimeout);
          attached();
          notice(error.message, true);
        });
    };
    await ready;
    if (generation === view.generation) return { ok: true };
  } catch (e) {
    if (generation !== view.generation) return;
    $("#timeline .loading-surface")?.remove();
    connection("Unavailable", false);
    notice(e.message, true);
    updateActivity(null);
    return { ok: false, message: e.message };
  }
}
let threadListVersion = 0,
  threadListAbort,
  threadSearchTimer;
function threadInfo(id) {
  return view.threadIndex.get(id) || view.threads.find((t) => t.session === id);
}
function rememberThreads(rows) {
  for (const thread of rows) view.threadIndex.set(thread.session, thread);
  while (view.threadIndex.size > 2048) {
    const key = [...view.threadIndex.keys()].find((id) => id !== view.id);
    if (!key) break;
    view.threadIndex.delete(key);
  }
}
async function loadThreads(more = false) {
  const version = ++threadListVersion;
  threadListAbort?.abort();
  threadListAbort = new AbortController();
  const offset = more ? view.nextThreads : 0;
  const query = new URLSearchParams({
    q: $("#search").value,
    project: view.projectFilter,
  });
  if (offset) query.set("offset", offset);
  let value;
  try {
    value = await api("threads?" + query, undefined, threadListAbort.signal);
  } catch (error) {
    if (version !== threadListVersion || error.name === "AbortError") return;
    throw error;
  }
  if (version !== threadListVersion) return;
  const current = new Map(
    (more ? view.threads : []).map((t) => [t.session, t]),
  );
  for (const thread of value.threads || []) current.set(thread.session, thread);
  view.threads = [...current.values()];
  rememberThreads(value.threads || []);
  view.projectOptions = value.projects || [];
  view.totalThreads = value.total_threads ?? view.threads.length;
  view.nextThreads = value.next_offset ?? null;
  $("#more-threads").classList.toggle("hidden", view.nextThreads === null);
  renderThreads();
}
function requestThreadSearch() {
  clearTimeout(threadSearchTimer);
  renderThreads();
  threadSearchTimer = setTimeout(
    () => loadThreads().catch((error) => notice(error.message, true)),
    150,
  );
}
function projectName(path) {
  return path ? path.split("/").filter(Boolean).pop() || "/" : "Unassigned";
}
function renderThreads() {
  const search = $("#search").value.toLowerCase().trim();
  const container = $("#threads");
  container.replaceChildren();
  const projects = new Map();
  for (const thread of view.threads) {
    const path = thread.workspace || "";
    if (!projects.has(path)) projects.set(path, []);
    projects.get(path).push(thread);
  }
  const scope = $("#project-scope");
  scope.replaceChildren(new Option("All projects", ""));
  const options = view.projectOptions.length
    ? view.projectOptions
    : [...projects].map(([workspace, rows]) => ({
        workspace,
        count: rows.length,
      }));
  for (const { workspace: path, count } of options) {
    const option = new Option(
      `${path ? path.replace(/^\/home\/[^/]+\//, "~/") : "Unassigned"} · ${count}`,
      path || "@unassigned",
    );
    option.title = path || "Project was not recorded";
    scope.append(option);
  }
  if (
    view.projectFilter &&
    ![...scope.options].some((option) => option.value === view.projectFilter)
  )
    scope.append(new Option(`${view.projectFilter} · 0`, view.projectFilter));
  scope.value = view.projectFilter;
  let count = 0;
  for (const [path, rows] of [...projects].sort(
    ([a, ar], [b, br]) =>
      Number(!(a && ar.some((t) => t.workspace_available !== false))) -
        Number(!(b && br.some((t) => t.workspace_available !== false))) ||
      Number(!a) - Number(!b),
  )) {
    if (view.projectFilter && view.projectFilter !== (path || "@unassigned"))
      continue;
    const matches = rows.filter(
      (thread) =>
        !search ||
        search
          .split(/\s+/)
          .every((word) =>
            `${thread.title || ""} ${path} ${thread.session}`
              .toLowerCase()
              .includes(word),
          ),
    );
    if (!matches.length) continue;
    count += matches.length;
    const section = el("details", "project-group");
    section.open =
      !!search ||
      !!view.projectFilter ||
      view.projectExpanded.has(path) ||
      matches.some((t) => t.session === view.id);
    const summary = el("summary", "project-heading");
    const heading = el("div");
    heading.append(
      el("strong", "", projectName(path)),
      el("small", "project-path", path || "Project was not recorded"),
    );
    const disclosure = el("span", "project-disclosure", "›");
    disclosure.setAttribute("aria-hidden", "true");
    summary.append(
      disclosure,
      heading,
      el("span", "project-count", String(matches.length)),
    );
    section.append(summary);
    section.addEventListener("toggle", () => {
      if (section.open) view.projectExpanded.add(path);
      else view.projectExpanded.delete(path);
    });
    const threadGroup = el("div", "project-threads");
    for (const thread of matches) {
      const button = el(
        "button",
        "thread" + (thread.session === view.id ? " active" : ""),
      );
      button.title = `${thread.title || "New conversation"}\n${path || "Unassigned"}\n${thread.session}`;
      button.append(el("strong", "", thread.title || "New conversation"));
      const small = el("small");
      small.append(
        el(
          "span",
          thread.running_process_verified ? "running" : "",
          thread.running_process_verified
            ? "Working"
            : thread.last_status === "completed"
              ? "Completed"
              : thread.workspace_available === false ||
                  thread.workspace_assigned === false
                ? "Archive"
                : "Saved",
        ),
        el("span", "", thread.modified || ""),
      );
      if (thread.running_process_verified) {
        const dots = el("span", "thread-working-dots");
        dots.setAttribute("aria-hidden", "true");
        for (let i = 0; i < 3; i++) dots.append(el("i"));
        small.querySelector(".running").prepend(dots);
      }
      button.append(small);
      button.onclick = () => openThread(thread.session);
      threadGroup.append(button);
    }
    section.append(threadGroup);
    container.append(section);
  }
  $("#thread-count").textContent =
    view.nextThreads !== null
      ? `${count} / ${view.totalThreads}`
      : `${count} threads`;
  if (!container.children.length)
    container.append(el("p", "empty", "No matching threads"));
}
$("#project-scope").onchange = (e) => {
  view.projectFilter = e.target.value;
  requestThreadSearch();
};
$("#more-threads").onclick = async () => {
  try {
    await loadThreads(true);
  } catch (error) {
    notice(error.message, true);
  }
};
$("#search").oninput = requestThreadSearch;
async function newThread(workspace) {
  try {
    const { id } = await api("new", {
      workspace:
        typeof workspace === "string"
          ? workspace
          : (view.projectFilter && view.projectFilter !== "@unassigned"
              ? view.projectFilter
              : null) ||
            threadInfo(view.id)?.workspace ||
            view.workspace,
    });
    await loadThreads();
    await openThread(id);
    $("#prompt").focus({ preventScroll: true });
  } catch (e) {
    notice(e.message, true);
  }
}
$("#new").onclick = newThread;
$("#welcome-new").onclick = newThread;
let submitting = false;
$("#composer").onsubmit = async (e) => {
  e.preventDefault();
  const prompt = $("#prompt"),
    value = prompt.value.trim();
  if (!value || submitting) return;
  if (view.attaching)
    return notice("Wait for attachments to finish before sending.");
  if (value.startsWith("/")) {
    await dispatchSlash(value);
    return;
  }
  submitting = true;
  let receipt, submissionKey, submissionGeneration;
  try {
    if (!view.id) {
      await newThread();
      if (!view.id) return;
    }
    if (!view.control) {
      notice("Take control before sending a message.");
      return;
    }
    submissionKey = draftKey();
    submissionGeneration = view.generation;
    const id =
      crypto.randomUUID?.() ??
      Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) =>
        b.toString(16).padStart(2, "0"),
      ).join("");
    view.lastPrompt = value;
    const queued = view.active;
    const steering = queued && $("#send-mode").dataset.mode !== "queue";
    receipt = { text: value, shown: !queued, status: "sending" };
    view.pending.set(id, receipt);
    clearNotice();
    if (queued)
      notice(
        steering
          ? "Sending correction to the host…"
          : "Sending follow-up to the host…",
      );
    if (!queued) {
      receipt.row = message("user", value).closest("article");
      receipt.row.classList.add("pending");
    }
    prompt.value = "";
    saveDraft();
    prompt.style.height = "";
    if (matchMedia("(pointer:coarse)").matches) prompt.blur();
    syncViewport();
    view.follow = true;
    follow();
    await send({
      t: steering ? "steer" : "prompt",
      text: value,
      request_id: id,
    });
    if (
      receipt.status === "rejected" ||
      submissionGeneration !== view.generation
    )
      return;

    view.follow = true;
    follow();
    if (!queued && receipt.status === "sending") updateActivity("Starting");
  } catch (e) {
    if (submissionKey && !drafts.read(submissionKey))
      drafts.write(submissionKey, value);
    if (submissionGeneration === view.generation && !prompt.value.trim()) {
      prompt.value = value;
      saveDraft();
    }
    receipt?.row?.classList.remove("pending");
    notice(e.message, true);
  } finally {
    submitting = false;
  }
};
$("#prompt").onkeydown = (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    $("#composer").requestSubmit();
  }
};
$("#prompt").oninput = (e) => {
  saveDraft();
  if (e.target.value.trim() === "/") openCommands("", { focusSearch: true });
  e.target.style.height = "auto";
  e.target.style.height = Math.min(e.target.scrollHeight, 180) + "px";
};
$("#stop").onclick = () =>
  send({ t: "cancel" }).catch((e) => notice(e.message, true));
$("#ownership").onclick = () =>
  send({ t: "control", action: view.control ? "release" : "claim" }).catch(
    (e) => notice(e.message, true),
  );
function interaction() {
  const panel = $("#interaction"),
    m = view.interaction;
  panel.replaceChildren();
  panel.classList.toggle("hidden", !m);
  if (!m) return;
  const r = m.request;
  panel.append(
    el(
      "h3",
      "",
      r.kind === "approval"
        ? "Approval required · turn paused"
        : "Your input is needed",
    ),
  );
  if (!view.control) {
    panel.append(el("p", "", "Take control to answer this request."));
    return;
  }
  const submit = async (answer) => {
    try {
      await send({ t: "answer", id: m.id, answer });
      panel.querySelectorAll("button").forEach((b) => (b.disabled = true));
    } catch (e) {
      notice(e.message, true);
    }
  };
  if (r.kind === "approval") {
    panel.append(
      el("p", "", r.reason),
      el("pre", "", JSON.stringify(r.operation, null, 2)),
      el(
        "p",
        "",
        "Allow this exact operation once outside the configured sandbox.",
      ),
    );
    const actions = el("div", "decision-actions");
    const allow = el("button", "primary", "Allow once"),
      deny = el("button", "quiet", "Deny");
    allow.onclick = () => submit({ allow: true });
    deny.onclick = () => submit({ allow: false });
    actions.append(deny, allow);
    panel.append(actions);
  } else {
    const form = el("form");
    for (const [i, q] of (r.request.questions || []).entries()) {
      const field = el("fieldset");
      field.append(el("legend", "", q.question));
      for (const option of q.options || []) {
        const label = el("label"),
          input = el("input");
        input.type = q.multiSelect ? "checkbox" : "radio";
        input.name = "q" + i;
        input.value = option.label;
        const words = el("span", "", option.label);
        if (option.description)
          words.append(el("small", "", option.description));
        label.append(input, words);
        field.append(label);
      }
      const custom = el("textarea");
      custom.name = "custom" + i;
      custom.placeholder = "Or write your own answer…";
      custom.maxLength = 4096;
      field.append(custom);
      form.append(field);
    }
    const actions = el("div", "decision-actions"),
      cancel = el("button", "quiet", "Cancel"),
      answer = el("button", "primary", "Send answer");
    cancel.type = "button";
    cancel.onclick = () => submit({ status: "cancelled" });
    answer.type = "submit";
    actions.append(cancel, answer);
    form.append(actions);
    form.onsubmit = (e) => {
      e.preventDefault();
      const data = new FormData(form);
      const answers = r.request.questions.map((q, i) => {
        const custom = String(data.get("custom" + i) || "").trim();
        return { answers: custom ? [custom] : data.getAll("q" + i) };
      });
      if (answers.some((a) => !a.answers.length)) {
        notice("Answer each question or cancel.");
        return;
      }
      submit({ status: "answered", answers });
    };
    panel.append(form);
  }
}
let reasoningLoading = null;
async function loadEarlierReasoning(automatic = false) {
  const id = view.id,
    generation = view.generation;
  if (!id || !view.reasoningBefore || reasoningLoading === generation) return;
  reasoningLoading = generation;
  details();
  try {
    for (let page = 0; page < 4 && view.reasoningBefore; page++) {
      const before = view.reasoningBefore;
      const data = await api(
        "history?id=" + encodeURIComponent(id) + "&before=" + before,
      );
      if (generation !== view.generation) return;
      if (data.before >= before)
        throw new Error("Reasoning history cursor did not advance");
      view.reasoningBefore = data.before;
      const prefix = data.records
        .filter((item) => item.record.k === "reasoning")
        .map((item) => item.record.t || "")
        .join("");
      if (!prefix) {
        view.reasoningSources = [
          ...(data.omitted || [])
            .filter((item) => item.readable)
            .map((item) => item.recordOffset),
          ...view.reasoningSources,
        ].slice(0, 64);
        details();
        continue;
      }
      const prefixSources = data.records
        .filter((item) => item.record.k === "reasoning")
        .map((item) => item.index)
        .concat(
          (data.omitted || [])
            .filter((item) => item.readable)
            .map((item) => item.recordOffset),
        );
      const pane = $("#detail-content"),
        top = pane.scrollTop,
        height = pane.scrollHeight;
      if (prefix.length + view.reasoning.length > limits.reasoning) {
        view.reasoningWindows.push({
          text: view.reasoning,
          before,
          sources: view.reasoningSources,
        });
        if (view.reasoningWindows.length > 3) view.reasoningWindows.shift();
        view.reasoning = prefix.slice(-limits.reasoning);
      } else view.reasoning = prefix + view.reasoning;
      view.reasoningSources = [
        ...prefixSources,
        ...view.reasoningSources,
      ].slice(0, 64);
      if (view.tab === "reasoning" && (!automatic || view.reasoningFollow)) {
        const following = view.reasoningFollow;
        view.reasoningFollow = true;
        details();
        view.reasoningFollow = following;
        pane.scrollTop = following
          ? pane.scrollHeight
          : top + pane.scrollHeight - height;
      }
      if (!automatic || pane.scrollHeight > pane.clientHeight + 100) break;
    }
  } catch (e) {
    if (generation === view.generation) notice(e.message, true);
  } finally {
    if (reasoningLoading === generation) {
      reasoningLoading = null;
      if (generation === view.generation) {
        const button = $(".reasoning-earlier");
        if (button) {
          button.disabled = !view.reasoningBefore;
          button.textContent = view.reasoningBefore
            ? "↑ Earlier reasoning"
            : "All loaded";
        }
      }
    }
  }
}
let inlineWorkTimer,
  inlineWorkAbort,
  inlineAgents = [],
  inlineAgentThread;
function renderInlineWork(root) {
  root.replaceChildren();
  const tasks = view.tasks || [];
  if (tasks.length)
    root.append(
      el(
        "p",
        "quiet",
        tasks.filter((t) => t.status === "completed").length +
          "/" +
          tasks.length +
          " tasks complete",
      ),
    );
  if (inlineAgentThread === view.id)
    for (const agent of inlineAgents.slice(0, 12)) {
      const row = el(
        "button",
        "inline-agent",
        (agent.name || agent.id) + " · " + (agent.status || "Saved"),
      );
      row.type = "button";
      row.onclick = () => loadAgents();
      root.append(row);
    }
  const manage = el("button", "quiet", "Work & teammates");
  manage.onclick = () => loadAgents();
  root.append(manage);
}
function pollInlineWork() {
  if (inlineWorkAbort || !view.id) return;
  const id = view.id,
    generation = view.generation,
    controller = new AbortController();
  inlineWorkAbort = controller;
  const refresh = async () => {
    if (
      controller.signal.aborted ||
      generation !== view.generation ||
      view.tab !== "activity" ||
      !$("#details").classList.contains("open")
    ) {
      if (inlineWorkAbort === controller) inlineWorkAbort = null;
      return;
    }
    if (document.visibilityState === "visible")
      try {
        const data = await api(
          "agents?id=" + encodeURIComponent(id),
          undefined,
          controller.signal,
        );
        if (controller.signal.aborted || generation !== view.generation) return;
        inlineAgentThread = id;
        inlineAgents = (Array.isArray(data) ? data : data.agents || []).slice(
          0,
          64,
        );
        const root = $(".inline-work");
        if (root && view.tab === "activity") renderInlineWork(root);
      } catch {}
    inlineWorkTimer = setTimeout(refresh, 5000);
  };
  refresh();
}
function details() {
  const n = $("#detail-content");
  if (!$("#details").classList.contains("open")) return;
  $("#reasoning-latest").hidden = view.tab !== "reasoning";
  if (view.tab === "files") {
    const thread = threadInfo(view.id);
    if (
      thread?.workspace_assigned === false ||
      thread?.workspace_available === false
    ) {
      n.dataset.tab = "archive";
      n.replaceChildren(
        el("p", "quiet", "This archive has no available project directory."),
      );
      return;
    }
    if (n.dataset.tab !== "files") browseDirectory();
    return;
  }
  if (view.tab !== "reasoning" || n.dataset.tab !== "reasoning")
    n.replaceChildren();
  n.dataset.tab = view.tab;
  if (view.tab === "reasoning") {
    const body = n.querySelector(".prose") || el("div", "prose");
    const readerTop = n.scrollTop;
    const source = view.reasoning || "No reasoning yet.";
    const changed =
      !reasoningGesture &&
      (body.dataset.reasoningSource !== source ||
        body.dataset.reasoningStreaming !== String(view.active));
    if (changed) {
      markdown(body, source, view.active);
      body.dataset.reasoningSource = source;
      body.dataset.reasoningStreaming = String(view.active);
    }
    if (!body.isConnected) {
      const earlier = el(
        "button",
        "quiet reasoning-earlier",
        "↑ Earlier reasoning",
      );
      earlier.onclick = () => loadEarlierReasoning(false);
      const newer = el("button", "quiet reasoning-newer", "↓ Newer reasoning");
      newer.onclick = () => {
        const window = view.reasoningWindows.pop();
        if (!window) return;
        view.reasoning = window.text;
        view.reasoningBefore = window.before;
        view.reasoningSources = window.sources || [];
        details();
        n.scrollTop = 0;
      };
      const saved = el(
        "button",
        "quiet reasoning-saved",
        "Read full saved reasoning",
      );
      saved.onclick = async () => {
        await openWorkflow("reasoningArchive", view.reasoningSources);
      };
      n.append(earlier, newer, saved, body);
    }
    const earlier = n.querySelector(".reasoning-earlier");
    earlier.disabled =
      !view.reasoningBefore || reasoningLoading === view.generation;
    n.querySelector(".reasoning-saved").hidden =
      !view.reasoningSources.length || !view.hostWorkflows;
    n.querySelector(".reasoning-newer").hidden = !view.reasoningWindows.length;
    earlier.textContent =
      reasoningLoading === view.generation
        ? "Loading reasoning…"
        : view.reasoningBefore
          ? "↑ Earlier reasoning"
          : "All loaded";
    if (changed)
      n.scrollTop = view.reasoningFollow ? n.scrollHeight : readerTop;
  } else if (view.tab === "context") {
    const thread = threadInfo(view.id) || {};
    for (const [name, value] of [
      ["Workspace", thread.workspace],
      ["Model", thread.model],
      ["Thread", view.id],
      [
        "Execution",
        thread.running_process_verified ? "Working on host" : "Saved on host",
      ],
    ]) {
      const card = el("div", "detail-card");
      card.append(el("strong", "", name), el("p", "", value || "—"));
      n.append(card);
    }
    if (view.tasks?.length) {
      const card = el("div", "detail-card");
      card.append(el("strong", "", "Checklist"));
      for (const task of view.tasks)
        card.append(
          el(
            "p",
            "",
            typeof task === "string"
              ? task
              : (task.status === "completed"
                  ? "✓ "
                  : task.status === "in_progress"
                    ? "◆ "
                    : "○ ") + (task.text || task.title || JSON.stringify(task)),
          ),
        );
      n.append(card);
    }
  } else {
    const work = el("section", "inline-work");
    n.append(work);
    renderInlineWork(work);
    pollInlineWork();
    for (const t of [...view.tools.values()].slice(-40).reverse()) {
      const card = el("div", "detail-card");
      card.append(
        el("strong", "", t.name),
        el("p", "", t.command || t.output.textContent.slice(0, 140)),
      );
      const button = el("button", "quiet", "View output");
      button.onclick = () =>
        preview(
          t.name,
          "Tool output",
          text(t.output.textContent, limits.preview),
          "text",
        );
      card.append(button);
      n.append(card);
    }
    if (!n.children.length) n.append(el("p", "", "No tool activity."));
    const agents = el("button", "quiet", "Inspect teammates");
    agents.onclick = () => loadAgents();
    n.append(agents);
  }
}
function syncPanels() {
  const mobile = matchMedia("(max-width:700px)").matches;
  const overlay = matchMedia("(max-width:1100px)").matches;
  const left = mobile && $("#sidebar").classList.contains("open");
  const right = overlay && $("#details").classList.contains("open");
  $("#scrim").classList.toggle("open", left || right);
  $("#sidebar").inert = mobile && !left;
  $("#sidebar").setAttribute("aria-hidden", String(mobile && !left));
  $("#details").inert = !$("#details").classList.contains("open");
  $("#details").setAttribute("aria-hidden", String($("#details").inert));
  $("#menu").setAttribute("aria-expanded", String(left));
  $("#details-toggle").setAttribute(
    "aria-expanded",
    String($("#details").classList.contains("open")),
  );
  $(".workspace").inert = left || right;
}
function closePanels() {
  clearTimeout(inlineWorkTimer);
  inlineWorkAbort?.abort();
  inlineWorkAbort = null;
  $("#sidebar").classList.remove("open");
  $("#details").classList.remove("open");
  syncPanels();
}
function showPanel(panel, open = true, force = false) {
  const target = $(panel),
    wasOpen = target.classList.contains("open");
  closePanels();
  target.classList.toggle("open", open && (force || !wasOpen));
  if (document.activeElement instanceof HTMLElement)
    document.activeElement.blur();
  syncPanels();
  if (panel === "#details" && target.classList.contains("open")) details();
}
$("#details-toggle").onclick = () => {
  if (view.tab === "activity") showPanel("#details");
  else openCommandTab("activity");
};
$("#details-close").onclick = () => {
  closePanels();
  $("#details-toggle").focus();
};
$("#sidebar-close").onclick = () => {
  closePanels();
  $("#menu").focus();
};
// Following resumes only on explicit Latest or when entering the reasoning tab.
// Do not insert controls into the scroll surface during an iOS touch gesture.
function pauseReasoning() {
  if (view.tab === "reasoning") view.reasoningFollow = false;
}
$("#reasoning-latest").onclick = async () => {
  if (view.reasoningWindows.length) {
    view.reasoningWindows = [];
    await history();
  }
  view.reasoningFollow = true;
  details();
  const pane = $("#detail-content");
  pane.scrollTop = pane.scrollHeight;
};
let reasoningGesture = null;
$("#detail-content").addEventListener(
  "touchstart",
  (event) => {
    if (view.tab !== "reasoning") return;
    reasoningGesture = {
      following: view.reasoningFollow,
      top: event.currentTarget.scrollTop,
      y: event.touches[0]?.clientY,
      moved: false,
    };
    pauseReasoning();
  },
  { passive: true },
);
$("#detail-content").addEventListener(
  "touchmove",
  (event) => {
    if (
      reasoningGesture &&
      Math.abs((event.touches[0]?.clientY || 0) - reasoningGesture.y) > 6
    )
      reasoningGesture.moved = true;
  },
  { passive: true },
);
for (const name of ["touchend", "touchcancel"])
  $("#detail-content").addEventListener(
    name,
    (event) => {
      const gesture = reasoningGesture;
      reasoningGesture = null;
      if (
        gesture?.following &&
        !gesture.moved &&
        view.tab === "reasoning" &&
        Math.abs(event.currentTarget.scrollTop - gesture.top) <= 2
      )
        view.reasoningFollow = true;
      if (gesture) details();
    },
    { passive: true },
  );
$("#detail-content").addEventListener(
  "wheel",
  (event) => {
    if (event.deltaY) pauseReasoning();
  },
  { passive: true },
);
$("#detail-content").addEventListener("keydown", (event) => {
  if (
    ["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End", " "].includes(
      event.key,
    )
  )
    pauseReasoning();
});
$("#detail-content").addEventListener(
  "scroll",
  () => {
    const pane = $("#detail-content");
    if (pane.scrollHeight - pane.scrollTop - pane.clientHeight > 2)
      pauseReasoning();
  },
  { passive: true },
);
for (const tab of document.querySelectorAll("[data-tab]"))
  tab.onclick = () => {
    view.tab = tab.dataset.tab;
    if (view.tab === "reasoning") view.reasoningFollow = true;
    document
      .querySelectorAll("[data-tab]")
      .forEach((n) => n.classList.toggle("selected", n === tab));
    details();
    if (view.tab === "reasoning") {
      const pane = $("#detail-content");
      pane.scrollTop = pane.scrollHeight;
      if (pane.scrollHeight <= pane.clientHeight + 100)
        loadEarlierReasoning(true);
    }
  };
$("#menu").onclick = () => showPanel("#sidebar");
$("#scrim").onclick = () => closePanels();
syncPanels();
const systemTheme = matchMedia("(prefers-color-scheme: light)");
let themeMode = "system";
function applyTheme() {
  document.documentElement.classList.add("theme-ready");
  const light =
    themeMode === "system" ? systemTheme.matches : themeMode === "light";
  document.documentElement.classList.toggle("light", light);
  for (const meta of document.querySelectorAll('meta[name="theme-color"]')) {
    meta.content = light ? "#f7f8f3" : "#141916";
    meta.removeAttribute("media");
  }
  const action = light ? "Switch to dark mode" : "Switch to light mode";
  $("#theme").title = action;
  $("#theme").setAttribute("aria-label", action);
}
systemTheme.addEventListener("change", applyTheme);
$("#theme").onclick = () => {
  themeMode = document.documentElement.classList.contains("light")
    ? "dark"
    : "light";
  applyTheme();
};
applyTheme();
document.addEventListener("keydown", (event) => {
  if (event.key === "Tab")
    document.documentElement.classList.add("keyboard-navigation");
});
document.addEventListener(
  "pointerdown",
  () => {
    document.documentElement.classList.remove("keyboard-navigation");
  },
  { passive: true },
);
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "k") {
    e.preventDefault();
    showPanel("#sidebar", true, true);
    $("#search").focus();
  }
  if (e.key === "Escape" && !document.querySelector("dialog[open]")) {
    closePanels();
  }
});
let browseVersion = 0,
  browseAbort;
const sizes = new Map();
function bytes(value) {
  if (value == null) return "—";
  if (value < 1024) return `${value} B`;
  const power = Math.min(4, Math.floor(Math.log(value) / Math.log(1024)));
  return `${(value / 1024 ** power).toFixed(power > 1 ? 1 : 0)} ${["B", "KiB", "MiB", "GiB", "TiB"][power]}`;
}
async function browseDirectory(path, offset = 0) {
  const version = ++browseVersion;
  browseAbort?.abort();
  browseAbort = new AbortController();
  path ??= view.directory || threadInfo(view.id)?.workspace || view.workspace;
  const content = $("#detail-content");
  content.dataset.tab = "files";
  content.replaceChildren(el("div", "loading-surface"));
  try {
    const result = await api(
      `browse?path=${encodeURIComponent(path || "")}&offset=${offset}`,
      undefined,
      browseAbort.signal,
    );
    if (version !== browseVersion || view.tab !== "files") return;
    view.directory = result.path;
    view.directoryResult = result;
    renderDirectory(result);
  } catch (e) {
    if (
      e.name !== "AbortError" &&
      version === browseVersion &&
      view.tab === "files"
    ) {
      if (view.directoryResult) renderDirectory(view.directoryResult);
      else {
        content.replaceChildren();
        const back = el("button", "quiet", "Workspace");
        back.onclick = () => browseDirectory(view.workspace);
        content.append(back);
      }
      content.prepend(el("p", "notice error", e.message));
    }
  }
}
function renderDirectory(result) {
  const content = $("#detail-content");
  content.replaceChildren();
  content.dataset.tab = "files";
  const controls = el("div", "file-controls");
  const up = el("button", "icon-button", "↑");
  up.title = "Parent directory";
  up.setAttribute("aria-label", "Parent directory");
  up.disabled = !result.parent;
  up.onclick = () => browseDirectory(result.parent);
  const home = el("button", "icon-button", "⌂");
  home.title = "Launch workspace";
  home.setAttribute("aria-label", "Launch workspace");
  home.onclick = () => browseDirectory(view.workspace);
  const start = el("button", "quiet file-start", "New thread here");
  start.onclick = () => newThread(result.path);
  controls.append(up, home, start);
  content.append(controls);
  const form = el("form", "file-path");
  const path = el("input");
  path.value = result.path;
  path.setAttribute("aria-label", "Directory path");
  path.autocomplete = "off";
  path.spellcheck = false;
  const go = el("button", "icon-button", "→");
  go.setAttribute("aria-label", "Open directory");
  const pathField = el("div", "file-path-field");
  pathField.append(path);
  form.append(pathField, go);
  form.onsubmit = (e) => {
    e.preventDefault();
    browseDirectory(path.value);
  };
  content.append(form);
  const head = el("div", "file-heading");
  const hidden = el(
    "button",
    "quiet",
    view.showHidden ? "Hide dotfiles" : "Dotfiles",
  );
  hidden.setAttribute("aria-pressed", String(Boolean(view.showHidden)));
  hidden.onclick = () => {
    view.showHidden = !view.showHidden;
    renderDirectory(result);
  };
  const measure = el("button", "quiet", "Measure");
  measure.title = "Measure disk usage · bounded scan";
  measure.onclick = async () => {
    measure.disabled = true;
    try {
      const usage = await api(`usage?path=${encodeURIComponent(result.path)}`);
      if (view.directory === result.path && view.tab === "files")
        measure.textContent = `${usage.partial ? "≥ " : ""}${bytes(usage.bytes)}`;
    } catch (e) {
      notice(e.message, true);
    } finally {
      measure.disabled = false;
    }
  };
  const sort = el(
    "button",
    "quiet",
    view.fileSort === "size" ? "Size ↓" : "Name",
  );
  sort.setAttribute("aria-label", "Sort files");
  sort.onclick = () => {
    view.fileSort = view.fileSort === "size" ? "name" : "size";
    renderDirectory(result);
  };
  head.append(hidden, sort, measure);
  content.append(head);
  const list = el("div", "file-list");
  const entries = result.entries.filter(
    (entry) => view.showHidden || !entry.name.startsWith("."),
  );
  const fullPath = (name) => `${result.path.replace(/\/$/, "")}/${name}`;
  if (view.fileSort === "size")
    entries.sort(
      (a, b) =>
        (sizes.get(fullPath(b.name))?.bytes || b.bytes) -
        (sizes.get(fullPath(a.name))?.bytes || a.bytes),
    );
  const largest = Math.max(
    1,
    ...entries.map(
      (entry) => sizes.get(fullPath(entry.name))?.bytes || entry.bytes,
    ),
  );
  for (const entry of entries) {
    const full = `${result.path.replace(/\/$/, "")}/${entry.name}`;
    const row = el("div", "file-row");
    row.dataset.kind = entry.kind;
    const open = el("button", "file-open");
    open.title = entry.name;
    open.append(
      el(
        "span",
        "file-icon",
        entry.kind === "directory" ? "▱" : entry.kind === "link" ? "↗" : "▤",
      ),
      el("span", "file-name", entry.name),
    );
    open.onclick = async () => {
      if (entry.kind === "directory") return browseDirectory(full);
      if (entry.kind === "link") {
        try {
          const target = await api(`browse?path=${encodeURIComponent(full)}`);
          view.directory = target.path;
          view.directoryResult = target;
          renderDirectory(target);
        } catch {
          return openFile(entry.name, result.path);
        }
      } else return openFile(entry.name, result.path);
    };
    open.disabled = entry.kind === "special";
    const known = sizes.get(full);
    const size = el(
      "button",
      "file-size",
      entry.kind === "directory"
        ? known
          ? `${known.partial ? "≥ " : ""}${bytes(known.bytes)}`
          : "—"
        : bytes(entry.bytes),
    );
    size.title =
      entry.kind === "directory"
        ? "Measure disk usage"
        : `${bytes(entry.size)} logical · ${bytes(entry.bytes)} allocated`;
    size.setAttribute(
      "aria-label",
      entry.kind === "directory" ? `Measure ${entry.name}` : size.title,
    );
    if (entry.kind === "directory")
      size.onclick = async () => {
        size.disabled = true;
        size.textContent = "···";
        try {
          const usage = await api(`usage?path=${encodeURIComponent(full)}`);
          if (sizes.size >= 256) sizes.clear();
          sizes.set(full, usage);
          if (view.directory === result.path && view.tab === "files")
            renderDirectory(result);
        } catch (e) {
          notice(e.message, true);
        } finally {
          size.disabled = false;
        }
      };
    else size.disabled = true;
    const bar = el("span", "file-bar");
    bar.style.width = `${Math.min(100, (100 * (known?.bytes || entry.bytes)) / largest)}%`;
    row.append(bar, open, size);
    list.append(row);
  }
  if (!entries.length)
    list.append(
      el(
        "p",
        "empty",
        result.entries.length ? "Dotfiles hidden" : "Empty directory",
      ),
    );
  content.append(list);
  if (result.next != null) {
    const more = el("button", "quiet", "Next 256");
    more.onclick = () => browseDirectory(result.path, result.next);
    content.append(more);
  }
  if (result.limited)
    content.append(el("small", "file-limit", "Showing a bounded page"));
}
function showFiles() {
  view.tab = "files";
  document
    .querySelectorAll("[data-tab]")
    .forEach((n) => n.classList.toggle("selected", n.dataset.tab === "files"));
  showPanel("#details", true, true);
}
$("#welcome-files").onclick = showFiles;
$("#files-toggle").onclick = showFiles;
let fileVersion = 0,
  fileAbort;
async function openFile(path, workspace) {
  try {
    path = decodeURIComponent(path.split("#")[0]);
  } catch {
    path = path.split("#")[0];
  }
  preview(path, "File", "", "text");
  const version = fileVersion;
  fileAbort = new AbortController();
  $("#preview-body").replaceChildren(el("div", "loading-surface"));
  $("#download").disabled = true;
  try {
    const query = workspace
      ? `workspace=${encodeURIComponent(workspace)}`
      : `id=${encodeURIComponent(view.id)}`;
    const r = await fetch(
      `/api/file?${query}&path=${encodeURIComponent(path)}`,
      { signal: fileAbort.signal },
    );
    if (!r.ok) {
      const error = await r.json();
      throw Error(error.error || "Cannot read file");
    }
    if (version !== fileVersion || !$("#preview").open) return;
    const type = r.headers.get("Content-Type") || "";
    if (type === "application/pdf") {
      const source = await r.arrayBuffer();
      if (version === fileVersion && $("#preview").open)
        preview(path, "PDF", source, "pdf", undefined, true);
    } else if (type.startsWith("image/")) {
      const blob = await r.blob();
      if (version !== fileVersion || !$("#preview").open) return;
      const url = URL.createObjectURL(blob);
      preview(path, "Image", url, "image", undefined, true);
      $("#preview").addEventListener("close", () => URL.revokeObjectURL(url), {
        once: true,
      });
    } else {
      const source = await r.text();
      if (version !== fileVersion || !$("#preview").open) return;
      preview(
        path,
        path.split(".").pop(),
        source,
        /\.(html?|svg)$/i.test(path)
          ? "html"
          : /\.(md|markdown)$/i.test(path)
            ? "markdown"
            : /\.json$/i.test(path)
              ? "json"
              : "text",
        {
          workspace,
          id: view.id,
          base: path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "",
        },
        true,
      );
      if (
        view.id &&
        (!workspace || workspace === threadInfo(view.id)?.workspace)
      ) {
        const actions = el("div", "file-actions");
        const edit = el("button", "quiet", "Edit file"),
          attach = el("button", "quiet", "Attach to context");
        edit.onclick = async () => {
          await openWorkflow("editor", path);
        };
        attach.onclick = async () => {
          try {
            await commandTransport("attach", { args: path });
            notice("File attached to future requests.");
          } catch (e) {
            notice(e.message, true);
          }
        };
        actions.append(edit, attach);
        $("#preview-body").prepend(actions);
      }
    }
  } catch (e) {
    if (e.name !== "AbortError" && version === fileVersion)
      $("#preview-body").replaceChildren(el("p", "notice error", e.message));
  }
}
async function loadAgents() {
  if (view.id) await openWorkflow("work");
}
let previewBlob, previewController;
const previewStack = [];
function preview(
  title,
  kind,
  source,
  format = "text",
  context,
  replace = false,
) {
  const dialog = $("#preview");
  if (dialog.open && !replace) {
    const body = $("#preview-body");
    previewStack.push({
      nodes: [...body.childNodes],
      className: body.className,
      top: body.scrollTop,
      left: body.scrollLeft,
      title: $("#preview-title").textContent,
      kind: $("#preview-kind").textContent,
      blob: previewBlob,
      download: $("#download").onclick,
      disabled: $("#download").disabled,
      context: markdownContexts.get(body),
      markdown: markdownStates.get(body),
    });
    previewBlob = null;
    if (previewStack.length > 8) {
      const oldest = previewStack.shift();
      if (oldest.blob) URL.revokeObjectURL(oldest.blob);
    }
  }
  ++fileVersion;
  fileAbort?.abort();
  previewController?.abort();
  previewController = new AbortController();
  const signal = previewController.signal;
  $("#download").disabled = format === "table";
  $("#preview-title").textContent = title;
  $("#preview-kind").textContent = kind;
  const body = $("#preview-body");
  body.replaceChildren();
  body.className = "";
  if (previewBlob) URL.revokeObjectURL(previewBlob);
  previewBlob =
    format === "image"
      ? source
      : URL.createObjectURL(
          new Blob([source], {
            type:
              format === "pdf"
                ? "application/pdf"
                : format === "html"
                  ? "text/html"
                  : "text/plain",
          }),
        );
  $("#download").onclick = () => {
    const a = el("a");
    a.href = previewBlob;
    a.download = title.replace(/[^a-z0-9._-]/gi, "-");
    a.click();
  };
  if (format === "html") {
    const iframe = el("iframe");
    iframe.setAttribute("sandbox", "");
    iframe.referrerPolicy = "no-referrer";
    iframe.srcdoc =
      "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:\">" +
      source;
    const activate = el(
      "button",
      "quiet artifact-run",
      "Run interactive preview",
    );
    activate.type = "button";
    activate.title =
      "Runs scripts in an isolated frame. External resources, fetch, forms and host access are restricted.";
    activate.onclick = async () => {
      const actionSignal = previewController?.signal;
      if (!view.interactiveArtifacts || !view.id)
        return notice(
          "Interactive previews need the updated gateway and an open thread.",
        );
      activate.disabled = true;
      try {
        const response = await fetch(
          "/api/artifact?id=" + encodeURIComponent(view.id),
          {
            method: "POST",
            headers: { "Content-Type": "text/html; charset=utf-8" },
            body: source,
            signal: actionSignal,
          },
        );
        const result = await response.json();
        if (!response.ok)
          throw Error(result.error || "Cannot prepare interactive preview");
        if (actionSignal?.aborted) return;
        iframe.setAttribute("sandbox", "allow-scripts");
        iframe.removeAttribute("srcdoc");
        iframe.src = result.url;
        activate.textContent = "Interactive · isolated from Glass";
      } catch (e) {
        activate.disabled = false;
        if (!actionSignal?.aborted) notice(e.message, true);
      }
    };
    body.append(activate, iframe);
  } else if (format === "table") {
    // The caller supplies a sanitized DOM table after the modal is opened.
  } else if (format === "diagram") {
    const controls = el("div", "pdf-controls"),
      target = el("div", "diagram-render");
    let zoom = 1;
    for (const [label, factor] of [
      ["−", 0.8],
      ["+", 1.25],
    ]) {
      const button = el("button", "quiet", label);
      button.onclick = () => {
        zoom = Math.max(0.25, Math.min(4, zoom * factor));
        const svg = target.querySelector("svg");
        if (svg) {
          svg.style.maxWidth = "none";
          svg.style.width =
            Number(svg.getAttribute("viewBox")?.split(" ")[2] || 800) * zoom +
            "px";
        }
      };
      controls.append(button);
    }
    body.append(controls, target);
    target.textContent = "Rendering diagram…";
    loadMermaid()
      .then(({ diagram }) => diagram(source))
      .then((svg) => {
        if (signal.aborted) return;
        target.innerHTML = DOMPurify.sanitize(svg, {
          USE_PROFILES: { html: true, svg: true, svgFilters: true },
          ADD_TAGS: ["foreignObject"],
          ADD_ATTR: ["xmlns"],
        });
      })
      .catch(
        (e) =>
          (target.textContent =
            "Source preserved: " + e.message + "\n" + source),
      );
  } else if (format === "pdf") {
    import("./pdf.js")
      .then(({ pdf }) => {
        if (!signal.aborted) return pdf(body, source, signal);
      })
      .catch((e) => {
        if (!signal.aborted) body.append(el("p", "notice", e.message));
      });
  } else if (format === "json") {
    let value;
    try {
      value = JSON.parse(source);
    } catch {
      value = source;
    }
    let nodes = 0;
    function tree(v, key, depth = 0) {
      if (++nodes > 2000 || depth > 20)
        return el("pre", "tool-output", "Preview limit reached");
      if (v && typeof v === "object") {
        const d = el("details", "json-node");
        d.open = depth < 2;
        d.append(
          el(
            "summary",
            "",
            key +
              " " +
              (Array.isArray(v)
                ? "[" + v.length + "]"
                : "{" + Object.keys(v).length + "}"),
          ),
        );
        for (const [k, x] of Object.entries(v).slice(0, 200))
          d.append(tree(x, k, depth + 1));
        return d;
      }
      return el("div", "json-value", key + ": " + JSON.stringify(v));
    }
    body.append(tree(value, "root"));
  } else if (format === "markdown") {
    body.className = "prose";
    markdownContexts.set(body, context || { id: view.id });
    markdown(body, source);
  } else if (format === "image") {
    const img = el("img");
    img.src = source;
    img.alt = title;
    body.append(img);
  } else {
    body.className = "prose";
    const pre = el("pre"),
      code = el("code", "", source);
    code.dataset.language = kind.toLowerCase();
    pre.append(code);
    body.append(pre);
    enhance(code);
  }
  if (!dialog.open) dialog.showModal();
}
function closePreview() {
  if (!previewStack.length) {
    $("#preview").close();
    return;
  }
  ++fileVersion;
  fileAbort?.abort();
  previewController?.abort();
  if (previewBlob) URL.revokeObjectURL(previewBlob);
  previewController = new AbortController();
  const parent = previewStack.pop(),
    body = $("#preview-body");
  body.replaceChildren(...parent.nodes);
  body.className = parent.className;
  $("#preview-title").textContent = parent.title;
  $("#preview-kind").textContent = parent.kind;
  $("#download").onclick = parent.download;
  $("#download").disabled = parent.disabled;
  previewBlob = parent.blob;
  if (parent.context) markdownContexts.set(body, parent.context);
  else markdownContexts.delete(body);
  if (parent.markdown) markdownStates.set(body, parent.markdown);
  else markdownStates.delete(body);
  body.scrollTop = parent.top;
  body.scrollLeft = parent.left;
}
$("#preview-close").onclick = closePreview;
$("#preview").addEventListener("cancel", (event) => {
  event.preventDefault();
  closePreview();
});
$("#preview").addEventListener("close", () => {
  ++fileVersion;
  fileAbort?.abort();
  previewController?.abort();
  if (previewBlob) URL.revokeObjectURL(previewBlob);
  previewBlob = null;
  for (const parent of previewStack)
    if (parent.blob) URL.revokeObjectURL(parent.blob);
  previewStack.length = 0;
});
async function copyText(source) {
  try {
    await navigator.clipboard.writeText(source);
  } catch {
    const field = el("textarea");
    field.value = source;
    field.readOnly = true;
    field.style.cssText = "position:fixed;left:-9999px;font-size:16px";
    const focused = document.activeElement;
    (document.querySelector("dialog[open]") || document.body).append(field);
    field.select();
    const copied = document.execCommand("copy");
    field.remove();
    if (focused instanceof HTMLElement) focused.focus({ preventScroll: true });
    if (!copied) notice("Select text to copy.");
  }
}
function codeSource(code) {
  return codeSources.get(code) ?? code.textContent;
}
async function renderDiagram(button, id) {
  const code = document.getElementById(id),
    figure = code?.closest("figure");
  if (!figure) return;
  const rendered = figure.querySelector(".diagram-render");
  if (rendered.querySelector("svg")) {
    const showSource = !figure
      .querySelector("pre")
      .classList.contains("hidden");
    figure.querySelector("pre").classList.toggle("hidden", showSource);
    rendered.classList.toggle("hidden", !showSource);
    button.textContent = showSource ? "Source" : "Diagram";
    return;
  }
  button.disabled = true;
  button.textContent = "Rendering…";
  try {
    const { diagram } = await loadMermaid();
    const svg = await diagram(code.textContent);
    const target = figure.querySelector(".diagram-render");
    target.innerHTML = DOMPurify.sanitize(svg, {
      USE_PROFILES: { html: true, svg: true, svgFilters: true },
      ADD_TAGS: ["foreignObject"],
      ADD_ATTR: ["xmlns"],
    });
    figure.querySelector(".diagram-error")?.remove();
    target.classList.remove("hidden");
    figure.querySelector("pre").classList.add("hidden");
    button.textContent = "Source";
    button.disabled = false;
  } catch (e) {
    button.textContent = "Retry diagram";
    let error = figure.querySelector(".diagram-error");
    if (!error) {
      error = el("p", "diagram-error");
      figure.append(error);
    }
    error.textContent = "Source preserved · " + String(e.message).slice(0, 250);
    button.disabled = false;
  }
}
document.addEventListener("click", async (e) => {
  const image = e.target.closest(".prose img");
  if (image?.src) {
    preview(image.alt || "Image", "Image", image.src, "image");
    return;
  }
  const button = e.target.closest("button");
  if (button?.dataset.table) {
    const block = button.closest(".table-block");
    const table = block.querySelector("table");
    if (button.dataset.table === "wrap") {
      block.classList.toggle("table-wrapped");
      button.textContent = block.classList.contains("table-wrapped")
        ? "Compact"
        : "Wrap";
    } else if (button.dataset.table === "copy") {
      copyText(
        [...table.rows]
          .map((row) =>
            [...row.cells]
              .map((cell) => '"' + cell.textContent.replaceAll('"', '""') + '"')
              .join(","),
          )
          .join("\n"),
      );
    } else {
      preview("Table", "Data", "", "table");
      const body = $("#preview-body");
      body.className = "prose";
      const figure = el("figure", "table-block table-wrapped expanded-table");
      figure.style.setProperty(
        "--table-columns",
        Math.max(1, table.rows[0]?.cells.length || 1),
      );
      const wrapper = el("div", "table-wrap");
      wrapper.append(table.cloneNode(true));
      figure.append(wrapper);
      body.append(figure);
    }
    return;
  }
  if (button) {
    const id =
      button.dataset.copy ||
      button.dataset.preview ||
      button.dataset.html ||
      button.dataset.render;
    if (!id) return;
    const code = document.getElementById(id);
    if (!code) return;
    if (button.dataset.copy) copyText(codeSource(code));
    else if (button.dataset.html)
      preview(
        code.dataset.language,
        "HTML / SVG preview",
        code.textContent,
        "html",
      );
    else if (button.dataset.render) renderDiagram(button, id);
    else
      preview(
        code.dataset.language || "Source",
        code.dataset.language || "Code",
        codeSource(code),
        code.dataset.language === "mermaid" ? "diagram" : "text",
      );
    return;
  }
  const a = e.target.closest(".prose a");
  if (a) {
    const href = a.getAttribute("href");
    if (href && !/^(https?:|mailto:|#)/i.test(href)) {
      e.preventDefault();
      const block = a.closest(
        "[data-asset-workspace],[data-asset-base],[data-asset-id]",
      );
      const path =
        !href.startsWith("/") && block?.dataset.assetBase
          ? block.dataset.assetBase + "/" + href
          : href;
      await openFile(path, block?.dataset.assetWorkspace);
    }
  }
});
function renderCommandResult(root, value, format) {
  if (format === "markdown") {
    root.classList.add("prose");
    markdown(root, String(value || ""));
    return;
  }
  let count = 0;
  function node(value, key, depth = 0) {
    if (++count > 3000 || depth > 16)
      return el("p", "quiet", "Display limit reached. Narrow the query.");
    if (value && typeof value === "object") {
      const section = el("details", "command-section");
      section.open = depth < 2;
      section.append(
        el(
          "summary",
          "",
          `${key || "Result"}${Array.isArray(value) ? ` · ${value.length}` : ""}`,
        ),
      );
      for (const [name, item] of Object.entries(value)) {
        if (count >= 3000) {
          section.append(
            el("p", "quiet", "Display limit reached. Narrow the query."),
          );
          break;
        }
        section.append(node(item, name, depth + 1));
      }
      return section;
    }
    const row = el("div", "command-value");
    if (key) row.append(el("span", "command-key", key.replaceAll("_", " ")));
    if (
      ["proposal", "draft", "answer", "text", "diff", "preview"].includes(
        key,
      ) &&
      typeof value === "string"
    ) {
      const content = el("div", "prose");
      markdown(
        content,
        key === "diff" || key === "preview"
          ? "```diff\n" + value + "\n```"
          : value,
      );
      row.append(content);
    } else
      row.append(el("span", "", value === null ? "—" : String(value ?? "")));
    return row;
  }
  root.append(node(value, ""));
}
function openDraftEditor() {
  const modal = $("#draft-editor");
  $("#draft-text").value = $("#prompt").value;
  modal.showModal();
  $("#draft-text").focus({ preventScroll: true });
}
$("#draft-save").onclick = () => {
  $("#prompt").value = $("#draft-text").value;
  saveDraft();
  $("#draft-editor").close();
  $("#prompt").focus({ preventScroll: true });
};
$("#draft-cancel").onclick = () => $("#draft-editor").close();
function openAnimationPicker() {
  $("#animations").showModal();
}
for (const button of document.querySelectorAll("[data-animation]"))
  button.onclick = () => {
    document.documentElement.dataset.animation = button.dataset.animation;
    $("#animations").close();
  };
const commandRequests = new Map();
let commandViewVersion = 0;
function commandTransport(name, values, inspect = false) {
  if (!view.commandTransportReady)
    return Promise.reject(
      new Error(
        "This thread runtime is incompatible with browser commands. The host backend needs an update.",
      ),
    );
  if (!view.control && !inspect)
    return Promise.reject(
      new Error("Take control before changing this session."),
    );
  const request_id = Array.from(
    crypto.getRandomValues(new Uint8Array(16)),
    (b) => b.toString(16).padStart(2, "0"),
  ).join("");
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      commandRequests.delete(request_id);
      reject(
        new Error(
          "No completion receipt received. The command may still be running; inspect its state before retrying.",
        ),
      );
    }, 300000);
    view.commandRequestId = request_id;
    commandRequests.set(request_id, { resolve, reject, timer, name });
    send({ t: "command", name, values, request_id }).catch((error) => {
      clearTimeout(timer);
      commandRequests.delete(request_id);
      reject(error);
    });
  });
}
async function openNativeCommand(command, initialArgs = "") {
  if (!view.id) {
    notice("Open a conversation first.");
    return;
  }
  const version = ++commandViewVersion;
  closeCommands();
  const modal = $("#command-view"),
    form = $("#command-form"),
    body = $("#command-result");
  $("#command-title").textContent = `/${command.name}`;
  $("#command-status").textContent = "";
  body.replaceChildren();
  form.replaceChildren();
  if (!modal.open) modal.showModal();
  if (!view.commandTransportReady && !command.readonly) {
    body.append(
      el(
        "p",
        "quiet",
        "This thread runtime is incompatible with browser commands. The host backend needs an update.",
      ),
    );
    return;
  }
  const fields = command.form?.fields || [];
  for (const field of fields) {
    const label = el("label", "command-field", field.label);
    const choices =
      field.key === "action" && field.label.includes("|")
        ? field.label.split("|").map((s) => s.trim().split(/\s/)[0])
        : null;
    const input = el(
      choices ? "select" : field.kind === "textarea" ? "textarea" : "input",
    );
    if (choices)
      for (const choice of choices) {
        const option = el("option", "", choice);
        option.value = choice;
        input.append(option);
      }
    input.name = field.key;
    if (!choices && field.kind !== "textarea") input.type = field.kind;
    input.autocomplete = "off";
    if (field.key === "args") input.value = initialArgs;
    if (command.name === "rewrite" && initialArgs) {
      const split = initialArgs.indexOf(" ");
      if (field.key === "turn")
        input.value = split < 0 ? initialArgs : initialArgs.slice(0, split);
      if (field.key === "text")
        input.value = split < 0 ? "" : initialArgs.slice(split + 1);
    }
    if (command.name === "job" && initialArgs) {
      const [id, action = "poll"] = initialArgs.split(/\s+/);
      if (field.key === "id") input.value = id;
      if (field.key === "action") input.value = action;
    }
    if (field.key === "action" && command.name === "plan" && initialArgs)
      input.value = "generate";
    if (field.key === "secret") {
      input.autocomplete = "new-password";
      input.spellcheck = false;
      input.autocapitalize = "off";
    }
    label.append(input);
    form.append(label);
  }
  if (command.form?.confirm || command.name === "todo") {
    const label = el("label", "command-confirm", "Confirm this change");
    const input = el("input");
    input.type = "checkbox";
    input.name = "confirmed";
    label.prepend(input);
    form.append(label);
  }
  const variants = {
    handoff: {
      inspect: [],
      save: ["args"],
      "preview-import": ["args"],
      import: ["args", "expected"],
    },
    queue: {
      list: [],
      remove: ["args"],
      clear: [],
      "saved-edit": ["args", "text"],
      "saved-earlier": ["args"],
      "saved-later": ["args"],
      "saved-promote": ["args"],
      "saved-remove": ["args"],
      "saved-clear": [],
    },
    login: {
      list: [],
      use: ["args"],
      api: ["provider", "endpoint", "model", "label", "secret"],
      oauth: [],
      callback: ["args", "model", "label", "secret"],
    },
    profiles: {
      list: [],
      save: ["args"],
      use: ["args"],
      delete: ["args"],
      instructions: ["text"],
      global: [],
    },
    plan: { inspect: [], generate: ["args"], execute: [] },
    init: { draft: ["args"], write: ["text", "expected"] },
    git: {
      status: [],
      diff: [],
      stage: ["args", "expected"],
      unstage: ["args", "expected"],
      "stage-patch": ["patch", "expected"],
      "unstage-patch": ["patch", "expected"],
      "revert-patch": ["patch", "expected"],
      "revert-file": ["args", "expected"],
      commit: ["args", "expected"],
    },
    pr: {
      status: [],
      checks: [],
      reviews: [],
      preview: ["title", "body_file", "base"],
      create: ["title", "body_file", "base", "expected"],
    },
    neuralnets: {
      list: [],
      search: ["query"],
      read: ["id"],
      save: ["kind", "text"],
      update: ["id", "text", "expected"],
      forget: ["id", "expected"],
      restore: ["id", "expected"],
      purge: ["expected"],
    },
    capture: { draft: ["args", "first", "last"], save: ["args", "text"] },
    context: {
      inspect: [],
      instructions: [],
      system: [],
      skills: [],
      requests: [],
      remove: ["args"],
    },
    security: { inspect: [], profile: ["args"], approve: ["args"] },
    advisor: {
      inspect: [],
      request: [],
      cadence: ["args"],
      disposition: ["args", "text"],
    },
    agents: {
      list: [],
      transcript: ["id"],
      create: ["name", "args", "model", "options"],
      send: ["id", "args"],
      resume: ["id", "args"],
      interrupt: ["id"],
      archive: ["id"],
      budget: ["id", "options"],
      limits: ["id", "options"],
    },
    tasks: {
      list: [],
      "task-add": ["args", "acceptance", "depends", "paths"],
      "task-claim": ["id", "owner"],
      "task-finish": ["id", "owner", "check"],
      "task-release": ["id", "owner"],
      "debug-add": ["args"],
      "debug-evidence": ["id", "args"],
      "debug-instrument": ["id", "args"],
      "debug-resolve": ["id"],
      "artifact-add": ["args", "acceptance"],
      "artifact-preview": ["id"],
      "artifact-read": ["id", "offset", "limit"],
      "findings-import": ["args"],
      "findings-show": ["id"],
      "findings-disposition": ["id", "args"],
      "role-set": ["args", "model", "tokens"],
      "handoff-show": ["id"],
      "handoff-import": ["args"],
      advanced: ["args"],
    },
  };
  const updateFields = () => {
    const action = form.elements.namedItem("action")?.value;
    const visible = variants[command.name]?.[action];
    for (const label of form.querySelectorAll(".command-field")) {
      const input = label.querySelector("input,textarea,select");
      label.hidden =
        !!visible && input.name !== "action" && !visible.includes(input.name);
      input.disabled = label.hidden;
      if (label.hidden && input.name === "secret") input.value = "";
    }
    const confirmed = form.elements.namedItem("confirmed");
    if (confirmed) confirmed.checked = false;
  };
  form.elements.namedItem("action")?.addEventListener("change", updateFields);
  updateFields();
  const button = el(
    "button",
    "command-run",
    command.readonly ? "Refresh" : fields.length ? "Apply" : "Run",
  );
  button.type = "submit";
  form.append(button);
  const cancel = el("button", "quiet", "Cancel command");
  cancel.type = "button";
  cancel.hidden = true;
  cancel.onclick = () => {
    if (view.commandRequestId) {
      send({ t: "cancel_command", request_id: view.commandRequestId }).catch(
        (error) => ($("#command-status").textContent = error.message),
      );
      $("#command-status").textContent = "Cancellation requested…";
    }
  };
  form.append(cancel);
  const execute = async (inspect = false) => {
    const id = view.id;
    button.disabled = true;
    $("#command-status").textContent = inspect
      ? "Loading…"
      : command.form?.model_request
        ? "Working · uses model tokens"
        : view.active
          ? "Waiting for the current turn…"
          : "Working…";
    const values = Object.fromEntries(new FormData(form));
    cancel.hidden =
      inspect ||
      !["compact", "btw", "code-review", "plan", "init", "doctor"].includes(
        command.name,
      );
    $("#command-status").dataset.working = "true";
    values.confirmed = form.elements.namedItem("confirmed")?.checked === true;
    try {
      const result = command.readonly
        ? await api("command", {
            id,
            name: command.name,
            values: inspect ? {} : values,
          })
        : await commandTransport(command.name, inspect ? {} : values, inspect);
      if (version !== commandViewVersion || id !== view.id) return;
      body.replaceChildren();
      body.className = "command-data";
      renderCommandResult(body, result.body, result.format);
      for (const key of ["expected", "draft", "attempt"]) {
        const target = form.elements.namedItem(
          key === "draft" ? "text" : key === "attempt" ? "args" : key,
        );
        if (target && result.body?.[key] !== undefined)
          target.value = String(result.body[key]);
      }
      if (result.body?.authorize_url) {
        const link = el("a", "command-run", "Continue sign-in");
        link.href = result.body.authorize_url;
        link.target = "_blank";
        link.rel = "noopener noreferrer";
        body.prepend(link);
        const action = form.elements.namedItem("action");
        if (action) {
          action.value = "callback";
          updateFields();
        }
      }
      const secret = form.elements.namedItem("secret");
      if (secret) secret.value = "";
      $("#command-status").textContent = "";
      if (result.body?.open_thread) {
        modal.close();
        await loadThreads();
        await openThread(result.body.open_thread);
      }
      if (command.name === "todo") await history();
      if (command.name === "name") {
        await loadThreads();
        $("#thread-name").textContent = result.body.title;
      }
      if (["model", "login"].includes(command.name) && result.body?.model)
        $("#model").textContent = result.body.model;
      if (command.name === "export" && result.body?.download) {
        const download = el("a", "command-run", "Download Markdown");
        download.href = result.body.download;
        download.download = result.body.filename;
        body.prepend(download);
      }
    } catch (error) {
      if (version === commandViewVersion)
        $("#command-status").textContent = error.message;
    } finally {
      if (version === commandViewVersion) {
        button.disabled = false;
        cancel.hidden = true;
        $("#command-status").dataset.working = "false";
      }
    }
  };
  form.onsubmit = (e) => {
    e.preventDefault();
    execute();
  };
  if (
    command.readonly ||
    [
      "handoff",
      "todo",
      "budget",
      "pacing",
      "model",
      "mods",
      "jobs",
      "security",
      "prewalk",
      "constraints",
      "loop",
      "neuralnets",
      "profiles",
      "login",
      "plan",
      "advisor",
      "git",
      "pr",
      "agents",
      "extensions",
      "tasks",
      "queue",
      "context",
      "status",
      "settings-info",
      "doctor",
      "quote",
    ].includes(command.name)
  )
    await execute(true);
  else if ((initialArgs || !fields.length) && !command.form?.confirm)
    await execute();
  else
    $("#command-status").textContent = command.form?.model_request
      ? "Uses model tokens when run."
      : "Changes run on your host. They are not sent to the model.";
}
$("#command-close").onclick = () => $("#command-view").close();

const commandActions = {
  layout: () => inspectLayout(),
  clear: () => newThread(),
  tools: () => {
    document.documentElement.classList.toggle("compact-tools");
    notice(
      document.documentElement.classList.contains("compact-tools")
        ? "Compact tool cards"
        : "Expanded tool cards",
    );
  },
  editor: () => openDraftEditor(),
  animation: () => openAnimationPicker(),
  conversations: () => showPanel("#sidebar", true, true),
  work: () => openCommandTab("activity"),
  settings: () => openCommandTab("context"),
  outputs: () => openCommandTab("activity"),
  thinking: () => openCommandTab("reasoning"),
  context: () => openCommandTab("context"),
  status: () => openCommandTab("context"),
  usage: () => openCommandTab("context"),
  todo: () => openCommandTab("context"),
  tasks: () => openCommandTab("context"),
  agents: () => openCommandTab("activity"),
  attention: () => openCommandTab("activity"),
  queue: () => openCommandTab("activity"),
  theme: () => $("#theme").click(),
  redraw: () => {
    syncViewport();
    details();
  },
  search: () => {
    showPanel("#sidebar", true, true);
    $("#search").focus();
  },
  keys: () =>
    notice(
      "/ commands · Esc back · Enter send · Shift Enter newline · ⌘/Ctrl K threads",
    ),
};
let commands = [],
  commandGroup = "",
  commandSelection = 0;
function openCommandTab(tab) {
  document.querySelector(`[data-tab="${tab}"]`).click();
  showPanel("#details", true, true);
}
async function openCommands(
  group = "",
  { focusSearch = !matchMedia("(pointer:coarse)").matches } = {},
) {
  commandGroup = group;
  commandSelection = 0;
  if (!commands.length) {
    try {
      commands = (await api("commands")).commands;
      commands.push({
        name: "layout",
        description: "Inspect local viewport geometry",
        group: "settings",
        portable: false,
      });
    } catch (e) {
      notice(`Commands unavailable: ${e.message}`, true);
      return;
    }
  }
  const modal = $("#commands");
  $("#command-search").value = "";
  if (!focusSearch && document.activeElement instanceof HTMLElement)
    document.activeElement.blur();
  if (!modal.open) modal.showModal();
  $("#command-help").textContent = group
    ? "↑ ↓ select · Enter choose · Esc back"
    : "↑ ↓ select · Enter choose · Esc close";
  renderCommands();
  (focusSearch ? $("#command-search") : $("#commands-close")).focus({
    preventScroll: true,
  });
  syncViewport();
}
function commandMatches() {
  const query = $("#command-search")
    .value.toLowerCase()
    .replace(/^\//, "")
    .trim();
  return commands.filter((c) =>
    query
      ? [c.name, c.description, c.keywords, ...(c.aliases || [])]
          .join(" ")
          .toLowerCase()
          .includes(query)
      : commandGroup
        ? c.group === commandGroup && !c.primary
        : c.primary,
  );
}
let commandListSignature = "";
function renderCommands({ revealSelection = false } = {}) {
  const list = $("#command-list"),
    matches = commandMatches();
  const signature = JSON.stringify(
    matches.map((c) => [c.name, c.hint, c.description, c.primary, c.portable]),
  );
  const rebuild = signature !== commandListSignature;
  if (rebuild) {
    list.replaceChildren();
    commandListSignature = signature;
  }
  commandSelection = Math.max(
    0,
    Math.min(commandSelection, matches.length - 1),
  );
  $("#commands-title").textContent = commandGroup || "Commands";
  if (rebuild)
    matches.forEach((c, index) => {
      const button = el("button", "command-choice");
      button.type = "button";
      button.setAttribute("role", "option");
      button.setAttribute("aria-selected", String(index === commandSelection));
      button.append(
        el("strong", "", `/${c.name} ${c.hint || ""}`),
        el(
          "span",
          "",
          c.primary
            ? {
                conversations: "Resume or start a thread",
                work: "Changes, tasks and agents",
                settings: "Models and preferences",
              }[c.name] || c.description
            : c.description.replace(/ · local$/, ""),
        ),
      );
      if (
        !c.primary &&
        !c.portable &&
        !commandActions[c.name] &&
        c.name !== "help"
      )
        button.append(el("small", "", "Terminal only"));
      button.onclick = (event) =>
        chooseCommand(commands.find((entry) => entry.name === c.name) || c, {
          focusSearch: event.detail === 0 && event.pointerType !== "touch",
        });
      list.append(button);
    });
  if (rebuild && !matches.length)
    list.append(el("p", "", "No matching commands."));
  [...list.querySelectorAll(".command-choice")].forEach((button, index) =>
    button.setAttribute("aria-selected", String(index === commandSelection)),
  );
  if (rebuild) list.scrollTop = 0;
  if (revealSelection) {
    const selected = list.querySelector('[aria-selected="true"]');
    selected?.scrollIntoView({ block: "nearest" });
    if (document.activeElement !== $("#command-search"))
      selected?.focus({ preventScroll: true });
  }
}
function chooseCommand(c, { focusSearch = false } = {}) {
  if (c.primary) {
    openCommands(c.name, { focusSearch });
    return;
  }
  if (!c.portable && !commandActions[c.name] && c.name !== "help") {
    $("#command-help").textContent =
      `/${c.name} requires the terminal. Browser support is not implemented yet.`;
    return;
  }
  closeCommands();
  if ($("#prompt").value.trim().startsWith("/")) $("#prompt").value = "";
  if (c.portable) openNativeCommand(c);
  else if (c.name === "help") openCommands("", { focusSearch });
  else commandActions[c.name]();
}
async function inspectLayout() {
  preview(
    "Layout measurements",
    "Local diagnostics",
    "Waiting for the keyboard to close…",
  );
  const controller = previewController;
  document.activeElement?.blur();
  let settled = false,
    previous = "",
    stable = 0;
  // Keyboard dismissal animates viewport height and offset after focus has moved.
  await new Promise((resolve) => setTimeout(resolve, 400));
  for (let attempt = 0; attempt < 16; attempt++) {
    if (controller.signal.aborted) return;
    const viewport = window.visualViewport;
    const sample = `${innerHeight}:${viewport?.height}:${viewport?.offsetTop}`;
    stable = sample === previous ? stable + 1 : 0;
    previous = sample;
    const layoutHeight = Math.max(
      document.documentElement.clientHeight,
      idleViewportHeight,
      innerHeight,
    );
    if (
      stable >= 2 &&
      (!viewport ||
        viewport.scale !== 1 ||
        layoutHeight - viewport.height < 120)
    ) {
      settled = true;
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  syncViewport();
  await new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(resolve)),
  );
  if (controller.signal.aborted) return;
  const probe = el("div");
  probe.style.cssText =
    "position:fixed;visibility:hidden;padding-top:var(--safe-top);padding-bottom:var(--safe-bottom)";
  document.body.append(probe);
  const computed = getComputedStyle(probe);
  const safe = {
    top: parseFloat(computed.paddingTop),
    bottom: parseFloat(computed.paddingBottom),
  };
  probe.remove();
  const rect = (selector) => {
    const r = $(selector).getBoundingClientRect();
    return {
      top: Math.round(r.top),
      height: Math.round(r.height),
      bottom: Math.round(r.bottom),
    };
  };
  const v = window.visualViewport;
  const data = {
    version: "layout-2",
    settled,
    layoutViewport: document.documentElement.clientHeight,
    standalone: Boolean(
      navigator.standalone || matchMedia("(display-mode: standalone)").matches,
    ),
    viewport: {
      window: innerHeight,
      visible: v?.height,
      offset: v?.offsetTop,
      scale: v?.scale,
      screen: screen.height,
    },
    safeArea: safe,
    app: rect(".app"),
    toolbar: rect(".toolbar"),
    composer: rect(".composer"),
    composeZone: rect(".compose-zone"),
    configuredHeight:
      document.documentElement.style.getPropertyValue("--app-height"),
  };
  const source = JSON.stringify(data, null, 2);
  preview(
    "Layout measurements",
    "Local diagnostics",
    source,
    "json",
    undefined,
    true,
  );
  const copy = el("button", "quiet", "Copy measurements");
  copy.onclick = () => copyText(source);
  $("#preview-body").prepend(copy);
}
async function dispatchSlash(value) {
  const name = value.slice(1).split(/\s/, 1)[0];
  if (name === "layout") {
    closeCommands();
    $("#prompt").value = "";
    inspectLayout();
    return;
  }
  if (!commands.length) {
    try {
      commands = (await api("commands")).commands;
      commands.push({
        name: "layout",
        description: "Inspect local viewport geometry",
        group: "settings",
        portable: false,
      });
    } catch (e) {
      notice(e.message, true);
      return;
    }
  }
  const c = commands.find((c) => c.name === name || c.aliases?.includes(name));
  if (!c) {
    notice(`Unknown command /${name}. Type / to browse commands.`, true);
    return;
  }
  const args = value
    .trim()
    .slice(name.length + 1)
    .trim();
  if (args && c.portable) {
    $("#prompt").value = "";
    await openNativeCommand(c, args);
    return;
  }
  if (value.trim() !== `/${name}`) {
    notice(
      `Arguments for /${name} are not supported in the browser yet.`,
      true,
    );
    return;
  }
  if (
    !c.primary &&
    !c.portable &&
    !commandActions[c.name] &&
    c.name !== "help"
  ) {
    notice(`/${c.name} is currently available in the terminal only.`, true);
    return;
  }
  chooseCommand(c, { focusSearch: true });
}
$("#command-search").oninput = () => {
  commandSelection = 0;
  renderCommands();
};
$("#open-commands").onclick = () => openCommands();
function closeCommands() {
  if (/^\s*\/\s*$/.test($("#prompt").value)) $("#prompt").value = "";
  $("#commands").close();
}
$("#commands").addEventListener("close", () => {
  if (/^\s*\/\s*$/.test($("#prompt").value)) $("#prompt").value = "";
});
$("#commands-close").onclick = () => closeCommands();
$("#commands").addEventListener("cancel", (e) => {
  e.preventDefault();
  if (commandGroup)
    openCommands("", {
      focusSearch: document.activeElement === $("#command-search"),
    });
  else closeCommands();
});
$("#commands").addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const count = commandMatches().length;
    if (count)
      commandSelection =
        (commandSelection + (e.key === "ArrowDown" ? 1 : count - 1)) % count;
    renderCommands({ revealSelection: true });
  } else if (e.key === "Enter" && e.target === $("#command-search")) {
    e.preventDefault();
    const c = commandMatches()[commandSelection];
    if (c) chooseCommand(c, { focusSearch: true });
  }
});

async function start() {
  try {
    const secret = location.hash.slice(1);
    if (secret) {
      await api("local", { token: secret });
      window.history.replaceState(
        null,
        "",
        location.pathname + location.search,
      );
    }
    const status = await api("status");
    view.hostWorkflows = status.web_workflows === 1;
    view.interactiveArtifacts = status.interactive_artifacts === 1;
    $("#host").textContent = location.hostname;
    $("#workspace-name").textContent = "All projects";
    $("#profile").textContent = status.profile;
    view.workspace = status.workspace;
    $(".sidebar-footer small").textContent = "";
    await loadThreads();
    const id = new URLSearchParams(location.search).get("thread");
    if (id) {
      const opened = await openThread(id);
      if (!opened?.ok)
        return (
          opened || {
            ok: false,
            message: "The requested thread could not open.",
          }
        );
    } else connection("Connected");
    if ("serviceWorker" in navigator)
      navigator.serviceWorker
        .register("/sw.js", { updateViaCache: "none" })
        .catch(() => {});
    return { ok: true };
  } catch (e) {
    connection("Unavailable", false);
    notice(e.message, true);
    return { ok: false, message: e.message };
  }
}
$("#connection").setAttribute("role", "button");
$("#connection").tabIndex = 0;
$("#connection").title = "Connection · reconnect";
$("#connection").onkeydown = (e) => {
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    $("#connection").click();
  }
};
$("#connection").onclick = () => {
  if (view.id) openThread(view.id);
  else start();
};
let reconnecting = false,
  suspended = false;
async function resumeConnection() {
  syncViewport();
  syncPanels();
  if (initialOpening || reconnecting || document.visibilityState === "hidden")
    return;
  reconnecting = true;
  try {
    if (view.id) await openThread(view.id, { resume: true });
    else await start();
  } finally {
    reconnecting = false;
  }
}
window.addEventListener("online", resumeConnection);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden") {
    suspended = true;
    view.source?.close();
    if (view.frame) {
      clearTimeout(view.frame);
      view.frame = 0;
    }
  } else if (suspended) {
    suspended = false;
    resumeConnection();
  }
});
window.addEventListener("pageshow", (e) => {
  syncViewport();
  syncPanels();
  if (e.persisted) resumeConnection();
});
window.addEventListener("focus", () => {
  syncViewport();
  if (suspended && document.visibilityState !== "hidden") {
    suspended = false;
    resumeConnection();
  }
});
restoreDraft();
export const initialReady = start().finally(() => {
  initialOpening = false;
});

$("#send-mode").onclick = () => {
  const button = $("#send-mode");
  button.dataset.mode = button.dataset.mode === "queue" ? "steer" : "queue";
  button.textContent =
    button.dataset.mode === "queue" ? "Queue follow-up" : "Steer task";
};

let workflowPromise;
function workflow() {
  return (workflowPromise ||= import("./workflows.js").then(({ workflows }) =>
    workflows({
      api,
      command: commandTransport,
      view,
      el,
      notice,
      saveDraft,
      onAttached: () => refreshContext(),
    }),
  ));
}
for (const [id, name] of [
  ["model", "model"],
  ["profile", "profiles"],
]) {
  const node = $("#" + id);
  node.setAttribute("role", "button");
  node.tabIndex = 0;
  node.title = name === "model" ? "Choose a model" : "Execution profiles";
  node.onclick = () => {
    if (!view.id) return notice("Open a thread first.");
    workflow()
      .then((w) => w.picker(name))
      .catch((e) => notice(e.message, true));
  };
  node.onkeydown = (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      node.click();
    }
  };
}
$("#attach").onclick = () => {
  if (!view.id || !view.control)
    return notice("Open a thread and take control before attaching files.");
  $("#attachment-input").click();
};
$("#attachment-input").onchange = async (e) => {
  const files = [...e.target.files];
  e.target.value = "";
  await (await workflow()).attach(files);
};
$("#changes-toggle").onclick = async () => {
  if (!view.id) return notice("Open a thread first.");
  await openWorkflow("changes");
};
$("#context-toggle").onclick = async () => {
  if (!view.id) return notice("Open a thread first.");
  await openWorkflow("context");
};
$("#prompt").addEventListener("paste", (e) => {
  const files = [...e.clipboardData.files];
  if (files.length) {
    e.preventDefault();
    workflow().then((w) => w.attach(files));
  }
});

$("#work-toggle").onclick = async () => {
  if (!view.id) return notice("Open a thread first.");
  await openWorkflow("work");
};
$("#queue-toggle").onclick = async () => {
  if (!view.id) return notice("Open a thread first.");
  await openWorkflow("queue");
};

async function refreshContext() {
  if (!view.hostWorkflows || !view.id) return;
  const id = view.id,
    generation = view.generation;
  try {
    const result = await api("context?id=" + encodeURIComponent(id));
    if (generation !== view.generation) return;
    const chips = $("#context-chips");
    chips.replaceChildren();
    for (const attachment of (result.attachments || []).slice(0, 8)) {
      const chip = el(
        "button",
        "context-chip",
        attachment.source?.match(/^Uploaded (.*?) ·/)?.[1] ||
          attachment.path.split("/").pop(),
      );
      chip.type = "button";
      chip.title = attachment.path + " · active context";
      chip.onclick = () => $("#context-toggle").click();
      chips.append(chip);
    }
    $("#context-toggle").textContent =
      "Context" +
      (result.attachments?.length ? " · " + result.attachments.length : "");
    $("#context-toggle").title =
      "Active attachments and context · usage is not reported by this runtime";
  } catch {}
}

navigator.serviceWorker?.addEventListener("message", (event) => {
  if (
    event.data?.t === "open_thread" &&
    typeof event.data.id === "string" &&
    /^[a-zA-Z0-9_-]{1,128}$/.test(event.data.id)
  )
    openThread(event.data.id);
});

async function openWorkflow(name, ...args) {
  try {
    if (!view.id) return notice("Open a thread first.");
    return await (await workflow())[name](...args);
  } catch (e) {
    notice(e.message, true);
  }
}

$(".brand-home").addEventListener("click", async (event) => {
  if (!workflowPromise) return;
  event.preventDefault();
  const work = await workflowPromise;
  if (work.prepareLeave()) {
    saveDraft();
    drafts.flush();
    location.href = "/";
  }
});

export function revealShell() {
  syncViewport();
  syncPanels();
  follow();
}
