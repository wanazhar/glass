import { ownerFetch } from "./owner-auth.js";
import { consumeDesktopUpdates } from "./desktop-updates.js";
// Loaded only when the full-screen shared desktop surface is opened.
export function desktopSurface({ api }) {
  const $ = (id) => document.getElementById(id);
  const dialog = $("desktop-viewer");
  const opener = $("desktop-toggle");
  const closeButton = $("desktop-close");
  const viewport = $("desktop-viewport");
  const stream = $("desktop-stream");
  const regionCanvas = $("desktop-regions");
  const placeholder = $("desktop-placeholder");
  const owner = $("desktop-owner");
  const display = $("desktop-display");
  const workspace = $("desktop-workspace");
  const workspaceFull = $("desktop-workspace-full");
  const resolution = $("desktop-resolution");
  const environmentPicker = $("desktop-environment-picker");
  const quickPreset = $("desktop-preset-quick");
  const reusablePreset = $("desktop-preset-reusable");
  const reusableNote = $("desktop-reusable-note");
  const environmentNote = $("desktop-environment-note");
  const activeEnvironment = $("desktop-environment-active");
  const status = $("desktop-status");
  const leaseNote = $("desktop-lease-note");
  const textInput = $("desktop-text");
  const sendText = $("desktop-send-text");
  const keyboardRow = $("desktop-keyboard-row");
  const trackpadMode = $("desktop-trackpad");
  const touchMode = $("desktop-touch-mode");
  const dragMode = $("desktop-drag-mode");
  let trackpadDrag = false;
  function setTrackpadDrag(armed) {
    trackpadDrag = armed;
    dragMode?.setAttribute("aria-pressed", String(armed));
  }
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
  const deleteProfile = $("desktop-profile-delete");
  const stop = $("desktop-stop");
  const retry = $("desktop-retry");
  const more = $("desktop-more");
  const moreSummary = more.querySelector("summary");
  let snapshot = null;
  let snapshotObservedAt = 0;
  let connectionError = null;
  let requestController = null;
  let pollTimer = 0;
  let pollGeneration = 0;
  let moveTimer = 0;
  let streamSession = null;
  let streamFrameSeqAtStart = null;
  let streamDecodedSeq = 0;
  let streamDecodedSource = null;
  let streamDecodedFrameTimeMs = null;
  let streamProfileWidth = null;
  let streamGeneration = 0;
  let streamController = null;
  let streamLatestFrame = null;
  let streamDecodeRunning = false;
  let streamObjectUrl = null;
  let streamLoaded = false;
  let streamFailed = false;
  let streamPausedReason = null;
  let startPending = false;
  let startAttempt = 0;
  let statusRequest = 0;
  let activePointer = null;
  let lastMoveAt = 0;
  let inputQueue = [];
  let pendingMove = null;
  let inputPumpRunning = false;
  const heldButtons = new Set();
  const heldKeys = new Set();
  const maxQueuedInput = 128;
  const maxInputFrameAgeMs = 3000;
  const freshFrameAgeMs = 1000;
  const streamOpenTimeoutMs = 8000;
  const streamReadTimeoutMs = 12000;
  const snapshotFallbackIntervalMs = 500;
  const snapshotFallbackFastIntervalMs = 250;
  const snapshotFallbackMaxFailures = 8;
  const snapshotFallbackFailureWindowMs = 60000;
  const snapshotFallbackMaxBackoffMs = 15000;
  const snapshotRequestTimeoutMs = 5000;
  const pendingFrameTimings = [];
  let snapshotFallbackActive = false;
  let snapshotFallbackReason = null;
  let snapshotFallbackFailures = 0;
  let snapshotFallbackFailureWindowStartedAt = 0;
  let snapshotFallbackTimer = 0;
  let snapshotFallbackController = null;
  let snapshotFallbackGeneration = 0;
  let frameFreshnessTimer = 0;
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
  let capabilities = [{ environment: "xfce", lifecycle: "disposable", available: true, reason: null }];
  let capabilitiesKnown = false;
  let selectedPreset = "xfce:disposable";

  function updatePresetNote() {
    const selected = capabilities.find((item) => `${item.environment}:${item.lifecycle}` === selectedPreset);
    const reusable = capabilities.find((item) => item.environment === "gnome" && item.lifecycle === "reusable");
    if (!capabilitiesKnown) {
      environmentNote.textContent = "Checking which desktop workspaces this host supports.";
    } else if (reusable && !reusable.available) {
      environmentNote.textContent = `Reusable workspace unavailable: ${reusable.reason || "GNOME is not supported on this host."}`;
    } else if (!capabilities.some((item) => item.available)) {
      environmentNote.textContent = capabilities.find((item) => item.reason)?.reason || "No managed desktop environment is currently available.";
    } else if (!selected?.available) {
      environmentNote.textContent = selected?.reason || "This workspace option is not available on this host.";
    } else {
      environmentNote.textContent = selected.lifecycle === "reusable"
        ? "Your GNOME workspace keeps its files when you stop it."
        : "A quick XFCE desktop is removed when you stop it.";
    }
  }

  function renderPresets(next) {
    const supplied = Array.isArray(next?.capabilities) ? next.capabilities : null;
    if (supplied) capabilitiesKnown = true;
    capabilities = (supplied || (capabilitiesKnown ? capabilities : [{ environment: "xfce", lifecycle: "disposable", available: true }]))
      .filter((item) => ["xfce", "gnome"].includes(item?.environment)
        && ["disposable", "reusable"].includes(item?.lifecycle))
      .map((item) => ({
        environment: item.environment,
        lifecycle: item.lifecycle,
        available: item.available === true,
        reason: typeof item.reason === "string" ? item.reason : null,
        profileExists: item.profileExists === true,
      }));
    const quick = capabilities.find((item) => item.environment === "xfce" && item.lifecycle === "disposable");
    const reusable = capabilities.find((item) => item.environment === "gnome" && item.lifecycle === "reusable");
    quickPreset.disabled = !quick?.available;
    reusablePreset.disabled = !reusable?.available;
    if (!capabilitiesKnown) reusableNote.textContent = "GNOME · checking host support";
    else if (!reusable) reusableNote.textContent = "GNOME · not offered by this host";
    else if (reusable.reason && !reusable.available) reusableNote.textContent = `GNOME · unavailable: ${reusable.reason}`;
    else reusableNote.textContent = reusable.profileExists ? "GNOME · saved workspace" : "GNOME · keeps its profile";
    if (!capabilities.some((item) => `${item.environment}:${item.lifecycle}` === selectedPreset && item.available)) {
      selectedPreset = quick?.available ? "xfce:disposable" : reusable?.available ? "gnome:reusable" : "xfce:disposable";
    }
    quickPreset.checked = selectedPreset === "xfce:disposable";
    reusablePreset.checked = selectedPreset === "gnome:reusable";
    quickPreset.closest(".desktop-preset-option")?.classList.toggle("selected", quickPreset.checked);
    reusablePreset.closest(".desktop-preset-option")?.classList.toggle("selected", reusablePreset.checked);
    quickPreset.closest(".desktop-preset-option")?.classList.toggle("unavailable", quickPreset.disabled);
    reusablePreset.closest(".desktop-preset-option")?.classList.toggle("unavailable", reusablePreset.disabled);
    const selected = capabilities.find((item) => `${item.environment}:${item.lifecycle}` === selectedPreset);
    environmentNote.hidden = capabilities.some((item) => item.available) && !selected?.reason && capabilities.every((item) => item.available);
    updatePresetNote();
  }

  const isOpen = () => dialog.open && requestController && !requestController.signal.aborted;
  const controlled = () => isOpen() && snapshot?.available && snapshot.lease === "human" && hasFreshDecodedFrame();

  function setStatus(message, error = false) {
    if (status.textContent !== message) status.textContent = message;
    const nextError = error ? "true" : "false";
    if (status.dataset.error !== nextError) status.dataset.error = nextError;
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

  function stopStream({ preserveFrame = false } = {}) {
    streamGeneration += 1;
    streamController?.abort();
    streamController = null;
    clearTimeout(snapshotFallbackTimer);
    snapshotFallbackTimer = 0;
    snapshotFallbackController?.abort();
    snapshotFallbackController = null;
    snapshotFallbackGeneration += 1;
    snapshotFallbackActive = false;
    snapshotFallbackReason = null;
    snapshotFallbackFailures = 0;
    snapshotFallbackFailureWindowStartedAt = 0;
    streamFailed = false;
    streamPausedReason = null;
    streamLatestFrame = null;
    streamProfileWidth = null;
    if (!preserveFrame) {
      clearTimeout(frameFreshnessTimer);
      frameFreshnessTimer = 0;
      streamLoaded = false;
      streamDecodedFrameTimeMs = null;
      streamDecodedSource = null;
      regionCanvas.hidden = true;
      if (streamObjectUrl) URL.revokeObjectURL(streamObjectUrl);
      streamObjectUrl = null;
      stream.removeAttribute("src");
    }
  }

  function setStream(available) {
    const session = snapshot?.sessionId || null;
    const nativeWidth = Math.min(1280, Math.max(320, Number(snapshot?.width) || 1280));
    const viewportPixels = Math.ceil((viewport.clientWidth || window.innerWidth)
      * Math.min(2, Math.max(1, Number(window.devicePixelRatio) || 1)));
    const requestedWidth = viewportPixels >= nativeWidth
      ? null
      : Math.max(320, Math.round(viewportPixels));
    if (!available || !session) {
      stopStream();
      streamSession = null;
      streamFrameSeqAtStart = null;
      stream.hidden = true;
      return;
    }
    if (streamSession === session && (snapshotFallbackActive || streamFailed)) return;
    if (streamSession !== session || !streamController || streamProfileWidth !== requestedWidth) {
      const sameSession = streamSession === session;
      if (streamController) stopStream({ preserveFrame: sameSession });
      streamFailed = false;
      if (!sameSession) {
        streamLoaded = false;
        streamDecodedFrameTimeMs = null;
        streamDecodedSeq = 0;
        streamDecodedSource = null;
      }
      streamSession = session;
      if (!sameSession) streamFrameSeqAtStart = Number(snapshot?.frameSeq) || 0;
      streamProfileWidth = requestedWidth;
      streamController = new AbortController();
      const generation = ++streamGeneration;
      const controller = streamController;
      if (snapshot?.regionUpdates === 1) {
        void consumeDesktopUpdates({
          session, canvas: regionCanvas, signal: controller.signal,
          current: () => isOpen() && streamGeneration === generation && snapshot?.sessionId === session,
          emit: (frame) => {
            if (controller.signal.aborted || generation !== streamGeneration || snapshot?.sessionId !== session) return;
            if (streamLoaded && !streamDecodeRunning && !streamLatestFrame
              && streamDecodedSource === "regions") {
              streamDecodedSeq = frame.seq;
              streamDecodedFrameTimeMs = frame.timeMs;
              scheduleFrameFreshnessCheck();
              updateStreamPresentation();
              return;
            }
            streamLatestFrame = { ...frame, generation, session, source: "regions" };
            void decodeNewestFrame();
          },
        }).catch((error) => {
          const known = ["Invalid desktop region", "Desktop region dimensions changed", "Desktop region connection unavailable",
            "Desktop update exceeds bounds", "Invalid desktop update", "Invalid desktop hashes", "Invalid full desktop update",
            "Desktop update has no base frame", "Invalid desktop tile"];
          viewport.dataset.regionError = Number.isInteger(error.httpStatus) ? `HTTP ${error.httpStatus}`
            : known.includes(error.message) ? error.message : error.name || "Error";
          failStream(session, generation, controller, error);
        });
      } else {
        void consumeStream(session, generation, controller);
      }
    }
    stream.hidden = !streamLoaded || streamFailed;
  }

  function estimatedFrameAgeMs(frameTimeMs, seq = streamDecodedSeq) {
    const statusAge = Number(snapshot?.frameAgeMs);
    const statusTime = Number(snapshot?.frameTimeMs);
    const elapsed = snapshotObservedAt ? performance.now() - snapshotObservedAt : 0;
    if (Number.isFinite(statusAge) && Number.isFinite(statusTime) && Number.isFinite(frameTimeMs)) {
      return Math.max(0, statusAge + statusTime - frameTimeMs + elapsed);
    }
    const statusSeq = Number(snapshot?.frameSeq);
    const approximateFrameAge = Number.isFinite(statusAge) ? statusAge : 0;
    return approximateFrameAge + (Number.isFinite(statusSeq) ? Math.max(0, statusSeq - seq) * 200 : 0) + elapsed;
  }

  function hasFreshDecodedFrame() {
    return Boolean(hasDecodedFrameForCurrentSession()
      && (!streamFailed || snapshotFallbackActive)
      && estimatedFrameAgeMs(streamDecodedFrameTimeMs) <= maxInputFrameAgeMs);
  }

  function hasDecodedFrameForCurrentSession() {
    const frameSeq = Number(streamDecodedSeq);
    const startingSeq = streamFrameSeqAtStart ?? 0;
    const frameIsNewEnough = frameSeq > startingSeq
      || (["snapshot", "regions"].includes(streamDecodedSource) && frameSeq === startingSeq);
    return Boolean(snapshot?.available
      && snapshot?.sessionId
      && streamSession === snapshot.sessionId
      && streamLoaded
      && Number.isFinite(streamDecodedFrameTimeMs)
      && frameIsNewEnough);
  }

  function scheduleFrameFreshnessCheck() {
    clearTimeout(frameFreshnessTimer);
    frameFreshnessTimer = 0;
    if (!streamLoaded || !Number.isFinite(streamDecodedFrameTimeMs)) return;
    const ageMs = estimatedFrameAgeMs(streamDecodedFrameTimeMs);
    const delayMs = Math.max(0, maxInputFrameAgeMs - ageMs + 1);
    frameFreshnessTimer = setTimeout(() => {
      frameFreshnessTimer = 0;
      updateStreamPresentation();
    }, delayMs);
  }

  function updateFrameInputControls() {
    const canInput = Boolean(snapshot?.available && snapshot?.lease === "human" && hasFreshDecodedFrame());
    textInput.disabled = !canInput;
    sendText.disabled = !canInput || !textInput.value;
    keyboardOpen.hidden = !snapshot?.available || snapshot?.lease !== "human";
    keyboardOpen.disabled = !canInput;
    for (const button of shortcuts.querySelectorAll("button")) button.disabled = !canInput;
    if (dragMode) dragMode.disabled = !canInput;
    if (!canInput && trackpadDrag) setTrackpadDrag(false);
    viewport.dataset.frameFresh = hasFreshDecodedFrame() ? "true" : "false";
    viewport.dataset.frameSeq = String(streamDecodedSeq || 0);
    viewport.dataset.frameSource = streamDecodedSource || "";
    viewport.dataset.frameTimeMs = String(streamDecodedFrameTimeMs ?? "");
    cursor.hidden = !coarsePointer.matches || directTouch || !controlled();
    if (!canInput) closeKeyboard();
  }

  function updateStreamPresentation() {
    if (!snapshot?.available || !snapshot?.sessionId) {
      updateFrameInputControls();
      return;
    }
    const hasDecodedFrame = stream.naturalWidth > 0
      && stream.naturalHeight > 0
      && hasDecodedFrameForCurrentSession();
    const ageMs = hasDecodedFrame ? estimatedFrameAgeMs(streamDecodedFrameTimeMs) : Infinity;
    // Keep the last successfully decoded image visible even when it is stale.
    // Freshness gates input below, while the status makes the delay explicit.
    const visible = hasDecodedFrame;
    stream.hidden = !visible;
    regionCanvas.hidden = !(visible && streamDecodedSource === "regions");
    placeholder.hidden = visible;
    if (streamFailed && !snapshotFallbackActive) {
      if (streamPausedReason) {
        setStatus(
          `Stream paused · ${streamPausedReason}${visible ? ` · last frame ${Math.max(1, Math.ceil(ageMs / 1000))} sec` : ""}`,
          true,
        );
      }
      updateFrameInputControls();
      return;
    }
    if (snapshotFallbackActive) {
      placeholder.textContent = "The stream is unavailable. Checking refreshed desktop images…";
      if (visible && ageMs >= freshFrameAgeMs) {
        setStatus(`Snapshots · delayed · ${Math.max(1, Math.ceil(ageMs / 1000))} sec · ${snapshotFallbackReason}`, false);
      } else if (visible) {
        setStatus(`Snapshots · live · ${snapshotFallbackReason}`, false);
      } else {
        setStatus(`Snapshots · waiting · ${snapshotFallbackReason}`, false);
      }
      updateFrameInputControls();
      return;
    }
    placeholder.textContent = hasDecodedFrame && !visible
      ? "The latest image is delayed. Waiting for a recent frame…"
      : "Connecting to the managed desktop…";
    if (visible && ageMs >= freshFrameAgeMs) {
      setStatus(`Delayed · ${Math.max(1, Math.ceil(ageMs / 1000))} sec`, false);
    } else if (visible) {
      setStatus(`Live · updated ${ageMs < 1000 ? "just now" : `${Math.round(ageMs / 1000)} sec ago`}`, false);
    } else if (hasDecodedFrame) {
      setStatus("Waiting for a recent frame", false);
    } else {
      setStatus("Connecting to the managed desktop…", false);
    }
    updateFrameInputControls();
  }

  function findBytes(haystack, needle, start = 0) {
    outer: for (let index = start; index <= haystack.length - needle.length; index += 1) {
      for (let offset = 0; offset < needle.length; offset += 1) {
        if (haystack[index + offset] !== needle[offset]) continue outer;
      }
      return index;
    }
    return -1;
  }

  function appendBytes(first, second) {
    const joined = new Uint8Array(first.length + second.length);
    joined.set(first);
    joined.set(second, first.length);
    return joined;
  }

  function streamFailure(reason, status = null) {
    const error = new Error();
    error.streamFailureReason = reason;
    error.httpStatus = Number.isInteger(status) ? status : null;
    return error;
  }

  function safeStreamFailureLabel(error) {
    if (error?.streamFailureReason === "authorization") return "owner authorization failed";
    if (error?.streamFailureReason === "open-timeout") return "stream response timed out";
    if (error?.streamFailureReason === "read-timeout") return "stream stopped sending data";
    if (error?.streamFailureReason === "response-ended") return "stream ended";
    if (error?.streamFailureReason === "body-unavailable") return "stream body unavailable";
    if (error?.streamFailureReason === "frame-format") return "unsupported stream frame";
    if (error?.streamFailureReason === "frame-decode") return "image decode failed";
    if (error?.streamFailureReason === "frame-too-large") return "image exceeded the display limit";
    if (error?.streamFailureReason === "frame-lag") return "stream frames are arriving late";
    if (error?.streamFailureReason === "snapshot-timeout") return "snapshot request timed out";
    if (error?.streamFailureReason === "snapshot-format") return "snapshot response was not an image";
    if (error?.streamFailureReason === "snapshot-decode") return "snapshot image could not be decoded";
    if (Number.isInteger(error?.httpStatus)) return `HTTP ${error.httpStatus}`;
    return "network error";
  }

  function snapshotFallbackIsCurrent(session, generation) {
    return snapshotFallbackActive
      && snapshotFallbackGeneration === generation
      && streamGeneration === generation
      && snapshot?.sessionId === session
      && isOpen();
  }

  function stopSnapshotFallback(reason, terminal = false) {
    clearTimeout(snapshotFallbackTimer);
    snapshotFallbackTimer = 0;
    snapshotFallbackController?.abort();
    snapshotFallbackController = null;
    snapshotFallbackActive = false;
    if (!isOpen()) return;
    retry.hidden = false;
    retry.textContent = "Reconnect stream";
    const hasFrame = hasDecodedFrameForCurrentSession();
    stream.hidden = !hasFrame;
    placeholder.hidden = hasFrame;
    placeholder.textContent = terminal
      ? "The desktop is still running. Reconnect the stream to try again."
      : "The desktop image snapshots paused. Reconnect the stream to try again.";
    const shortReason = reason === "owner authorization failed"
      ? "owner auth failed"
      : reason === "automatic recovery paused after repeated network failures"
        ? "repeated network failures"
        : reason;
    setStatus(`Frames paused · ${shortReason}`, true);
    updateFrameInputControls();
  }

  function scheduleSnapshotFallback(session, generation, delay = snapshotFallbackIntervalMs) {
    if (!snapshotFallbackIsCurrent(session, generation)) return;
    clearTimeout(snapshotFallbackTimer);
    snapshotFallbackTimer = setTimeout(() => {
      snapshotFallbackTimer = 0;
      void pollSnapshotFallback(session, generation);
    }, delay);
  }

  function startSnapshotFallback(session, generation, reason) {
    if (!isOpen() || snapshot?.sessionId !== session || generation !== streamGeneration) return;
    snapshotFallbackActive = true;
    streamPausedReason = null;
    snapshotFallbackGeneration = generation;
    snapshotFallbackReason = reason;
    snapshotFallbackFailures = 0;
    snapshotFallbackFailureWindowStartedAt = 0;
    retry.hidden = false;
    retry.textContent = "Reconnect stream";
    placeholder.hidden = !streamLoaded;
    if (!streamLoaded) placeholder.textContent = "The stream is unavailable. Checking refreshed desktop images…";
    setStatus(`Checking desktop snapshots · ${reason}`, false);
    void pollSnapshotFallback(session, generation);
  }

  function recordSnapshotFallbackFailure(session, generation, reason) {
    const now = Date.now();
    if (snapshotFallbackFailures === 0) snapshotFallbackFailureWindowStartedAt = now;
    snapshotFallbackFailures += 1;
    snapshotFallbackReason = reason;
    if (snapshotFallbackFailures >= snapshotFallbackMaxFailures
      || now - snapshotFallbackFailureWindowStartedAt >= snapshotFallbackFailureWindowMs) {
      stopSnapshotFallback("automatic recovery paused after repeated network failures", true);
      return;
    }
    const delay = Math.min(
      snapshotFallbackIntervalMs * (2 ** (snapshotFallbackFailures - 1)),
      snapshotFallbackMaxBackoffMs,
    );
    scheduleSnapshotFallback(session, generation, delay);
  }

  async function pollSnapshotFallback(session, generation) {
    if (!snapshotFallbackIsCurrent(session, generation)) return;

    const controller = new AbortController();
    snapshotFallbackController = controller;
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, snapshotRequestTimeoutMs);

    try {
      const response = await ownerFetch("/api/desktop/frame", {
        credentials: "same-origin",
        cache: "no-store",
        signal: controller.signal,
      });
      if (!snapshotFallbackIsCurrent(session, generation)) return;
      if (response.status === 401 || response.status === 403) {
        clearTimeout(timeout);
        controller.abort();
        stopSnapshotFallback("owner authorization failed", true);
        return;
      }
      if (response.status === 409 || response.status === 503) {
        clearTimeout(timeout);
        controller.abort();
        recordSnapshotFallbackFailure(
          session,
          generation,
          response.status === 409 ? "no fresh desktop frame" : "snapshot service unavailable",
        );
        return;
      }
      if (!response.ok) {
        clearTimeout(timeout);
        controller.abort();
        recordSnapshotFallbackFailure(session, generation, `snapshot HTTP ${response.status}`);
        return;
      }
      const contentType = (response.headers.get("content-type") || "").split(";", 1)[0].trim().toLowerCase();
      const sequence = Number(response.headers.get("x-frame-seq"));
      const timeMs = Number(response.headers.get("x-frame-time-ms"));
      const declaredLength = Number(response.headers.get("content-length"));
      if (contentType !== "image/jpeg"
        || !Number.isSafeInteger(sequence)
        || sequence <= 0
        || !Number.isFinite(timeMs)
        || (Number.isFinite(declaredLength) && declaredLength > 4 * 1024 * 1024)) {
        clearTimeout(timeout);
        controller.abort();
        stopSnapshotFallback(safeStreamFailureLabel(streamFailure("snapshot-format")), true);
        return;
      }
      const blob = await response.blob();
      clearTimeout(timeout);
      if (!snapshotFallbackIsCurrent(session, generation)) return;
      if (blob.size === 0 || blob.size > 4 * 1024 * 1024) {
        stopSnapshotFallback(safeStreamFailureLabel(streamFailure("frame-too-large")), true);
        return;
      }
      snapshotFallbackFailures = 0;
      snapshotFallbackFailureWindowStartedAt = 0;
      const startingSeq = streamFrameSeqAtStart ?? 0;
      const firstSnapshotCanReuseCurrentFrame = !streamLoaded && sequence === startingSeq;
      if (sequence > streamDecodedSeq && (sequence > startingSeq || firstSnapshotCanReuseCurrentFrame)) {
        streamLatestFrame = {
          seq: sequence,
          timeMs,
          generation,
          session,
          source: "snapshot",
          blob,
        };
        void decodeNewestFrame();
      }
      const nextDelay = streamLoaded ? snapshotFallbackIntervalMs : snapshotFallbackFastIntervalMs;
      scheduleSnapshotFallback(session, generation, nextDelay);
    } catch {
      clearTimeout(timeout);
      if (!snapshotFallbackIsCurrent(session, generation)) return;
      const reason = timedOut
        ? safeStreamFailureLabel(streamFailure("snapshot-timeout"))
        : "snapshot network error";
      recordSnapshotFallbackFailure(session, generation, reason);
    } finally {
      clearTimeout(timeout);
      if (snapshotFallbackController === controller) snapshotFallbackController = null;
    }
  }

  function failStream(session, generation, controller, error) {
    if (!isOpen() || generation !== streamGeneration || snapshot?.sessionId !== session) return;
    if (snapshotFallbackActive) return;
    if (streamController === controller) streamController = null;
    controller?.abort();
    const reason = safeStreamFailureLabel(error);
    streamFailed = true;
    streamPausedReason = reason;
    stream.hidden = !streamLoaded;
    retry.hidden = false;
    retry.textContent = "Reconnect stream";
    placeholder.hidden = false;
    if (error?.streamFailureReason === "authorization") {
      placeholder.textContent = "The desktop session is still running. Reconnect to restore image authorization.";
      updateStreamPresentation();
      setStatus(`Stream paused · ${reason}`, true);
      return;
    }
    placeholder.textContent = "The desktop is still running. Trying refreshed image snapshots.";
    startSnapshotFallback(session, generation, reason);
  }

  function readStreamChunk(reader) {
    let timer = 0;
    const timeout = new Promise((resolve, reject) => {
      timer = setTimeout(() => reject(streamFailure("read-timeout")), streamReadTimeoutMs);
    });
    return Promise.race([reader.read(), timeout]).finally(() => clearTimeout(timer));
  }

  async function decodeNewestFrame() {
    if (streamDecodeRunning) return;
    streamDecodeRunning = true;
    let processingSource = "stream";
    let processingGeneration = streamGeneration;
    let processingSession = streamSession;
    try {
      while (streamLatestFrame && isOpen()) {
        const frame = streamLatestFrame;
        streamLatestFrame = null;
        processingSource = frame.source || "stream";
        processingGeneration = frame.generation;
        processingSession = frame.session || streamSession;
        const startingSeq = streamFrameSeqAtStart ?? 0;
        const firstSnapshotCanReuseCurrentFrame = ["snapshot", "regions"].includes(frame.source)
          && !streamLoaded
          && frame.seq === startingSeq;
        if (frame.generation !== streamGeneration
          || frame.seq <= streamDecodedSeq
          || (frame.seq <= startingSeq && !firstSnapshotCanReuseCurrentFrame)) continue;
        if (frame.source === "regions" && streamLoaded && streamDecodedSource === "regions") {
          streamDecodedSeq = frame.seq;
          streamDecodedFrameTimeMs = frame.timeMs;
          scheduleFrameFreshnessCheck();
          updateStreamPresentation();
          continue;
        }
        const objectUrl = URL.createObjectURL(frame.blob);
        const decoded = new Image();
        decoded.src = objectUrl;
        try {
          await decoded.decode();
        } catch {
          URL.revokeObjectURL(objectUrl);
          throw new Error("The desktop sent an image that could not be decoded.");
        }
        if (!isOpen() || frame.generation !== streamGeneration || streamSession !== snapshot?.sessionId) {
          URL.revokeObjectURL(objectUrl);
          return;
        }
        if (frame.source !== "regions" && streamLatestFrame && streamLatestFrame.seq > frame.seq) {
          URL.revokeObjectURL(objectUrl);
          continue;
        }
        const previousUrl = streamObjectUrl;
        streamObjectUrl = objectUrl;
        stream.src = objectUrl;
        stream.hidden = false;
        await stream.decode();
        if (!isOpen()
          || frame.generation !== streamGeneration
          || (!snapshotFallbackActive && (streamFailed || !streamController))
          || processingSession !== snapshot?.sessionId) {
          if (streamObjectUrl === objectUrl) streamObjectUrl = null;
          if (previousUrl) URL.revokeObjectURL(previousUrl);
          URL.revokeObjectURL(objectUrl);
          if (stream.src === objectUrl) stream.removeAttribute("src");
          stream.hidden = true;
          return;
        }
        streamDecodedSeq = frame.seq;
        streamDecodedSource = processingSource;
        streamDecodedFrameTimeMs = frame.timeMs;
        streamLoaded = true;
        scheduleFrameFreshnessCheck();
        if (previousUrl) URL.revokeObjectURL(previousUrl);
        updateStreamPresentation();
      }
    } catch {
      if (isOpen()) {
        if (processingSource === "snapshot") {
          if (snapshotFallbackIsCurrent(processingSession, processingGeneration)) {
            stopSnapshotFallback(safeStreamFailureLabel(streamFailure("snapshot-decode")), true);
          }
        } else {
          failStream(
            processingSession,
            processingGeneration,
            streamController,
            streamFailure("frame-decode"),
          );
        }
      }
    } finally {
      streamDecodeRunning = false;
      if (streamLatestFrame && isOpen()) void decodeNewestFrame();
    }
  }

  async function consumeStream(session, generation, controller) {
    const boundary = new TextEncoder().encode("--glassframe");
    const headerDelimiter = new Uint8Array([13, 10, 13, 10]);
    let buffer = new Uint8Array(0);
    let openTimedOut = false;
    let openTimer = 0;
    try {
      const streamPath = streamProfileWidth ? `/api/desktop/stream?width=${streamProfileWidth}` : "/api/desktop/stream";
      openTimer = setTimeout(() => {
        openTimedOut = true;
        controller.abort();
      }, streamOpenTimeoutMs);
      let response;
      try {
        response = await ownerFetch(streamPath, {
          credentials: "same-origin",
          cache: "no-store",
          signal: controller.signal,
        });
      } catch {
        if (openTimedOut) throw streamFailure("open-timeout");
        throw streamFailure("network");
      } finally {
        clearTimeout(openTimer);
      }
      if (response.status === 401 || response.status === 403) {
        throw streamFailure("authorization", response.status);
      }
      if (!response.ok) {
        throw streamFailure("http-response", response.status);
      }
      if (!response.body) throw streamFailure("body-unavailable");
      const reader = response.body.getReader();
      while (isOpen() && generation === streamGeneration && session === snapshot?.sessionId) {
        const { value, done } = await readStreamChunk(reader);
        if (done) throw streamFailure("response-ended");
        buffer = appendBytes(buffer, value);
        if (buffer.length > 8 * 1024 * 1024) throw streamFailure("frame-format");
        while (true) {
          const boundaryAt = findBytes(buffer, boundary);
          if (boundaryAt < 0) {
            if (buffer.length > boundary.length) buffer = buffer.slice(buffer.length - boundary.length);
            break;
          }
          if (boundaryAt > 0) buffer = buffer.slice(boundaryAt);
          const headerAt = findBytes(buffer, headerDelimiter, boundary.length);
          if (headerAt < 0) break;
          const headerText = new TextDecoder().decode(buffer.slice(boundary.length, headerAt));
          if (/(?:^|\r\n)X-Glass-Heartbeat:\s*1(?:\r\n|$)/i.test(headerText)) {
            buffer = buffer.slice(headerAt + headerDelimiter.length);
            continue;
          }
          const length = Number(headerText.match(/(?:^|\r\n)Content-Length:\s*(\d+)/i)?.[1]);
          const seq = Number(headerText.match(/(?:^|\r\n)X-Frame-Seq:\s*(\d+)/i)?.[1]);
          const timeMs = Number(headerText.match(/(?:^|\r\n)X-Frame-Time-Ms:\s*(\d+(?:\.\d+)?)/i)?.[1]);
          if (!Number.isSafeInteger(length) || length <= 0 || length > 4 * 1024 * 1024 || !Number.isSafeInteger(seq)) {
            throw streamFailure("frame-format");
          }
          if (!Number.isFinite(timeMs)) throw streamFailure("frame-format");
          const frameAt = headerAt + headerDelimiter.length;
          const afterFrame = frameAt + length;
          if (buffer.length < afterFrame) break;
          const frame = {
            seq,
            timeMs,
            generation,
            session,
            source: "stream",
            blob: new Blob([buffer.slice(frameAt, afterFrame)], { type: "image/jpeg" }),
          };
          buffer = buffer.slice(afterFrame);
          const frameAgeMs = estimatedFrameAgeMs(timeMs, seq);
          const captureAgeMs = Number(snapshot?.frameAgeMs)
            + (snapshotObservedAt ? performance.now() - snapshotObservedAt : 0);
          if (snapshot?.frameAgeMs != null
            && Number.isFinite(captureAgeMs)
            && captureAgeMs <= maxInputFrameAgeMs
            && frameAgeMs > 2000) {
            failStream(session, generation, controller, streamFailure("frame-lag"));
            return;
          }
          if (seq > streamDecodedSeq && seq > (streamFrameSeqAtStart ?? 0)) {
            streamLatestFrame = frame;
            void decodeNewestFrame();
          }
        }
      }
    } catch (error) {
      if ((controller.signal.aborted && !openTimedOut) || generation !== streamGeneration || !isOpen()) return;
      failStream(session, generation, controller, error);
    } finally {
      clearTimeout(openTimer);
    }
  }

  function render(next) {
    const priorFrameSeq = Number(snapshot?.frameSeq) || 0;
    const wasRunning = Boolean(snapshot?.sessionId);
    snapshot = next;
    snapshotObservedAt = performance.now();
    const running = Boolean(next?.sessionId);
    renderPresets(next);
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
    activeEnvironment.textContent = next?.environment === "gnome" ? "GNOME" : next?.environment === "xfce" ? "XFCE" : "—";
    $("desktop-lifecycle-active").textContent = next?.lifecycle === "reusable" ? "Reusable workspace" : next?.lifecycle === "disposable" ? "Quick desktop" : "—";
    leaseNote.hidden = !humanOwnsInput;
    take.hidden = !available || humanOwnsInput;
    returnToAgent.hidden = !humanOwnsInput;
    start.hidden = running;
    environmentPicker.hidden = running;
    environmentNote.hidden = running || !environmentNote.textContent;
    stop.hidden = !running || next?.mode !== "managedVirtualSession";
    reconnect.hidden = !running;
    more.hidden = !running;
    if (!running && wasRunning) $("desktop-details").open = false;
    if (more.hidden && more.contains(document.activeElement)) {
      start.focus({ preventScroll: true });
    }
    retry.hidden = !(connectionError || (running && (!available || streamFailed)));
    retry.textContent = connectionError ? "Retry connection" : "Reconnect stream";
    const selectedCapability = capabilities.find((item) => `${item.environment}:${item.lifecycle}` === selectedPreset);
    start.textContent = selectedCapability?.lifecycle === "reusable" && selectedCapability.profileExists ? "Resume workspace" : selectedCapability?.lifecycle === "reusable" ? "Open workspace" : "Start quick desktop";
    start.disabled = startPending || Boolean(connectionError) || !selectedCapability?.available;
    deleteProfile.hidden = running || !capabilities.some((item) => item.environment === "gnome" && item.lifecycle === "reusable" && item.profileExists);
    deleteProfile.disabled = Boolean(connectionError);
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
        : "Choose a desktop workspace above to open it here.";
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
    updateStreamPresentation();
    const frameSeq = Number(next.frameSeq);
    const currentAgeMs = estimatedFrameAgeMs(streamDecodedFrameTimeMs);
    const connected = streamLoaded && streamDecodedSeq > (streamFrameSeqAtStart ?? 0)
      && currentAgeMs <= maxInputFrameAgeMs && !streamFailed;
    if (connected && frameSeq > priorFrameSeq) {
      const now = performance.now();
      for (let index = pendingFrameTimings.length - 1; index >= 0; index -= 1) {
        const timing = pendingFrameTimings[index];
        if (frameSeq <= timing.afterSeq) continue;
        window.dispatchEvent(new CustomEvent("glass-desktop-frame-timing", {
          detail: {
            action: timing.action,
            eventToFrameMs: now - timing.eventAt,
            requestToFrameMs: now - timing.requestFinished,
            frameSeq,
          },
        }));
        pendingFrameTimings.splice(index, 1);
      }
    }
  }

  async function refresh() {
    if (!isOpen()) return;
    const requestAttempt = startAttempt;
    const requestId = ++statusRequest;
    try {
      const next = await api("desktop/status", undefined, requestController.signal);
      if (isOpen() && requestId === statusRequest && requestAttempt === startAttempt && !startPending) {
        connectionError = null;
        render(next);
      }
    } catch (error) {
      // Starting a managed desktop holds the backend control lock until the
      // display and desktop are ready. Status requests made before or during
      // that work can time out even though Connect is healthy. Do not let one
      // of those responses overwrite the in-progress start state.
      if (!isOpen() || requestId !== statusRequest || requestAttempt !== startAttempt || startPending) return;
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

  function schedulePoll(generation, delay = null) {
    clearTimeout(pollTimer);
    pollTimer = setTimeout(async () => {
      if (!isOpen() || generation !== pollGeneration) return;
      if (startPending) {
        schedulePoll(generation, 1800);
        return;
      }
      await refresh();
      schedulePoll(generation);
    }, delay ?? (pendingFrameTimings.length ? 100 : snapshot?.available ? 450 : 1800));
  }

  function flushPendingMove() {
    if (!pendingMove) return;
    clearTimeout(moveTimer);
    moveTimer = 0;
    inputQueue.push(pendingMove);
    pendingMove = null;
  }

  function discardPendingMove() {
    clearTimeout(moveTimer);
    moveTimer = 0;
    pendingMove = null;
  }

  function settleInput(entry) {
    for (const resolve of entry.resolvers || []) resolve();
  }

  function pumpInputQueue() {
    if (inputPumpRunning) return;
    inputPumpRunning = true;
    void (async () => {
      while (inputQueue.length || pendingMove) {
        const entry = inputQueue.length ? inputQueue.shift() : pendingMove;
        if (entry === pendingMove) pendingMove = null;
        if (!controlled()) {
          settleInput(entry);
          continue;
        }
        const requestStarted = performance.now();
        let succeeded = false;
        if (entry.payload.action === "pointer-button" && entry.payload.pressed) {
          heldButtons.add(entry.payload.button);
        }
        if (entry.payload.action === "key" && entry.payload.pressed) heldKeys.add(entry.payload.key);
        try {
          await api("desktop/input", entry.payload, requestController.signal);
          succeeded = true;
        } catch (error) {
          if (isOpen()) {
            setStatus(error.message || "Desktop input was not accepted.", true);
            void refresh();
          }
        }
        const requestFinished = performance.now();
        window.dispatchEvent(new CustomEvent("glass-desktop-input-timing", {
          detail: {
            action: entry.payload.action,
            queuedMs: Math.max(0, requestStarted - entry.queuedAt),
            requestMs: requestFinished - requestStarted,
            totalMs: requestFinished - entry.queuedAt,
            accepted: succeeded,
          },
        }));
        if (succeeded) {
          if (entry.payload.action === "pointer-button") {
            if (entry.payload.pressed) heldButtons.add(entry.payload.button);
            else heldButtons.delete(entry.payload.button);
          }
          if (entry.payload.action === "key") {
            if (entry.payload.pressed) heldKeys.add(entry.payload.key);
            else heldKeys.delete(entry.payload.key);
          }
          pendingFrameTimings.push({
            action: entry.payload.action,
            eventAt: entry.queuedAt,
            requestFinished,
            afterSeq: Number(snapshot?.frameSeq) || 0,
          });
          if (pendingFrameTimings.length > 32) pendingFrameTimings.splice(0, pendingFrameTimings.length - 32);
          schedulePoll(pollGeneration);
        }
        settleInput(entry);
      }
    })().finally(() => {
      inputPumpRunning = false;
      if (inputQueue.length || pendingMove) pumpInputQueue();
    });
  }

  function postInput(payload, eventAt = performance.now()) {
    if (!controlled()) return Promise.resolve();
    if (["pointer-button", "pointer-click", "key", "text", "scroll"].includes(payload.action)) {
      discardPendingMove();
    } else {
      flushPendingMove();
    }
    return new Promise((resolve) => {
      const entry = { payload: { ...payload }, queuedAt: eventAt, resolvers: [resolve] };
      const last = inputQueue.at(-1);
      if (payload.action === "scroll" && last?.payload.action === "scroll") {
        last.payload.deltaX = Math.max(-800, Math.min(800, last.payload.deltaX + payload.deltaX));
        last.payload.deltaY = Math.max(-800, Math.min(800, last.payload.deltaY + payload.deltaY));
        last.payload.x = payload.x;
        last.payload.y = payload.y;
        last.resolvers.push(resolve);
      } else if (payload.action === "text" && last?.payload.action === "text"
        && last.payload.text.length + payload.text.length <= 512) {
        last.payload.text += payload.text;
        last.resolvers.push(resolve);
      } else {
        if (inputQueue.length >= maxQueuedInput) {
          setStatus("Desktop input is backed up. Wait a moment and try again.", true);
          window.dispatchEvent(new CustomEvent("glass-desktop-input-timing", {
            detail: { action: payload.action, queuedMs: 0, requestMs: 0, totalMs: 0, accepted: false },
          }));
          resolve();
          return;
        }
        inputQueue.push(entry);
      }
      pumpInputQueue();
    });
  }

  function moveInput(payload, eventAt = performance.now()) {
    const now = performance.now();
    if (!controlled()) return;
    pendingMove = { payload: { ...payload }, queuedAt: Number.isFinite(eventAt) ? eventAt : now, resolvers: [] };
    if (inputPumpRunning) return;
    const delay = Math.max(0, 35 - (now - lastMoveAt));
    clearTimeout(moveTimer);
    if (delay > 0) {
      moveTimer = setTimeout(() => {
        moveTimer = 0;
        lastMoveAt = performance.now();
        pumpInputQueue();
      }, delay);
    } else {
      lastMoveAt = now;
      pumpInputQueue();
    }
  }

  function setPointerPosition(x, y, transmit = true, eventAt = performance.now()) {
    pointerX = Math.min(1, Math.max(0, x));
    pointerY = Math.min(1, Math.max(0, y));
    updateCursor();
    if (transmit) moveInput({ action: "pointer-move", x: pointerX, y: pointerY }, eventAt);
  }

  function buttonAt(button, pressed, coordinates = { x: pointerX, y: pointerY }, eventAt = performance.now()) {
    void postInput({ action: "pointer-button", button, pressed, ...coordinates }, eventAt);
  }

  function clickAt(button, coordinates, eventAt = performance.now()) {
    if (coordinates) setPointerPosition(coordinates.x, coordinates.y, false, eventAt);
    void postInput({ action: "pointer-click", button, x: pointerX, y: pointerY }, eventAt);
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
    for (const surface of [stream, regionCanvas]) {
      surface.style.setProperty("--desktop-zoom", String(zoom));
      surface.style.setProperty("--desktop-pan-x", `${panX}px`);
      surface.style.setProperty("--desktop-pan-y", `${panY}px`);
    }
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
    void postInput({ action: "pointer-button", button, pressed, ...coordinates }, event.timeStamp);
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
    stopStream({ preserveFrame: true });
    stream.hidden = !streamLoaded;
    placeholder.hidden = streamLoaded;
    placeholder.textContent = "Reconnecting to the managed desktop…";
    requestAnimationFrame(() => {
      if (isOpen() && snapshot?.available) setStream(true);
    });
  }

  async function startDesktop() {
    const attempt = ++startAttempt;
    start.disabled = true;
    try {
      const selected = capabilities.find((item) => `${item.environment}:${item.lifecycle}` === selectedPreset);
      if (!selected?.available) {
        setStatus(selected?.reason || "That desktop environment is unavailable.", true);
        return;
      }
      startPending = true;
      clearTimeout(pollTimer);
      setStatus(selected.lifecycle === "reusable" && selected.profileExists
        ? "Reopening your saved workspace…"
        : selected.lifecycle === "reusable" ? "Creating your reusable workspace…" : "Starting a quick desktop…");
      start.textContent = "Starting…";
      await api("desktop/session", { action: "start", environment: selected.environment, lifecycle: selected.lifecycle }, requestController.signal, { timeoutMs: 60_000 });
      if (!isOpen() || attempt !== startAttempt) return;
      startPending = false;
      startAttempt += 1;
      await refresh();
    } catch (error) {
      if (!isOpen() || attempt !== startAttempt) return;
      startPending = false;
      startAttempt += 1;
      if (/timed out|connection unavailable/i.test(error.message || "")) {
        connectionError = error.message;
        await refresh();
        if (connectionError) setStatus("Host connection unavailable", true);
      } else {
        setStatus(error.message || "Could not start the managed desktop.", true);
      }
    } finally {
      if (isOpen()) {
        const selected = capabilities.find((item) => `${item.environment}:${item.lifecycle}` === selectedPreset);
        start.disabled = startPending || Boolean(connectionError) || !selected?.available;
        schedulePoll(pollGeneration, connectionError ? 1800 : 250);
      }
    }
  }

  async function deleteReusableProfile() {
    const profile = capabilities.find((item) => item.environment === "gnome" && item.lifecycle === "reusable" && item.profileExists);
    if (!profile || snapshot?.sessionId) return;
    if (!window.confirm("Delete the saved GNOME workspace and its files? This cannot be undone.")) return;
    deleteProfile.disabled = true;
    setStatus("Deleting the saved workspace…");
    try {
      await api("desktop/profile", { action: "delete", profileId: "default" }, requestController.signal);
      await refresh();
    } catch (error) {
      if (isOpen()) setStatus(error.message || "Could not delete the saved workspace.", true);
    } finally {
      if (isOpen()) deleteProfile.disabled = Boolean(connectionError);
    }
  }

  async function stopDesktop() {
    if (!snapshot?.sessionId) return;
    const confirmMessage = snapshot.lifecycle === "reusable"
      ? "Stop this desktop? Your saved GNOME workspace and files will be kept."
      : "Stop this quick desktop? Its temporary workspace will be removed.";
    if (!window.confirm(confirmMessage)) return;
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
    setTrackpadDrag(false);
    clearTimeout(pollTimer);
    clearTimeout(moveTimer);
    pollGeneration += 1;
    startAttempt += 1;
    startPending = false;
    requestController?.abort();
    requestController = null;
    pendingMove = null;
    inputQueue.splice(0).forEach(settleInput);
    for (const button of heldButtons) {
      void api("desktop/input", { action: "pointer-button", button, pressed: false, x: pointerX, y: pointerY })
        .then(() => heldButtons.delete(button))
        .catch(() => {});
    }
    for (const key of heldKeys) {
      void api("desktop/input", { action: "key", key, pressed: false })
        .then(() => heldKeys.delete(key))
        .catch(() => {});
    }
    activePointer = null;
    more.open = false;
    $("desktop-details").open = false;
    closeKeyboard();
    stopStream();
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
    startAttempt += 1;
    startPending = false;
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
  for (const preset of [quickPreset, reusablePreset]) preset.addEventListener("change", () => {
    if (!preset.checked) return;
    selectedPreset = preset.value;
    quickPreset.closest(".desktop-preset-option")?.classList.toggle("selected", quickPreset.checked);
    reusablePreset.closest(".desktop-preset-option")?.classList.toggle("selected", reusablePreset.checked);
    updatePresetNote();
    const selected = capabilities.find((item) => `${item.environment}:${item.lifecycle}` === selectedPreset);
    start.textContent = selected?.lifecycle === "reusable" && selected.profileExists ? "Resume workspace" : selected?.lifecycle === "reusable" ? "Open workspace" : "Start quick desktop";
    start.disabled = Boolean(connectionError) || !selected?.available;
  });
  deleteProfile.addEventListener("click", () => void deleteReusableProfile());
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
          startTime: event.timeStamp,
          startX: event.clientX,
          startY: event.clientY,
          lastX: event.clientX,
          lastY: event.clientY,
          startCoordinates: coordinates,
          moved: false,
          drag: !directTouch && trackpadDrag,
          pressed: null,
          timer: setTimeout(() => {
            if (!controlled() || !gesture || gesture.id !== event.pointerId || gesture.moved || gesture.drag) return;
            gesture.pressed = "right";
            buttonAt("right", true, directTouch ? gesture.startCoordinates : undefined, gesture.startTime);
          }, 480),
        };
      } else if (touchPoints.size === 2) {
        setTrackpadDrag(false);
        if (gesture?.timer) clearTimeout(gesture.timer);
        if (gesture?.pressed) buttonAt(gesture.pressed, false, undefined, event.timeStamp);
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
          }, event.timeStamp);
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
          if (gesture.drag && !gesture.pressed) {
            gesture.pressed = "left";
            buttonAt("left", true, undefined, event.timeStamp);
          }
          const bounds = viewport.getBoundingClientRect();
          setPointerPosition(pointerX + dx * 1.4 / bounds.width, pointerY + dy * 1.4 / bounds.height, true, event.timeStamp);
        } else {
          if (!gesture.pressed) {
            gesture.pressed = "left";
            setPointerPosition(gesture.startCoordinates.x, gesture.startCoordinates.y, false, event.timeStamp);
            buttonAt("left", true, gesture.startCoordinates, event.timeStamp);
          }
          const coordinates = point(event);
          if (coordinates) setPointerPosition(coordinates.x, coordinates.y, true, event.timeStamp);
        }
      } else if (gesture.pressed === "right") {
        const coordinates = directTouch ? point(event) : null;
        if (coordinates) setPointerPosition(coordinates.x, coordinates.y, true, event.timeStamp);
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
          buttonAt(gesture.pressed, false, directTouch ? point(event) || gesture.startCoordinates : undefined, event.timeStamp);
        } else if (controlled() && !gesture.moved && !gesture.drag && event.type === "pointerup") {
          const coordinates = directTouch ? point(event) : { x: pointerX, y: pointerY };
          clickAt("left", coordinates, event.timeStamp);
        }
        gesture = null;
        setTrackpadDrag(false);
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
    void postInput({ action: "scroll", deltaX: event.deltaX, deltaY: event.deltaY, ...coordinates }, event.timeStamp);
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
    setTrackpadDrag(false);
    directTouch = nextDirect;
    if (dragMode) dragMode.hidden = directTouch;
    touchMode.setAttribute("aria-pressed", String(directTouch));
    trackpadMode.setAttribute("aria-pressed", String(!directTouch));
    touchHint.textContent = directTouch
      ? "Touch to click · drag to move or drag-and-drop · hold for right-click · pinch to zoom/pan"
      : "Swipe to move · tap to click · two fingers to scroll · Drag for dragging";
    cursor.hidden = directTouch || !controlled();
    if (!directTouch) applyViewTransform();
  }
  dragMode?.addEventListener("click", () => {
    if (controlled() && !directTouch) setTrackpadDrag(!trackpadDrag);
  });
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
      stopStream();
      streamSession = null;
      streamLoaded = false;
    } else {
      void reconnectStream();
    }
  });

  return { open };
}
