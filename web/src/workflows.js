// Loaded only when a work surface is opened. Native actor remains the writer.
export function workflows({
  api,
  command,
  view,
  el,
  notice,
  saveDraft,
  onAttached,
}) {
  let modal,
    controller,
    mayLeave,
    uploadController,
    hasUnsaved = () => false;
  window.addEventListener("beforeunload", (event) => {
    if (hasUnsaved()) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
  function open(title, floating = false) {
    if (mayLeave && !mayLeave())
      throw Error("The file editor still has unsaved changes.");
    mayLeave = null;
    hasUnsaved = () => false;
    controller?.abort();
    controller = new AbortController();
    const own = controller,
      id = view.id;
    const current = () =>
      modal?.open &&
      own === controller &&
      id === view.id &&
      !own.signal.aborted;
    modal ||= document.body.appendChild(el("dialog", "workflow"));
    modal.classList.toggle("floating-palette", floating);
    modal.oncancel = null;
    delete modal.dataset.pending;
    modal.replaceChildren();
    modal.dataset.thread = view.id || "";
    const header = el("header"),
      heading = el("h2", "", title),
      close = el("button", "icon-button", "×");
    close.type = "button";
    close.setAttribute("aria-label", "Close " + title);
    close.onclick = () => modal.close();
    header.append(heading, close);
    const body = el("div", "workflow-body"),
      status = el("footer", "workflow-status");
    status.setAttribute("role", "status");
    modal.append(header, body, status);
    modal.onclose = () => {
      if (!modal.open) {
        own.abort();
        mayLeave = null;
        hasUnsaved = () => false;
      }
    };
    if (!modal.open) modal.showModal();
    return {
      body,
      status,
      signal: own.signal,
      current,
      run: (status, name, values, refresh) =>
        mutate(current, status, name, values, refresh),
    };
  }
  async function mutate(current, status, name, values, refresh) {
    if (!current() || modal.dataset.pending) return;
    if (!view.workerWorkflows && ["file", "git"].includes(name)) {
      status.textContent =
        "This thread runtime is incompatible with these controls. The host backend needs an update.";
      return;
    }
    if (!view.control) {
      status.textContent = "Take control before making changes.";
      return;
    }
    status.textContent = "Working…";
    const surface = modal,
      token = String(Date.now()) + Math.random();
    surface.dataset.pending = token;
    try {
      await command(name, values);
      if (!current()) return;
      status.textContent = "Saved";
      if (refresh) await refresh();
    } catch (e) {
      if (current()) status.textContent = e.message;
    } finally {
      if (surface.dataset.pending === token) delete surface.dataset.pending;
    }
  }
  const button = (title, fn, cls = "quiet") => {
    const n = el("button", cls, title);
    n.type = "button";
    n.onclick = fn;
    return n;
  };
  function requireHost(writer = false) {
    if (!view.hostWorkflows)
      throw Error("This feature needs the updated Glass gateway.");
    if (writer && !view.workerWorkflows) {
      throw Error(
        "This thread runtime is incompatible with these controls. The host backend needs an update.",
      );
    }
  }
  async function attach(files) {
    try {
      requireHost(true);
    } catch (e) {
      notice(e.message, true);
      return;
    }
    if (!view.id || !view.control) {
      notice("Open a thread and take control before attaching files.");
      return;
    }
    if (view.attaching) {
      notice("An attachment is already being saved.");
      return;
    }
    view.attaching = true;
    uploadController?.abort();
    uploadController = new AbortController();
    const uploadSignal = uploadController.signal;
    const id = view.id,
      generation = view.generation;
    if (files.length > 8) notice("Attach up to 8 files at a time.");
    try {
      for (const file of [...files].slice(0, 8)) {
        const image = /^image\/(png|jpeg|gif|webp)$/.test(file.type),
          max = image ? 8 * 1024 * 1024 : 32768;
        if (!file.size || file.size > max) {
          notice(
            file.name +
              (image
                ? " exceeds 8 MiB."
                : " must be a UTF-8 text file of at most 32 KiB."),
            true,
          );
          continue;
        }
        try {
          notice("Attaching " + file.name + "…");
          const timeout = setTimeout(() => uploadController?.abort(), 20000);
          let r, data;
          try {
            r = await fetch(
              "/api/upload?id=" +
                encodeURIComponent(id) +
                "&name=" +
                encodeURIComponent(file.name),
              {
                method: "POST",
                headers: { "Content-Type": "application/octet-stream" },
                body: file,
                signal: uploadSignal,
              },
            );
            data = await r.json();
          } finally {
            clearTimeout(timeout);
          }
          if (!r.ok) throw Error(data.error || "Upload rejected");
          if (id !== view.id || generation !== view.generation) {
            notice("Thread changed. Attachment was not added.");
            return;
          }
          await command("attach", {
            upload: data.token,
            upload_name: file.name,
            args: "",
          });
          if (id === view.id && generation === view.generation) {
            onAttached(file.name);
            notice("Attached " + file.name + " to future requests.");
          }
        } catch (e) {
          if (generation === view.generation)
            notice(
              e.name === "AbortError"
                ? "Attachment interrupted or timed out. Retry when connected."
                : e.message,
              true,
            );
          if (uploadSignal.aborted) break;
        }
      }
    } finally {
      view.attaching = false;
    }
  }
  async function editor(path) {
    requireHost(true);
    const { body, status, signal, current, run } = open(path);
    status.textContent = "Loading file…";
    try {
      const data = await api(
        "edit?id=" +
          encodeURIComponent(view.id) +
          "&path=" +
          encodeURIComponent(path),
        undefined,
        signal,
      );
      if (!current()) return;
      const area = el("textarea", "file-editor");
      area.value = data.text;
      area.spellcheck = false;
      area.setAttribute("aria-label", "Edit " + path);
      let saved = data.text,
        revision = data.revision;
      const save = button(
        "Save file",
        async () => {
          const captured = area.value;
          save.disabled = true;
          try {
            if (!current()) return;
            if (!view.control) throw Error("Take control before saving files.");
            const result = await command("file", {
              action: "write",
              path: data.path,
              expected: revision,
              text: captured,
              confirmed: true,
            });
            if (!current()) return;
            saved = captured;
            if (result.body?.revision) revision = result.body.revision;
            else {
              const latest = await api(
                "edit?id=" +
                  encodeURIComponent(view.id) +
                  "&path=" +
                  encodeURIComponent(path),
                undefined,
                signal,
              );
              if (latest.text !== captured)
                throw Error(
                  "File changed after saving. Reopen it before the next write.",
                );
              revision = latest.revision;
            }
            status.textContent = "Saved. Workspace file updated.";
          } catch (e) {
            if (current()) status.textContent = e.message;
          } finally {
            save.disabled = false;
          }
        },
        "command-run",
      );
      const actions = el("div", "workflow-actions");
      actions.append(
        save,
        button("Attach file", () =>
          run(status, "attach", { args: data.path }, () =>
            onAttached(data.path),
          ),
        ),
      );
      area.oninput = () => {
        status.textContent = area.value === saved ? "Saved" : "Unsaved changes";
      };
      const close = modal.querySelector("header button");
      const mayClose = () =>
        area.value === saved ||
        confirm("Discard unsaved changes to " + path + "?");
      mayLeave = mayClose;
      hasUnsaved = () => area.value !== saved;
      close.onclick = () => {
        if (mayClose()) modal.close();
      };
      modal.oncancel = (e) => {
        if (!mayClose()) e.preventDefault();
      };
      body.append(actions, area);
      status.textContent =
        "UTF-8 text · " +
        (data.maxBytes || 32768) +
        " byte limit · conflicts are rejected";
    } catch (e) {
      if (!signal.aborted) status.textContent = e.message;
    }
  }
  async function changes() {
    requireHost(false);
    const { body, status, signal, current, run } = open("Changes", true);
    const id = view.id;
    async function refresh() {
      status.textContent = "Reading Git state…";
      const data = await api(
        "changes?id=" + encodeURIComponent(id),
        undefined,
        signal,
      );
      if (!current()) return;
      body.replaceChildren();
      const summary = el(
        "pre",
        "git-status",
        typeof data.status === "string"
          ? data.status
          : JSON.stringify(data.status, null, 2),
      );
      body.append(summary);
      const fileList = el("div", "change-paths"),
        fileNav = el("div", "workflow-actions");
      let filePage = 0;
      const allFiles = data.files || [];
      const previousFiles = button("← Files", () => {
        filePage--;
        drawFiles();
      });
      const nextFiles = button("Files →", () => {
        filePage++;
        drawFiles();
      });
      fileNav.append(previousFiles, nextFiles);
      body.append(fileNav, fileList);
      function drawFiles() {
        fileList.replaceChildren();
        for (const file of allFiles.slice(filePage * 64, filePage * 64 + 64)) {
          const card = el("div", "change-path");
          card.append(el("span", "", file.path));
          if (file.worktree !== " " || file.index === "?")
            card.append(
              button("Stage", () =>
                run(
                  status,
                  "git",
                  {
                    action: "stage",
                    args: file.path,
                    expected: data.expected,
                    confirmed: true,
                  },
                  refresh,
                ),
              ),
            );
          if (file.index !== " " && file.index !== "?")
            card.append(
              button("Unstage", () =>
                run(
                  status,
                  "git",
                  {
                    action: "unstage",
                    args: file.path,
                    expected: data.expected,
                    confirmed: true,
                  },
                  refresh,
                ),
              ),
            );
          fileList.append(card);
        }
        previousFiles.disabled = filePage === 0;
        nextFiles.disabled = (filePage + 1) * 64 >= allFiles.length;
        fileNav.hidden = allFiles.length <= 64;
      }
      drawFiles();
      for (const [title, diff, staged] of [
        ["Working tree", data.diff, false],
        ["Staged", data.staged, true],
      ]) {
        const section = el("section", "change-section");
        section.append(el("h3", "", title));
        if (!diff) {
          section.append(el("p", "quiet", "No changes"));
          body.append(section);
          continue;
        }
        // Keep hunk headers with their own file headers, never construct partial line patches.
        const files = String(diff)
          .split(/(?=^diff --git )/m)
          .filter(Boolean);
        for (const file of files.slice(0, 512)) {
          const firstHunk = file.indexOf("\n@@"),
            header = firstHunk < 0 ? file : file.slice(0, firstHunk + 1);
          const rawPath =
            file.match(/^\+\+\+ b\/(.+)$/m)?.[1] ||
            file.match(/^--- a\/(.+)$/m)?.[1];
          const path = (data.files || []).find(
            (item) => item.path === rawPath,
          )?.path;
          const card = el("details", "change-file");
          card.append(el("summary", "", path || file.split("\n")[0]));
          const content = el("div");
          card.append(content);
          card.ontoggle = () => {
            if (!card.open) {
              content.replaceChildren();
              return;
            }
            if (content.children.length) return;
            const whole = button(staged ? "Unstage file" : "Stage file", () =>
              run(
                status,
                "git",
                {
                  action: staged ? "unstage" : "stage",
                  args: path,
                  expected: data.expected,
                  confirmed: true,
                },
                refresh,
              ),
            );
            if (path) content.append(whole);
            const simple =
              !/^(new file mode|deleted file mode|rename from|rename to|old mode|new mode|GIT binary patch|Binary files)/m.test(
                file,
              );
            const hunks =
              firstHunk < 0 || !simple
                ? []
                : file
                    .slice(firstHunk + 1)
                    .split(/(?=^@@)/m)
                    .filter(Boolean);
            if (!hunks.length) content.append(el("pre", "diff-source", file));
            for (const hunk of hunks) {
              const block = el("div", "diff-hunk"),
                patch = header + hunk;
              block.append(el("pre", "diff-source", hunk));
              block.append(
                button(staged ? "Unstage hunk" : "Stage hunk", () =>
                  run(
                    status,
                    "git",
                    {
                      action: staged ? "unstage-patch" : "stage-patch",
                      patch,
                      expected: data.expected,
                      confirmed: true,
                    },
                    refresh,
                  ),
                ),
              );
              if (!staged)
                block.append(
                  button("Discard hunk", () => {
                    if (confirm("Discard this hunk from the workspace?"))
                      run(
                        status,
                        "git",
                        {
                          action: "revert-patch",
                          patch,
                          expected: data.expected,
                          confirmed: true,
                        },
                        refresh,
                      );
                  }),
                );
              content.append(block);
            }
          };
          section.append(card);
        }
        body.append(section);
      }
      body.prepend(
        button("Refresh", () =>
          refresh().catch((e) => (status.textContent = e.message)),
        ),
      );
      status.textContent = "Changes are checked again before each write.";
    }
    try {
      await refresh();
    } catch (e) {
      if (!signal.aborted) status.textContent = e.message;
    }
  }
  async function picker(name) {
    const { body, status, signal, current, run } = open(
      name === "model" ? "Choose model" : "Execution profile",
    );
    status.textContent = "Loading…";
    try {
      const result = await command(name, { action: "list" }, true);
      if (!current()) return;
      const rows = result.body?.models || result.body?.profiles || result.body;
      const search = el("input", "workflow-search");
      search.placeholder = "Filter…";
      search.setAttribute("aria-label", "Filter choices");
      const list = el("div", "workflow-choices");
      const entries = Array.isArray(rows)
        ? rows
        : rows && typeof rows === "object"
          ? Object.entries(rows).map(([id, v]) => ({
              id,
              ...(typeof v === "object" ? v : { label: v }),
            }))
          : [];
      const draw = () => {
        list.replaceChildren();
        for (const item of entries
          .filter((v) =>
            JSON.stringify(v)
              .toLowerCase()
              .includes(search.value.toLowerCase()),
          )
          .slice(0, 100)) {
          const key = typeof item === "string" ? item : item.id || item.name;
          if (!key) continue;
          list.append(
            button(
              typeof item === "string"
                ? item
                : item.label || item.name || item.id,
              () =>
                run(
                  status,
                  name,
                  {
                    action: name === "model" ? "set" : "use",
                    args: key,
                    confirmed: true,
                  },
                  () => {
                    if (name === "model")
                      document.querySelector("#model").textContent = key;
                    modal.close();
                  },
                ),
            ),
          );
        }
      };
      search.oninput = draw;
      body.append(search, list);
      draw();
      // Custom IDs remain first-class for providers without a model catalogue.
      if (name === "model") {
        const form = el("form", "workflow-actions"),
          input = el("input");
        input.placeholder = "Custom model ID";
        input.required = true;
        const submit = el("button", "command-run", "Use model");
        submit.type = "submit";
        form.append(input, submit);
        form.onsubmit = (e) => {
          e.preventDefault();
          run(
            status,
            "model",
            { action: "set", args: input.value.trim() },
            () => {
              document.querySelector("#model").textContent = input.value.trim();
              modal.close();
            },
          );
        };
        body.append(form);
      }
      status.textContent = entries.length
        ? "Choose a saved option"
        : "No saved choices available.";
    } catch (e) {
      if (!signal.aborted) status.textContent = e.message;
    }
  }
  async function queue() {
    const { body, status, signal, current, run } = open("Follow-ups");
    const refresh = async () => {
      const result = await command("queue", { action: "list" }, true);
      if (!current()) return;
      body.replaceChildren(
        el(
          "p",
          "quiet",
          result.body?.delivery || "Saved follow-ups remain on the host.",
        ),
      );
      for (const item of result.body?.queued || []) {
        const card = el("div", "detail-card");
        card.append(el("p", "", item.text || "Queued message"));
        card.append(
          button("Remove", () =>
            run(
              status,
              "queue",
              { action: "remove", args: item.request_id },
              refresh,
            ),
          ),
        );
        body.append(card);
      }
      (result.body?.saved || []).slice(0, 64).forEach((item, i) => {
        const card = el("div", "detail-card"),
          area = el("textarea", "queue-editor");
        const label =
          typeof item === "string" ? item : item.text || JSON.stringify(item);
        area.value = label
          .replace(/^\d+[.)]\s*/, "")
          .replace(/^(Steer|Queued|Queue|Follow-up):\s*/i, "");
        area.maxLength = 60000;
        area.setAttribute("aria-label", "Saved follow-up " + (i + 1));
        card.append(area);
        for (const [title, action] of [
          ["Save edit", "edit"],
          ["Move earlier", "earlier"],
          ["Move later", "later"],
          ["Use as correction", "promote"],
          ["Remove", "remove"],
        ])
          card.append(
            button(title, () =>
              run(
                status,
                "queue",
                {
                  action: "saved-" + action,
                  args: String(i + 1),
                  text: area.value,
                },
                refresh,
              ),
            ),
          );
        body.append(card);
      });
      if (!(result.body?.queued?.length || result.body?.saved?.length))
        body.append(el("p", "quiet", "No queued follow-ups."));
      status.textContent =
        "Live follow-ups can be removed. Saved terminal follow-ups can also be edited and reordered.";
    };
    try {
      await refresh();
    } catch (e) {
      if (!signal.aborted) status.textContent = e.message;
    }
  }
  async function work() {
    const { body, status, signal, current, run } = open("Work & teammates", true);
    const id = view.id,
      tasks = el("section"),
      agents = el("section"),
      taskRows = el("div"),
      agentRows = el("div");
    tasks.append(el("h3", "", "Tasks"), taskRows);
    agents.append(el("h3", "", "Teammates"), agentRows);
    body.append(tasks, agents);
    const form = el("form", "workflow-actions"),
      input = el("input");
    input.required = true;
    input.maxLength = 1000;
    input.placeholder = "New task…";
    input.setAttribute("aria-label", "New task");
    const add = el("button", "quiet", "Add task");
    add.type = "submit";
    form.append(input, add);
    tasks.append(form);
    form.onsubmit = (e) => {
      e.preventDefault();
      run(
        status,
        "todo",
        { action: "add", args: input.value, confirmed: true },
        () => {
          input.value = "";
          status.textContent =
            "Task added. Updates arrive with session activity.";
        },
      );
    };
    let timer,
      taskSignature = "";
    const cards = new Map();
    async function refresh() {
      if (!current()) return;
      if (document.visibilityState !== "visible") {
        timer = setTimeout(refresh, 5000);
        return;
      }
      const signature = JSON.stringify(view.tasks || []);
      if (signature !== taskSignature) {
        taskSignature = signature;
        taskRows.replaceChildren();
        for (const task of (view.tasks || []).slice(0, 64)) {
          const card = el("div", "detail-card");
          card.append(
            el(
              "p",
              "",
              typeof task === "string"
                ? task
                : (task.status || "pending") +
                    " · " +
                    (task.text || task.title || task.id),
            ),
          );
          if (
            typeof task === "object" &&
            task.id &&
            task.status !== "completed"
          )
            card.append(
              button("Mark complete", () =>
                run(
                  status,
                  "todo",
                  { action: "complete", args: task.id, confirmed: true },
                  refresh,
                ),
              ),
            );
          taskRows.append(card);
        }
        if (!taskRows.children.length)
          taskRows.append(el("p", "quiet", "No recorded tasks."));
      }
      try {
        const data = await api(
          "agents?id=" + encodeURIComponent(id),
          undefined,
          signal,
        );
        if (!current()) return;
        const rows = (Array.isArray(data) ? data : data.agents || []).slice(
            0,
            64,
          ),
          present = new Set();
        agentRows.querySelector(".empty-agents")?.remove();
        for (const agent of rows) {
          const key = agent.id || agent.name;
          present.add(key);
          let entry = cards.get(key);
          if (!entry) {
            const card = el("div", "detail-card"),
              heading = el("strong"),
              state = el("p"),
              assignment = el("p", "quiet");
            card.append(heading, state, assignment);
            entry = { card, heading, state, assignment };
            cards.set(key, entry);
            card.append(
              button("Read transcript", async () => {
                try {
                  const result = await api(
                    "agent?id=" +
                      encodeURIComponent(id) +
                      "&agent=" +
                      encodeURIComponent(agent.id),
                    undefined,
                    signal,
                  );
                  if (!current()) return;
                  let transcript = card.querySelector("pre");
                  if (!transcript) {
                    transcript = el("pre", "agent-transcript");
                    card.append(transcript);
                  }
                  transcript.textContent = String(
                    typeof result.text === "string"
                      ? result.text
                      : "Transcript unavailable.",
                  ).slice(0, 262144);
                } catch (e) {
                  if (current()) status.textContent = e.message;
                }
              }),
            );
            agentRows.append(card);
          }
          entry.heading.textContent = agent.name || agent.id;
          entry.state.textContent = agent.status || "";
          entry.assignment.textContent = String(
            agent.task || agent.prompt || "",
          ).slice(0, 1000);
        }
        for (const [key, entry] of cards)
          if (!present.has(key)) {
            entry.card.remove();
            cards.delete(key);
          }
        if (!rows.length)
          agentRows.append(
            el(
              "p",
              "quiet empty-agents",
              "No teammates have been created for this thread.",
            ),
          );
        status.textContent = "Live while open · refreshes every 5 seconds";
      } catch (e) {
        if (!signal.aborted) status.textContent = e.message;
      }
      clearTimeout(timer);
      timer = setTimeout(refresh, 5000);
    }
    signal.addEventListener("abort", () => clearTimeout(timer), { once: true });
    await refresh();
  }
  async function context() {
    requireHost();
    const { body, status, signal, current, run } = open("Context");
    const id = view.id;
    async function refresh() {
      const data = await api(
        "context?id=" + encodeURIComponent(id),
        undefined,
        signal,
      );
      if (!current()) return;
      body.replaceChildren(
        el(
          "p",
          "quiet",
          "Active snapshots are supplied to future requests until removed.",
        ),
      );
      for (const item of (data.attachments || []).slice(0, 128)) {
        const card = el("div", "detail-card");
        card.append(
          el(
            "strong",
            "",
            item.source?.match(/^Uploaded (.*?) ·/)?.[1] || item.path,
          ),
          el(
            "p",
            "quiet",
            (item.mediaType || "Text snapshot") + " · " + item.bytes + " bytes",
          ),
        );
        card.append(
          button("Remove from context", () =>
            run(
              status,
              "context",
              { action: "remove", args: item.path },
              async () => {
                onAttached();
                await refresh();
              },
            ),
          ),
        );
        body.append(card);
      }
      if (!data.attachments?.length)
        body.append(el("p", "quiet", "No attached snapshots."));
      const form = el("form", "workflow-actions"),
        input = el("input");
      input.placeholder = "Project file path";
      input.required = true;
      input.setAttribute("aria-label", "Project file path to attach");
      const attach = el("button", "quiet", "Attach file");
      attach.type = "submit";
      form.append(input, attach);
      form.onsubmit = (e) => {
        e.preventDefault();
        run(status, "attach", { args: input.value }, async () => {
          onAttached();
          await refresh();
        });
      };
      body.append(form);
      status.textContent = data.usage
        ? JSON.stringify(data.usage)
        : "Context usage is not reported. No percentage is estimated.";
    }
    try {
      await refresh();
    } catch (e) {
      if (!signal.aborted) status.textContent = e.message;
    }
  }
  async function reasoningArchive(offsets) {
    requireHost();
    const { body, status } = open("Saved reasoning");
    for (const [i, offset] of [...offsets].entries())
      body.append(
        button("Read saved reasoning " + (i + 1), () => largeRecord(offset)),
      );
    status.textContent =
      "Saved source is paged independently of the live reading window.";
  }
  async function largeRecord(recordOffset) {
    requireHost(false);
    const { body, status, signal, current } = open("Saved activity");
    const id = view.id,
      actions = el("div", "workflow-actions"),
      source = el("pre", "saved-activity-source");
    let offset = 0,
      revision,
      nextOffset = 0,
      pages = [],
      busy = false;
    const previous = button("← Previous", () => {
        if (pages.length) load(pages.at(-1), false);
      }),
      next = button("Next →", () => load(nextOffset, true));
    actions.append(previous, next);
    body.append(actions, source);
    async function load(target, forward) {
      if (busy || !current()) return;
      busy = true;
      previous.disabled = true;
      next.disabled = true;
      status.textContent = "Reading saved activity…";
      try {
        const part = await api(
          "activity-text?id=" +
            encodeURIComponent(id) +
            "&recordOffset=" +
            recordOffset +
            "&offset=" +
            target +
            (revision ? "&expected=" + encodeURIComponent(revision) : ""),
          undefined,
          signal,
        );
        if (!current()) return;
        if (forward) pages.push(offset);
        else if (target !== offset && pages.length) pages.pop();
        offset = target;
        revision = part.revision;
        nextOffset = part.nextOffset;
        source.textContent = part.text;
        previous.disabled = !pages.length;
        next.disabled = part.eof;
        status.textContent =
          "Saved " +
          (part.kind || "activity") +
          " · " +
          Math.round(offset / 1024) +
          "–" +
          Math.round(nextOffset / 1024) +
          " KiB of " +
          Math.round(part.totalBytes / 1024) +
          " KiB";
        body.scrollTop = 0;
      } catch (e) {
        if (!signal.aborted) {
          status.textContent = e.message;
          previous.disabled = !pages.length;
          next.disabled = false;
        }
      } finally {
        busy = false;
      }
    }
    await load(0, false);
  }
  return {
    attach,
    editor,
    changes,
    picker,
    queue,
    work,
    context,
    largeRecord,
    reasoningArchive,
    prepareLeave: () => !mayLeave || mayLeave(),
    close: () => {
      controller?.abort();
      uploadController?.abort();
      modal?.close();
    },
  };
}
