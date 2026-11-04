// Presentation components driven by native records and actual browser events.
const node = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
};
export function progressRing(value, label) {
  const ring = node("span", "progress-ring");
  ring.setAttribute("role", "progressbar");
  ring.setAttribute("aria-label", label);
  if (Number.isFinite(value)) {
    const percent = Math.max(0, Math.min(100, value));
    ring.style.setProperty("--progress", `${percent}%`);
    ring.setAttribute("aria-valuenow", String(Math.round(percent)));
    ring.setAttribute("aria-valuemin", "0"); ring.setAttribute("aria-valuemax", "100");
  } else ring.classList.add("indeterminate");
  return ring;
}
const checklists = new WeakMap();
export function renderChecklist(root, tasks) {
  root.hidden = !Array.isArray(tasks) || !tasks.length;
  if (root.hidden) { root.replaceChildren(); checklists.delete(root); return; }
  const rows = tasks.slice(0, 128).filter(t => t && typeof t === "object");
  if (!rows.length) { root.hidden = true; return; }
  const signature = JSON.stringify(rows);
  if (checklists.get(root)?.signature === signature) return;
  const previous = checklists.get(root);
  const finished = rows.every(t => ["completed", "abandoned"].includes(t.status));
  const open = finished && !previous?.finished ? false : (previous?.card.open ?? !finished);
  const card = node("details", "task-card"); card.open = open;
  const completed = rows.filter(t => t.status === "completed").length;
  const remaining = rows.filter(t => !["completed", "abandoned"].includes(t.status)).length;
  const summary = node("summary", "task-card-heading");
  summary.append(progressRing(100 * completed / rows.length, "Checklist completion"), node("strong", "", remaining ? `${remaining} ${remaining === 1 ? "step" : "steps"} left` : "All steps complete"), node("span", "task-disclosure", "−"));
  const list = node("ol", "task-card-list");
  for (const task of rows) {
    const row = node("li", "task-card-row");
    row.dataset.status = task.status || "pending";
    const icon = node("span", "task-status-icon", task.status === "completed" ? "✓" : task.status === "blocked" ? "!" : "");
    icon.setAttribute("aria-label", (task.status || "pending").replaceAll("_", " "));
    const label = node("div", "task-card-label");
    label.append(node("span", "", task.title || task.text || "Untitled task"));
    if (task.blocker) label.append(node("small", "", task.blocker));
    row.append(icon, label); list.append(row);
  }
  card.append(summary, list); root.replaceChildren(card);
  checklists.set(root, { signature, card, finished });
}
const stages = new WeakMap();
export function updateStage(root, label) {
  let state = stages.get(root);
  if (!state) {
    const sigil = node("span", "agent-sigil"); sigil.setAttribute("aria-hidden", "true");
    for (let i = 0; i < 9; i++) { const dot = node("i"); dot.style.setProperty("--i", i); sigil.append(dot); }
    root.querySelector(".activity-dot")?.replaceWith(sigil);
    const time = node("span", "agent-elapsed");
    root.querySelector("#activity-text").after(time);
    state = { time, started: null, timer: null }; stages.set(root, state);
    const tick = () => {
      if (state.started === null || document.hidden) return;
      const seconds = Math.floor((performance.now() - state.started) / 1000);
      state.time.textContent = seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
    };
    state.tick = tick;
    document.addEventListener("visibilitychange", tick);
  }
  const active = Boolean(label);
  if (active && state.started === null) { state.started = performance.now(); state.timer = setInterval(state.tick, 1000); }
  if (!active) { state.started = null; clearInterval(state.timer); state.time.textContent = ""; }
  root.dataset.phase = !active ? "idle" : /search|fetch/i.test(label) ? "search" : /read|inspect/i.test(label) ? "read" : /writ/i.test(label) ? "write" : "think";
  state.tick();
}
export function attachmentPreview(file, signal) {
  const row = node("div", "attachment-preview uploading");
  const image = /^image\/(png|jpeg|gif|webp)$/.test(file.type);
  let url = null;
  if (image) {
    url = URL.createObjectURL(file);
    const thumb = node("img", "attachment-thumbnail"); thumb.src = url; thumb.alt = file.name; row.append(thumb);
  } else row.append(node("span", "attachment-file-icon", file.name.split(".").pop()?.slice(0, 5).toUpperCase() || "FILE"));
  const name = node("span", "attachment-filename", file.name); name.title = file.name;
  const status = node("span", "attachment-status", "Uploading…");
  const bar = node("progress", "attachment-progress"); bar.max = 100; bar.setAttribute("aria-label", `Uploading ${file.name}`);
  const cancel = node("button", "attachment-remove", "×"); cancel.type = "button"; cancel.setAttribute("aria-label", `Cancel upload ${file.name}`);
  row.append(name, status, bar, cancel);
  let revoked = false;
  const dispose = () => { if (url && !revoked) { URL.revokeObjectURL(url); revoked = true; } };
  signal?.addEventListener("abort", dispose, { once: true });
  return {
    row, cancel, dispose,
    progress(loaded, total) {
      if (total > 0) { const value = Math.min(100, 100 * loaded / total); bar.value = value; status.textContent = value >= 100 ? "Processing…" : `${Math.floor(value)}%`; }
      else { bar.removeAttribute("value"); status.textContent = "Uploading…"; }
    },
    saving() { status.textContent = "Saving…"; bar.removeAttribute("value"); },
    finish() { dispose(); row.remove(); },
  };
}
