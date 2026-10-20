  const workerWebSocketMessageLimit = __GLASS_WORKER_WEBSOCKET_MESSAGE_LIMIT__;
  const workerWebSocketProtocolLimit = __GLASS_WORKER_WEBSOCKET_PROTOCOL_LIMIT__;
  const workerWebSocketProtocolCountLimit = __GLASS_WORKER_WEBSOCKET_PROTOCOL_COUNT_LIMIT__;
  const workerWebSocketCloseReasonLimit = __GLASS_WORKER_WEBSOCKET_CLOSE_REASON_LIMIT__;
  const workerWebSocketSockets = globalThis.__glassWorkerWebSocketSockets instanceof Map
    ? globalThis.__glassWorkerWebSocketSockets
    : new Map();
  let nextWorkerWebSocketId = Number.isSafeInteger(globalThis.__glassNextWorkerWebSocketId)
    ? globalThis.__glassNextWorkerWebSocketId
    : 1;
  const workerWebSocketProtocolToken = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const normalizeWorkerWebSocketProtocols = (value) => {
    if (value === undefined) return [];
    const values = typeof value === "string"
      ? [value]
      : value && typeof value[Symbol.iterator] === "function"
        ? Array.from(value)
        : (() => { throw new TypeError("native Worker WebSocket protocols must be a string or iterable"); })();
    if (values.length > workerWebSocketProtocolCountLimit)
      throw new RangeError("native Worker WebSocket protocol count limit exceeded");
    const seen = new Set();
    return values.map((protocol) => {
      const text = String(protocol);
      if (!text || text.length > workerWebSocketProtocolLimit
          || !workerWebSocketProtocolToken.test(text) || seen.has(text))
        throw new SyntaxError("native Worker WebSocket protocol is invalid or duplicated");
      seen.add(text);
      return text;
    });
  };
  const workerWebSocketDispatch = (socket, type, event) => {
    const handler = socket["on" + type];
    if (typeof handler === "function") {
      try { handler.call(socket, event); } catch (_) {}
    }
    const listeners = socket.__glassWorkerWebSocketListeners[type] || [];
    for (const listener of listeners.slice()) {
      try { listener.call(socket, event); } catch (_) {}
    }
  };
  globalThis.__glassWorkerWebSocketSockets = workerWebSocketSockets;
  globalThis.__glassNextWorkerWebSocketId = nextWorkerWebSocketId;
  globalThis.__glassDispatchWorkerWebSocketEvent = (socketId, payload) => {
    const socket = workerWebSocketSockets.get(Number(socketId));
    if (!socket || !payload || typeof payload !== "object") return null;
    const type = String(payload.type || "");
    if (!["open", "message", "error", "close"].includes(type)) return null;
    if (type === "open") {
      socket.readyState = WorkerWebSocketNative.OPEN;
      socket.protocol = String(payload.protocol || "");
      workerWebSocketDispatch(socket, "open", { type: "open", target: socket, currentTarget: socket });
    } else if (type === "message") {
      let data = String(payload.data || "");
      if (payload.binary === true) {
        const bytes = decodeWorkerBase64(String(payload.dataBase64 || ""), workerWebSocketMessageLimit);
        if (socket.binaryType === "arraybuffer") data = new Uint8Array(bytes).buffer;
        else data = new WorkerBlob([new Uint8Array(bytes)], { type: "application/octet-stream" });
      }
      workerWebSocketDispatch(socket, "message", {
        type: "message",
        data,
        origin: String(payload.origin || ""),
        target: socket,
        currentTarget: socket,
      });
    } else if (type === "error") {
      workerWebSocketDispatch(socket, "error", {
        type: "error",
        message: String(payload.message || ""),
        target: socket,
        currentTarget: socket,
      });
    } else {
      socket.readyState = WorkerWebSocketNative.CLOSED;
      workerWebSocketDispatch(socket, "close", {
        type: "close",
        code: Number(payload.code) || 1006,
        reason: String(payload.reason || ""),
        wasClean: payload.wasClean === true,
        target: socket,
        currentTarget: socket,
      });
      workerWebSocketSockets.delete(Number(socketId));
    }
    return null;
  };
  const WorkerWebSocketNative = typeof globalThis.__glassWorkerWebSocketConstructor === "function"
    ? globalThis.__glassWorkerWebSocketConstructor
    : function(input, protocols) {
    if (!(this instanceof WorkerWebSocketNative)) throw new TypeError("native Worker WebSocket requires new");
    const source = input && input.__glassUrl === true ? input.href : input;
    const href = String(source);
    if (!/^(ws|wss):\/\/[^\s/]+(?:\/[^\s]*)?$/i.test(href) || href.includes("@"))
      throw new SyntaxError("native Worker WebSocket URL must use ws or wss without credentials");
    const normalizedProtocols = normalizeWorkerWebSocketProtocols(protocols);
    const socketId = nextWorkerWebSocketId;
    nextWorkerWebSocketId += 1;
    globalThis.__glassNextWorkerWebSocketId = nextWorkerWebSocketId;
    this.url = href;
    this.readyState = WorkerWebSocketNative.CONNECTING;
    this.bufferedAmount = 0;
    this.extensions = "";
    this.protocol = "";
    this.binaryType = "blob";
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.onclose = null;
    this.__glassSocketId = socketId;
    this.__glassWorkerWebSocketListeners = { open: [], message: [], error: [], close: [] };
    workerWebSocketSockets.set(socketId, this);
    pushCommand({ kind: "webSocketOpen", worker_id: workerId, socket_id: socketId, href, protocols: normalizedProtocols });
  };
  WorkerWebSocketNative.CONNECTING = 0;
  WorkerWebSocketNative.OPEN = 1;
  WorkerWebSocketNative.CLOSING = 2;
  WorkerWebSocketNative.CLOSED = 3;
  WorkerWebSocketNative.prototype.addEventListener = function(type, listener) {
    const name = String(type);
    if (!this.__glassWorkerWebSocketListeners[name] || typeof listener !== "function") return;
    if (!this.__glassWorkerWebSocketListeners[name].includes(listener)) this.__glassWorkerWebSocketListeners[name].push(listener);
  };
  WorkerWebSocketNative.prototype.removeEventListener = function(type, listener) {
    const name = String(type);
    if (!this.__glassWorkerWebSocketListeners[name]) return;
    this.__glassWorkerWebSocketListeners[name] = this.__glassWorkerWebSocketListeners[name].filter(candidate => candidate !== listener);
  };
  WorkerWebSocketNative.prototype.dispatchEvent = function(event) {
    if (!event || !event.type) throw new TypeError("native Worker WebSocket event is invalid");
    workerWebSocketDispatch(this, String(event.type), event);
    return true;
  };
  WorkerWebSocketNative.prototype.send = function(data) {
    if (this.readyState !== WorkerWebSocketNative.OPEN) throw new Error("native Worker WebSocket is not open");
    if (typeof data === "string") {
      if (data.length > workerWebSocketMessageLimit) throw new RangeError("native Worker WebSocket message exceeds its limit");
      pushCommand({ kind: "webSocketSend", worker_id: workerId, socket_id: this.__glassSocketId, data, data_base64: null });
      return;
    }
    let bytes = null;
    if (data && data.__glassWorkerBlob === true) bytes = data._bytes.slice();
    else if (data instanceof ArrayBuffer) bytes = Array.from(new Uint8Array(data));
    else if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(data))
      bytes = Array.from(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    if (!bytes) throw new TypeError("native Worker WebSocket data must be text, Blob, or an ArrayBuffer view");
    pushCommand({ kind: "webSocketSend", worker_id: workerId, socket_id: this.__glassSocketId, data: null, data_base64: encodeWorkerBase64(bytes, workerWebSocketMessageLimit) });
  };
  WorkerWebSocketNative.prototype.close = function(code, reason) {
    if (this.readyState === WorkerWebSocketNative.CLOSING || this.readyState === WorkerWebSocketNative.CLOSED) return;
    const normalizedCode = code === undefined ? 1000 : Number(code);
    if (!Number.isInteger(normalizedCode) || (normalizedCode !== 1000 && (normalizedCode < 3000 || normalizedCode > 4999)))
      throw new RangeError("native Worker WebSocket close code is invalid");
    const normalizedReason = reason === undefined ? "" : String(reason);
    if (normalizedReason.length > workerWebSocketCloseReasonLimit)
      throw new SyntaxError("native Worker WebSocket close reason exceeds its limit");
    this.readyState = WorkerWebSocketNative.CLOSING;
    pushCommand({ kind: "webSocketClose", worker_id: workerId, socket_id: this.__glassSocketId, code: normalizedCode, reason: normalizedReason });
  };
  Object.defineProperties(WorkerWebSocketNative.prototype, {
    CONNECTING: { value: 0 }, OPEN: { value: 1 }, CLOSING: { value: 2 }, CLOSED: { value: 3 },
  });
  globalThis.__glassWorkerWebSocketConstructor = WorkerWebSocketNative;
  globalThis.WebSocket = WorkerWebSocketNative;
  const workerEventSourceMessageLimit = __GLASS_WORKER_EVENTSOURCE_MESSAGE_LIMIT__;
  const workerEventSourceFieldLimit = __GLASS_WORKER_EVENTSOURCE_FIELD_LIMIT__;
  const workerEventSources = globalThis.__glassWorkerEventSources instanceof Map
    ? globalThis.__glassWorkerEventSources
    : new Map();
  let nextWorkerEventSourceId = Number.isSafeInteger(globalThis.__glassNextWorkerEventSourceId)
    ? globalThis.__glassNextWorkerEventSourceId
    : 1;
  const workerEventSourceDispatch = (source, type, event) => {
    const handler = source["on" + type];
    if (typeof handler === "function") {
      try { handler.call(source, event); } catch (_) {}
    }
    const listeners = source.__glassWorkerEventSourceListeners[type] || [];
    for (const listener of listeners.slice()) {
      try { listener.call(source, event); } catch (_) {}
    }
  };
  globalThis.__glassWorkerEventSources = workerEventSources;
  globalThis.__glassNextWorkerEventSourceId = nextWorkerEventSourceId;
  globalThis.__glassDispatchWorkerEventSourceEvent = (sourceId, payload) => {
    const source = workerEventSources.get(Number(sourceId));
    if (!source || !payload || typeof payload !== "object") return null;
    const type = String(payload.type || "");
    if (type === "open") {
      source.readyState = WorkerEventSourceNative.OPEN;
      workerEventSourceDispatch(source, "open", { type: "open", target: source, currentTarget: source });
    } else if (type === "message") {
      const data = String(payload.data || "");
      if (data.length > workerEventSourceMessageLimit) return null;
      const eventType = String(payload.event || "message");
      if (!eventType || eventType.length > workerEventSourceFieldLimit) return null;
      workerEventSourceDispatch(source, eventType, {
        type: eventType,
        data,
        origin: String(payload.origin || ""),
        lastEventId: String(payload.lastEventId || ""),
        target: source,
        currentTarget: source,
      });
    } else if (type === "error") {
      source.readyState = WorkerEventSourceNative.CONNECTING;
      workerEventSourceDispatch(source, "error", {
        type: "error",
        message: String(payload.message || ""),
        target: source,
        currentTarget: source,
      });
    } else if (type === "close") {
      source.readyState = WorkerEventSourceNative.CLOSED;
      workerEventSourceDispatch(source, "close", { type: "close", target: source, currentTarget: source });
      workerEventSources.delete(Number(sourceId));
    }
    return null;
  };
  const WorkerEventSourceNative = typeof globalThis.__glassWorkerEventSourceConstructor === "function"
    ? globalThis.__glassWorkerEventSourceConstructor
    : function(input, options) {
    if (!(this instanceof WorkerEventSourceNative)) throw new TypeError("native Worker EventSource requires new");
    const source = input && input.__glassUrl === true ? input.href : input;
    const href = String(source);
    if (!href || href.includes("\\u0000"))
      throw new SyntaxError("native Worker EventSource URL must be a non-empty URL");
    if (options !== undefined && (options === null || typeof options !== "object"))
      throw new TypeError("native Worker EventSource options must be an object");
    const withCredentials = options !== undefined && options.withCredentials === true;
    const sourceId = nextWorkerEventSourceId;
    nextWorkerEventSourceId += 1;
    globalThis.__glassNextWorkerEventSourceId = nextWorkerEventSourceId;
    this.url = href;
    this.readyState = WorkerEventSourceNative.CONNECTING;
    this.withCredentials = withCredentials;
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.__glassSourceId = sourceId;
    this.__glassWorkerEventSourceListeners = {};
    workerEventSources.set(sourceId, this);
    pushCommand({ kind: "eventSourceOpen", worker_id: workerId, source_id: sourceId, href, with_credentials: withCredentials });
  };
  WorkerEventSourceNative.CONNECTING = 0;
  WorkerEventSourceNative.OPEN = 1;
  WorkerEventSourceNative.CLOSED = 2;
  WorkerEventSourceNative.prototype.addEventListener = function(type, listener) {
    const name = String(type);
    if (!name || name.length > workerEventSourceFieldLimit || typeof listener !== "function") return;
    if (!this.__glassWorkerEventSourceListeners[name]) this.__glassWorkerEventSourceListeners[name] = [];
    if (!this.__glassWorkerEventSourceListeners[name].includes(listener)) this.__glassWorkerEventSourceListeners[name].push(listener);
  };
  WorkerEventSourceNative.prototype.removeEventListener = function(type, listener) {
    const name = String(type);
    if (!this.__glassWorkerEventSourceListeners[name]) return;
    this.__glassWorkerEventSourceListeners[name] = this.__glassWorkerEventSourceListeners[name].filter(candidate => candidate !== listener);
  };
  WorkerEventSourceNative.prototype.dispatchEvent = function(event) {
    if (!event || !event.type) throw new TypeError("native Worker EventSource event is invalid");
    workerEventSourceDispatch(this, String(event.type), event);
    return true;
  };
  WorkerEventSourceNative.prototype.close = function() {
    if (this.readyState === WorkerEventSourceNative.CLOSED) return;
    this.readyState = WorkerEventSourceNative.CLOSED;
    workerEventSources.delete(this.__glassSourceId);
    pushCommand({ kind: "eventSourceClose", worker_id: workerId, source_id: this.__glassSourceId });
  };
  Object.defineProperties(WorkerEventSourceNative.prototype, {
    CONNECTING: { value: 0 }, OPEN: { value: 1 }, CLOSED: { value: 2 },
  });
  globalThis.__glassWorkerEventSourceConstructor = WorkerEventSourceNative;
  globalThis.EventSource = WorkerEventSourceNative;
