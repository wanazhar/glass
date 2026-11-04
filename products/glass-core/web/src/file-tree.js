// Lazy file explorer. Browsing never starts a model request or executes a file.
const node = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text !== undefined) n.textContent = text; return n; };
const icon = directory => {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24"); svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS(svg.namespaceURI, "path");
  path.setAttribute("d", directory ? "M3 7V5h7l2 2h9v13H3Z" : "M6 3h8l4 4v14H6Z M14 3v5h4"); svg.append(path); return svg;
};
export function fileTree({ result, api, openFile, showHidden, signal, sortBySize = false }) {
  const tree = node("ul", "workspace-tree"); tree.setAttribute("role", "tree"); tree.setAttribute("aria-label", "Workspace files");
  let count = 0, selected = null, typed = "", typedAt = 0;
  const visible = () => [...tree.querySelectorAll('[role="treeitem"]')].filter(n => !n.closest('[role="group"][hidden]'));
  function focus(item) {
    if (!item) return;
    for (const n of tree.querySelectorAll('[role="treeitem"]')) { n.tabIndex = -1; n.setAttribute("aria-selected", String(n === item)); }
    selected = item; item.tabIndex = 0; item.focus({ preventScroll: true }); item.scrollIntoView({ block: "nearest" });
  }
  function populate(parent, page, level) {
    const entries = [...(page.entries || [])];
    if (sortBySize) entries.sort((a, b) => (b.bytes || 0) - (a.bytes || 0));
    for (const entry of entries) {
      if (!showHidden && entry.name.startsWith(".")) continue;
      if (count >= 256) { parent.append(node("li", "file-tree-limit", "Explorer limit reached. Open this folder to browse more.")); break; }
      count++;
      const directory = entry.kind === "directory";
      const full = `${page.path.replace(/\/$/, "")}/${entry.name}`;
      const item = node("li", "workspace-tree-item"); item.setAttribute("role", "treeitem"); item.setAttribute("aria-level", String(level)); item.setAttribute("aria-label", entry.name); item.setAttribute("aria-selected", "false"); item.tabIndex = -1;
      if (entry.kind === "special") item.setAttribute("aria-disabled", "true");
      item.dataset.kind = entry.kind; item.dataset.name = entry.name.toLowerCase(); item.dataset.path = full;
      item.dataset.extension = directory ? "folder" : entry.name.split(".").pop()?.toLowerCase() || "file";
      const row = node("div", "file-tree-row");
      const arrow = node("span", "file-tree-arrow", directory ? "›" : ""); arrow.setAttribute("aria-hidden", "true");
      row.append(arrow, icon(directory), node("span", "file-tree-name", entry.name)); item.append(row);
      const group = node("ul", "file-tree-children"); group.setAttribute("role", "group"); group.hidden = true;
      if (directory) { item.setAttribute("aria-expanded", "false"); item.append(group); }
      let loaded = false, loading = false;
      item.openDirectory = async (open = true) => {
        if (!directory) return;
        group.hidden = !open; item.setAttribute("aria-expanded", String(open));
        if (!open || loaded || loading) return;
        loading = true; item.setAttribute("aria-busy", "true"); group.replaceChildren(node("li", "file-tree-loading", "Loading…"));
        try {
          const next = await api(`browse?path=${encodeURIComponent(full)}`, undefined, signal);
          if (signal?.aborted || !tree.isConnected) return;
          group.replaceChildren(); populate(group, next, level + 1); loaded = true;
          if (!group.children.length) group.append(node("li", "file-tree-empty", "Empty folder"));
        } catch (error) {
          if (error.name !== "AbortError") { group.replaceChildren(node("li", "file-tree-error", "Could not load folder. Close and reopen to retry.")); }
        } finally { loading = false; item.removeAttribute("aria-busy"); }
      };
      const activate = () => {
        focus(item);
        if (directory) item.openDirectory(item.getAttribute("aria-expanded") !== "true");
        else if (entry.kind !== "special") openFile(entry.name, page.path, entry.kind);
      };
      row.onclick = event => { event.stopPropagation(); activate(); };
      item.onfocus = event => { if (event.target === item) { selected = item; item.setAttribute("aria-selected", "true"); } };
      item.onkeydown = event => {
        if (event.target !== item) return;
        const items = visible(), index = items.indexOf(item);
        const parentItem = item.parentElement.closest('[role="treeitem"]');
        switch (event.key) {
          case "ArrowDown": focus(items[index + 1]); break;
          case "ArrowUp": focus(items[index - 1]); break;
          case "Home": focus(items[0]); break;
          case "End": focus(items.at(-1)); break;
          case "ArrowRight": if (directory) { if (item.getAttribute("aria-expanded") !== "true") item.openDirectory(); else focus(group.querySelector('[role="treeitem"]')); } break;
          case "ArrowLeft": if (directory && item.getAttribute("aria-expanded") === "true") item.openDirectory(false); else focus(parentItem); break;
          case "Enter": case " ": activate(); break;
          default:
            if (event.key.length !== 1 || event.ctrlKey || event.metaKey || event.altKey) return;
            typed = performance.now() - typedAt > 700 ? event.key.toLowerCase() : typed + event.key.toLowerCase(); typedAt = performance.now();
            focus([...items.slice(index + 1), ...items.slice(0, index + 1)].find(n => n.dataset.name.startsWith(typed)));
        }
        event.preventDefault(); event.stopPropagation();
      };
      parent.append(item);
    }
    if (page.next != null && count < 256) {
      const moreRow = node("li", "file-tree-more"), more = node("button", "quiet", "Load more files"); more.type = "button";
      more.onclick = async () => {
        more.disabled = true; more.textContent = "Loading…";
        try {
          const next = await api(`browse?path=${encodeURIComponent(page.path)}&offset=${page.next}`, undefined, signal);
          if (!signal?.aborted && tree.isConnected) { moreRow.remove(); populate(parent, next, level); }
        } catch (error) { if (error.name !== "AbortError") more.textContent = "Retry loading files"; }
        finally { more.disabled = false; }
      };
      moreRow.append(more); parent.append(moreRow);
    }
  }
  populate(tree, result, 1);
  const first = tree.querySelector('[role="treeitem"]'); if (first) first.tabIndex = 0;
  else tree.append(node("li", "file-tree-empty", result.entries?.length ? "Dotfiles hidden" : "Empty directory"));
  tree.addEventListener("focusout", event => {
    if (!tree.contains(event.relatedTarget) && selected) selected.tabIndex = 0;
  });
  return tree;
}
