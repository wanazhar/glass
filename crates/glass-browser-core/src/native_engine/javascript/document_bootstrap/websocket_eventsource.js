  const websocketMessageLimit = {websocket_message_limit};
  const websocketProtocolLimit = {websocket_protocol_limit};
  const websocketProtocolCountLimit = {websocket_protocol_count_limit};
  const websocketCloseReasonLimit = {websocket_close_reason_limit};
  const websocketSockets = globalThis.__glassWebSocketSockets instanceof Map
    ? globalThis.__glassWebSocketSockets
    : new Map();
  let nextWebSocketId = Number.isSafeInteger(globalThis.__glassNextWebSocketId)
    ? globalThis.__glassNextWebSocketId
    : 1;
  const websocketProtocolToken = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const normalizeWebSocketProtocols = (value) => {{
    if (value === undefined) return [];
    const values = typeof value === "string"
      ? [value]
      : value && typeof value[Symbol.iterator] === "function"
        ? Array.from(value)
        : (() => {{ throw new TypeError("native WebSocket protocols must be a string or iterable"); }})();
    if (values.length > websocketProtocolCountLimit)
      throw new RangeError("native WebSocket protocol count limit exceeded");
    const seen = new Set();
    return values.map((protocol) => {{
      const text = String(protocol);
      if (!text || text.length > websocketProtocolLimit || !websocketProtocolToken.test(text) || seen.has(text))
        throw new SyntaxError("native WebSocket protocol is invalid or duplicated");
      seen.add(text);
      return text;
    }});
  }};
  const websocketDispatch = (socket, type, event) => {{
    const listeners = socket.__glassWebSocketListeners[type]
      ? socket.__glassWebSocketListeners[type].slice()
      : [];
    if (typeof socket["on" + type] === "function") {{
      try {{ socket["on" + type].call(socket, event); }} catch (_) {{}}
    }}
    for (const listener of listeners) {{
      try {{ listener.call(socket, event); }} catch (_) {{}}
    }}
  }};
  globalThis.__glassWebSocketSockets = websocketSockets;
  globalThis.__glassNextWebSocketId = nextWebSocketId;
  globalThis.__glassDispatchWebSocketEvent = (socketId, payload) => {{
    const socket = websocketSockets.get(Number(socketId));
    if (!socket || !payload || typeof payload !== "object") return null;
    const type = String(payload.type || "");
    if (!["open", "message", "error", "close"].includes(type)) return null;
    if (type === "open") {{
      socket.readyState = WebSocketNative.OPEN;
      socket.protocol = String(payload.protocol || "");
      websocketDispatch(socket, "open", {{ type: "open", target: socket, currentTarget: socket }});
    }} else if (type === "message") {{
      let data = String(payload.data || "");
      if (payload.binary === true) {{
        const bytes = decodeBase64(String(payload.dataBase64 || ""), websocketMessageLimit);
        if (socket.binaryType === "arraybuffer") data = new Uint8Array(bytes).buffer;
        else {{
          const blob = new BlobNative([], {{ type: "application/octet-stream" }});
          blob._bytes = bytes;
          blob._text = utf8TextFromBytes(bytes);
          blob.size = bytes.length;
          data = blob;
        }}
      }}
      websocketDispatch(socket, "message", {{ type: "message", data, origin: String(payload.origin || ""), target: socket, currentTarget: socket }});
    }} else if (type === "error") {{
      websocketDispatch(socket, "error", {{ type: "error", message: String(payload.message || ""), target: socket, currentTarget: socket }});
    }} else {{
      socket.readyState = WebSocketNative.CLOSED;
      websocketDispatch(socket, "close", {{
        type: "close",
        code: Number(payload.code) || 1006,
        reason: String(payload.reason || ""),
        wasClean: payload.wasClean === true,
        target: socket,
        currentTarget: socket,
      }});
      websocketSockets.delete(Number(socketId));
    }}
    return null;
  }};
  const WebSocketNative = function(input, protocols) {{
    if (!(this instanceof WebSocketNative)) throw new TypeError("native WebSocket requires new");
    const source = input && input.__glassUrl === true ? input.href : input;
    const resolved = new URLNative(String(source), host.url);
    if (!["ws:", "wss:"].includes(resolved.protocol) || resolved.username || resolved.password || !resolved.host)
      throw new SyntaxError("native WebSocket URL must use ws or wss without credentials");
    const normalizedProtocols = normalizeWebSocketProtocols(protocols);
    const socketId = nextWebSocketId;
    nextWebSocketId += 1;
    globalThis.__glassNextWebSocketId = nextWebSocketId;
    this.url = resolved.href;
    this.readyState = WebSocketNative.CONNECTING;
    this.bufferedAmount = 0;
    this.extensions = "";
    this.protocol = "";
    this.binaryType = "blob";
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.onclose = null;
    this.__glassSocketId = socketId;
    this.__glassWebSocketListeners = {{ open: [], message: [], error: [], close: [] }};
    websocketSockets.set(socketId, this);
    pushCommand({{ kind: "webSocketOpen", socket_id: socketId, href: resolved.href, protocols: normalizedProtocols }});
  }};
  WebSocketNative.CONNECTING = 0;
  WebSocketNative.OPEN = 1;
  WebSocketNative.CLOSING = 2;
  WebSocketNative.CLOSED = 3;
  WebSocketNative.prototype.addEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWebSocketListeners[name] || typeof listener !== "function") return;
    if (!this.__glassWebSocketListeners[name].includes(listener)) this.__glassWebSocketListeners[name].push(listener);
  }};
  WebSocketNative.prototype.removeEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassWebSocketListeners[name]) return;
    this.__glassWebSocketListeners[name] = this.__glassWebSocketListeners[name].filter(candidate => candidate !== listener);
  }};
  WebSocketNative.prototype.dispatchEvent = function(event) {{
    if (!event || !event.type) throw new TypeError("native WebSocket event is invalid");
    websocketDispatch(this, String(event.type), event);
    return true;
  }};
  WebSocketNative.prototype.send = function(data) {{
    if (this.readyState !== WebSocketNative.OPEN) throw new Error("native WebSocket is not open");
    if (typeof data === "string") {{
      if (data.length > websocketMessageLimit) throw new RangeError("native WebSocket message exceeds its limit");
      pushCommand({{ kind: "webSocketSend", socket_id: this.__glassSocketId, data, data_base64: null }});
      return;
    }}
    let bytes = null;
    if (data && data.__glassNativeBlob === true) bytes = blobBytes(data);
    else if (data instanceof ArrayBuffer) bytes = Array.from(new Uint8Array(data));
    else if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(data))
      bytes = Array.from(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    if (!bytes) throw new TypeError("native WebSocket data must be text, Blob, or an ArrayBuffer view");
    pushCommand({{ kind: "webSocketSend", socket_id: this.__glassSocketId, data: null, data_base64: encodeBase64(bytes, websocketMessageLimit) }});
  }};
  WebSocketNative.prototype.close = function(code, reason) {{
    if (this.readyState === WebSocketNative.CLOSING || this.readyState === WebSocketNative.CLOSED) return;
    const normalizedCode = code === undefined ? 1000 : Number(code);
    if (!Number.isInteger(normalizedCode) || (normalizedCode !== 1000 && (normalizedCode < 3000 || normalizedCode > 4999)))
      throw new RangeError("native WebSocket close code is invalid");
    const normalizedReason = reason === undefined ? "" : String(reason);
    if (normalizedReason.length > websocketCloseReasonLimit)
      throw new SyntaxError("native WebSocket close reason exceeds its limit");
    this.readyState = WebSocketNative.CLOSING;
    pushCommand({{ kind: "webSocketClose", socket_id: this.__glassSocketId, code: normalizedCode, reason: normalizedReason }});
  }};
  Object.defineProperties(WebSocketNative.prototype, {{
    CONNECTING: {{ value: 0 }}, OPEN: {{ value: 1 }}, CLOSING: {{ value: 2 }}, CLOSED: {{ value: 3 }},
  }});
  globalThis.WebSocket = WebSocketNative;
  const eventSourceMessageLimit = {eventsource_message_limit};
  const eventSourceFieldLimit = {eventsource_field_limit};
  const eventSources = globalThis.__glassEventSources instanceof Map
    ? globalThis.__glassEventSources
    : new Map();
  let nextEventSourceId = Number.isSafeInteger(globalThis.__glassNextEventSourceId)
    ? globalThis.__glassNextEventSourceId
    : 1;
  const eventSourceDispatch = (source, type, event) => {{
    const listeners = source.__glassEventSourceListeners[type]
      ? source.__glassEventSourceListeners[type].slice()
      : [];
    const handler = source["on" + type];
    if (typeof handler === "function") {{
      try {{ handler.call(source, event); }} catch (_) {{}}
    }}
    for (const listener of listeners) {{
      try {{ listener.call(source, event); }} catch (_) {{}}
    }}
  }};
  globalThis.__glassEventSources = eventSources;
  globalThis.__glassNextEventSourceId = nextEventSourceId;
  globalThis.__glassDispatchEventSourceEvent = (sourceId, payload) => {{
    const source = eventSources.get(Number(sourceId));
    if (!source || !payload || typeof payload !== "object") return null;
    const type = String(payload.type || "");
    if (type === "open") {{
      source.readyState = EventSourceNative.OPEN;
      eventSourceDispatch(source, "open", {{ type: "open", target: source, currentTarget: source }});
    }} else if (type === "message") {{
      const data = String(payload.data || "");
      if (data.length > eventSourceMessageLimit) return null;
      const eventType = String(payload.event || "message");
      if (!eventType || eventType.length > eventSourceFieldLimit) return null;
      const event = {{
        type: eventType,
        data,
        origin: String(payload.origin || ""),
        lastEventId: String(payload.lastEventId || ""),
        target: source,
        currentTarget: source,
      }};
      eventSourceDispatch(source, eventType, event);
    }} else if (type === "error") {{
      source.readyState = EventSourceNative.CONNECTING;
      eventSourceDispatch(source, "error", {{ type: "error", message: String(payload.message || ""), target: source, currentTarget: source }});
    }} else if (type === "close") {{
      source.readyState = EventSourceNative.CLOSED;
      eventSourceDispatch(source, "close", {{ type: "close", target: source, currentTarget: source }});
      eventSources.delete(Number(sourceId));
    }}
    return null;
  }};
  const EventSourceNative = function(input, options) {{
    if (!(this instanceof EventSourceNative)) throw new TypeError("native EventSource requires new");
    const source = input && input.__glassUrl === true ? input.href : input;
    const resolved = new URLNative(String(source), host.url);
    if (!["http:", "https:"].includes(resolved.protocol) || resolved.username || resolved.password || !resolved.host)
      throw new SyntaxError("native EventSource URL must use http or https without credentials");
    if (options !== undefined && (options === null || typeof options !== "object"))
      throw new TypeError("native EventSource options must be an object");
    const withCredentials = options !== undefined && options.withCredentials === true;
    const sourceId = nextEventSourceId;
    nextEventSourceId += 1;
    globalThis.__glassNextEventSourceId = nextEventSourceId;
    this.url = resolved.href;
    this.readyState = EventSourceNative.CONNECTING;
    this.withCredentials = withCredentials;
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.__glassSourceId = sourceId;
    this.__glassEventSourceListeners = {{}};
    eventSources.set(sourceId, this);
    pushCommand({{ kind: "eventSourceOpen", source_id: sourceId, href: resolved.href, with_credentials: withCredentials }});
  }};
  EventSourceNative.CONNECTING = 0;
  EventSourceNative.OPEN = 1;
  EventSourceNative.CLOSED = 2;
  EventSourceNative.prototype.addEventListener = function(type, listener) {{
    const name = String(type);
    if (!name || name.length > eventSourceFieldLimit || typeof listener !== "function") return;
    if (!this.__glassEventSourceListeners[name]) this.__glassEventSourceListeners[name] = [];
    if (!this.__glassEventSourceListeners[name].includes(listener)) this.__glassEventSourceListeners[name].push(listener);
  }};
  EventSourceNative.prototype.removeEventListener = function(type, listener) {{
    const name = String(type);
    if (!this.__glassEventSourceListeners[name]) return;
    this.__glassEventSourceListeners[name] = this.__glassEventSourceListeners[name].filter(candidate => candidate !== listener);
  }};
  EventSourceNative.prototype.dispatchEvent = function(event) {{
    if (!event || !event.type) throw new TypeError("native EventSource event is invalid");
    eventSourceDispatch(this, String(event.type), event);
    return true;
  }};
  EventSourceNative.prototype.close = function() {{
    if (this.readyState === EventSourceNative.CLOSED) return;
    this.readyState = EventSourceNative.CLOSED;
    eventSources.delete(this.__glassSourceId);
    pushCommand({{ kind: "eventSourceClose", source_id: this.__glassSourceId }});
  }};
  Object.defineProperties(EventSourceNative.prototype, {{
    CONNECTING: {{ value: 0 }}, OPEN: {{ value: 1 }}, CLOSED: {{ value: 2 }},
  }});
  globalThis.EventSource = EventSourceNative;
