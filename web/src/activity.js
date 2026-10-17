// Presentation only: execution and result ownership stay on the host.
const node = (tag, className, value) => {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (value !== undefined) element.textContent = value;
  return element;
};
const verbs = {
  bash: ["Running command", "Ran command", "›_"],
  read: ["Reading", "Read", "▤"],
  edit: ["Editing", "Edited", "✎"],
  write: ["Writing", "Wrote", "▧"],
  view_image: ["Inspecting image", "Inspected image", "▣"],
  ask_user_question: ["Asking a question", "Asked a question", "?"],
};
export function elapsedLabel(milliseconds) {
  const seconds = Math.max(0, milliseconds) / 1000;
  return seconds < 60 ? `${seconds.toFixed(1)}s` : `${Math.floor(seconds / 60)}m ${Math.floor(seconds % 60)}s`;
}
export function setToolInput(item, command, args) {
  if (command) item.command = String(command);
  if (args && Object.keys(args).length) {
    const source = JSON.stringify(args, null, 2);
    item.inputValue = source.slice(0, 65536) + (source.length > 65536 ? "\n[Input preview capped at 64K characters]" : "");
  }
  const target = item.command || "";
  item.description.textContent = item.name === "bash" ? target : target.split(/[\\/]/).pop() || target;
  item.description.title = target;
  const hasArgs = Boolean(item.inputValue);
  item.input.textContent = hasArgs ? item.inputValue : (target || "Input details unavailable in this event.").slice(0, 65536);
  item.inputHeading.textContent = hasArgs ? "Input" : item.name === "bash" ? "Command" : "Target";
}
export function refreshToolTime(item) {
  if (item.startedAt !== null && item.state === "running")
    item.time.textContent = elapsedLabel(performance.now() - item.startedAt);
}
export function createToolActivity({ name, command, args, saved, preview, copy, activity }) {
  const row = node("details", "tool-row"), summary = node("summary");
  const icon = node("span", "tool-icon", verbs[name]?.[2] || "◇");
  icon.setAttribute("aria-hidden", "true");
  const label = node("span", "tool-label", saved ? verbs[name]?.[1] || name : verbs[name]?.[0] || `Using ${name}`);
  const description = node("span", "tool-description");
  const stateLabel = node("span", "tool-state", saved ? "Saved call" : "Running");
  const time = node("span", "tool-time");
  time.title = "Elapsed time observed in this browser";
  summary.append(icon, label, description, stateLabel, time, node("span", "tool-chevron", "⌄"));
  const panel = node("div", "tool-panel");
  const heading = node("div", "tool-panel-heading", name);
  const inputHeading = node("div", "tool-section-label", "Input");
  const input = node("pre", "tool-input");
  const output = node("pre", "tool-output", saved ? "No result saved for this call." : "Waiting for output…");
  const actions = node("div", "tool-actions");
  for (const [title, action] of [["Copy result", () => copy(output.textContent)], ["Expand output", () => preview(name, "Tool result", output.textContent, "text")], ["Agent activity", activity]]) {
    const button = node("button", "", title);
    button.type = "button";
    button.onclick = action;
    actions.append(button);
  }
  panel.append(heading, inputHeading, input, node("div", "tool-section-label", "Result"), output, actions);
  row.append(summary, panel);
  const item = { row, output, name, command, label, description, stateLabel, time, input, inputHeading, startedAt: saved ? null : performance.now(), state: saved ? "saved" : "running" };
  row.classList.toggle("running", !saved);
  row.dataset.state = item.state;
  setToolInput(item, command, args);
  refreshToolTime(item);
  return item;
}
export function startToolActivity(item) {
  if (item.state !== "saved") return;
  item.state = "running";
  item.startedAt = performance.now();
  item.row.dataset.state = "running";
  item.row.classList.add("running");
  item.label.textContent = verbs[item.name]?.[0] || `Using ${item.name}`;
  item.stateLabel.textContent = "Running";
  item.output.textContent = "Waiting for output…";
  refreshToolTime(item);
}
export function finishToolActivity(item, failed) {
  refreshToolTime(item);
  item.state = failed ? "failed" : "completed";
  item.row.dataset.state = item.state;
  item.row.classList.remove("running");
  item.row.classList.add("completed");
  item.row.classList.toggle("failed", failed);
  item.label.textContent = failed ? "Failed · " + item.name : verbs[item.name]?.[1] || `Called ${item.name}`;
  item.stateLabel.textContent = failed ? "Failed" : "Done";
}
export function activitySnapshot(items, budget = 131072) {
  const chunks = [];
  let remaining = budget;
  let omitted = false;
  for (const item of [...items].reverse()) {
    const header = `${item.state === "running" ? "● Running" : item.state === "failed" ? "× Failed" : item.state === "saved" ? "○ Saved call" : "✓ Finished"} · ${item.name}${item.time.textContent ? " · " + item.time.textContent : ""}\n`;
    const input = item.command ? `${item.name === "bash" ? "$ " : "Target: "}${item.command.slice(0, 4096)}\n` : "";
    const space = remaining - header.length - input.length - 2;
    if (space <= 0) { omitted = true; break; }
    const output = item.output.textContent;
    const body = output.length > space ? `[Earlier output omitted]\n${output.slice(-Math.max(0, space - 32))}` : output;
    chunks.unshift(header + input + body);
    remaining -= header.length + input.length + body.length + 2;
    if (output.length > space) { omitted = true; break; }
  }
  return (omitted ? "[Earlier activity omitted; inspect individual steps for their full preview.]\n\n" : "") + (chunks.join("\n\n") || "No tool activity yet.");
}

