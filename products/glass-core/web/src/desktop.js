// Loaded only when the full-screen shared desktop surface is opened.
export function desktopSurface({ api }) {
  const $ = (id) => document.getElementById(id);
  const dialog = $("desktop-viewer");
  const opener = $("desktop-toggle");
  const closeButton = $("desktop-close");
  const viewport = $("desktop-viewport");
  const stream = $("desktop-stream");
  const placeholder = $("desktop-placeholder");
  const owner = $("desktop-owner");
  const display = $("desktop-display");
  const workspace = $("desktop-workspace");
  const workspaceFull = $("desktop-workspace-full");
  const resolution = $("desktop-resolution");
  const status = $("desktop-status");
  const leaseNote = $("desktop-lease-note");
  const textInput = $("desktop-text");
  const sendText = $("desktop-send-text");
  const keyboardRow = $("desktop-keyboard-row");
  const trackpadMode = $("desktop-trackpad");
  const touchMode = $("desktop-touch-mode");
  const touchHint = $("desktop-touch-hint");
  const keyboardOpen = $("desktop-keyboard-open");
  const shortcuts = $("desktop-shortcuts");
  const controlModifier = $("desktop-mod-control");
  const altModifier = $("desktop-mod-alt");
  const viewReset = $("desktop-view-reset");
  const cursor = $("desktop-cursor");
  const take = $("desktop-take");
  const returnToAgent = $("desktop-return");
  const reconnect = $("desktop-reconnect");
  const start = $("desktop-start");
  const stop = $("desktop-stop");
  const retry = $("desktop-retry");
  const more = $("desktop-more");
  const moreSummary = more.querySelector("summary");
  let snapshot = null;
  let connectionError = null;
  let requestController = null;
  let pollTimer = 0;
  let pollGeneration = 0;
  let streamSession = null;
  let streamFrameSeqAtStart = null;
  let streamLoaded = false;
  let streamFailed = false;
  let activePointer = null;
  let lastMoveAt = 0;
  let inputTail = Promise.resolve();
  let directTouch = false;
  let pointerX = 0.5;
  let pointerY = 0.5;
  let zoom = 1;
  let panX = 0;
  let panY = 0;
  let gesture = null;
  let pinch = null;
  const touchPoints = new Map();
  const modifiers = { Control: false, Alt: false };
  const coarsePointer = window.matchMedia("(pointer: coarse)");

  const isOpen = () => dialog.open && requestController && !requestController.signal.aborted;
  const controlled = () => isOpen() && snapshot?.available && snapshot.lease === "human";

  function setStatus(message, error = false) {
    status.textContent = message;
    status.dataset.error = error ? "true" : "false";
  }

  function closeKeyboard() {
    keyboardRow.hidden = true;
    shortcuts.hidden = true;
    keyboardOpen.textContent = "Keyboard";
    keyboardOpen.setAttribute("aria-label", "Open keyboard");
    textInput.blur();
    for (const [name, button] of [["Control", controlModifier], ["Alt", altModifier]]) {
      modifiers[name] = false;
      button.setAttribute("aria-pressed", "false");
      button.classList.remove("active");
    }
  }

  function setStream(available) {
    const session = snapshot?.sessionId || null;
    if (!available || !session) {
      stream.removeAttribute("src");
      streamSession = null;
      streamFrameSeqAtStart = null;
      streamLoaded = false;
      stream.hidden = true;
      return;
    }
    if (streamSession !== session || !stream.getAttribute("src")) {
      streamFailed = false;
      streamLoaded = false;
      streamSession = session;
      streamFrameSeqAtStart = Number(snapshot?.frameSeq) || 0;
      stream.src = "/api/desktop/stream";
    }
    stream.hidden = false;
  }

  function render(next) {
    snapshot = next;
    const running = Boolean(next?.sessionId);
    const available = Boolean(next?.available && running);
    const humanOwnsInput = next?.lease === "human";
    owner.textContent = humanOwnsInput
      ? connectionError ? "Last confirmed: you control" : "You control · GC paused"
      : available
        ? "View only · GC may control"
        : connectionError
          ? running ? "Last confirmed: GC" : "Input owner unavailable"
          : running
            ? "Desktop connection unavailable"
            : "No active input owner";
    display.textContent = next?.displayId || "No display";
    workspace.textContent = next?.workspaceRoot || "—";
    workspace.title = next?.workspaceRoot || "Workspace root unavailable";
    workspaceFull.textContent = next?.workspaceRoot || "Workspace root unavailable";
    resolution.textContent =
      Number(next?.width) > 0 && Number(next?.height) > 0
        ? `${next.width} × ${next.height}`
        : "—";
    leaseNote.hidden = !humanOwnsInput;
    take.hidden = !available || humanOwnsInput;
    returnToAgent.hidden = !humanOwnsInput;
    start.hidden = running;
    stop.hidden = !running || next?.mode !== "managedVirtualSession";
    reconnect.hidden = !running;
    more.hidden = !running;
    if (!running) $("desktop-details").open = false;
    if (more.hidden && more.contains(document.activeElement)) {
      start.focus({ preventScroll: true });
    }
    retry.hidden = !(connectionError || (running && (!available || streamFailed)));
    retry.textContent = connectionError ? "Retry connection" : "Reconnect stream";
    start.disabled = Boolean(connectionError);
    stop.disabled = Boolean(connectionError);
    take.disabled = Boolean(connectionError);
    returnToAgent.disabled = Boolean(connectionError);
    keyboardOpen.textContent = keyboardRow.hidden ? "Keyboard" : "Done";
    keyboardOpen.setAttribute("aria-label", keyboardRow.hidden ? "Open keyboard" : "Hide keyboard");
    textInput.disabled = !available || !humanOwnsInput;
    sendText.disabled = !available || !humanOwnsInput || !textInput.value;
    touchMode.disabled = !available;
    document.querySelector(".desktop-mobile-tools").hidden = !available;
    keyboardOpen.hidden = !available || !humanOwnsInput;
    keyboardOpen.disabled = !available || !humanOwnsInput;
    trackpadMode.disabled = !available;
    if (!available || !humanOwnsInput) closeKeyboard();
    for (const button of shortcuts.querySelectorAll("button")) button.disabled = !available || !humanOwnsInput;
    viewport.dataset.available = available ? "true" : "false";
    viewport.dataset.control = humanOwnsInput ? "human" : "agent";
    cursor.hidden = !coarsePointer.matches || directTouch || !controlled();

    if (!running) {
      placeholder.textContent = connectionError
        ? "Reconnect when the host is available."
        : "No desktop is open. Start a desktop to see the workspace here.";
      placeholder.hidden = false;
      setStatus(connectionError ? "Host connection unavailable" : "Ready to start", Boolean(connectionError));
      setStream(false);
      return;
    }
    if (!available) {
      placeholder.textContent = connectionError
        ? "Reconnect to check the desktop."
        : "Waiting for a fresh desktop image. The session is still running.";
      placeholder.hidden = false;
      setStatus(connectionError ? "Host connection unavailable" : "Desktop image unavailable", true);
      setStream(false);
      return;
    }

    setStream(true);
    const frameSeq = Number(next.frameSeq);
    const hasDecodedFreshFrame = stream.naturalWidth > 0
      && stream.naturalHeight > 0
      && streamSession === next.sessionId
      && Number.isFinite(frameSeq)
      && frameSeq > (streamFrameSeqAtStart ?? frameSeq - 1);
    if (hasDecodedFreshFrame && !streamFailed) {
      streamLoaded = true;
      stream.hidden = false;
    }
    const connected = hasDecodedFreshFrame && streamLoaded && !streamFailed;
    placeholder.hidden = connected;
    placeholder.textContent = streamFailed
      ? "The desktop keeps running. Reconnect the stream to continue."
      : "Connecting to the managed desktop…";
    const ageMs = Number(next.frameAgeMs);
    const recency = Number.isFinite(ageMs)
      ? ageMs < 1000
        ? "just now"
        : `${Math.round(ageMs / 1000)} sec ago`
      : null;
    setStatus(
      streamFailed
        ? "Stream disconnected · desktop is still running"
        : connected
          ? `Live · updated ${recency || "recently"}`
          : "Connecting to the managed desktop…",
      streamFailed,
    );
  }

  async function refresh() {
    if (!isOpen()) return;
    try {
      const next = await api("desktop/status", undefined, requestController.signal);
      if (isOpen()) {
        connectionError = null;
        render(next);
      }
    } catch (error) {
      if (!isOpen()) return;
      connectionError = error.message || "Connect is temporarily unavailable.";
      if (!snapshot) {
        render({
          available: false,
          sessionId: null,
          mode: null,
          displayId: null,
          workspaceRoot: "",
          lease: "agent",
          width: null,
          height: null,
          error: null,
        });
      } else {
        const previous = snapshot;
        render({ ...previous, available: false });
      }
    }
  }

  function schedulePoll(generation) {
    clearTimeout(pollTimer);
    pollTimer = setTimeout(async () => {
      if (!isOpen() || generation !== pollGeneration) return;
      await refresh();
      schedulePoll(generation);
    }, 1800);
  }

  function postInput(payload) {
    if (!controlled()) return Promise.resolve();
    inputTail = inputTail
      .catch(() => {})
      .then(async () => {
        if (!controlled()) return;
        try {
          await api("desktop/input", payload, requestController.signal);
        } catch (error) {
          if (!isOpen()) return;
          setStatus(error.message || "Desktop input was not accepted.", true);
          await refresh();
        }
      });
    return inputTail;
  }

  function moveInput(payload) {
    const now = performance.now();
    if (now - lastMoveAt < 35) return;
    lastMoveAt = now;
    void postInput(payload);
  }

  function setPointerPosition(x, y, transmit = true) {
    pointerX = Math.min(1, Math.max(0, x));
    pointerY = Math.min(1, Math.max(0, y));
    updateCursor();
    if (transmit) moveInput({ action: "pointer-move", x: pointerX, y: pointerY });
  }

  function buttonAt(button, pressed, coordinates = { x: pointerX, y: pointerY }) {
    void postInput({ action: "pointer-button", button, pressed, ...coordinates });
  }

  function clickAt(button, coordinates) {
    if (coordinates) setPointerPosition(coordinates.x, coordinates.y);
    buttonAt(button, true, coordinates);
    buttonAt(button, false, coordinates);
  }

  function point(event) {
    const bounds = viewport.getBoundingClientRect();
    const width = Number(snapshot?.width) || stream.naturalWidth;
    const height = Number(snapshot?.height) || stream.naturalHeight;
    if (!bounds.width || !bounds.height || !width || !height) return null;
    const frameScale = Math.min(bounds.width / width, bounds.height / height);
    const renderedWidth = width * frameScale;
    const renderedHeight = height * frameScale;
    const centerX = bounds.left + bounds.width / 2;
    const centerY = bounds.top + bounds.height / 2;
    const viewX = centerX + (event.clientX - centerX - panX) / zoom;
    const viewY = centerY + (event.clientY - centerY - panY) / zoom;
    const left = bounds.left + (bounds.width - renderedWidth) / 2;
    const top = bounds.top + (bounds.height - renderedHeight) / 2;
    return {
      x: Math.min(1, Math.max(0, (viewX - left) / renderedWidth)),
      y: Math.min(1, Math.max(0, (viewY - top) / renderedHeight)),
    };
  }

  function updateCursor() {
    const bounds = viewport.getBoundingClientRect();
    const width = Number(snapshot?.width);
    const height = Number(snapshot?.height);
    if (!width || !height || !bounds.width || !bounds.height) return;
    const frameScale = Math.min(bounds.width / width, bounds.height / height);
    const renderedWidth = width * frameScale;
    const renderedHeight = height * frameScale;
    const contentX = (bounds.width - renderedWidth) / 2 + pointerX * renderedWidth;
    const contentY = (bounds.height - renderedHeight) / 2 + pointerY * renderedHeight;
    cursor.style.left = `${bounds.width / 2 + (contentX - bounds.width / 2) * zoom + panX}px`;
    cursor.style.top = `${bounds.height / 2 + (contentY - bounds.height / 2) * zoom + panY}px`;
  }
  new ResizeObserver(updateCursor).observe(viewport);

  function applyViewTransform() {
    stream.style.setProperty("--desktop-zoom", String(zoom));
    stream.style.setProperty("--desktop-pan-x", `${panX}px`);
    stream.style.setProperty("--desktop-pan-y", `${panY}px`);
    viewReset.hidden = zoom <= 1.001;
    updateCursor();
  }

  function adjustPinch() {
    if (!pinch || touchPoints.size < 2) return;
    const [first, second] = [...touchPoints.values()];
    const midX = (first.x + second.x) / 2;
    const midY = (first.y + second.y) / 2;
    const distance = Math.max(1, Math.hypot(first.x - second.x, first.y - second.y));
    const bounds = viewport.getBoundingClientRect();
    const nextZoom = Math.min(4, Math.max(1, pinch.zoom * distance / pinch.distance));
    const anchorX = (pinch.midX - bounds.left - bounds.width / 2 - pinch.panX) / pinch.zoom;
    const anchorY = (pinch.midY - bounds.top - bounds.height / 2 - pinch.panY) / pinch.zoom;
    panX = midX - bounds.left - bounds.width / 2 - anchorX * nextZoom;
    panY = midY - bounds.top - bounds.height / 2 - anchorY * nextZoom;
    const maxX = bounds.width * (nextZoom - 1) / 2;
    const maxY = bounds.height * (nextZoom - 1) / 2;
    panX = Math.min(maxX, Math.max(-maxX, panX));
    panY = Math.min(maxY, Math.max(-maxY, panY));
    zoom = nextZoom;
    applyViewTransform();
  }

  function pointerButton(event, pressed) {
    const button = ["left", "middle", "right"][event.button];
    if (!button) return;
    const coordinates = point(event);
    if (!coordinates) return;
    void postInput({ action: "pointer-button", button, pressed, ...coordinates });
  }

  function setLease(action) {
    return api("desktop/lease", { action }, requestController.signal)
      .then(refresh)
      .catch(async (error) => {
        if (!isOpen()) return;
        setStatus(error.message || "Desktop input ownership changed.", true);
        await refresh();
      });
  }

  async function reconnectStream() {
    await refresh();
    if (!isOpen() || !snapshot?.available || !snapshot.sessionId) return;
    streamFailed = false;
    streamLoaded = false;
    stream.removeAttribute("src");
    streamSession = null;
    streamFrameSeqAtStart = null;
    streamLoaded = false;
    stream.hidden = false;
    placeholder.hidden = false;
    placeholder.textContent = "Reconnecting to the managed desktop…";
    requestAnimationFrame(() => {
      if (isOpen() && snapshot?.available) setStream(true);
    });
  }

  async function startDesktop() {
    start.disabled = true;
    setStatus("Starting a managed desktop on this host…");
    try {
      await api("desktop/session", { action: "start" }, requestController.signal, { timeoutMs: 30_000 });
      await refresh();
    } catch (error) {
      if (!isOpen()) return;
      if (/timed out|connection unavailable/i.test(error.message || "")) {
        connectionError = error.message;
        await refresh();
        if (connectionError) setStatus("Host connection unavailable", true);
      } else {
        setStatus(error.message || "Could not start the managed desktop.", true);
      }
    } finally {
      if (isOpen()) start.disabled = false;
    }
  }

  async function stopDesktop() {
    if (!snapshot?.sessionId || !window.confirm("Stop the managed desktop for everyone using it?")) return;
    stop.disabled = true;
    setStatus("Stopping the managed desktop…");
    try {
      await api("desktop/session", { action: "stop" }, requestController.signal);
      await refresh();
    } catch (error) {
      if (isOpen()) setStatus(error.message || "Could not stop the managed desktop.", true);
    } finally {
      if (isOpen()) stop.disabled = false;
    }
  }

  function close() {
    clearTimeout(pollTimer);
    pollGeneration += 1;
    requestController?.abort();
    requestController = null;
    activePointer = null;
    more.open = false;
    $("desktop-details").open = false;
    closeKeyboard();
    stream.removeAttribute("src");
    streamSession = null;
    // Closing disconnects the view but deliberately keeps the human input lease.
  }

  function open() {
    if (!dialog.open) dialog.showModal();
    closeButton.focus({ preventScroll: true });
    more.open = false;
    $("desktop-details").open = false;
    requestController?.abort();
    requestController = new AbortController();
    const generation = ++pollGeneration;
    streamFailed = false;
    snapshot = null;
    placeholder.hidden = false;
    placeholder.textContent = "Checking the host display…";
    void refresh().then(() => schedulePoll(generation));
  }

  closeButton.addEventListener("click", () => dialog.close());
  dialog.addEventListener("close", () => {
    close();
    opener.focus({ preventScroll: true });
  });
  take.addEventListener("click", () => void setLease("take-control"));
  returnToAgent.addEventListener("click", () => void setLease("return-to-agent"));
  function closeMore({ focus = false } = {}) {
    if (!more.open) return;
    more.open = false;
    if (focus && isOpen()) moreSummary.focus({ preventScroll: true });
  }
  function closeMoreAfterAction() {
    closeMore({ focus: true });
  }
  reconnect.addEventListener("click", () => {
    closeMoreAfterAction();
    void reconnectStream();
  });
  retry.addEventListener("click", () => {
    if (connectionError) void refresh();
    else if (snapshot?.sessionId) void reconnectStream();
    else void refresh();
  });
  start.addEventListener("click", () => void startDesktop());
  stop.addEventListener("click", () => {
    closeMoreAfterAction();
    void stopDesktop();
  });
  document.addEventListener("pointerdown", (event) => {
    if (!more.open || more.contains(event.target)) return;
    closeMore();
  }, true);
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !more.open) return;
    event.preventDefault();
    event.stopPropagation();
    closeMore({ focus: true });
  }, true);
  stream.addEventListener("load", () => {
    if (!isOpen()) return;
    if (!stream.naturalWidth || !stream.naturalHeight) {
      streamLoaded = false;
      streamFailed = true;
      stream.hidden = true;
      retry.hidden = false;
      retry.textContent = "Reconnect stream";
      placeholder.hidden = false;
      placeholder.textContent = "Reconnect the stream to see the desktop again.";
      setStatus("Desktop image unavailable", true);
      return;
    }
    streamFailed = false;
    streamLoaded = true;
    stream.hidden = false;
    retry.hidden = true;
    placeholder.hidden = true;
  });
  stream.addEventListener("error", () => {
    if (!isOpen()) return;
    streamFailed = true;
    streamLoaded = false;
    stream.hidden = true;
    retry.hidden = false;
    retry.textContent = "Reconnect stream";
    placeholder.hidden = false;
    placeholder.textContent = "The desktop keeps running. Reconnect the stream to continue.";
    setStatus("Stream disconnected", true);
  });
  viewport.addEventListener("pointerdown", (event) => {
    if (!controlled() && !(event.pointerType === "touch" && directTouch && snapshot?.available)) return;
    if (event.pointerType === "touch") {
      event.preventDefault();
      viewport.setPointerCapture(event.pointerId);
      touchPoints.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (touchPoints.size === 1) {
        const coordinates = point(event);
        if (!coordinates) return;
        gesture = {
          id: event.pointerId,
          startX: event.clientX,
          startY: event.clientY,
          lastX: event.clientX,
          lastY: event.clientY,
          startCoordinates: coordinates,
          moved: false,
          pressed: null,
          timer: setTimeout(() => {
            if (!controlled() || !gesture || gesture.id !== event.pointerId || gesture.moved) return;
            gesture.pressed = "right";
            buttonAt("right", true, directTouch ? gesture.startCoordinates : undefined);
          }, 480),
        };
      } else if (touchPoints.size === 2) {
        if (gesture?.timer) clearTimeout(gesture.timer);
        if (gesture?.pressed) buttonAt(gesture.pressed, false);
        gesture = null;
        if (directTouch) {
          const [first, second] = [...touchPoints.values()];
          const bounds = viewport.getBoundingClientRect();
          pinch = {
            distance: Math.max(1, Math.hypot(first.x - second.x, first.y - second.y)),
            midX: (first.x + second.x) / 2,
            midY: (first.y + second.y) / 2,
            zoom,
            panX,
            panY,
            bounds,
          };
        } else {
          pinch = null;
        }
      }
      return;
    }
    if (!event.isPrimary) return;
    const coordinates = point(event);
    if (!coordinates) return;
    event.preventDefault();
    activePointer = event.pointerId;
    viewport.setPointerCapture(event.pointerId);
    pointerButton(event, true);
    viewport.focus({ preventScroll: true });
  });
  viewport.addEventListener("pointermove", (event) => {
    if (!controlled() && !(event.pointerType === "touch" && directTouch && snapshot?.available)) return;
    if (event.pointerType === "touch") {
      const previous = touchPoints.get(event.pointerId);
      if (!previous) return;
      touchPoints.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (touchPoints.size >= 2) {
        event.preventDefault();
        if (directTouch) {
          adjustPinch();
        } else {
          void postInput({
            action: "scroll",
            deltaX: previous.x - event.clientX,
            deltaY: previous.y - event.clientY,
            x: pointerX,
            y: pointerY,
          });
        }
        return;
      }
      if (!controlled()) return;
      if (!gesture || gesture.id !== event.pointerId) return;
      const dx = event.clientX - gesture.lastX;
      const dy = event.clientY - gesture.lastY;
      gesture.lastX = event.clientX;
      gesture.lastY = event.clientY;
      if (Math.hypot(event.clientX - gesture.startX, event.clientY - gesture.startY) > 7) {
        gesture.moved = true;
        clearTimeout(gesture.timer);
        if (!directTouch) {
          if (!gesture.pressed) {
            gesture.pressed = "left";
            buttonAt("left", true);
          }
          const bounds = viewport.getBoundingClientRect();
          setPointerPosition(pointerX + dx * 1.4 / bounds.width, pointerY + dy * 1.4 / bounds.height);
        } else {
          if (!gesture.pressed) {
            gesture.pressed = "left";
            setPointerPosition(gesture.startCoordinates.x, gesture.startCoordinates.y);
            buttonAt("left", true, gesture.startCoordinates);
          }
          const coordinates = point(event);
          if (coordinates) setPointerPosition(coordinates.x, coordinates.y);
        }
      } else if (gesture.pressed === "right") {
        const coordinates = directTouch ? point(event) : null;
        if (coordinates) setPointerPosition(coordinates.x, coordinates.y);
      }
      return;
    }
    if (activePointer !== null && activePointer !== event.pointerId) return;
    const coordinates = point(event);
    if (coordinates) {
      pointerX = coordinates.x;
      pointerY = coordinates.y;
      moveInput({ action: "pointer-move", ...coordinates });
    }
  });
  const releasePointer = (event) => {
    if (event.pointerType === "touch") {
      if (gesture?.id === event.pointerId) {
        clearTimeout(gesture.timer);
        if (controlled() && gesture.pressed) {
          buttonAt(gesture.pressed, false, directTouch ? point(event) || gesture.startCoordinates : undefined);
        } else if (controlled() && !gesture.moved) {
          const coordinates = directTouch ? point(event) : { x: pointerX, y: pointerY };
          clickAt("left", coordinates);
        }
        gesture = null;
      }
      touchPoints.delete(event.pointerId);
      if (touchPoints.size < 2) pinch = null;
      return;
    }
    if (!controlled() || activePointer !== event.pointerId) return;
    pointerButton(event, false);
    activePointer = null;
  };
  viewport.addEventListener("pointerup", releasePointer);
  viewport.addEventListener("pointercancel", releasePointer);
  viewport.addEventListener("lostpointercapture", releasePointer);
  viewport.addEventListener("contextmenu", (event) => {
    if (controlled()) event.preventDefault();
  });
  viewport.addEventListener("wheel", (event) => {
    if (!controlled()) return;
    const coordinates = point(event);
    if (!coordinates) return;
    event.preventDefault();
    void postInput({ action: "scroll", deltaX: event.deltaX, deltaY: event.deltaY, ...coordinates });
  }, { passive: false });
  viewport.addEventListener("keydown", (event) => {
    if (!controlled() || event.isComposing || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key.length === 1 && event.key.charCodeAt(0) >= 32 && event.key.charCodeAt(0) <= 126) {
      event.preventDefault();
      void postInput({ action: "text", text: event.key });
      return;
    }
    if (event.key.length === 1) {
      event.preventDefault();
      setStatus("This desktop accepts printable ASCII from the keyboard.", true);
      return;
    }
    const key = ({ " ": "Space", Backspace: "BackSpace" })[event.key]
      || (["Enter", "Escape", "Tab", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key) ? event.key : null);
    if (!key) return;
    event.preventDefault();
    void postInput({ action: "key", key, pressed: true }).then(() => {
      setTimeout(() => void postInput({ action: "key", key, pressed: false }), 16);
    });
  });
  for (const [name, button] of [["Control", controlModifier], ["Alt", altModifier]]) {
    button.addEventListener("click", () => {
      modifiers[name] = !modifiers[name];
      button.setAttribute("aria-pressed", String(modifiers[name]));
      button.classList.toggle("active", modifiers[name]);
      setStatus(modifiers[name] ? `${name} held for the next keyboard key.` : `${name} released.`);
    });
  }
  keyboardOpen.addEventListener("click", () => {
    const opening = keyboardRow.hidden;
    keyboardRow.hidden = !opening;
    shortcuts.hidden = !opening;
    keyboardOpen.textContent = opening ? "Done" : "Keyboard";
    keyboardOpen.setAttribute("aria-label", opening ? "Hide keyboard" : "Open keyboard");
    if (opening) textInput.focus({ preventScroll: true });
    else textInput.blur();
  });
  for (const button of shortcuts.querySelectorAll("[data-desktop-key]")) {
    button.addEventListener("click", () => {
      const key = button.dataset.desktopKey;
      void postInput({ action: "key", key, pressed: true }).then(() => {
        setTimeout(() => void postInput({ action: "key", key, pressed: false }), 16);
      });
    });
  }
  textInput.addEventListener("keydown", (event) => {
    const key = event.key.length === 1 ? event.key.toUpperCase() : null;
    if (!controlled() || !key || !(modifiers.Control || modifiers.Alt)) return;
    event.preventDefault();
    const active = Object.entries(modifiers).filter(([, pressed]) => pressed).map(([name]) => name);
    for (const modifier of active) void postInput({ action: "key", key: modifier, pressed: true });
    void postInput({ action: "key", key, pressed: true }).then(() => {
      void postInput({ action: "key", key, pressed: false });
      for (const modifier of active.reverse()) void postInput({ action: "key", key: modifier, pressed: false });
    });
    for (const modifier of active) {
      modifiers[modifier] = false;
      (modifier === "Control" ? controlModifier : altModifier).setAttribute("aria-pressed", "false");
      (modifier === "Control" ? controlModifier : altModifier).classList.remove("active");
    }
  });
  function setTouchMode(nextDirect) {
    directTouch = nextDirect;
    touchMode.setAttribute("aria-pressed", String(directTouch));
    trackpadMode.setAttribute("aria-pressed", String(!directTouch));
    touchHint.textContent = directTouch
      ? "Touch to click · drag to move or drag-and-drop · hold for right-click · pinch to zoom/pan"
      : "Drag to move · tap to click · two fingers to scroll · hold for right-click";
    cursor.hidden = directTouch || !controlled();
    if (!directTouch) applyViewTransform();
  }
  touchMode.addEventListener("click", () => {
    setTouchMode(true);
  });
  trackpadMode.addEventListener("click", () => {
    setTouchMode(false);
  });
  viewReset.addEventListener("click", () => {
    zoom = 1;
    panX = 0;
    panY = 0;
    applyViewTransform();
  });
  textInput.addEventListener("input", () => {
    sendText.disabled = !controlled() || !textInput.value;
  });
  sendText.addEventListener("click", async () => {
    if (!controlled() || !textInput.value) return;
    const value = textInput.value;
    sendText.disabled = true;
    await postInput({ action: "text", text: value });
    if (isOpen()) {
      textInput.value = "";
      sendText.disabled = true;
      textInput.focus({ preventScroll: true });
    }
  });
  document.addEventListener("visibilitychange", () => {
    if (!isOpen()) return;
    if (document.visibilityState === "hidden") {
      stream.removeAttribute("src");
      streamSession = null;
      streamLoaded = false;
    } else {
      void reconnectStream();
    }
  });

  return { open };
}
