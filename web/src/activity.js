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
