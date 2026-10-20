  const workerRequestBodyPayload = (input) => {
    if (input === undefined || input === null)
      return { bodyNull: true, bytes: [], stream: null, contentType: null };
    if (input.__glassWorkerReadableStream === true) {
      const state = workerReadableStreamState(input);
      if (state.locked || state.disturbed || state.consumedByResponse || state.consumedByRequest)
        throw new TypeError("native Worker body stream is unusable");
      return { bodyNull: false, bytes: null, stream: input, contentType: null };
    }
    if (input.__glassWorkerBlob === true)
      return { bodyNull: false, bytes: input._bytes.slice(), stream: null, contentType: input.type || null };
    if (input instanceof ArrayBuffer)
      return { bodyNull: false, bytes: Array.from(new Uint8Array(input)), stream: null, contentType: null };
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(input))
      return { bodyNull: false, bytes: Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength)), stream: null, contentType: null };
    const text = String(input);
    return { bodyNull: false, bytes: workerUtf8Bytes(text), stream: null, contentType: null };
  };
  const workerRequestBodyIsUsed = (request) => request.__glassWorkerRequestBodyState.used === true
    || Boolean(request.body && workerReadableStreamState(request.body).disturbed);
  const workerRequestBodyUse = (request) => {
    const body = request.body;
    if (workerRequestBodyIsUsed(request) || (body && workerReadableStreamState(body).locked)) return false;
    request.__glassWorkerRequestBodyState.used = true;
    if (body) {
      const state = workerReadableStreamState(body);
      state.consumedByRequest = true;
      workerReadableStreamDisturb(state);
      if (!request.__glassWorkerRequestBodyPayload.stream) {
        state.offset = state.bytes.length;
        workerReadableStreamSetDone(state);
      }
    }
    return true;
  };
  const workerRequestBodyUnusable = () => Promise.reject(new TypeError("native Worker Request body is unusable"));
  const workerRequestBodyPromise = (request, transform) => {
    if (!workerRequestBodyUse(request)) return workerRequestBodyUnusable();
    const payload = request.__glassWorkerRequestBodyPayload;
    if (payload.stream)
      return workerDrainReadableStream(payload.stream, true, false).then(bytes => transform(bytes, payload));
    try { return Promise.resolve(transform(payload.bytes.slice(), payload)); }
    catch (error) { return Promise.reject(error); }
  };
  const workerNavigationRequestToken = (() => {
    const existing = globalThis.__glassWorkerNavigationRequestToken;
    if (existing && typeof existing === "object") return existing;
    const token = Object.freeze({});
    Object.defineProperty(globalThis, "__glassWorkerNavigationRequestToken", {
      value: token,
      enumerable: false,
      configurable: false,
      writable: false,
    });
    return token;
  })();
  const workerNavigationRequestInstances = (() => {
    const existing = globalThis.__glassWorkerNavigationRequestInstances;
    if (existing instanceof WeakSet) return existing;
    const instances = new WeakSet();
    Object.defineProperty(globalThis, "__glassWorkerNavigationRequestInstances", {
      value: instances,
      enumerable: false,
      configurable: false,
      writable: false,
    });
    return instances;
  })();
  const WorkerRequestNative = typeof globalThis.__glassWorkerRequestConstructor === "function"
    ? globalThis.__glassWorkerRequestConstructor
    : function(input, init, internalNavigationToken) {
    if (!(this instanceof WorkerRequestNative)) throw new TypeError("native Worker Request requires new");
    const source = input && input.__glassWorkerRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    const overrides = init && typeof init === "object" ? init : {};
    const requestInitIsNonEmpty = Object.keys(overrides).length > 0;
    const hasBodyOverride = Object.prototype.hasOwnProperty.call(overrides, "body");
    const inheritedNavigationRequest = source
      && workerNavigationRequestInstances.has(source)
      && !requestInitIsNonEmpty;
    if (source && !hasBodyOverride && (source.bodyUsed || source.body && workerReadableStreamState(source.body).locked))
      throw new TypeError("native Worker Request body is unusable");
    if (!source && !sourceUrl && typeof input !== "string")
      throw new TypeError("native Worker Request URL must be a string");
    const href = workerRequestUrl(source ? source.url : sourceUrl || input);
    const settings = Object.assign({}, source ? source._settings : {}, overrides);
    if (source && workerNavigationRequestInstances.has(source)
        && requestInitIsNonEmpty && !Object.prototype.hasOwnProperty.call(overrides, "mode")
        && String(settings.mode).toLowerCase() === "navigate")
      settings.mode = "same-origin";
    const inheritedPayload = source && !hasBodyOverride
      ? source.__glassWorkerRequestBodyPayload
      : null;
    let payload;
    if (inheritedPayload && inheritedPayload.stream) {
      const [sourceBody, cloneBody] = source.body.tee();
      inheritedPayload.stream = sourceBody;
      source.__glassWorkerRequestBodyState.stream = sourceBody;
      payload = {
        bodyNull: false,
        bytes: null,
        stream: cloneBody,
        contentType: inheritedPayload.contentType,
      };
    } else if (inheritedPayload) {
      payload = {
        bodyNull: inheritedPayload.bodyNull,
        bytes: inheritedPayload.bytes.slice(),
        stream: null,
        contentType: inheritedPayload.contentType,
      };
    } else {
      payload = workerRequestBodyPayload(settings.body);
    }
    const method = workerRequestMethod(settings.method === undefined ? "GET" : settings.method);
    if (["GET", "HEAD"].includes(method) && !payload.bodyNull)
      throw new TypeError("native Worker " + method + " Requests must not have a body");
    const mode = settings.mode === undefined ? "cors" : String(settings.mode).toLowerCase();
    const internalNavigationRequest = mode === "navigate"
      && (internalNavigationToken === workerNavigationRequestToken || inheritedNavigationRequest);
    if (!["cors", "no-cors", "same-origin"].includes(mode) && !internalNavigationRequest)
      throw new TypeError("native Worker Request mode is unsupported");
    const redirect = settings.redirect === undefined ? "follow" : String(settings.redirect).toLowerCase();
    if (!["follow", "error", "manual"].includes(redirect))
      throw new TypeError("native Worker Request redirect mode is unsupported");
    const cache = settings.cache === undefined ? "default" : String(settings.cache).toLowerCase();
    if (!["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"].includes(cache))
      throw new TypeError("native Worker Request cache mode is unsupported");
    if (cache === "only-if-cached" && mode !== "same-origin")
      throw new TypeError("native Worker only-if-cached Requests require same-origin mode");
    const credentials = settings.credentials === undefined ? "same-origin" : String(settings.credentials);
    if (!["omit", "same-origin", "include"].includes(credentials))
      throw new TypeError("native Worker Request credentials are unsupported");
    const referrer = workerRequestReferrer(
      Object.prototype.hasOwnProperty.call(overrides, "referrer") ? overrides.referrer : undefined,
      source && !requestInitIsNonEmpty ? workerRequestReferrerState.get(source) : null,
      internalNavigationRequest,
    );
    const referrerPolicy = workerRequestReferrerPolicy(
      source && requestInitIsNonEmpty
        && !Object.prototype.hasOwnProperty.call(overrides, "referrerPolicy")
        ? ""
        : settings.referrerPolicy,
    );
    const destination = settings.destination === undefined ? "" : String(settings.destination);
    if (!["", "font", "fetch", "document"].includes(destination))
      throw new TypeError("native Worker Request destination is unsupported");
    const headers = new WorkerHeadersNative(settings.headers);
    if (payload.contentType && !headers.has("content-type")) headers.set("content-type", payload.contentType);
    settings.method = method;
    settings.mode = mode;
    settings.redirect = redirect;
    settings.cache = cache;
    settings.credentials = credentials;
    settings.referrer = referrer.referrer;
    settings.referrerPolicy = referrerPolicy;
    settings.destination = destination;
    settings.headers = headers;
    const signal = settings.signal === undefined ? new AbortSignalNative() : settings.signal;
    if (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean"
        || typeof signal.addEventListener !== "function"
        || typeof signal.removeEventListener !== "function")
      throw new TypeError("native Worker Request signal is invalid");
    settings.signal = signal;
    Object.defineProperty(this, "__glassWorkerRequest", { value: true });
    Object.defineProperty(this, "_settings", { value: settings });
    Object.defineProperty(this, "__glassWorkerRequestBodyPayload", { value: payload });
    Object.defineProperty(this, "__glassWorkerRequestBodyState", { value: { used: false, stream: null } });
    this.method = method;
    this.url = href;
    this.headers = headers;
    this.mode = mode;
    this.redirect = redirect;
    this.cache = cache;
    this.credentials = credentials;
    this.referrer = referrer.referrer;
    this.referrerPolicy = referrerPolicy;
    this.destination = destination;
    this.signal = signal;
    const bodyState = this.__glassWorkerRequestBodyState;
    bodyState.stream = payload.bodyNull
      ? null
      : payload.stream || new WorkerReadableStreamNative(payload.bytes, () => { bodyState.used = true; });
    Object.defineProperty(this, "body", {
      configurable: true,
      enumerable: true,
      get() { return bodyState.stream; },
    });
    Object.defineProperty(this, "bodyUsed", {
      configurable: true,
      enumerable: true,
      get() { return workerRequestBodyIsUsed(this); },
    });
    workerRequestReferrerState.set(this, referrer);
    if (internalNavigationRequest) workerNavigationRequestInstances.add(this);
    Object.freeze(this);
  };
  WorkerRequestNative.prototype.clone = function() {
    if (this.bodyUsed || this.body && workerReadableStreamState(this.body).locked)
      throw new TypeError("native Worker Request body is unusable");
    return new WorkerRequestNative(this);
  };
  WorkerRequestNative.prototype.text = function() {
    return workerRequestBodyPromise(this, bytes => workerUtf8Text(bytes));
  };
  WorkerRequestNative.prototype.json = function() {
    return workerRequestBodyPromise(this, bytes => JSON.parse(workerUtf8Text(bytes)));
  };
  WorkerRequestNative.prototype.arrayBuffer = function() {
    return workerRequestBodyPromise(this, bytes => new Uint8Array(bytes).buffer);
  };
  WorkerRequestNative.prototype.bytes = function() {
    return workerRequestBodyPromise(this, bytes => new Uint8Array(bytes));
  };
  WorkerRequestNative.prototype.blob = function() {
    return workerRequestBodyPromise(this, (bytes, payload) => {
      const blob = new WorkerBlob([], { type: payload.contentType || this.headers.get("content-type") || "" });
      blob._bytes = bytes;
      blob.size = bytes.length;
      return blob;
    });
  };
  globalThis.__glassWorkerRequestConstructor = WorkerRequestNative;
  globalThis.Request = WorkerRequestNative;
  const workerResponseBodyPayload = (body) => {
    if (body === undefined || body === null)
      return { bodyNull: true, body: "", bytes: [], stream: null, contentType: null };
    if (body.__glassWorkerReadableStream === true) {
      const state = workerReadableStreamState(body);
      if (state.locked || state.disturbed || state.consumedByResponse || state.consumedByRequest)
        throw new TypeError("native Worker Response body stream is unusable");
      return { bodyNull: false, body: "", bytes: null, stream: body, contentType: null };
    }
    const payload = workerRequestBodyPayload(body);
    return {
      bodyNull: payload.bodyNull,
      body: workerUtf8Text(payload.bytes),
      bytes: payload.bytes,
      stream: null,
      contentType: payload.contentType,
    };
  };
  const WorkerResponseNative = typeof globalThis.__glassWorkerResponseConstructor === "function"
    ? globalThis.__glassWorkerResponseConstructor
    : function(body, options) {
    if (!(this instanceof WorkerResponseNative)) throw new TypeError("native Worker Response requires new");
    const settings = options && typeof options === "object" ? options : {};
    const bodyPayload = workerResponseBodyPayload(body);
    const status = settings.status === undefined ? 200 : Number(settings.status);
    if (!Number.isInteger(status) || status < 200 || status > 599)
      throw new RangeError("native Worker Response status is outside the bounded range");
    const headers = new WorkerHeadersNative(settings.headers);
    const contentType = headers.get("content-type") || bodyPayload.contentType;
    return responseFromWorkerFetch({
      url: "",
      status,
      statusText: settings.statusText === undefined ? "" : String(settings.statusText),
      headers: headers._entries,
      contentType,
      body: bodyPayload.body,
      bodyBase64: bodyPayload.bytes === null
        ? null
        : encodeWorkerBase64(bodyPayload.bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      bodyStream: bodyPayload.stream || null,
      bodyNull: bodyPayload.bodyNull,
      redirected: false,
      opaque: false,
      opaqueRedirect: false,
    });
  };
  WorkerResponseNative.prototype.constructor = WorkerResponseNative;
  const responseFromWorkerFetch = (payload) => {
    const bodyStream = payload && payload.bodyStream
      && payload.bodyStream.__glassWorkerReadableStream === true
      ? payload.bodyStream
      : null;
    const hostStreamId = payload && Number.isSafeInteger(Number(payload.bodyStreamId))
      ? Number(payload.bodyStreamId)
      : null;
    const bytes = bodyStream ? null : workerResponseBytes(payload);
    let bodyUsed = false;
    const body = payload && payload.bodyNull === true
      ? null
      : bodyStream
        ? bodyStream
        : hostStreamId === null
          ? new WorkerReadableStreamNative(bytes, () => { bodyUsed = true; })
          : new WorkerReadableStreamNative({ __glassWorkerFetchStreamId: hostStreamId }, null, () => { bodyUsed = true; });
    const responseBodyState = bodyStream ? { stream: bodyStream } : null;
    const activeBody = () => responseBodyState ? responseBodyState.stream : body;
    const activeBodyState = () => {
      const stream = activeBody();
      return stream ? workerReadableStreamState(stream) : null;
    };
    const responseIsUsed = () => bodyUsed || Boolean(activeBodyState() && activeBodyState().disturbed);
    const consume = transform => {
      const stream = activeBody();
      const state = activeBodyState();
      if (responseIsUsed() || state && state.locked)
        return Promise.reject(new TypeError("native Worker Response body is unusable"));
      if (bodyStream || hostStreamId !== null) {
        bodyUsed = true;
        return workerDrainReadableStream(stream, true, true).then(transform);
      }
      if (state && !workerReadableStreamConsume(state))
        return Promise.reject(new TypeError("native Worker Response body is unusable"));
      bodyUsed = true;
      return Promise.resolve().then(() => transform(bytes.slice()));
    };
    const response = Object.create(WorkerResponseNative.prototype);
    Object.assign(response, {
      type: payload && typeof payload.responseType === "string"
        ? payload.responseType
        : payload && payload.opaqueRedirect === true ? "opaqueredirect" : payload && payload.opaque === true ? "opaque" : "basic",
      ok: Boolean(payload) && Number(payload.status) >= 200 && Number(payload.status) < 300,
      status: payload && Number.isFinite(Number(payload.status)) ? Number(payload.status) : 0,
      statusText: payload && payload.statusText !== undefined ? String(payload.statusText) : String(payload && payload.status || ""),
      url: payload && typeof payload.url === "string" ? payload.url : "",
      redirected: payload && payload.redirected === true,
      headers: workerResponseHeaders(payload && payload.headers, payload && payload.contentType),
      __glassPayload: payload,
      body,
      get bodyUsed() { return responseIsUsed(); },
      text() { return consume(value => workerUtf8Text(value)); },
      json() { return consume(value => JSON.parse(workerUtf8Text(value))); },
      arrayBuffer() { return consume(value => new Uint8Array(value).buffer); },
      bytes() { return consume(value => new Uint8Array(value)); },
      blob() {
        return consume(value => {
          const blob = new WorkerBlob([], { type: payload && payload.contentType || "" });
          blob._bytes = value;
          blob.size = value.length;
          return blob;
        });
      },
      clone() {
        const stream = activeBody();
        const state = activeBodyState();
        if (responseIsUsed() || state && state.locked)
          throw new TypeError("native Worker Response body is unusable");
        if (bodyStream) {
          const [sourceBody, cloneBody] = stream.tee();
          payload.bodyStream = sourceBody;
          responseBodyState.stream = sourceBody;
          return responseFromWorkerFetch(Object.assign({}, payload, {
            body: "",
            bodyBase64: null,
            bodyStream: cloneBody,
            bodyNull: false,
          }));
        }
        return responseFromWorkerFetch(payload);
      },
    });
    if (responseBodyState) Object.defineProperty(response, "body", {
      configurable: true,
      enumerable: true,
      get() { return responseBodyState.stream; },
    });
    Object.defineProperty(response, "bodyUsed", {
      configurable: true,
      enumerable: true,
      get() { return responseIsUsed(); },
    });
    return Object.freeze(response);
  };
  WorkerResponseNative.json = function(data, options) {
    const settings = options && typeof options === "object" ? Object.assign({}, options) : {};
    const headers = new WorkerHeadersNative(settings.headers);
    if (!headers.has("content-type")) headers.set("content-type", "application/json");
    settings.headers = headers;
    return new WorkerResponseNative(JSON.stringify(data), settings);
  };
  WorkerResponseNative.error = function() {
    const response = Object.create(WorkerResponseNative.prototype);
    Object.assign(response, {
      type: "error",
      ok: false,
      status: 0,
      statusText: "",
      url: "",
      headers: new WorkerHeadersNative(),
      body: null,
      bodyUsed: false,
      clone() { return WorkerResponseNative.error(); },
      text() { return Promise.reject(new TypeError("native Worker error Response has no body")); },
      json() { return Promise.reject(new TypeError("native Worker error Response has no body")); },
      arrayBuffer() { return Promise.reject(new TypeError("native Worker error Response has no body")); },
      bytes() { return Promise.reject(new TypeError("native Worker error Response has no body")); },
      blob() { return Promise.reject(new TypeError("native Worker error Response has no body")); },
    });
    return Object.freeze(response);
  };
  WorkerResponseNative.redirect = function(url, status) {
    const code = status === undefined ? 302 : Number(status);
    if (![301, 302, 303, 307, 308].includes(code))
      throw new RangeError("native Worker Response redirect status is unsupported");
    return responseFromWorkerFetch({
      url: "",
      status: code,
      statusText: "",
      headers: [["location", String(url)]],
      contentType: null,
      body: "",
      bodyBase64: encodeWorkerBase64([], __GLASS_WORKER_FETCH_BODY_LIMIT__),
      bodyNull: true,
      redirected: false,
      opaque: false,
      opaqueRedirect: false,
    });
  };
  globalThis.__glassWorkerResponseConstructor = WorkerResponseNative;
  globalThis.Response = WorkerResponseNative;
  const workerFetchNative = (input, options) => {
    const sourceRequest = input && input.__glassWorkerRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    if (typeof input !== "string" && !sourceRequest && !sourceUrl)
      return Promise.reject(new TypeError("native Worker fetch requires a URL string or Request"));
    let href;
    try {
      href = workerRequestUrl(sourceRequest ? sourceRequest.url : sourceUrl || input);
    } catch (error) {
      return Promise.reject(error);
    }
    const optionsObject = options && typeof options === "object" ? options : {};
    const hasBodyOverride = Object.prototype.hasOwnProperty.call(optionsObject, "body");
    const requestInitIsNonEmpty = Object.keys(optionsObject).length > 0;
    const settings = Object.assign({}, sourceRequest ? sourceRequest._settings : {}, optionsObject);
    const signal = settings.signal === undefined ? null : settings.signal;
    if (signal !== null && (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean"
        || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
      return Promise.reject(new TypeError("native Worker fetch signal is invalid"));
    if (signal && signal.aborted)
      return Promise.reject(signal.reason === undefined ? nativeWorkerAbortError() : signal.reason);
    let timeoutMs = null;
    if (settings.__glassTimeoutMs !== undefined) {
      const numericTimeout = Number(settings.__glassTimeoutMs);
      if (!Number.isFinite(numericTimeout) || numericTimeout < 0 || numericTimeout > __GLASS_WORKER_MAX_NATIVE_XHR_TIMEOUT_MS__)
        return Promise.reject(new RangeError("native Worker XMLHttpRequest timeout is outside the bounded range"));
      timeoutMs = numericTimeout === 0 ? null : Math.trunc(numericTimeout);
    }
    let payload;
    try {
      if (sourceRequest && !hasBodyOverride) {
        if (!workerRequestBodyUse(sourceRequest)) return workerRequestBodyUnusable();
        payload = sourceRequest.__glassWorkerRequestBodyPayload;
      } else payload = workerRequestBodyPayload(settings.body);
    } catch (error) { return Promise.reject(error); }
    let method;
    try { method = workerRequestMethod(settings.method === undefined ? "GET" : settings.method); }
    catch (error) { return Promise.reject(error); }
    let mode = settings.mode === undefined ? "cors" : String(settings.mode).toLowerCase();
    const inheritedNavigationRequest = sourceRequest
      && workerNavigationRequestInstances.has(sourceRequest)
      && String(sourceRequest._settings.mode).toLowerCase() === "navigate";
    if (inheritedNavigationRequest && requestInitIsNonEmpty
        && !Object.prototype.hasOwnProperty.call(optionsObject, "mode")
        && mode === "navigate")
      mode = "same-origin";
    const forwardingNavigationRequest = inheritedNavigationRequest
      && !requestInitIsNonEmpty && mode === "navigate";
    if (!["cors", "no-cors", "same-origin"].includes(mode) && !forwardingNavigationRequest)
      return Promise.reject(new TypeError("native Worker fetch mode is unsupported"));
    const redirect = settings.redirect === undefined ? "follow" : String(settings.redirect).toLowerCase();
    if (!["follow", "error", "manual"].includes(redirect))
      return Promise.reject(new TypeError("native Worker fetch redirect mode is unsupported"));
    const cache = settings.cache === undefined ? "default" : String(settings.cache).toLowerCase();
    if (!["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"].includes(cache))
      return Promise.reject(new TypeError("native Worker fetch cache mode is unsupported"));
    if (cache === "only-if-cached" && mode !== "same-origin")
      return Promise.reject(new TypeError("native Worker only-if-cached fetches require same-origin mode"));
    const credentialsMode = settings.credentials === undefined ? "same-origin" : String(settings.credentials);
    if (!["omit", "same-origin", "include"].includes(credentialsMode))
      return Promise.reject(new TypeError("native Worker fetch credentials are unsupported"));
    let referrer;
    let referrerPolicy;
    try {
      const hasReferrerOverride = Object.prototype.hasOwnProperty.call(optionsObject, "referrer")
        && optionsObject.referrer !== undefined;
      referrer = workerRequestReferrer(
        hasReferrerOverride ? optionsObject.referrer : undefined,
        sourceRequest && !requestInitIsNonEmpty
          ? workerRequestReferrerState.get(sourceRequest)
          : null,
      );
      referrerPolicy = workerRequestReferrerPolicy(
        sourceRequest && requestInitIsNonEmpty
          && !Object.prototype.hasOwnProperty.call(optionsObject, "referrerPolicy")
          ? ""
          : settings.referrerPolicy,
      );
    } catch (error) { return Promise.reject(error); }
    let requestHeaders;
    try { requestHeaders = normalizeWorkerRequestHeaders(settings.headers); }
    catch (error) { return Promise.reject(error); }
    let contentType = null;
    if (Object.prototype.hasOwnProperty.call(requestHeaders, "content-type")) {
      contentType = requestHeaders["content-type"];
      delete requestHeaders["content-type"];
    }
    if (contentType === null && payload.contentType) contentType = payload.contentType;
    const objectUrlEntry = workerObjectUrlRegistry.get(href);
    if (objectUrlEntry) {
      if (!payload.bodyNull || !["GET", "HEAD"].includes(method))
        return Promise.reject(new TypeError("native Worker object URL fetches support only GET and HEAD"));
      const objectBytes = workerBlobBytes(objectUrlEntry.blob);
      const responseBytes = method === "HEAD" ? [] : objectBytes;
      const objectContentType = objectUrlEntry.blob.type || null;
      const objectHeaders = [["content-length", String(objectBytes.length)]];
      if (objectContentType) objectHeaders.unshift(["content-type", objectContentType]);
      return Promise.resolve(responseFromWorkerFetch({
        url: href,
        status: 200,
        statusText: "OK",
        headers: objectHeaders,
        contentType: objectContentType,
        body: workerUtf8Text(responseBytes),
        bodyBase64: encodeWorkerBase64(responseBytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
        bodyNull: method === "HEAD",
        redirected: false,
        opaque: false,
        opaqueRedirect: false,
      }));
    }
    const dispatchRequest = (bytes, streamedBody) => {
      if (signal && signal.aborted)
        return Promise.reject(signal.reason === undefined ? nativeWorkerAbortError() : signal.reason);
      const isStreamedBody = streamedBody !== null;
      const bodyBytes = payload.bodyNull || isStreamedBody ? [] : bytes;
      if (["GET", "HEAD"].includes(method) && !payload.bodyNull)
        return Promise.reject(new TypeError(method + " Worker fetch requests must not have a body"));
      if (!isStreamedBody && bodyBytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        return Promise.reject(new RangeError("native Worker fetch body exceeds its limit"));
      const body = payload.bodyNull || isStreamedBody ? null : workerUtf8Text(bodyBytes);
      const bodyBase64 = payload.bodyNull || isStreamedBody
        ? null
        : encodeWorkerBase64(bodyBytes, __GLASS_WORKER_FETCH_BODY_LIMIT__);
      const requestId = nextWorkerFetchRequestId;
      nextWorkerFetchRequestId += 1;
      globalThis.__glassNextWorkerFetchRequestId = nextWorkerFetchRequestId;
      if (isStreamedBody) {
        try { startWorkerFetchUpload(streamedBody, requestId); }
        catch (error) { return Promise.reject(error); }
      }
      return new Promise((resolve, reject) => {
        const pending = { resolve, reject, signal, abortListener: null, uploadStreamId: isStreamedBody ? requestId : null };
        const abort = () => {
          if (workerFetchRequests.get(requestId) !== pending) return;
          workerFetchRequests.delete(requestId);
          if (pending.signal && pending.abortListener)
            pending.signal.removeEventListener("abort", pending.abortListener);
          if (pending.uploadStreamId !== null)
            cancelWorkerFetchUploadGroup(workerFetchUploadGroups.get(pending.uploadStreamId), "native Worker fetch was aborted");
          reject(pending.signal.reason === undefined ? nativeWorkerAbortError() : pending.signal.reason);
        };
        pending.abortListener = abort;
        workerFetchRequests.set(requestId, pending);
        if (signal) signal.addEventListener("abort", abort);
        if (!workerFetchRequests.has(requestId)) return;
        pushCommand({
          kind: "fetch",
          request_id: requestId,
          worker_id: workerId,
          href,
          credentials: credentialsMode !== "omit",
          credentials_mode: credentialsMode,
          referrer: referrer.referrer,
          referrer_url: referrer.url,
          referrer_policy: referrerPolicy || workerGlobalReferrerPolicy,
          method,
          headers: requestHeaders,
          body,
          body_base64: bodyBase64,
          content_type: contentType,
          mode,
          redirect,
          cache,
          timeout_ms: timeoutMs,
          upload_stream_id: isStreamedBody ? requestId : null,
        });
      });
    };
    if (payload.stream) {
      if (["GET", "HEAD"].includes(method))
        return Promise.reject(new TypeError(method + " Worker fetch requests must not have a body"));
      return dispatchRequest(null, payload.stream);
    }
    return dispatchRequest(payload.bodyNull ? [] : payload.bytes, null);
  };
  const workerXhrDispatch = (xhr, type, extra) => {
    const event = extra && Object.prototype.hasOwnProperty.call(extra, "loaded")
      && typeof globalThis.ProgressEvent === "function"
      ? new globalThis.ProgressEvent(type, extra)
      : Object.assign({ type, target: xhr, currentTarget: xhr }, extra || {});
    return xhr.dispatchEvent(event);
  };
  const workerXhrEventTypes = ["loadstart", "progress", "load", "error", "timeout", "abort", "loadend"];
  const WorkerXMLHttpRequestUpload = typeof globalThis.__glassWorkerXmlHttpRequestUploadConstructor === "function"
    ? globalThis.__glassWorkerXmlHttpRequestUploadConstructor
    : function() {
      WorkerEventTargetNative.call(this);
      for (const type of workerXhrEventTypes) this["on" + type] = null;
    };
  globalThis.__glassWorkerXmlHttpRequestUploadConstructor = WorkerXMLHttpRequestUpload;
  globalThis.XMLHttpRequestUpload = WorkerXMLHttpRequestUpload;
  const workerXhrBodyLength = (body) => {
    const payload = workerRequestBodyPayload(body);
    return payload.bodyNull ? 0 : payload.stream ? null : payload.bytes.length;
  };
  const workerXhrStartUpload = (xhr, body) => {
    const total = workerXhrBodyLength(body);
    xhr._uploadStarted = true;
    xhr._uploadFinished = false;
    xhr._uploadTotal = total;
    xhr._uploadLoaded = 0;
    const lengthComputable = total !== null;
    workerXhrDispatch(xhr, "loadstart", {
      lengthComputable: false,
      loaded: 0,
      total: 0,
    });
    workerXhrDispatch(xhr.upload, "loadstart", {
      lengthComputable,
      loaded: 0,
      total: total === null ? 0 : total,
    });
    if (total !== null) {
      xhr._uploadLoaded = total;
      workerXhrDispatch(xhr.upload, "progress", {
        lengthComputable: true,
        loaded: total,
        total,
      });
    }
  };
  const workerXhrFinishUpload = (xhr, type) => {
    if (!xhr._uploadStarted || xhr._uploadFinished) return;
    xhr._uploadFinished = true;
    const lengthComputable = xhr._uploadTotal !== null;
    const loaded = type === "load" && lengthComputable ? xhr._uploadTotal : xhr._uploadLoaded;
    const extra = {
      lengthComputable,
      loaded,
      total: lengthComputable ? xhr._uploadTotal : 0,
    };
    workerXhrDispatch(xhr.upload, type, extra);
    workerXhrDispatch(xhr.upload, "loadend", extra);
  };
  const workerXhrResponseProgress = (xhr, value, loadedOverride) => {
    const header = xhr._responseHeaders.get("content-length");
    const total = header === null || header === undefined ? -1 : Number(header);
    const lengthComputable = Number.isSafeInteger(total) && total >= 0 && total <= __GLASS_WORKER_FETCH_BODY_LIMIT__;
    const loaded = Number.isSafeInteger(loadedOverride) && loadedOverride >= 0
      ? loadedOverride
      : typeof value === "string"
      ? workerUtf8Bytes(value).length
      : value instanceof ArrayBuffer
        ? value.byteLength
        : value && typeof value.size === "number"
          ? value.size
          : lengthComputable ? total : 0;
    workerXhrDispatch(xhr, "progress", {
      lengthComputable,
      loaded,
      total: lengthComputable ? total : 0,
    });
  };
  const workerXhrUtf8SafePrefix = (bytes) => {
    let start = bytes.length - 1;
    while (start >= 0 && bytes[start] >= 0x80 && bytes[start] <= 0xbf) start -= 1;
    if (start < 0) return bytes.length;
    const first = bytes[start];
    let width = 1;
    if (first >= 0xc2 && first <= 0xdf) width = 2;
    else if (first >= 0xe0 && first <= 0xef) width = 3;
    else if (first >= 0xf0 && first <= 0xf4) width = 4;
    return width > bytes.length - start ? start : bytes.length;
  };
  const workerXhrAppendResponseText = (xhr, responseType, bytes, final) => {
    if (!["", "text"].includes(responseType)) return;
    const pending = xhr._responseUtf8Pending.concat(bytes);
    const safeLength = final ? pending.length : workerXhrUtf8SafePrefix(pending);
    if (safeLength > 0) xhr._responseText += workerUtf8Text(pending.slice(0, safeLength));
    xhr._responseUtf8Pending = pending.slice(safeLength);
  };
  const workerXhrReadResponse = (xhr, response, responseType, controller) => {
    const stream = response && response.body;
    if (!stream || typeof stream.getReader !== "function") {
      if (xhr._controller !== controller || xhr._aborted) return Promise.resolve(null);
      const bytes = response && response.__glassPayload
        ? workerResponseBytes(response.__glassPayload)
        : [];
      workerXhrAppendResponseText(xhr, responseType, bytes, true);
      xhr._readyState = 3;
      xhr._notifyReadyState();
      if (xhr._controller !== controller || xhr._aborted) return Promise.resolve(null);
      workerXhrResponseProgress(xhr, null, bytes.length);
      return Promise.resolve(bytes);
    }
    let reader;
    try { reader = stream.getReader(); }
    catch (error) { return Promise.reject(error); }
    xhr._responseReader = reader;
    const bytes = [];
    const release = () => {
      if (xhr._responseReader === reader) xhr._responseReader = null;
      reader.releaseLock();
    };
    const readNext = () => {
      if (xhr._controller !== controller || xhr._aborted) {
        release();
        return Promise.resolve(null);
      }
      return reader.read().then(
        result => {
          if (result.done) {
            workerXhrAppendResponseText(xhr, responseType, [], true);
            if (bytes.length === 0) {
              xhr._readyState = 3;
              xhr._notifyReadyState();
              workerXhrResponseProgress(xhr, null, 0);
            }
            release();
            return bytes;
          }
          let chunk;
          try { chunk = workerReadableStreamChunkBytes(result.value); }
          catch (error) { release(); throw error; }
          if (chunk.length === 0) return readNext();
          bytes.push(...chunk);
          workerXhrAppendResponseText(xhr, responseType, chunk, false);
          xhr._readyState = 3;
          xhr._notifyReadyState();
          if (xhr._controller !== controller || xhr._aborted) {
            release();
            return null;
          }
          workerXhrResponseProgress(xhr, null, bytes.length);
          return readNext();
        },
        error => { release(); throw error; },
      );
    };
    return readNext();
  };
  const workerSyncXhrRequestPayload = (xhr, body) => {
    const payload = workerRequestBodyPayload(body);
    if (payload.stream)
      throw new TypeError("native synchronous Worker XMLHttpRequest cannot use a ReadableStream body");
    const headers = {};
    let contentType = payload.contentType;
    for (const entry of xhr._headers._entries) {
      const name = String(entry[0]).toLowerCase();
      if (name === "content-type") {
        if (contentType !== null && contentType !== undefined)
          throw new TypeError("native Worker XMLHttpRequest body chooses its own Content-Type");
        contentType = String(entry[1]);
      } else headers[name] = String(entry[1]);
    }
    const bytes = payload.bodyNull ? [] : payload.bytes;
    if (["GET", "HEAD"].includes(xhr._method) && !payload.bodyNull)
      throw new TypeError("native Worker " + xhr._method + " XMLHttpRequest requests must not have a body");
    return {
      documentUrl: workerUrl,
      href: xhr._url,
      method: xhr._method,
      headers,
      bodyBase64: payload.bodyNull ? null : encodeWorkerBase64(bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
      contentType: contentType === undefined ? null : contentType,
      credentials: xhr.withCredentials,
      timeoutMs: xhr._timeout === 0 ? null : xhr._timeout,
      referrerPolicy: workerGlobalReferrerPolicy,
    };
  };
  const workerXhrNormalizeOverrideMimeType = (value) => {
    const text = String(value).trim();
    const essence = text.split(";", 1)[0].trim();
    if (text.length === 0 || text.length > __GLASS_WORKER_FETCH_BODY_LIMIT__
        || /[\u0000-\u001f\u007f]/.test(text)
        || !/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+\/[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(essence))
      return "application/octet-stream";
    return text;
  };
  const workerXhrOpenMethod = (method) => {
    const normalized = String(method).toUpperCase();
    if (!workerRequestHeaderName.test(normalized))
      throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest method is invalid", "SyntaxError");
    if (["CONNECT", "TRACE", "TRACK"].includes(normalized))
      throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest method is forbidden", "SecurityError");
    if (normalized.length > __GLASS_WORKER_FETCH_METHOD_LIMIT__)
      throw new TypeError("native Worker XMLHttpRequest method is unsupported");
    return normalized;
  };
  const workerXhrOpenUrl = (value, username, password) => {
    const source = value && value.__glassUrl === true ? value.href : String(value);
    let parts;
    try {
      parts = workerUrlParts(workerUrlResolve(source, workerUrl));
    } catch (_) {
      throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest URL is invalid", "SyntaxError");
    }
    if (parts.authority && (username !== undefined && username !== null
        || password !== undefined && password !== null)) {
      const nextUsername = username === undefined || username === null
        ? parts.username
        : encodeURIComponent(String(username));
      const nextPassword = password === undefined || password === null
        ? parts.password
        : encodeURIComponent(String(password));
      const credentials = nextUsername || nextPassword
        ? nextUsername + (nextPassword ? ":" + nextPassword : "") + "@"
        : "";
      parts.href = parts.protocol + "//" + credentials + parts.host
        + parts.pathname + parts.search + parts.hash;
    }
    return parts.href;
  };
  const workerXhrApplySyncResponse = (xhr, payload, responseType) => {
    const fail = (error, eventType) => {
      xhr._controller = null;
      xhr._responseReader = null;
      xhr._status = 0;
      xhr._statusText = "";
      xhr._responseText = "";
      xhr._responseUtf8Pending = [];
      xhr._responseURL = "";
      xhr._response = "";
      xhr._responseXML = null;
      xhr._responseContentType = null;
      xhr._responseHeaders = new WorkerHeadersNative();
      xhr._sent = false;
      xhr._readyState = 4;
      xhr._notifyReadyState();
      workerXhrFinishUpload(xhr, eventType);
      workerXhrDispatch(xhr, eventType, { error });
      workerXhrDispatch(xhr, "loadend", {});
    };
    if (!payload || payload.error === true) {
      const error = new Error(String(payload && payload.errorMessage || "native synchronous Worker XMLHttpRequest failed"));
      error.name = payload && payload.timeout === true ? "TimeoutError" : "NetworkError";
      fail(error, payload && payload.timeout === true ? "timeout" : "error");
      return;
    }
    let bytes;
    try {
      bytes = workerResponseBytes(payload);
      const contentType = payload.contentType === null || payload.contentType === undefined
        ? null
        : String(payload.contentType);
      const responseContentType = xhr._overrideMimeType || contentType;
      xhr._status = Number(payload.status) || 0;
      xhr._statusText = payload.statusText === undefined ? String(xhr._status) : String(payload.statusText);
      xhr._responseURL = payload.url === undefined ? "" : String(payload.url);
      xhr._responseContentType = responseContentType;
      xhr._responseHeaders = workerResponseHeaders(payload.headers, contentType);
      xhr._responseText = responseType === "" || responseType === "text"
        ? workerUtf8Text(bytes)
        : "";
      xhr._response = responseType === "json"
        ? JSON.parse(workerUtf8Text(bytes))
        : responseType === "arraybuffer"
          ? new Uint8Array(bytes).buffer
          : responseType === "blob"
            ? new WorkerBlob([new Uint8Array(bytes)], { type: responseContentType || "" })
            : xhr._responseText;
    } catch (error) {
      fail(error, "error");
      return;
    }
    xhr._controller = null;
    xhr._responseReader = null;
    xhr._sent = false;
    xhr._readyState = 4;
    xhr._notifyReadyState();
    workerXhrFinishUpload(xhr, "load");
    workerXhrDispatch(xhr, "load", {});
    workerXhrDispatch(xhr, "loadend", {});
  };
  const WorkerXMLHttpRequestNative = typeof globalThis.__glassWorkerXmlHttpRequestConstructor === "function"
    ? globalThis.__glassWorkerXmlHttpRequestConstructor
    : function() {
    if (!(this instanceof WorkerXMLHttpRequestNative))
      throw new TypeError("native Worker XMLHttpRequest requires new");
    WorkerEventTargetNative.call(this);
    this._readyState = 0;
    this._status = 0;
    this._statusText = "";
    this._responseText = "";
    this._responseUtf8Pending = [];
    this._responseURL = "";
    this._response = "";
    this._responseType = "";
    this._responseXML = null;
    this._overrideMimeType = null;
    this._responseContentType = null;
    this._withCredentials = false;
    this.onreadystatechange = null;
    this.onprogress = null;
    this.onload = null;
    this.onerror = null;
    this.onabort = null;
    this.ontimeout = null;
    this.onloadend = null;
    this._method = "GET";
    this._url = "";
    this._async = true;
    this._headers = new WorkerHeadersNative();
    this._responseHeaders = new WorkerHeadersNative();
    this._aborted = false;
    this._controller = null;
    this._responseReader = null;
    this._sent = false;
    this._token = 0;
    this._timeout = 0;
    this._upload = new WorkerXMLHttpRequestUpload();
    this._uploadStarted = false;
    this._uploadFinished = false;
    this._uploadTotal = null;
    this._uploadLoaded = 0;
  };
  WorkerXMLHttpRequestNative.prototype._notifyReadyState = function() {
    workerXhrDispatch(this, "readystatechange", {});
  };
  Object.defineProperties(WorkerXMLHttpRequestNative.prototype, {
    readyState: { configurable: true, get() { return this._readyState; } },
    status: { configurable: true, get() { return this._status; } },
    statusText: { configurable: true, get() { return this._statusText; } },
    responseURL: { configurable: true, get() { return this._responseURL; } },
    response: { configurable: true, get() { return this._response; } },
    upload: { configurable: true, get() { return this._upload; } },
  });
  Object.defineProperty(WorkerXMLHttpRequestNative.prototype, "timeout", {
    configurable: true,
    get() { return this._timeout; },
    set(value) {
      const numeric = Number(value);
      if (!Number.isFinite(numeric) || numeric < 0 || numeric > __GLASS_WORKER_MAX_NATIVE_XHR_TIMEOUT_MS__)
        throw new RangeError("native Worker XMLHttpRequest timeout is outside the bounded range");
      this._timeout = Math.trunc(numeric);
    },
  });
  Object.defineProperty(WorkerXMLHttpRequestNative.prototype, "withCredentials", {
    configurable: true,
    get() { return this._withCredentials; },
    set(value) {
      if ((this.readyState !== 0 && this.readyState !== 1) || this._sent)
        throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest credentials cannot change after opening", "InvalidStateError");
      this._withCredentials = Boolean(value);
    },
  });
  Object.defineProperty(WorkerXMLHttpRequestNative.prototype, "responseText", {
    configurable: true,
    get() {
      const responseType = String(this.responseType || "").toLowerCase();
      if (!["", "text"].includes(responseType))
        throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest responseText is unavailable for this response type", "InvalidStateError");
      return this._responseText;
    },
  });
  Object.defineProperty(WorkerXMLHttpRequestNative.prototype, "responseType", {
    configurable: true,
    get() { return this._responseType; },
    set(value) {
      const normalized = String(value).toLowerCase();
      if (normalized === "document") return;
      if (this.readyState === 3 || this.readyState === 4)
        throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest responseType cannot change after loading", "InvalidStateError");
      if (!["", "text", "json", "arraybuffer", "blob"].includes(normalized))
        throw new TypeError("native Worker XMLHttpRequest responseType is unsupported");
      this._responseType = normalized;
    },
  });
  Object.defineProperty(WorkerXMLHttpRequestNative.prototype, "responseXML", {
    configurable: true,
    get() {
      const responseType = String(this.responseType || "").toLowerCase();
      if (!['', 'document'].includes(responseType))
        throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest responseXML is unavailable for this response type", "InvalidStateError");
      return this.readyState === 4 ? this._responseXML : null;
    },
  });
  WorkerXMLHttpRequestNative.prototype.overrideMimeType = function(value) {
    if (this.readyState === 3 || this.readyState === 4)
      throw new WorkerDOMExceptionNative("native Worker XMLHttpRequest MIME type cannot change after loading", "InvalidStateError");
    this._overrideMimeType = workerXhrNormalizeOverrideMimeType(value);
  };
  WorkerXMLHttpRequestNative.prototype.open = function(method, url, async, username, password) {
    const normalizedMethod = workerXhrOpenMethod(method);
    const resolvedUrl = workerXhrOpenUrl(url, username, password);
    const nextAsync = async === undefined ? true : Boolean(async);
    const previousController = this._controller;
    const previousReader = this._responseReader;
    this._controller = null;
    this._responseReader = null;
    if (previousController) previousController.abort();
    if (previousReader) { try { previousReader.cancel(); } catch (_) {} }
    this._method = normalizedMethod;
    this._url = resolvedUrl;
    this._async = nextAsync;
    this._headers = new WorkerHeadersNative();
    this._responseHeaders = new WorkerHeadersNative();
    this._aborted = false;
    this._sent = false;
    this._controller = null;
    this._responseReader = null;
    this._status = 0;
    this._statusText = "";
    this._responseText = "";
    this._responseUtf8Pending = [];
    this._responseURL = "";
    this._response = "";
    this._responseXML = null;
    this._responseContentType = null;
    this._token += 1;
    this._uploadStarted = false;
    this._uploadFinished = false;
    this._uploadTotal = null;
    this._uploadLoaded = 0;
    this._readyState = 1;
    this._notifyReadyState();
  };
  WorkerXMLHttpRequestNative.prototype.setRequestHeader = function(name, value) {
    if (this.readyState !== 1 || this._sent)
      throw new TypeError("native Worker XMLHttpRequest is not open");
    this._headers.append(name, value);
  };
  WorkerXMLHttpRequestNative.prototype.getResponseHeader = function(name) {
    if (this.readyState < 2 || this.readyState === 0) return null;
    return this._responseHeaders.get(name);
  };
  WorkerXMLHttpRequestNative.prototype.getAllResponseHeaders = function() {
    if (this.readyState < 2 || this.readyState === 0) return "";
    return Array.from(this._responseHeaders.entries())
      .map(entry => entry[0] + ": " + entry[1] + "\r\n")
      .join("");
  };
  WorkerXMLHttpRequestNative.prototype.abort = function() {
    const active = this._sent && this.readyState !== 0 && this.readyState !== 4;
    const controller = this._controller;
    const reader = this._responseReader;
    this._token += 1;
    this._aborted = true;
    this._controller = null;
    this._responseReader = null;
    if (controller) controller.abort();
    if (reader) { try { reader.cancel(); } catch (_) {} }
    if (!active) return;
    this._sent = false;
    this._readyState = 0;
    this._status = 0;
    this._statusText = "";
    this._responseText = "";
    this._responseUtf8Pending = [];
    this._responseURL = "";
    this._response = "";
    this._responseXML = null;
    this._responseContentType = null;
    this._responseHeaders = new WorkerHeadersNative();
    this._notifyReadyState();
    workerXhrFinishUpload(this, "abort");
    workerXhrDispatch(this, "abort", {});
    workerXhrDispatch(this, "loadend", {});
  };
  WorkerXMLHttpRequestNative.prototype.send = function(body) {
    if (this.readyState !== 1 || this._sent)
      throw new TypeError("native Worker XMLHttpRequest is not open");
    const responseType = String(this.responseType || "").toLowerCase();
    if (!["", "text", "json", "arraybuffer", "blob"].includes(responseType))
      throw new TypeError("native Worker XMLHttpRequest responseType is unsupported");
    const requestBody = body === undefined ? null : body;
    if (!this._async) {
      if (this._timeout !== 0)
        throw new WorkerDOMExceptionNative("native synchronous Worker XMLHttpRequest cannot use timeout", "InvalidAccessError");
      const request = workerSyncXhrRequestPayload(this, requestBody);
      this._token += 1;
      this._sent = true;
      this._aborted = false;
      workerXhrStartUpload(this, requestBody);
      const objectUrlEntry = workerObjectUrlRegistry.get(this._url);
      if (objectUrlEntry && ["GET", "HEAD"].includes(this._method) && requestBody === null) {
        const objectBytes = this._method === "HEAD" ? [] : workerBlobBytes(objectUrlEntry.blob);
        const objectContentType = objectUrlEntry.blob.type || null;
        const objectHeaders = [["content-length", String(objectUrlEntry.blob.size)]];
        if (objectContentType) objectHeaders.unshift(["content-type", objectContentType]);
        workerXhrApplySyncResponse(this, {
          error: false,
          url: this._url,
          status: 200,
          statusText: "OK",
          contentType: objectContentType,
          headers: objectHeaders,
          bodyBase64: encodeWorkerBase64(objectBytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
        }, responseType);
        return;
      }
      let payload;
      try {
        payload = JSON.parse(globalThis.__glassSyncXhr(JSON.stringify(request)));
      } catch (error) {
        workerXhrFinishUpload(this, "error");
        throw error;
      }
      if (typeof payload.documentCookie === "string"
          && typeof globalThis.__glassApplySyncCookieProjection === "function")
        globalThis.__glassApplySyncCookieProjection(payload.documentCookie);
      workerXhrApplySyncResponse(this, payload, responseType);
      return;
    }
    const token = this._token + 1;
    this._token = token;
    this._sent = true;
    this._aborted = false;
    const controller = new AbortControllerNative();
    this._controller = controller;
    workerXhrStartUpload(this, requestBody);
    workerFetchNative(this._url, {
      method: this._method,
      body: requestBody,
      headers: this._headers,
      credentials: this.withCredentials ? "include" : "same-origin",
      signal: controller.signal,
      __glassTimeoutMs: this._timeout,
    }).then(response => {
      if (this._token !== token || this._aborted) return null;
      this._status = response.status;
      this._statusText = response.statusText;
      this._responseURL = response.url;
      this._responseContentType = this._overrideMimeType || response.headers.get("content-type");
      this._responseHeaders = response.headers;
      this._readyState = 2;
      this._notifyReadyState();
      return workerXhrReadResponse(this, response, responseType, controller);
    }).then(value => {
      if (value === null || this._token !== token || this._aborted) return;
      workerXhrAppendResponseText(this, responseType, [], true);
      const bytes = value;
      const responseValue = responseType === "json"
        ? JSON.parse(workerUtf8Text(bytes))
        : responseType === "arraybuffer"
          ? new Uint8Array(bytes).buffer
          : responseType === "blob"
            ? new WorkerBlob([new Uint8Array(bytes)], { type: this._responseContentType || "" })
            : workerUtf8Text(bytes);
      this._controller = null;
      this._responseText = typeof responseValue === "string" ? responseValue : this._responseText;
      this._response = responseValue;
      this._sent = false;
      this._readyState = 4;
      this._notifyReadyState();
      workerXhrFinishUpload(this, "load");
      workerXhrDispatch(this, "load", {});
      workerXhrDispatch(this, "loadend", {});
    }).catch(error => {
      if (this._token !== token || this._aborted) return;
      this._controller = null;
      this._responseReader = null;
      this._sent = false;
      this._status = 0;
      this._statusText = "";
      this._responseText = "";
      this._responseUtf8Pending = [];
      this._responseURL = "";
      this._response = "";
      this._responseXML = null;
      this._responseContentType = null;
      this._responseHeaders = new WorkerHeadersNative();
      this._readyState = 4;
      this._notifyReadyState();
      if (error && error.name === "TimeoutError") {
        workerXhrFinishUpload(this, "timeout");
        workerXhrDispatch(this, "timeout", {});
      } else {
        workerXhrFinishUpload(this, "error");
        workerXhrDispatch(this, "error", { error });
      }
      workerXhrDispatch(this, "loadend", {});
    });
  };
  WorkerXMLHttpRequestNative.UNSENT = 0;
  WorkerXMLHttpRequestNative.OPENED = 1;
  WorkerXMLHttpRequestNative.HEADERS_RECEIVED = 2;
  WorkerXMLHttpRequestNative.LOADING = 3;
  WorkerXMLHttpRequestNative.DONE = 4;
  WorkerXMLHttpRequestNative.prototype.UNSENT = 0;
  WorkerXMLHttpRequestNative.prototype.OPENED = 1;
  WorkerXMLHttpRequestNative.prototype.HEADERS_RECEIVED = 2;
  WorkerXMLHttpRequestNative.prototype.LOADING = 3;
  WorkerXMLHttpRequestNative.prototype.DONE = 4;
  if (typeof globalThis.EventTarget === "function" && globalThis.EventTarget.prototype) {
    Object.setPrototypeOf(WorkerXMLHttpRequestUpload.prototype, globalThis.EventTarget.prototype);
    Object.setPrototypeOf(WorkerXMLHttpRequestNative.prototype, globalThis.EventTarget.prototype);
  }
  globalThis.__glassWorkerXmlHttpRequestConstructor = WorkerXMLHttpRequestNative;
  globalThis.XMLHttpRequest = WorkerXMLHttpRequestNative;
  globalThis.__glassWorkerFetchRequests = workerFetchRequests;
  globalThis.__glassNextWorkerFetchRequestId = nextWorkerFetchRequestId;
  globalThis.fetch = workerFetchNative;
  globalThis.__glassDynamicImport = (referrer, referrerPolicy, specifier, options) => {
    const intrinsics = globalThis.__glassDynamicImportIntrinsics;
    return new intrinsics.PromiseConstructor((resolve, reject) => {
      let normalizedReferrer;
      let normalizedRequest;
      try {
        if (typeof referrer === "symbol")
          throw new intrinsics.TypeErrorConstructor("dynamic import referrer cannot be a Symbol");
        normalizedReferrer = intrinsics.StringConstructor(referrer);
        normalizedRequest = intrinsics.normalize(specifier, options);
      } catch (error) { reject(error); return; }
      if (!Number.isSafeInteger(nextWorkerFetchRequestId)
          || nextWorkerFetchRequestId < 1
          || nextWorkerFetchRequestId > 0xFFFFFFFF) {
        reject(new RangeError("dynamic import request id limit exceeded"));
        return;
      }
      const requestId = nextWorkerFetchRequestId;
      nextWorkerFetchRequestId += 1;
      globalThis.__glassNextWorkerFetchRequestId = nextWorkerFetchRequestId;
      workerFetchRequests.set(requestId, {
        resolve, reject, signal: null, abortListener: null,
        uploadStreamId: null, dynamicModuleImport: true,
        moduleType: normalizedRequest.moduleType,
      });
      try {
        pushCommand({
          kind: "fetch", request_id: requestId, worker_id: workerId,
          href: normalizedRequest.specifier, credentials: true,
          method: "GET", headers: {}, body: null, body_base64: null,
          content_type: null, mode: "cors", redirect: "follow",
          cache: "default", timeout_ms: null, upload_stream_id: null,
          destination: "module", module_referrer: normalizedReferrer,
          referrer_policy: referrerPolicy,
          module_type: normalizedRequest.moduleType,
        });
      } catch (error) {
        workerFetchRequests.delete(requestId);
        reject(error);
      }
    });
  };
  globalThis.__glassResolveWorkerFetch = (requestId, payload) => {
    const pending = workerFetchRequests.get(Number(requestId));
    if (!pending) return null;
    workerFetchRequests.delete(Number(requestId));
    if (pending.signal && pending.abortListener)
      pending.signal.removeEventListener("abort", pending.abortListener);
    if (pending.uploadStreamId !== null) {
      const uploadGroup = workerFetchUploadGroups.get(pending.uploadStreamId);
      cancelWorkerFetchUploadGroup(uploadGroup, "native Worker fetch request completed");
      workerFetchUploadGroups.delete(pending.uploadStreamId);
    }
    if (pending.dynamicModuleImport) {
      const moduleResult = payload && payload.dynamicModuleImport;
      if (!moduleResult || moduleResult.error) {
        pending.reject(new TypeError(String(moduleResult && moduleResult.error
          || "Failed to load dynamically imported module")));
      } else if (typeof moduleResult.moduleKey !== "string") {
        pending.reject(new TypeError("Invalid dynamic module response"));
      } else {
        try {
          const modulePromise = pending.moduleType === "json"
            ? import(moduleResult.moduleKey, { with: { type: "json" } })
            : import(moduleResult.moduleKey);
          modulePromise.then(pending.resolve, pending.reject);
        } catch (error) { pending.reject(error); }
      }
      return null;
    }
    if (payload && payload.error) {
      const error = payload.timeout === true
        ? new Error(String(payload.error))
        : new TypeError(String(payload.error));
      if (payload.timeout === true) error.name = "TimeoutError";
      pending.reject(error);
    } else pending.resolve(responseFromWorkerFetch(payload));
    return null;
  };
