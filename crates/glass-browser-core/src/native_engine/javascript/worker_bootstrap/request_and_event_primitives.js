  const workerFetchRequests = globalThis.__glassWorkerFetchRequests instanceof Map
    ? globalThis.__glassWorkerFetchRequests
    : new Map();
  let nextWorkerFetchRequestId = Number.isSafeInteger(globalThis.__glassNextWorkerFetchRequestId)
    ? globalThis.__glassNextWorkerFetchRequestId
    : 1;
  const workerRequestHeaderName = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const workerRequestMethod = (value) => {
    const method = String(value).toUpperCase();
    if (!method || method.length > __GLASS_WORKER_FETCH_METHOD_LIMIT__ || !workerRequestHeaderName.test(method)
        || ["CONNECT", "TRACE", "TRACK"].includes(method))
      throw new TypeError("native Worker Request method is unsupported");
    return method;
  };
  const workerRequestUrl = (value) => {
    const source = value && value.__glassUrl === true ? value.href : String(value);
    try {
      return workerUrlParts(workerUrlResolve(source, workerUrl)).href;
    } catch (_) {
      throw new TypeError("native Worker Request URL is invalid");
    }
  };
  const workerRequestReferrerState = globalThis.__glassWorkerRequestReferrerState instanceof WeakMap
    ? globalThis.__glassWorkerRequestReferrerState
    : new WeakMap();
  globalThis.__glassWorkerRequestReferrerState = workerRequestReferrerState;
  const workerReferrerPolicyValues = new Set([
    "", "no-referrer", "no-referrer-when-downgrade", "same-origin", "origin",
    "strict-origin", "origin-when-cross-origin", "strict-origin-when-cross-origin", "unsafe-url",
  ]);
  const workerRequestReferrer = (value, inherited, allowCrossOrigin = false) => {
    const client = { referrer: "about:client", url: workerUrl };
    if (value === undefined) return inherited || client;
    const source = String(value);
    if (source === "") return { referrer: "", url: "" };
    if (source === "about:client") return inherited || client;
    let href;
    try { href = workerRequestUrl(source); }
    catch (_) { throw new TypeError("native Worker Request referrer is invalid"); }
    if (!allowCrossOrigin && workerUrlParts(href).origin !== workerUrlParts(workerUrl).origin)
      return client;
    const url = new WorkerURLNative(href);
    url.username = "";
    url.password = "";
    url.hash = "";
    return { referrer: url.href, url: url.href };
  };
  const workerRequestReferrerPolicy = (value) => {
    const policy = value === undefined ? "" : String(value);
    if (!workerReferrerPolicyValues.has(policy))
      throw new TypeError("native Worker Request referrer policy is unsupported");
    return policy;
  };
  const forbiddenWorkerRequestHeader = (name) => [
    "accept-charset", "accept-encoding", "access-control-request-headers",
    "access-control-request-method", "connection", "content-length",
    "cookie", "cookie2", "date", "dnt", "expect", "host", "keep-alive",
    "origin", "referer", "te", "trailer", "transfer-encoding", "upgrade",
    "user-agent", "via",
  ].includes(name) || name.startsWith("proxy-") || name.startsWith("sec-");
  const normalizeWorkerRequestHeaders = (input) => {
    if (input === undefined || input === null) return {};
    if (typeof input !== "object") throw new TypeError("native Worker fetch headers must be an object");
    const normalized = {};
    let count = 0;
    let totalBytes = 0;
    const entries = input.__glassWorkerHeaders === true
      ? input._entries.map(entry => [entry[0], entry[1]])
      : Object.keys(input).map(name => [name, input[name]]);
    for (const [name, rawValue] of entries) {
      const normalizedName = String(name).toLowerCase();
      if (!workerRequestHeaderName.test(String(name)) || normalizedName.length > __GLASS_WORKER_FETCH_HEADER_NAME_LIMIT__)
        throw new TypeError("native Worker fetch header name is invalid");
      if (forbiddenWorkerRequestHeader(normalizedName))
        throw new TypeError("native Worker fetch header is forbidden");
      const value = String(rawValue);
      if (value.length > __GLASS_WORKER_FETCH_HEADER_VALUE_LIMIT__ || /[\u0000-\u001f\u007f]/.test(value))
        throw new TypeError("native Worker fetch header value is invalid");
      if (Object.prototype.hasOwnProperty.call(normalized, normalizedName))
        normalized[normalizedName] += ", " + value;
      else {
        count += 1;
        if (count > __GLASS_WORKER_FETCH_HEADER_COUNT_LIMIT__) throw new RangeError("native Worker fetch header limit exceeded");
        normalized[normalizedName] = value;
      }
      totalBytes += normalizedName.length + value.length;
      if (totalBytes > __GLASS_WORKER_FETCH_HEADER_BYTES_LIMIT__) throw new RangeError("native Worker fetch headers exceed their limit");
    }
    return normalized;
  };
  const workerBase64Digit = (character) => {
    const code = character.charCodeAt(0);
    if (code >= 65 && code <= 90) return code - 65;
    if (code >= 97 && code <= 122) return code - 97 + 26;
    if (code >= 48 && code <= 57) return code - 48 + 52;
    if (character === "+") return 62;
    if (character === "/") return 63;
    if (character === "=") return -2;
    return -1;
  };
  const decodeWorkerBase64 = (encoded, maxBytes) => {
    if (typeof encoded !== "string" || encoded.length % 4 !== 0)
      throw new TypeError("native Worker bytes are not valid base64");
    const bytes = [];
    for (let index = 0; index < encoded.length; index += 4) {
      const first = workerBase64Digit(encoded[index]);
      const second = workerBase64Digit(encoded[index + 1]);
      const third = workerBase64Digit(encoded[index + 2]);
      const fourth = workerBase64Digit(encoded[index + 3]);
      if (first < 0 || second < 0 || third === -1 || fourth === -1
          || (third === -2 && fourth !== -2)
          || (third === -2 && index + 4 !== encoded.length)
          || (fourth === -2 && index + 4 !== encoded.length))
        throw new TypeError("native Worker bytes are not valid base64");
      bytes.push((first << 2) | (second >> 4));
      if (third !== -2) bytes.push(((second & 15) << 4) | (third >> 2));
      if (fourth !== -2) bytes.push(((third & 3) << 6) | fourth);
    }
    if (bytes.length > maxBytes) throw new RangeError("native Worker bytes exceed their limit");
    return bytes;
  };
  const encodeWorkerBase64 = (bytes, maxBytes) => {
    if (!Array.isArray(bytes) || bytes.length > maxBytes)
      throw new RangeError("native Worker bytes exceed their limit");
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let encoded = "";
    for (let index = 0; index < bytes.length; index += 3) {
      const first = bytes[index];
      const second = index + 1 < bytes.length ? bytes[index + 1] : 0;
      const third = index + 2 < bytes.length ? bytes[index + 2] : 0;
      for (const value of [first, second, third]) {
        if (!Number.isInteger(value) || value < 0 || value > 255)
          throw new TypeError("native Worker bytes are invalid");
      }
      encoded += alphabet[first >> 2];
      encoded += alphabet[((first & 3) << 4) | (second >> 4)];
      encoded += index + 1 < bytes.length ? alphabet[((second & 15) << 2) | (third >> 6)] : "=";
      encoded += index + 2 < bytes.length ? alphabet[third & 63] : "=";
    }
    return encoded;
  };
  const workerUtf8Bytes = (text) => {
    const bytes = [];
    for (let index = 0; index < text.length; index += 1) {
      let code = text.charCodeAt(index);
      if (code >= 0xd800 && code <= 0xdbff && index + 1 < text.length) {
        const low = text.charCodeAt(index + 1);
        if (low >= 0xdc00 && low <= 0xdfff) {
          code = 0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00);
          index += 1;
        }
      } else if (code >= 0xd800 && code <= 0xdfff) code = 0xfffd;
      if (code <= 0x7f) bytes.push(code);
      else if (code <= 0x7ff) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
      else if (code <= 0xffff) bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
      else bytes.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 0x3f), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    }
    if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__) throw new RangeError("native Worker bytes exceed their limit");
    return bytes;
  };
  const workerUtf8Text = (bytes) => {
    const continuation = value => value >= 0x80 && value <= 0xbf;
    let text = "";
    for (let index = 0; index < bytes.length;) {
      const first = bytes[index];
      let codePoint = -1;
      let width = 1;
      if (first <= 0x7f) codePoint = first;
      else if (first >= 0xc2 && first <= 0xdf && continuation(bytes[index + 1])) {
        codePoint = ((first & 0x1f) << 6) | (bytes[index + 1] & 0x3f);
        width = 2;
      } else if (first === 0xe0 && bytes[index + 1] >= 0xa0 && bytes[index + 1] <= 0xbf && continuation(bytes[index + 2])) {
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      } else if (first >= 0xe1 && first <= 0xec && continuation(bytes[index + 1]) && continuation(bytes[index + 2])) {
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      } else if (first === 0xed && bytes[index + 1] >= 0x80 && bytes[index + 1] <= 0x9f && continuation(bytes[index + 2])) {
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      } else if (first >= 0xee && first <= 0xef && continuation(bytes[index + 1]) && continuation(bytes[index + 2])) {
        codePoint = ((first & 0x0f) << 12) | ((bytes[index + 1] & 0x3f) << 6) | (bytes[index + 2] & 0x3f);
        width = 3;
      } else if (first === 0xf0 && bytes[index + 1] >= 0x90 && bytes[index + 1] <= 0xbf && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      } else if (first >= 0xf1 && first <= 0xf3 && continuation(bytes[index + 1]) && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      } else if (first === 0xf4 && bytes[index + 1] >= 0x80 && bytes[index + 1] <= 0x8f && continuation(bytes[index + 2]) && continuation(bytes[index + 3])) {
        codePoint = ((first & 0x07) << 18) | ((bytes[index + 1] & 0x3f) << 12) | ((bytes[index + 2] & 0x3f) << 6) | (bytes[index + 3] & 0x3f);
        width = 4;
      }
      if (codePoint < 0) { text += "\ufffd"; index += 1; }
      else { text += String.fromCodePoint(codePoint); index += width; }
    }
    return text;
  };
  const workerByteInput = (input) => {
    if (input === undefined || input === null) return [];
    if (input instanceof ArrayBuffer) return Array.from(new Uint8Array(input));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(input))
      return Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength));
    if (Array.isArray(input)) return input.slice();
    throw new TypeError("native Worker byte input must be an ArrayBuffer or view");
  };
  const WorkerTextEncoderNative = typeof globalThis.__glassWorkerTextEncoderConstructor === "function"
    ? globalThis.__glassWorkerTextEncoderConstructor
    : function() { this.encoding = "utf-8"; };
  WorkerTextEncoderNative.prototype.encode = function(input) {
    return new Uint8Array(workerUtf8Bytes(String(input === undefined ? "" : input)));
  };
  WorkerTextEncoderNative.prototype.encodeInto = function(input, destination) {
    if (!destination || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(destination)
        || destination.BYTES_PER_ELEMENT === undefined)
      throw new TypeError("native Worker TextEncoder destination must be a typed array");
    const source = String(input === undefined ? "" : input);
    const target = new Uint8Array(destination.buffer, destination.byteOffset, destination.byteLength);
    let read = 0;
    const bytes = [];
    for (const character of Array.from(source)) {
      const encoded = workerUtf8Bytes(character);
      if (bytes.length + encoded.length > target.length) break;
      bytes.push(...encoded);
      read += character.length;
    }
    target.set(bytes);
    return { read, written: bytes.length };
  };
  globalThis.__glassWorkerTextEncoderConstructor = WorkerTextEncoderNative;
  globalThis.TextEncoder = WorkerTextEncoderNative;
  const WorkerTextDecoderNative = typeof globalThis.__glassWorkerTextDecoderConstructor === "function"
    ? globalThis.__glassWorkerTextDecoderConstructor
    : function(label, options) {
        const normalized = label === undefined ? "utf-8" : String(label).toLowerCase();
        if (!["utf-8", "utf8", "unicode-1-1-utf-8"].includes(normalized))
          throw new RangeError("native Worker TextDecoder only supports UTF-8");
        const settings = options && typeof options === "object" ? options : {};
        this.encoding = "utf-8";
        this.fatal = settings.fatal === true;
        this.ignoreBOM = settings.ignoreBOM === true;
      };
  WorkerTextDecoderNative.prototype.decode = function(input) {
    const bytes = workerByteInput(input);
    if (bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__) throw new RangeError("native Worker TextDecoder input exceeds its limit");
    const decoded = workerUtf8Text(bytes);
    if (this.fatal && decoded.includes("\ufffd")) throw new TypeError("native Worker TextDecoder encountered invalid UTF-8");
    return decoded;
  };
  globalThis.__glassWorkerTextDecoderConstructor = WorkerTextDecoderNative;
  globalThis.TextDecoder = WorkerTextDecoderNative;
  const workerBinaryString = (bytes) => {
    let value = "";
    for (const byte of bytes) value += String.fromCharCode(byte);
    return value;
  };
  globalThis.atob = (value) => workerBinaryString(decodeWorkerBase64(String(value), __GLASS_WORKER_FETCH_BODY_LIMIT__));
  globalThis.btoa = (value) => {
    const text = String(value);
    const bytes = [];
    for (let index = 0; index < text.length; index += 1) {
      const code = text.charCodeAt(index);
      if (code > 255) throw new TypeError("native Worker btoa input contains a non-Latin-1 character");
      bytes.push(code);
    }
    return encodeWorkerBase64(bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__);
  };
  globalThis.structuredClone = (value, options) => cloneMessageData(value, options);
  globalThis.queueMicrotask = (callback) => {
    if (typeof callback !== "function") throw new TypeError("native Worker queueMicrotask callback must be callable");
    Promise.resolve().then(callback);
  };
  const WorkerDOMExceptionNative = typeof globalThis.__glassWorkerDomExceptionConstructor === "function"
    ? globalThis.__glassWorkerDomExceptionConstructor
    : function(message, name) {
        this.message = String(message === undefined ? "" : message);
        this.name = String(name === undefined ? "Error" : name);
        this.code = 0;
      };
  WorkerDOMExceptionNative.prototype.toString = function() { return this.name + ": " + this.message; };
  globalThis.__glassWorkerDomExceptionConstructor = WorkerDOMExceptionNative;
  globalThis.DOMException = WorkerDOMExceptionNative;
  const WorkerEventNative = typeof globalThis.__glassWorkerEventConstructor === "function"
    ? globalThis.__glassWorkerEventConstructor
    : function(type, init) {
        const settings = init && typeof init === "object" ? init : {};
        this.type = String(type);
        this.bubbles = settings.bubbles === true;
        this.cancelable = settings.cancelable === true;
        this.composed = settings.composed === true;
        this.defaultPrevented = false;
        this.isTrusted = false;
        this.timeStamp = Number(globalThis.performance && globalThis.performance.now && globalThis.performance.now()) || 0;
        this.target = null;
        this.currentTarget = null;
        this.eventPhase = 0;
        this.cancelBubble = false;
        this._immediateStopped = false;
      };
  WorkerEventNative.prototype.preventDefault = function() {
    if (this.cancelable) this.defaultPrevented = true;
  };
  WorkerEventNative.prototype.stopPropagation = function() { this.cancelBubble = true; };
  WorkerEventNative.prototype.stopImmediatePropagation = function() {
    this.cancelBubble = true;
    this._immediateStopped = true;
  };
  WorkerEventNative.prototype.composedPath = function() { return this.target ? [this.target] : []; };
  globalThis.__glassWorkerEventConstructor = WorkerEventNative;
  globalThis.Event = WorkerEventNative;
  const hasWorkerProgressEventConstructor = typeof globalThis.__glassWorkerProgressEventConstructor === "function";
  const WorkerProgressEventNative = hasWorkerProgressEventConstructor
    ? globalThis.__glassWorkerProgressEventConstructor
    : function ProgressEvent(type, init) {
        const settings = init && typeof init === "object" ? init : {};
        WorkerEventNative.call(this, type, settings);
        this.lengthComputable = settings.lengthComputable === true;
        const loaded = Number(settings.loaded);
        const total = Number(settings.total);
        this.loaded = Number.isFinite(loaded) ? Math.max(0, Math.trunc(loaded)) : 0;
        this.total = Number.isFinite(total) ? Math.max(0, Math.trunc(total)) : 0;
      };
  if (!hasWorkerProgressEventConstructor) {
    WorkerProgressEventNative.prototype = Object.create(WorkerEventNative.prototype);
    WorkerProgressEventNative.prototype.constructor = WorkerProgressEventNative;
  }
  globalThis.__glassWorkerProgressEventConstructor = WorkerProgressEventNative;
  globalThis.ProgressEvent = WorkerProgressEventNative;
  const hasWorkerCustomEventConstructor = typeof globalThis.__glassWorkerCustomEventConstructor === "function";
  const WorkerCustomEventNative = hasWorkerCustomEventConstructor
    ? globalThis.__glassWorkerCustomEventConstructor
    : function(type, init) {
        const settings = init && typeof init === "object" ? init : {};
        WorkerEventNative.call(this, type, settings);
        this.detail = settings.detail === undefined ? null : settings.detail;
      };
  if (!hasWorkerCustomEventConstructor) {
    WorkerCustomEventNative.prototype = Object.create(WorkerEventNative.prototype);
    WorkerCustomEventNative.prototype.constructor = WorkerCustomEventNative;
  }
  globalThis.__glassWorkerCustomEventConstructor = WorkerCustomEventNative;
  globalThis.CustomEvent = WorkerCustomEventNative;
  const hasWorkerMessageEventConstructor = typeof globalThis.__glassWorkerMessageEventConstructor === "function";
  const WorkerMessageEventNative = hasWorkerMessageEventConstructor
    ? globalThis.__glassWorkerMessageEventConstructor
    : function(type, init) {
        const settings = init && typeof init === "object" ? init : {};
        WorkerEventNative.call(this, type, settings);
        this.data = settings.data === undefined ? null : settings.data;
        this.origin = String(settings.origin || "");
        this.lastEventId = String(settings.lastEventId || "");
        this.source = settings.source === undefined ? null : settings.source;
        this.ports = Array.isArray(settings.ports) ? settings.ports.slice() : [];
      };
  if (!hasWorkerMessageEventConstructor) {
    WorkerMessageEventNative.prototype = Object.create(WorkerEventNative.prototype);
    WorkerMessageEventNative.prototype.constructor = WorkerMessageEventNative;
  }
  globalThis.__glassWorkerMessageEventConstructor = WorkerMessageEventNative;
  globalThis.MessageEvent = WorkerMessageEventNative;
  const hasWorkerErrorEventConstructor = typeof globalThis.__glassWorkerErrorEventConstructor === "function";
  const WorkerErrorEventNative = hasWorkerErrorEventConstructor
    ? globalThis.__glassWorkerErrorEventConstructor
    : function(type, init) {
        const settings = init && typeof init === "object" ? init : {};
        WorkerEventNative.call(this, type, settings);
        this.message = String(settings.message || "");
        this.filename = String(settings.filename || "");
        this.lineno = Number(settings.lineno || 0);
        this.colno = Number(settings.colno || 0);
        this.error = settings.error === undefined ? null : settings.error;
      };
  if (!hasWorkerErrorEventConstructor) {
    WorkerErrorEventNative.prototype = Object.create(WorkerEventNative.prototype);
    WorkerErrorEventNative.prototype.constructor = WorkerErrorEventNative;
  }
  globalThis.__glassWorkerErrorEventConstructor = WorkerErrorEventNative;
  globalThis.ErrorEvent = WorkerErrorEventNative;
  const hasWorkerSecurityPolicyViolationEventConstructor = typeof globalThis.__glassWorkerSecurityPolicyViolationEventConstructor === "function";
  const WorkerSecurityPolicyViolationEventNative = hasWorkerSecurityPolicyViolationEventConstructor
    ? globalThis.__glassWorkerSecurityPolicyViolationEventConstructor
    : function(type, init) {
        const settings = init && typeof init === "object" ? init : {};
        WorkerEventNative.call(this, type, settings);
        this.documentURI = settings.documentURI === undefined ? "" : String(settings.documentURI);
        this.referrer = settings.referrer === undefined ? "" : String(settings.referrer);
        this.blockedURI = settings.blockedURI === undefined ? "" : String(settings.blockedURI);
        this.effectiveDirective = settings.effectiveDirective === undefined ? "" : String(settings.effectiveDirective);
        this.violatedDirective = settings.violatedDirective === undefined
          ? this.effectiveDirective
          : String(settings.violatedDirective);
        this.originalPolicy = settings.originalPolicy === undefined ? "" : String(settings.originalPolicy);
        this.sourceFile = settings.sourceFile === undefined ? "" : String(settings.sourceFile);
        this.sample = settings.sample === undefined ? "" : String(settings.sample);
        this.disposition = settings.disposition === undefined ? "enforce" : String(settings.disposition);
        this.statusCode = Number(settings.statusCode) || 0;
        this.lineNumber = Number(settings.lineNumber) || 0;
        this.columnNumber = Number(settings.columnNumber) || 0;
      };
  if (!hasWorkerSecurityPolicyViolationEventConstructor) {
    WorkerSecurityPolicyViolationEventNative.prototype = Object.create(WorkerEventNative.prototype);
    WorkerSecurityPolicyViolationEventNative.prototype.constructor = WorkerSecurityPolicyViolationEventNative;
  }
  globalThis.__glassWorkerSecurityPolicyViolationEventConstructor = WorkerSecurityPolicyViolationEventNative;
  globalThis.SecurityPolicyViolationEvent = WorkerSecurityPolicyViolationEventNative;
  const WorkerEventTargetNative = typeof globalThis.__glassWorkerEventTargetConstructor === "function"
    ? globalThis.__glassWorkerEventTargetConstructor
    : function() { this._eventListeners = new Map(); };
  WorkerEventTargetNative.prototype.addEventListener = function(type, callback) {
    if (typeof callback !== "function" && !(callback && typeof callback.handleEvent === "function")) return;
    const name = String(type);
    const callbacks = this._eventListeners.get(name) || [];
    if (!callbacks.includes(callback)) callbacks.push(callback);
    this._eventListeners.set(name, callbacks);
  };
  WorkerEventTargetNative.prototype.removeEventListener = function(type, callback) {
    const name = String(type);
    const callbacks = this._eventListeners.get(name) || [];
    this._eventListeners.set(name, callbacks.filter(candidate => candidate !== callback));
  };
  WorkerEventTargetNative.prototype.dispatchEvent = function(event) {
    if (!event || typeof event !== "object" || !event.type)
      throw new TypeError("native Worker EventTarget event is invalid");
    if (event.target === null || event.target === undefined) event.target = this;
    event.currentTarget = this;
    event.eventPhase = 2;
    const handler = this["on" + String(event.type)];
    if (typeof handler === "function") {
      try { handler.call(this, event); } catch (_) {}
    }
    const callbacks = (this._eventListeners.get(String(event.type)) || []).slice();
    for (const callback of callbacks) {
      try {
        if (typeof callback === "function") callback.call(this, event);
        else callback.handleEvent(event);
      } catch (_) {}
      if (event._immediateStopped === true) break;
    }
    event.currentTarget = null;
    event.eventPhase = 0;
    return event.defaultPrevented !== true;
  };
  globalThis.__glassWorkerEventTargetConstructor = WorkerEventTargetNative;
  globalThis.EventTarget = WorkerEventTargetNative;
  __GLASS_WORKER_MESSAGE_CHANNEL_SCRIPT__