const consoles = new WeakMap();
// Never route execution output through the prose/code highlighter. These are
// literal, bounded records; all labels and output use textContent.
export function renderActivityConsole(root, iterable) {
  let state = consoles.get(root);
  if (!state || !root.contains(state.list)) {
    root.classList.remove("activity-wrap");
    const toolbar = node("div", "activity-console-toolbar");
    const count = node("p", "activity-console-count");
    const controls = node("div", "activity-console-controls");
    const filter = node("select");
    filter.setAttribute("aria-label", "Filter activity");
    for (const [value, label] of [["all", "All steps"], ["running", "Running"], ["failed", "Failed"]]) {
      const option = node("option", "", label); option.value = value; filter.append(option);
    }
    const list = node("div", "activity-entries");
    const empty = node("p", "activity-console-empty", "No tool activity yet.");
    state = { list, count, filter, empty, entries: new Map() };
    const applyFilter = () => {
      let visible = 0;
      for (const entry of state.entries.values()) {
        entry.row.hidden = filter.value !== "all" && entry.row.dataset.state !== filter.value;
        if (!entry.row.hidden) visible++;
      }
      empty.hidden = visible > 0;
      empty.textContent = state.entries.size ? "No steps match this filter." : "No tool activity yet.";
    };
    state.applyFilter = applyFilter;
    filter.onchange = applyFilter;
    controls.append(filter);
    for (const [label, open] of [["Expand all", true], ["Collapse all", false]]) {
      const button = node("button", "quiet", label); button.type = "button";
      button.onclick = () => { for (const entry of state.entries.values()) if (!entry.row.hidden) entry.row.open = open; };
      controls.append(button);
    }
    const wrap = node("button", "quiet", "Wrap lines");
    wrap.type = "button";
    wrap.setAttribute("aria-pressed", "false");
    wrap.onclick = () => {
      state.wrap = !state.wrap;
      root.classList.toggle("activity-wrap", state.wrap);
      wrap.setAttribute("aria-pressed", String(state.wrap));
    };
    controls.append(wrap);
    toolbar.append(count, controls);
    root.replaceChildren(toolbar, list, empty);
    consoles.set(root, state);
  }
  const all = [...iterable], items = all.slice(-48), keep = new Set();
  let remaining = 131072;
  const outputs = new Map();
  for (const item of [...items].reverse()) {
    const limit = Math.min(8192, remaining);
    if (!limit) { outputs.set(item, ""); continue; }
    const source = item.output.textContent;
    let result = source;
    if (source.length > limit) {
      const marker = "\n[… output omitted; inspect the step in the conversation for more …]\n";
      const room = Math.max(0, limit - marker.length);
      const head = Math.min(1024, Math.floor(room / 4)), tail = room - head;
      result = source.slice(0, head) + marker.slice(0, limit) + (tail ? source.slice(-tail) : "");
    }
    if (source.length <= limit && /^[\s]*[\[{]/.test(source)) {
      try {
        const parsed = JSON.parse(source);
        if (parsed !== null && typeof parsed === "object") {
          const formatted = JSON.stringify(parsed, null, 2);
          if (formatted.length <= limit) result = formatted;
        }
      } catch { /* Shell and native text remain literal. */ }
    }
    remaining = Math.max(0, remaining - result.length);
    outputs.set(item, result);
  }
  for (const [index, item] of items.entries()) {
    const key = item.row.dataset.callId || item;
    keep.add(key);
    let entry = state.entries.get(key);
    if (!entry) {
      const row = node("details", "activity-entry"), summary = node("summary", "activity-entry-summary");
      const heading = node("span", "activity-entry-heading"), title = node("strong"), meta = node("small");
      const badge = node("span", "activity-entry-status");
      heading.append(title, meta);
      summary.append(heading, badge, node("span", "activity-entry-chevron", "⌄"));
      const content = node("div", "activity-entry-content");
      const commandSection = node("section"), commandLabel = node("h3"), command = node("pre", "activity-command");
      commandSection.append(commandLabel, command);
      const outputSection = node("section"), output = node("pre", "activity-result");
      outputSection.append(node("h3", "", "Result"), output);
      content.append(commandSection, outputSection);
      row.append(summary, content);
      row.open = index >= items.length - 2 || item.state === "running" || item.state === "failed";
      entry = { row, title, meta, badge, commandSection, commandLabel, command, output };
      state.entries.set(key, entry);
      state.list.append(row);
    }
    entry.row.dataset.state = item.state;
    entry.title.textContent = `${all.length - items.length + index + 1}. ${item.label.textContent}`;
    entry.meta.textContent = item.name + (item.time.textContent ? " · " + item.time.textContent : "");
    entry.badge.textContent = ({ running: "Running", failed: "Failed", saved: "Saved call", completed: "Finished" })[item.state] || "Saved";
    entry.commandSection.hidden = !item.command;
    entry.commandLabel.textContent = item.name === "bash" ? "Command" : "Target";
    entry.command.textContent = (item.command || "").slice(0, 4096);
    const result = outputs.get(item) || "Output omitted from this bounded activity window.";
    if (entry.output.textContent !== result) entry.output.textContent = result;
  }
  for (const [key, entry] of state.entries) if (!keep.has(key)) {
    entry.row.remove(); state.entries.delete(key);
  }
  const running = items.filter(item => item.state === "running").length;
  const failed = items.filter(item => item.state === "failed").length;
  state.count.textContent = `${all.length > 48 ? "Latest 48 of " : ""}${all.length} steps · ${running} running · ${failed} failed`;
  state.applyFilter();
}
