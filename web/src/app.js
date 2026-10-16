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
function updateActivity(label) {
  view.active = Boolean(label);
  $("#activity").classList.toggle("working", view.active);
  $("#activity-text").textContent = label || "Ready when you are";
  $("#stop").classList.toggle("hidden", !view.active);
  $("#send-mode").classList.toggle("hidden", !view.active);
  $("#ownership").textContent = view.control
    ? "Release control"
    : "Take control";
  $("#send").disabled = !view.control && Boolean(view.id);
}
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
    const id = "code-" + ++codeId;
    if (language === "mermaid")
      return `<figure class="diagram-block" data-diagram="${id}"><div class="block-head"><span>◇ Mermaid diagram</span><span><button data-render="${id}">Render diagram</button><button data-preview="${id}">Expand</button></span></div><pre><code id="${id}" data-language="mermaid">${escape(source)}</code></pre><div class="diagram-render hidden"></div></figure>`;
    if (
      ["math", "latex", "tex"].includes(language) &&
      !source.includes("\\documentclass")
    )
      return `<div class="math" data-math="display"><code>${escape(source)}</code></div>`;
    return `<div class="code-block"><div class="block-head"><span>${escape(language)}</span><span><button data-copy="${id}">Copy</button><button data-preview="${id}">Expand</button>${["html", "svg"].includes(language) ? `<button data-html="${id}">Preview</button>` : ""}</span></div><pre><code id="${id}" data-language="${escape(language)}">${escape(source)}</code></pre></div>`;
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
function markdown(node, source, streaming = false) {
  node.innerHTML = DOMPurify.sanitize(marked.parse(text(source)), {
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
  for (const n of node.querySelectorAll("[data-math]")) observer.observe(n);
  if (!streaming)
    for (const n of node.querySelectorAll("code[data-language]"))
      observer.observe(n);
  for (const a of node.querySelectorAll("a")) {
    a.rel = "noopener noreferrer";
    a.target = "_blank";
  }
  for (const img of node.querySelectorAll("img")) {
    img.loading = "lazy";
    img.decoding = "async";
    const src = img.getAttribute("src") || "";
    if (src && !src.startsWith("data:") && !src.startsWith("blob:")) {
      if (!/^https?:/i.test(src) && view.id)
        img.src =
          "/api/file?id=" +
          encodeURIComponent(view.id) +
          "&path=" +
          encodeURIComponent(src);
      else {
        img.removeAttribute("src");
        img.alt = "External image · open its link to view";
      }
    }
  }
  if (source.length > limits.message)
    node.append(
      el(
        "p",
        "notice",
        "Display preview capped. The full response remains in the host transcript.",
      ),
    );
}
async function enhance(n) {
  try {
    if (n.dataset.math) {
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
    /* Source stays readable when an optional renderer fails. */
  }
}
function message(role, source, target = $("#timeline")) {
  view.group = null;
  view.stream = null;
  const article = el("article", "message " + role);
  article.append(el("div", "message-meta", role === "user" ? "You" : "Glass"));
  const body = el("div", "prose");
  if (role === "user") {
    body.textContent = source;
    body.style.whiteSpace = "pre-wrap";
  } else markdown(body, source);
  article.append(body);
  if (role !== "user") {
    const actions = el("div", "message-actions");
    const copy = el("button", "", "Copy response");
    copy.onclick = () => copyText(source);
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
  view.stream = null;
  const parent = group(target);
  const row = el("details", "tool-row");
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
      t.row.classList.toggle("failed", r.code !== 0 && r.code != null);
      break;
    }
    case "tool": {
      const t = tool(r.id, r.name, "", target);
      t.output.textContent = text(r.t, limits.preview);
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
    [...document.querySelectorAll("#timeline .work-group[open]")].map((n) => n.dataset.recordIndex),
  );
  if (!older) {
    view.tools.clear();
    view.reasoning = "";
  }
  const fragment = document.createDocumentFragment();
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
    $("#timeline").replaceChildren(fragment);
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
  view.frame = setTimeout(() => {
    view.frame = 0;
    requestAnimationFrame(() => {
      if (view.stream) markdown(view.stream, view.streamText, true);
      follow();
    });
  }, 100);
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
      }
      view.streamText = text(view.streamText + (m.delta || ""));
      updateActivity("Writing");
      schedule();
      break;
    case "reasoning":
      view.reasoning = text(view.reasoning + (m.delta || ""), limits.reasoning);
      updateActivity("Reasoning");
      if (view.tab === "reasoning") details();
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
      t.row.classList.toggle(
        "failed",
        /failed|error:|exit [1-9]/i.test(m.summary || ""),
      );
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
      if (view.stream) markdown(view.stream, view.streamText);
      view.stream = null;
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
  view.generation++;
  view.id = id;
  view.tools.clear();
  view.group = null;
  view.stream = null;
  view.streamText = "";
  view.reasoning = "";
  view.interaction = null;
  view.control = false;
  view.follow = true;
  view.active = false;
  interaction();
  clearNotice();
  $("#welcome").classList.add("hidden");
  $("#sidebar").classList.remove("open");
  $("#scrim").classList.remove("open");
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
    await history();
    follow();
    const source = new EventSource("/api/events?id=" + encodeURIComponent(id));
    view.source = source;
    const generation = view.generation;
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
    const name =
      (thread.workspace || "Other").split("/").filter(Boolean).pop() ||
      "Workspace";
    if (!groups.has(name)) {
      const section = el("section", "thread-group");
      section.append(el("div", "group-label", "⌘ " + name));
      container.append(section);
      groups.set(name, section);
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
    groups.get(name).append(button);
  }
  if (!container.children.length)
    container.append(el("p", "empty", "No threads yet. Start something new."));
}
$("#search").oninput = renderThreads;
async function newThread() {
  try {
    const { id } = await api("new", {});
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
    const id = crypto.randomUUID?.() ?? Array.from(crypto.getRandomValues(new Uint8Array(16)), b => b.toString(16).padStart(2, "0")).join("");
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
  n.replaceChildren();
  if (view.tab === "reasoning") {
    const body = el("div", "prose");
    markdown(
      body,
      view.reasoning || "Provider-exposed reasoning will appear here.",
    );
    n.append(body);
    if (view.follow) n.scrollTop = n.scrollHeight;
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
    if (!n.children.length)
      n.append(
        el("p", "", "Tool activity will appear here as work progresses."),
      );
    const agents = el("button", "quiet", "Inspect teammates");
    agents.onclick = () => loadAgents();
    n.append(agents);
  }
}
$("#details-toggle").onclick = () => {
  $("#details").classList.toggle("open");
  details();
};
$("#details-close").onclick = () => $("#details").classList.remove("open");
for (const tab of document.querySelectorAll("[data-tab]"))
  tab.onclick = () => {
    view.tab = tab.dataset.tab;
    document
      .querySelectorAll("[data-tab]")
      .forEach((n) => n.classList.toggle("selected", n === tab));
    details();
  };
$("#menu").onclick = () => {
  $("#sidebar").classList.toggle("open");
  $("#scrim").classList.toggle("open");
};
$("#scrim").onclick = () => {
  $("#sidebar").classList.remove("open");
  $("#scrim").classList.remove("open");
};
$("#theme").onclick = () => document.documentElement.classList.toggle("light");
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "k") {
    e.preventDefault();
    $("#sidebar").classList.add("open");
    $("#search").focus();
  }
  if (e.key === "Escape") {
    $("#details").classList.remove("open");
    $("#sidebar").classList.remove("open");
    $("#scrim").classList.remove("open");
  }
});
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
let previewBlob;
function preview(title, kind, source, format = "text") {
  const dialog = $("#preview");
  $("#preview-title").textContent = title;
  $("#preview-kind").textContent = kind;
  const body = $("#preview-body");
  body.replaceChildren();
  if (previewBlob) URL.revokeObjectURL(previewBlob);
  previewBlob = URL.createObjectURL(
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
      .then(({ pdf }) => pdf(body, source))
      .catch((e) => body.append(el("p", "notice", e.message)));
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
  if (previewBlob) URL.revokeObjectURL(previewBlob);
  previewBlob = null;
});
async function copyText(source) {
  try {
    await navigator.clipboard.writeText(source);
  } catch {
    notice("Clipboard unavailable. Select the text to copy it.");
  }
}
async function renderDiagram(button, id) {
  const code = document.getElementById(id),
    figure = code?.closest("figure");
  if (!figure) return;
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
    button.textContent = "Diagram ready";
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
    if (button.dataset.copy) copyText(code.textContent);
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
        code.textContent,
        code.dataset.language === "mermaid" ? "diagram" : "text",
      );
    return;
  }
  const a = e.target.closest(".prose a");
  if (a) {
    const href = a.getAttribute("href");
    if (href && !/^(https?:|mailto:|#)/i.test(href)) {
      e.preventDefault();
      try {
        const r = await fetch(
          "/api/file?id=" +
            encodeURIComponent(view.id) +
            "&path=" +
            encodeURIComponent(href),
        );
        if (!r.ok) {
          const j = await r.json();
          throw Error(j.error);
        }
        const type = r.headers.get("Content-Type") || "";
        if (type === "application/pdf") {
          preview(href, "PDF", await r.arrayBuffer(), "pdf");
        } else if (type.startsWith("image/")) {
          const blob = await r.blob();
          const url = URL.createObjectURL(blob);
          preview(href, "Image", url, "image");
          $("#preview").addEventListener(
            "close",
            () => URL.revokeObjectURL(url),
            { once: true },
          );
        } else {
          const source = await r.text();
          preview(
            href,
            href.split(".").pop(),
            source,
            /\.(html?|svg)$/.test(href)
              ? "html"
              : /\.md$/.test(href)
                ? "markdown"
                : /\.json$/.test(href)
                  ? "json"
                  : "text",
          );
        }
      } catch (error) {
        notice(error.message, true);
      }
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
    $(".sidebar-footer small").textContent = status.remote
      ? "Private connection · Tailscale"
      : "Private local connection";
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
