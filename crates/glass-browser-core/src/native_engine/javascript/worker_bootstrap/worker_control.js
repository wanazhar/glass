  globalThis.importScripts = isModuleWorker
    ? (() => { throw new TypeError("importScripts is unavailable in module workers"); })
    : importScriptsNative;
  globalThis.addEventListener = addEventListener;
  globalThis.removeEventListener = removeEventListener;
  globalThis.postMessage = (message, options) => {
    if (closed) return;
    const envelope = glassMessageCloneWithTransfers(message, glassMessageTransferList(options));
    pushCommand({ kind: "workerPostMessage", worker_id: workerId, data: envelope.data, transfer_ports: envelope.transfer_ports });
  };
  globalThis.close = () => {
    if (closed) return;
    closed = true;
    globalThis.__glassWorkerClosed = true;
    pushCommand({ kind: "workerClose", worker_id: workerId });
  };
  globalThis.__glassDispatchWorkerMessage = (payload) => {
    if (closed) return null;
    if (payload && Array.isArray(payload.object_urls))
      globalThis.__glassInstallObjectUrlTransfers(payload.object_urls);
    const MessageEventConstructor = globalThis.__glassWorkerMessageEventConstructor || globalThis.MessageEvent;
    let envelope;
    try {
      envelope = glassMessageDecodeEnvelope(payload);
    } catch (_error) {
      const event = new MessageEventConstructor("messageerror", {
        data: null,
        ports: [],
        origin: "",
        source: null,
        bubbles: false,
        cancelable: false,
      });
      event.target = globalThis;
      event.currentTarget = globalThis;
      event.eventPhase = 2;
      dispatch("messageerror", event);
      event.currentTarget = null;
      event.eventPhase = 0;
      return null;
    }
    const event = new MessageEventConstructor("message", {
      data: envelope.data,
      ports: envelope.ports,
      origin: "",
      source: null,
      bubbles: false,
      cancelable: false,
    });
    event.target = globalThis;
    event.currentTarget = globalThis;
    event.eventPhase = 2;
    dispatch("message", event);
    event.currentTarget = null;
    event.eventPhase = 0;
    return null;
  };
  globalThis.__glassReportWorkerScriptError = (error) => {
    let message;
    try {
      message = error && (typeof error === "object" || typeof error === "function")
        && "message" in error
        ? String(error.message)
        : String(error);
    } catch (_error) {
      message = "Script error.";
    }
    message = Array.from(message).slice(0, 4096).join("");
    if (workerErrorReporting) return JSON.stringify({ handled: false, message });
    workerErrorReporting = true;
    globalThis.__glassWorkerErrorReporting = true;
    const event = new WorkerErrorEventNative("error", {
      message,
      filename: workerUrl,
      lineno: 0,
      colno: 0,
      error,
      cancelable: true,
    });
    event.target = globalThis;
    event.currentTarget = globalThis;
    event.eventPhase = 2;
    try {
      const handler = onError;
      if (typeof handler === "function") {
        let result;
        try {
          result = handler.call(globalThis, event.message, event.filename, event.lineno, event.colno, event.error);
        } catch (_error) {}
        if (result === true) event.defaultPrevented = true;
      }
      const callbacks = listeners.get("error") || [];
      for (const callback of callbacks.slice()) {
        try { callback.call(globalThis, event); } catch (_error) {}
      }
    } finally {
      event.currentTarget = null;
      event.eventPhase = 0;
      workerErrorReporting = false;
      globalThis.__glassWorkerErrorReporting = false;
    }
    return JSON.stringify({ handled: event.defaultPrevented, message: event.message });
  };
  const reportWorkerCallbackException = (error, forwardUnhandledError) => {
    const report = JSON.parse(globalThis.__glassReportWorkerScriptError(error));
    if (!report || typeof report.handled !== "boolean" || typeof report.message !== "string")
      throw new TypeError("native Worker error reporter returned invalid data");
    if (forwardUnhandledError && !report.handled) pushCommand({
      kind: "workerScriptError",
      worker_id: workerId,
      message: report.message,
    });
  };
  if (reportMessageCallbackErrors || captureConnectCallbackErrors) {
    globalThis.__glassReportWorkerMessagePortCallbackException = (error) =>
      reportWorkerCallbackException(error, forwardMessageCallbackErrors);
  }
  globalThis.__glassDispatchWorkerCspViolations = (violations) => {
    if (!Array.isArray(violations)) throw new TypeError("native Worker CSP violations are invalid");
    return violations.map((descriptor) => {
      if (!descriptor || typeof descriptor !== "object") throw new TypeError("native Worker CSP violation is invalid");
      const event = new WorkerSecurityPolicyViolationEventNative("securitypolicyviolation", {
        documentURI: descriptor.document_uri,
        referrer: descriptor.referrer,
        blockedURI: descriptor.blocked_uri,
        effectiveDirective: descriptor.effective_directive,
        violatedDirective: descriptor.violated_directive,
        originalPolicy: descriptor.original_policy,
        sourceFile: descriptor.source_file,
        sample: descriptor.sample,
        disposition: descriptor.disposition,
        statusCode: descriptor.status_code,
        lineNumber: descriptor.line_number,
        columnNumber: descriptor.column_number,
        bubbles: true,
        cancelable: false,
      });
      event.target = globalThis;
      event.currentTarget = globalThis;
      event.eventPhase = 2;
      dispatch("securitypolicyviolation", event);
      event.currentTarget = null;
      event.eventPhase = 0;
      return null;
    });
  };
  globalThis.__glassWorkerListeners = listeners;
  globalThis.__glassWorkerOnMessage = onMessage;
  globalThis.__glassWorkerMessageErrorHandlerSlot = messageErrorHandlerSlot;
  globalThis.__glassWorkerOnError = onError;
  globalThis.__glassWorkerOnConnect = onConnect;
  globalThis.__glassWorkerOnSecurityPolicyViolation = onSecurityPolicyViolation;
  globalThis.__glassWorkerErrorReporting = workerErrorReporting;
  globalThis.__glassHostCommands = commands;
  Object.defineProperty(globalThis, "onmessage", {
    configurable: true,
    enumerable: true,
    get() { return onMessage; },
    set(value) { onMessage = typeof value === "function" ? value : null; globalThis.__glassWorkerOnMessage = onMessage; },
  });
  Object.defineProperty(globalThis, "onmessageerror", {
    configurable: true,
    enumerable: true,
    get() { return messageErrorHandlerSlot ? messageErrorHandlerSlot.value : null; },
    set(value) {
      const handlerValue = value !== null && value !== undefined
          && (typeof value === "object" || typeof value === "function")
        ? value
        : null;
      if (handlerValue === null) {
        if (messageErrorHandlerSlot) {
          messageErrorHandlerSlot.active = false;
          const callbacks = listeners.get("messageerror") || [];
          listeners.set(
            "messageerror",
            callbacks.filter((callback) => callback !== messageErrorHandlerSlot),
          );
          messageErrorHandlerSlot = null;
        }
      } else if (messageErrorHandlerSlot) {
        messageErrorHandlerSlot.value = handlerValue;
      } else {
        messageErrorHandlerSlot = {
          __glassWorkerMessageErrorHandlerSlot: true,
          active: true,
          value: handlerValue,
        };
        const callbacks = listeners.get("messageerror") || [];
        callbacks.push(messageErrorHandlerSlot);
        listeners.set("messageerror", callbacks);
      }
      globalThis.__glassWorkerMessageErrorHandlerSlot = messageErrorHandlerSlot;
    },
  });
  Object.defineProperty(globalThis, "onerror", {
    configurable: true,
    enumerable: true,
    get() { return onError; },
    set(value) { onError = typeof value === "function" ? value : null; globalThis.__glassWorkerOnError = onError; },
  });
  Object.defineProperty(globalThis, "onconnect", {
    configurable: true,
    enumerable: true,
    get() { return onConnect; },
    set(value) { onConnect = typeof value === "function" ? value : null; globalThis.__glassWorkerOnConnect = onConnect; },
  });
  Object.defineProperty(globalThis, "onsecuritypolicyviolation", {
    configurable: true,
    enumerable: true,
    get() { return onSecurityPolicyViolation; },
    set(value) { onSecurityPolicyViolation = typeof value === "function" ? value : null; globalThis.__glassWorkerOnSecurityPolicyViolation = onSecurityPolicyViolation; },
  });
  globalThis.__glassWorkerTimers = timers;
  globalThis.__glassWorkerRunningTimers = runningTimers;
  globalThis.__glassWorkerImportScriptCounts = importScriptCounts;
  globalThis.setTimeout = setTimeoutNative;
  globalThis.setInterval = setIntervalNative;
  globalThis.clearTimeout = clearTimer;
  globalThis.clearInterval = clearTimer;
  globalThis.__glassRunWorkerTimers = (currentNow) => {
    if (closed) return null;
    const now = Number.isFinite(Number(currentNow)) ? Number(currentNow) : nowMs;
    const pending = Array.from(timers.entries())
      .filter(([, timer]) => Number(timer.dueAt) <= now)
      .sort((left, right) => Number(left[1].dueAt) - Number(right[1].dueAt) || left[0] - right[0]);
    for (const [id, timer] of pending) {
      if (!timers.has(id)) continue;
      timers.delete(id);
      runningTimers.set(id, timer);
      try {
        timer.callback(...timer.args);
      } finally {
        runningTimers.delete(id);
        if (timer.intervalMs > 0 && !timer.cancelled && !closed) {
          timer.dueAt = now + timer.intervalMs;
          timers.set(id, timer);
        }
      }
    }
    return null;
  };
  globalThis.__glassWorkerCommandBuffer = commands;
})()