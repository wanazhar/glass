import { marked } from "marked";
import DOMPurify from "dompurify";
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
  before: 0,
  source: null,
  active: false,
  control: false,
  follow: true,
  reasoning: "",
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
  directory: null,
};
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
function notice(message, error = false) {
  $("#notice").textContent = message;
  $("#notice").className = "notice" + (error ? " error" : "");
}
function clearNotice() {
  $("#notice").classList.add("hidden");
}
async function api(path, data, signal) {
  const r = await fetch("/api/" + path, {
    method: data === undefined ? "GET" : "POST",
    headers: data === undefined ? {} : { "Content-Type": "application/json" },
    body: data === undefined ? undefined : JSON.stringify(data),
    signal,
  });
  let value;
  try {
    value = await r.json();
  } catch {
    throw Error("Host connection unavailable");
  }
  if (!r.ok) throw Error(value.error || "Request rejected");
  return value;
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
  const key = `${label || ""}|${view.control}|${view.id}`;
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
let viewportFrame = 0;
function syncViewport() {
  if (viewportFrame) return;
  viewportFrame = requestAnimationFrame(() => {
    viewportFrame = 0;
    const viewport = window.visualViewport;
    if (viewport && viewport.scale === 1) {
      document.documentElement.style.setProperty(
        "--app-height",
        `${Math.round(viewport.height)}px`,
      );
      document.documentElement.style.setProperty(
        "--viewport-top",
        `${Math.round(viewport.offsetTop)}px`,
      );
    }
    follow();
  });
}
window.visualViewport?.addEventListener("resize", syncViewport, {
  passive: true,
});
window.visualViewport?.addEventListener("scroll", syncViewport, {
  passive: true,
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
    view.follow = atBottom();
    $("#latest").classList.toggle("hidden", view.follow);
  },
  { passive: true },
);
$("#latest").onclick = () => {
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
      '<div class="table-wrap">' +
      marked.Renderer.prototype.table.call(this, token) +
      "</div>"
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
      view.reasoning = text(view.reasoning + (r.t || ""), limits.reasoning);
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
async function history(older = false) {
  const id = view.id,
    generation = view.generation;
  if (!id) return;
  const h = await api(
    "history?id=" +
      encodeURIComponent(id) +
      (older ? "&before=" + view.before : ""),
  );
  if (generation !== view.generation) return;
  const scroller = $("#conversation");
  const previousTop = scroller.scrollTop;
  const anchor = [...$("#timeline").children].find(
    (n) =>
      n.getBoundingClientRect().bottom > scroller.getBoundingClientRect().top,
  );
  const anchorKey = anchor?.dataset.recordIndex;
  const anchorTop = anchor?.getBoundingClientRect().top;
  const expanded = new Set(
    [...view.tools.entries()].filter(([, t]) => t.row.open).map(([id]) => id),
  );
  const expandedGroups = new Set(
    [...document.querySelectorAll("#timeline .work-group[open]")].map(
      (n) => n.dataset.recordIndex,
    ),
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
  view.group = null;
  for (const item of h.records) {
    record(item.record, fragment);
    const node = fragment.lastElementChild;
    if (node && !node.dataset.recordIndex)
      node.dataset.recordIndex = String(item.index);
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
  }
  if (h.omittedLargeRecord)
    notice(
      "A record exceeds the 512 KiB history window. Load earlier activity or use gc export for full source.",
    );
  for (const [id, t] of view.tools) if (expanded.has(id)) t.row.open = true;
  for (const group of $("#timeline").querySelectorAll(".work-group"))
    if (expandedGroups.has(group.dataset.recordIndex)) group.open = true;
  if (!older && !view.follow) {
    const restored = anchorKey
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
  $("#older").classList.toggle("hidden", h.before === 0);
  details();
}
$("#older").onclick = async () => {
  const node = $("#conversation"),
    height = node.scrollHeight;
  try {
    await history(true);
    node.scrollTop += node.scrollHeight - height;
  } catch (e) {
    notice(e.message, true);
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
    case "replay":
      view.control = Boolean(m.can_write);
      updateActivity(view.active ? "Working" : null);
      break;
    case "control":
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
        const next = text(view.reasoning + (m.delta || ""), limits.reasoning);
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
      if (m.settled) {
        view.interaction = null;
        interaction();
        updateActivity(null);
      }
      notice(m.message || "The operation failed", true);
      if (m.request_id && view.pending.has(m.request_id)) {
        const pending = view.pending.get(m.request_id);
        pending.status = "rejected";
        view.pending.delete(m.request_id);
        if (!$("#prompt").value.trim()) $("#prompt").value = pending.text;
      }
      break;
    case "retrying":
    case "provider_notice":
      notice(m.message || `Provider cooldown · retry ${m.attempt}`);
      break;
    case "disconnected":
      connection("Disconnected", false);
      notice(m.message, true);
      break;
    case "done":
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
async function openThread(id) {
  if (view.source) view.source.close();
  view.source = null;
  const generation = ++view.generation;
  view.id = id;
  view.tools.clear();
  view.group = null;
  view.stream = null;
  view.streamText = "";
  view.reasoning = "";
  view.tasks = [];
  view.streamTruncated = false;
  view.interaction = null;
  view.control = false;
  view.follow = true;
  view.active = false;
  interaction();
  clearNotice();
  $("#welcome").classList.add("hidden");
  closePanels();
  $("#timeline").replaceChildren(el("div", "loading-surface"));
  connection("Connecting");
  const thread = view.threads.find((t) => t.session === id);
  $("#thread-name").textContent = thread?.title || "Conversation";
  $("#workspace-name").textContent = (thread?.workspace || "Workspace")
    .split("/")
    .filter(Boolean)
    .pop();
  $("#model").textContent = thread?.model || "Glass Core";
  document.title = (thread?.title || "Conversation") + " · Glass";
  window.history.replaceState(null, "", "/?thread=" + encodeURIComponent(id));
  renderThreads();
  try {
    await api("open", { id });
    if (generation !== view.generation) return;
    await history();
    if (generation !== view.generation) return;
    follow();
    const source = new EventSource("/api/events?id=" + encodeURIComponent(id));
    view.source = source;
    let eventChain = Promise.resolve();
    source.onopen = () => connection("Connected");
    source.onerror = () => connection("Reconnecting", false);
    source.onmessage = (e) => {
      eventChain = eventChain
        .then(async () => {
          if (generation !== view.generation) return;
          const data = JSON.parse(e.data);
          if (data.reset) {
            view.control = Boolean(data.control?.can_write);
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
        })
        .catch((error) => notice(error.message, true));
    };
  } catch (e) {
    if (generation !== view.generation) return;
    $("#timeline .loading-surface")?.remove();
    connection("Unavailable", false);
    notice(e.message, true);
    updateActivity(null);
  }
}
async function loadThreads() {
  const value = await api("threads");
  view.threads = value.threads || [];
  renderThreads();
}
function renderThreads() {
  const search = $("#search").value.toLowerCase(),
    container = $("#threads");
  container.replaceChildren();
  const groups = new Map();
  for (const thread of view.threads) {
    if (
      search &&
      !((thread.title || "") + " " + thread.workspace)
        .toLowerCase()
        .includes(search)
    )
      continue;
    const workspaceKey = thread.workspace || "Other";
    const name =
      (thread.workspace || "Other").split("/").filter(Boolean).pop() ||
      "Workspace";
    if (!groups.has(workspaceKey)) {
      const section = el("section", "thread-group");
      section.append(el("div", "group-label", "⌘ " + name));
      container.append(section);
      groups.set(workspaceKey, section);
    }
    const button = el(
      "button",
      "thread" + (thread.session === view.id ? " active" : ""),
    );
    button.title = thread.title || "New conversation";
    button.append(el("strong", "", thread.title || "New conversation"));
    const small = el("small");
    small.append(
      el(
        "span",
        thread.running_process_verified ? "running" : "",
        thread.running_process_verified
          ? "● Working"
          : thread.last_status === "completed"
            ? "Completed"
            : "Saved",
      ),
      el("span", "", thread.model?.split("/").pop() || "Glass Core"),
    );
    button.append(small);
    button.onclick = () => openThread(thread.session);
    groups.get(workspaceKey).append(button);
  }
  if (!container.children.length)
    container.append(el("p", "empty", "No threads"));
}
$("#search").oninput = renderThreads;
async function newThread(workspace) {
  try {
    const { id } = await api("new", {
      workspace:
        typeof workspace === "string"
          ? workspace
          : view.threads.find((t) => t.session === view.id)?.workspace ||
            view.workspace,
    });
    await loadThreads();
    await openThread(id);
    $("#prompt").focus();
  } catch (e) {
    notice(e.message, true);
  }
}
$("#new").onclick = newThread;
$("#welcome-new").onclick = newThread;
$("#composer").onsubmit = async (e) => {
  e.preventDefault();
  const prompt = $("#prompt"),
    value = prompt.value.trim();
  if (!value) return;
  try {
    if (!view.id) {
      await newThread();
      if (!view.id) return;
    }
    if (!view.control) {
      notice("Take control before sending a message.");
      return;
    }
    const id =
      crypto.randomUUID?.() ??
      Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) =>
        b.toString(16).padStart(2, "0"),
      ).join("");
    view.lastPrompt = value;
    const queued = view.active;
    const steering = queued && $("#send-mode").dataset.mode !== "queue";
    const receipt = { text: value, shown: !queued, status: "sending" };
    view.pending.set(id, receipt);
    clearNotice();
    if (queued)
      notice(
        steering
          ? "Sending correction to the host…"
          : "Sending follow-up to the host…",
      );
    await send({
      t: steering ? "steer" : "prompt",
      text: value,
      request_id: id,
    });
    if (receipt.status === "rejected") return;
    if (!queued) message("user", value);
    prompt.value = "";
    prompt.style.height = "";
    view.follow = true;
    follow();
    if (!queued && receipt.status === "sending") updateActivity("Starting");
  } catch (e) {
    notice(e.message, true);
  }
};
$("#prompt").onkeydown = (e) => {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    $("#composer").requestSubmit();
  }
};
$("#prompt").oninput = (e) => {
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
function details() {
  const n = $("#detail-content");
  if (!$("#details").classList.contains("open")) return;
  if (view.tab === "files") {
    if (n.dataset.tab !== "files") browseDirectory();
    return;
  }
  if (view.tab !== "reasoning" || n.dataset.tab !== "reasoning")
    n.replaceChildren();
  n.dataset.tab = view.tab;
  if (view.tab === "reasoning") {
    const body = n.querySelector(".prose") || el("div", "prose");
    markdown(body, view.reasoning || "No reasoning yet.");
    if (!body.isConnected) n.append(body);
    if (view.reasoningFollow) n.scrollTop = n.scrollHeight;
  } else if (view.tab === "context") {
    const thread = view.threads.find((t) => t.session === view.id) || {};
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
$("#details-toggle").onclick = () => showPanel("#details");
$("#details-close").onclick = () => {
  closePanels();
  $("#details-toggle").focus();
};
$("#sidebar-close").onclick = () => {
  closePanels();
  $("#menu").focus();
};
$("#detail-content").addEventListener(
  "scroll",
  () => {
    const n = $("#detail-content");
    if (view.tab === "reasoning")
      view.reasoningFollow = n.scrollHeight - n.scrollTop - n.clientHeight < 60;
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
  };
$("#menu").onclick = () => showPanel("#sidebar");
$("#scrim").onclick = () => closePanels();
syncPanels();
$("#theme").onclick = () => document.documentElement.classList.toggle("light");
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "k") {
    e.preventDefault();
    showPanel("#sidebar", true, true);
    $("#search").focus();
  }
  if (e.key === "Escape") {
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
  path ??=
    view.directory ||
    view.threads.find((t) => t.session === view.id)?.workspace ||
    view.workspace;
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
  form.append(path, go);
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
        preview(path, "PDF", source, "pdf");
    } else if (type.startsWith("image/")) {
      const blob = await r.blob();
      if (version !== fileVersion || !$("#preview").open) return;
      const url = URL.createObjectURL(blob);
      preview(path, "Image", url, "image");
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
      );
    }
  } catch (e) {
    if (e.name !== "AbortError" && version === fileVersion)
      $("#preview-body").replaceChildren(el("p", "notice error", e.message));
  }
}
async function loadAgents() {
  try {
    const agents = await api("agents?id=" + encodeURIComponent(view.id));
    const n = $("#detail-content");
    for (const agent of Array.isArray(agents) ? agents : agents.agents || []) {
      const card = el("div", "detail-card");
      card.append(
        el("strong", "", agent.name || agent.id),
        el("p", "", agent.status || ""),
      );
      const button = el("button", "quiet", "Read transcript");
      button.onclick = async () => {
        try {
          const result = await api(
            "agent?id=" +
              encodeURIComponent(view.id) +
              "&agent=" +
              encodeURIComponent(agent.id),
          );
          preview(
            agent.name || "Teammate",
            "Transcript",
            result.text || JSON.stringify(result, null, 2),
          );
        } catch (e) {
          notice(e.message, true);
        }
      };
      card.append(button);
      n.append(card);
    }
  } catch (e) {
    notice(e.message, true);
  }
}
let previewBlob, previewController;
function preview(title, kind, source, format = "text", context) {
  const dialog = $("#preview");
  ++fileVersion;
  fileAbort?.abort();
  previewController?.abort();
  previewController = new AbortController();
  const signal = previewController.signal;
  $("#download").disabled = false;
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
    body.append(iframe);
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
    (mermaidPromise ??= import("./mermaid.js"))
      .then(({ diagram }) => diagram(source))
      .then((svg) => {
        if (signal.aborted) return;
        target.innerHTML = DOMPurify.sanitize(svg, {
          USE_PROFILES: { svg: true, svgFilters: true },
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
$("#preview-close").onclick = () => $("#preview").close();
$("#preview").addEventListener("close", () => {
  ++fileVersion;
  fileAbort?.abort();
  previewController?.abort();
  if (previewBlob) URL.revokeObjectURL(previewBlob);
  previewBlob = null;
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
    const { diagram } = await (mermaidPromise ??= import("./mermaid.js"));
    const svg = await diagram(code.textContent);
    const target = figure.querySelector(".diagram-render");
    target.innerHTML = DOMPurify.sanitize(svg, {
      USE_PROFILES: { html: true, svg: true, svgFilters: true },
      ADD_TAGS: ["foreignObject"],
      ADD_ATTR: ["xmlns"],
    });
    target.classList.remove("hidden");
    figure.querySelector("pre").classList.add("hidden");
    button.textContent = "Source";
    button.disabled = false;
  } catch (e) {
    button.textContent = "Retry diagram";
    notice(
      "Diagram could not render. Source is preserved: " +
        String(e.message).slice(0, 250),
    );
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
    $("#host").textContent = location.hostname;
    $("#workspace-name").textContent = String(status.workspace)
      .split("/")
      .pop();
    $("#profile").textContent = status.profile;
    view.workspace = status.workspace;
    $(".sidebar-footer small").textContent = "";
    await loadThreads();
    const id = new URLSearchParams(location.search).get("thread");
    if (id) await openThread(id);
    else connection("Connected");
    if ("serviceWorker" in navigator)
      navigator.serviceWorker.register("/sw.js").catch(() => {});
  } catch (e) {
    connection("Unavailable", false);
    notice(e.message, true);
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
window.addEventListener("online", () => {
  if (view.id) openThread(view.id);
  else start();
});
window.addEventListener("pageshow", (e) => {
  if (e.persisted && view.id) openThread(view.id);
});
start();

$("#send-mode").onclick = () => {
  const button = $("#send-mode");
  button.dataset.mode = button.dataset.mode === "queue" ? "steer" : "queue";
  button.textContent =
    button.dataset.mode === "queue" ? "Queue follow-up" : "Steer task";
};
