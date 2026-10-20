  const nativeWorkerAbortError = () => {
    const error = new Error("The operation was aborted");
    error.name = "AbortError";
    return error;
  };
  const nativeWorkerTimeoutError = () => {
    const error = new Error("The operation timed out");
    error.name = "TimeoutError";
    return error;
  };
  const AbortSignalNative = typeof globalThis.__glassWorkerAbortSignalConstructor === "function"
    ? globalThis.__glassWorkerAbortSignalConstructor
    : function() {
        this.aborted = false;
        this.reason = undefined;
        this.onabort = null;
        this._abortListeners = [];
      };
  AbortSignalNative.prototype.addEventListener = function(type, listener) {
    if (String(type) !== "abort" || typeof listener !== "function" || this.aborted) return;
    if (!this._abortListeners.includes(listener)) this._abortListeners.push(listener);
  };
  AbortSignalNative.prototype.removeEventListener = function(type, listener) {
    if (String(type) !== "abort") return;
    this._abortListeners = this._abortListeners.filter(candidate => candidate !== listener);
  };
  AbortSignalNative.prototype.throwIfAborted = function() {
    if (this.aborted) throw this.reason;
  };
  const dispatchWorkerAbort = (signal) => {
    const event = { type: "abort", target: signal, currentTarget: signal };
    const listeners = signal._abortListeners.slice();
    signal._abortListeners = [];
    for (const listener of listeners) {
      try { listener.call(signal, event); } catch (_) {}
    }
    if (typeof signal.onabort === "function") {
      try { signal.onabort.call(signal, event); } catch (_) {}
    }
  };
  AbortSignalNative.timeout = function(delay) {
    const numeric = Number(delay);
    if (!Number.isFinite(numeric) || numeric < 0 || numeric > 2147483647)
      throw new RangeError("native Worker AbortSignal timeout is outside the bounded range");
    const signal = new AbortSignalNative();
    scheduleTimer(() => {
      if (signal.aborted) return;
      signal.aborted = true;
      signal.reason = nativeWorkerTimeoutError();
      dispatchWorkerAbort(signal);
    }, Math.trunc(numeric), [], false);
    return signal;
  };
  AbortSignalNative.abort = function(reason) {
    const signal = new AbortSignalNative();
    signal.aborted = true;
    signal.reason = reason === undefined ? nativeWorkerAbortError() : reason;
    return signal;
  };
  AbortSignalNative.any = function(signals) {
    if (!signals || typeof signals[Symbol.iterator] !== "function")
      throw new TypeError("native Worker AbortSignal.any requires an iterable");
    const inputs = [];
    for (const signal of signals) {
      if (inputs.length >= __GLASS_WORKER_MAX_COMMANDS__)
        throw new RangeError("native Worker AbortSignal.any signal limit exceeded");
      if (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean"
          || typeof signal.addEventListener !== "function"
          || typeof signal.removeEventListener !== "function")
        throw new TypeError("native Worker AbortSignal.any input is invalid");
      inputs.push(signal);
    }
    const combined = new AbortSignalNative();
    const alreadyAborted = inputs.find(signal => signal.aborted);
    if (alreadyAborted) {
      combined.aborted = true;
      combined.reason = alreadyAborted.reason === undefined
        ? nativeWorkerAbortError() : alreadyAborted.reason;
      return combined;
    }
    const listeners = [];
    const abortFrom = (source) => {
      if (combined.aborted) return;
      combined.aborted = true;
      combined.reason = source.reason === undefined ? nativeWorkerAbortError() : source.reason;
      dispatchWorkerAbort(combined);
      for (const [input, listener] of listeners) input.removeEventListener("abort", listener);
    };
    for (const input of inputs) {
      const listener = () => abortFrom(input);
      listeners.push([input, listener]);
      input.addEventListener("abort", listener);
    }
    return combined;
  };
  globalThis.__glassWorkerAbortSignalConstructor = AbortSignalNative;
  globalThis.AbortSignal = AbortSignalNative;
  const AbortControllerNative = typeof globalThis.__glassWorkerAbortControllerConstructor === "function"
    ? globalThis.__glassWorkerAbortControllerConstructor
    : function() { this.signal = new AbortSignalNative(); };
  AbortControllerNative.prototype.abort = function(reason) {
    if (this.signal.aborted) return;
    this.signal.aborted = true;
    this.signal.reason = reason === undefined ? nativeWorkerAbortError() : reason;
    dispatchWorkerAbort(this.signal);
  };
  globalThis.__glassWorkerAbortControllerConstructor = AbortControllerNative;
  globalThis.AbortController = AbortControllerNative;
  const importScriptCounts = globalThis.__glassWorkerImportScriptCounts instanceof Map
    ? globalThis.__glassWorkerImportScriptCounts
    : new Map(Object.entries(initialImportScriptCounts).map(([url, count]) => [url, Number(count)]));
  const importScriptsNative = (...urls) => {
    const values = urls.map((value) => String(value));
    for (const value of values) {
      if (!value || Number(importScriptCounts.get(value) || 0) < 1) {
        throw new TypeError("native Worker importScripts requires a preloaded string URL");
      }
    }
    for (const value of values) importScriptCounts.set(value, Number(importScriptCounts.get(value)) - 1);
  };
