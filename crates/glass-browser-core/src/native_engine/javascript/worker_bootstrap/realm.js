(() => {
  const workerId = __GLASS_WORKER_WORKER_ID__;
  const workerUrl = __GLASS_WORKER_WORKER_URL__;
  const nowMs = __GLASS_WORKER_NOW_MS__;
  const initialImportScriptCounts = __GLASS_WORKER_IMPORT_SCRIPT_COUNTS__;
  const workerGlobalReferrerPolicy = __GLASS_WORKER_WORKER_GLOBAL_REFERRER_POLICY__;
  const initialWorkerCryptoBytes = __GLASS_WORKER_INITIAL_RANDOM_BYTES__;
  const isModuleWorker = __GLASS_WORKER_IS_MODULE__;
  const reportMessageCallbackErrors = __GLASS_WORKER_REPORT_MESSAGE_CALLBACK_ERRORS__;
  const forwardMessageCallbackErrors = __GLASS_WORKER_FORWARD_MESSAGE_CALLBACK_ERRORS__;
  const captureConnectCallbackErrors = __GLASS_WORKER_CAPTURE_CONNECT_CALLBACK_ERRORS__;
  globalThis.__glassWorkerId = workerId;
  globalThis.__glassMessageRealmKey = "worker:" + String(workerId);
  const commands = [];
  const activeCommands = () => Array.isArray(globalThis.__glassWorkerCommandBuffer)
    ? globalThis.__glassWorkerCommandBuffer
    : commands;
  const pushCommand = (command) => {
    const target = activeCommands();
    if (target.length >= __GLASS_WORKER_MAX_COMMANDS__) throw new RangeError("native Worker command limit exceeded");
    target.push(command);
  };
  const cloneMessageData = (value, options) => glassMessageStructuredClone(value, options);
  const timers = globalThis.__glassWorkerTimers instanceof Map
    ? globalThis.__glassWorkerTimers
    : new Map();
  const runningTimers = globalThis.__glassWorkerRunningTimers instanceof Map
    ? globalThis.__glassWorkerRunningTimers
    : new Map();
  let nextTimerId = Number.isSafeInteger(globalThis.__glassWorkerNextTimerId)
    ? globalThis.__glassWorkerNextTimerId
    : 1;
  globalThis.__glassWorkerNowMs = nowMs;
  globalThis.performance = {
    now() { return Number(globalThis.__glassWorkerNowMs) || 0; },
  };
  const scheduleTimer = (callback, delay, args, repeating) => {
    if (typeof callback !== "function") throw new TypeError("timer callback must be callable");
    if (timers.size >= __GLASS_WORKER_MAX_TIMERS__) throw new RangeError("native Worker timer limit exceeded");
    const id = nextTimerId;
    nextTimerId += 1;
    globalThis.__glassWorkerNextTimerId = nextTimerId;
    const numericDelay = Number(delay);
    const normalizedDelay = Number.isFinite(numericDelay)
      ? Math.max(0, Math.min(2147483647, numericDelay))
      : 0;
    timers.set(id, {
      callback,
      args,
      dueAt: nowMs + normalizedDelay,
      intervalMs: repeating ? Math.max(1, normalizedDelay) : 0,
      cancelled: false,
    });
    return id;
  };
  const setTimeoutNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, false);
  const setIntervalNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, true);
  const clearTimer = (id) => {
    const timerId = Number(id);
    const timer = timers.get(timerId) || runningTimers.get(timerId);
    if (timer) timer.cancelled = true;
    timers.delete(timerId);
  };
  const listeners = globalThis.__glassWorkerListeners instanceof Map
    ? globalThis.__glassWorkerListeners
    : new Map();
  let onMessage = typeof globalThis.__glassWorkerOnMessage === "function"
    ? globalThis.__glassWorkerOnMessage
    : null;
  const existingMessageErrorHandlerSlot = globalThis.__glassWorkerMessageErrorHandlerSlot;
  let messageErrorHandlerSlot = existingMessageErrorHandlerSlot
      && existingMessageErrorHandlerSlot.active === true
    ? existingMessageErrorHandlerSlot
    : null;
  let onError = typeof globalThis.__glassWorkerOnError === "function"
    ? globalThis.__glassWorkerOnError
    : null;
  let onConnect = typeof globalThis.__glassWorkerOnConnect === "function"
    ? globalThis.__glassWorkerOnConnect
    : null;
  let onSecurityPolicyViolation = typeof globalThis.__glassWorkerOnSecurityPolicyViolation === "function"
    ? globalThis.__glassWorkerOnSecurityPolicyViolation
    : null;
  let closed = globalThis.__glassWorkerClosed === true;
  let workerErrorReporting = globalThis.__glassWorkerErrorReporting === true;
  const dispatch = (type, event) => {
    const handler = type === "message"
      ? onMessage
      : type === "connect"
        ? onConnect
        : type === "securitypolicyviolation"
          ? onSecurityPolicyViolation
          : null;
    if (typeof handler === "function") {
      let result;
      try { result = handler.call(globalThis, event); } catch (error) {
        if (type === "message" && reportMessageCallbackErrors)
          reportWorkerCallbackException(error, forwardMessageCallbackErrors);
        else if (type === "connect" && captureConnectCallbackErrors)
          reportWorkerCallbackException(error, false);
      }
      if ((type === "message" || type === "connect") && result === false)
        event.defaultPrevented = true;
    }
    const callbacks = listeners.get(type) || [];
    for (const callback of callbacks.slice()) {
      try {
        if (callback && callback.__glassWorkerMessageErrorHandlerSlot === true) {
          if (!callback.active || typeof callback.value !== "function") continue;
          callback.value.call(globalThis, event);
        } else {
          callback.call(globalThis, event);
        }
      } catch (error) {
        if ((type === "message" || type === "messageerror") && reportMessageCallbackErrors)
          reportWorkerCallbackException(error, forwardMessageCallbackErrors);
        else if (type === "connect" && captureConnectCallbackErrors)
          reportWorkerCallbackException(error, false);
      }
    }
  };
  const addEventListener = (type, callback) => {
    const name = String(type);
    if (!name || typeof callback !== "function") return;
    const callbacks = listeners.get(name) || [];
    if (!callbacks.includes(callback)) callbacks.push(callback);
    listeners.set(name, callbacks);
  };
  const removeEventListener = (type, callback) => {
    const name = String(type);
    const callbacks = listeners.get(name) || [];
    listeners.set(name, callbacks.filter((candidate) => candidate !== callback));
  };
  globalThis.self = globalThis;
  globalThis.location = Object.freeze({ href: workerUrl, toString() { return workerUrl; } });
