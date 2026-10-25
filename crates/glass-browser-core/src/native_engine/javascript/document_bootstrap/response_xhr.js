  const responseBodyBytes = (payload, maxBytes = nativeFormBodyLimit) => typeof payload.bodyBase64 === "string"
    ? decodeBase64(payload.bodyBase64, maxBytes)
    : blobUtf8Bytes(String(payload.body || ""));
  const responseBodyBlob = (payload) => {{
    const blob = new BlobNative([], {{ type: payload.contentType || "" }});
    blob._bytes = responseBodyBytes(payload);
    blob._text = payload.body === undefined
      ? utf8TextFromBytes(blob._bytes)
      : String(payload.body);
    blob.size = blob._bytes.length;
    return blob;
  }};
  const responseBodyBlobFromBytes = (bytes, contentType) => {{
    const blob = new BlobNative([], {{ type: contentType || "" }});
    blob._bytes = bytes.slice();
    blob._text = utf8TextFromBytes(blob._bytes);
    blob.size = blob._bytes.length;
    return blob;
  }};
  const nativeRequestBodyPayload = (settings) => {{
    const input = settings.body;
    if (input === undefined || input === null) return {{ bodyNull: true, bytes: [], stream: null, contentType: null }};
    if (input.__glassReadableStream === true) {{
      const state = readableStreamState(input);
      if (state.locked || state.disturbed) throw new TypeError("native Request body stream is unusable");
      return {{ bodyNull: false, bytes: null, stream: input, contentType: null }};
    }}
    if (input.__glassFormData === true) {{
      const serialized = serializeFormData(input, nextFetchRequestId);
      const bytes = serialized.bodyBase64 === null
        ? blobUtf8Bytes(serialized.body)
        : decodeBase64(serialized.bodyBase64, nativeFormBodyLimit);
      return {{ bodyNull: false, bytes, stream: null, contentType: serialized.contentType }};
    }}
    if (input.__glassUrlSearchParams === true) {{
      const text = input.toString();
      return {{ bodyNull: false, bytes: blobUtf8Bytes(text), stream: null, contentType: "application/x-www-form-urlencoded;charset=UTF-8" }};
    }}
    if (input.__glassNativeBlob === true)
      return {{ bodyNull: false, bytes: blobBytes(input), stream: null, contentType: input.type || null }};
    if (input instanceof ArrayBuffer)
      return {{ bodyNull: false, bytes: Array.from(new Uint8Array(input)), stream: null, contentType: null }};
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(input))
      return {{ bodyNull: false, bytes: Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength)), stream: null, contentType: null }};
    return {{ bodyNull: false, bytes: blobUtf8Bytes(String(input)), stream: null, contentType: null }};
  }};
  const nativeMultipartParameter = (value, parameter) => {{
    for (const piece of String(value).split(";")) {{
      const separator = piece.indexOf("=");
      if (separator < 0 || piece.slice(0, separator).trim().toLowerCase() !== parameter) continue;
      let result = piece.slice(separator + 1).trim();
      if (result.startsWith('"') && result.endsWith('"')) result = result.slice(1, -1);
      return result.replace(/\\"/g, '"').replace(/\\\\/g, "\\");
    }}
    return null;
  }};
  const nativeRequestFormData = (bytes, payload, request) => {{
    if (bytes.length > nativeFormBodyLimit) throw new RangeError("native Request FormData body limit exceeded");
    const declaredType = request.headers.get("content-type") || payload.contentType || "";
    const mediaType = String(declaredType).split(";", 1)[0].trim().toLowerCase();
    if (mediaType === "application/x-www-form-urlencoded") {{
      const form = new FormDataNative();
      const params = new URLSearchParamsNative(utf8TextFromBytes(bytes));
      for (const entry of params._entries) form.append(entry[0], entry[1]);
      return form;
    }}
    if (mediaType !== "multipart/form-data")
      throw new TypeError("native Request body is not FormData-compatible");
    const boundaryMatch = String(declaredType).match(/boundary=(?:"([^"]+)"|([^;\s]+))/i);
    const boundary = boundaryMatch ? (boundaryMatch[1] || boundaryMatch[2]) : null;
    if (!boundary || boundary.length > 256) throw new TypeError("native multipart boundary is invalid");
    const source = (() => {{
      let text = "";
      for (let offset = 0; offset < bytes.length; offset += 4096)
        text += String.fromCharCode(...bytes.slice(offset, Math.min(offset + 4096, bytes.length)));
      return text;
    }})();
    const marker = "--" + boundary;
    if (!source.startsWith(marker)) throw new TypeError("native multipart body is malformed");
    const form = new FormDataNative();
    let offset = marker.length;
    let entryCount = 0;
    if (source.startsWith("--", offset)) return form;
    if (source.slice(offset, offset + 2) !== "\r\n") throw new TypeError("native multipart body is malformed");
    offset += 2;
    while (offset < source.length) {{
      const headerEnd = source.indexOf("\r\n\r\n", offset);
      if (headerEnd < 0) throw new TypeError("native multipart headers are malformed");
      const headerValues = {{}};
      for (const line of source.slice(offset, headerEnd).split("\r\n")) {{
        const separator = line.indexOf(":");
        if (separator < 0) continue;
        headerValues[line.slice(0, separator).trim().toLowerCase()] = line.slice(separator + 1).trim();
      }}
      const disposition = headerValues["content-disposition"] || "";
      if (disposition.toLowerCase().split(";", 1)[0].trim() !== "form-data")
        throw new TypeError("native multipart disposition is unsupported");
      const name = nativeMultipartParameter(disposition, "name");
      if (name === null || name.length > storageKeyLimit) throw new TypeError("native multipart field name is invalid");
      const bodyStart = headerEnd + 4;
      const nextBoundary = source.indexOf("\r\n" + marker, bodyStart);
      if (nextBoundary < 0) throw new TypeError("native multipart boundary is missing");
      const bodyEnd = nextBoundary;
      const filename = nativeMultipartParameter(disposition, "filename");
      const fieldBytes = bytes.slice(bodyStart, bodyEnd);
      entryCount += 1;
      if (entryCount > storageEntryLimit) throw new RangeError("native Request FormData entry limit exceeded");
      if (filename === null) form.append(name, utf8TextFromBytes(fieldBytes));
      else {{
        const file = new FileNative([], filename, {{ type: headerValues["content-type"] || "" }});
        file._bytes = fieldBytes;
        file._text = utf8TextFromBytes(fieldBytes);
        file.size = fieldBytes.length;
        form.append(name, file);
      }}
      offset = nextBoundary + 2 + marker.length;
      if (source.startsWith("--", offset)) return form;
      if (source.slice(offset, offset + 2) !== "\r\n") throw new TypeError("native multipart body is malformed");
      offset += 2;
    }}
    throw new TypeError("native multipart body is incomplete");
  }};
  const nativeRequestBodyIsUsed = (request) => request.__glassRequestBodyState.used === true
    || Boolean(request.body && readableStreamState(request.body).disturbed);
  const nativeRequestBodyUse = (request) => {{
    const body = request.body;
    if (nativeRequestBodyIsUsed(request) || (body && readableStreamState(body).locked)) return false;
    request.__glassRequestBodyState.used = true;
    if (body) {{
      const state = readableStreamState(body);
      state.consumedByRequest = true;
      markReadableStreamDisturbed(state);
    }}
    return true;
  }};
  const nativeRequestBodyUnusable = () => Promise.reject(new TypeError("native Request body is unusable"));
  const nativeRequestBodyPromise = (request, transform) => {{
    if (!nativeRequestBodyUse(request)) return nativeRequestBodyUnusable();
    const payload = request.__glassRequestBodyPayload;
    if (payload.stream)
      return nativeDrainReadableStream(payload.stream, true).then(bytes => transform(bytes, payload));
    try {{
      return Promise.resolve(transform(payload.bytes.slice(), payload));
    }} catch (error) {{
      return Promise.reject(error);
    }}
  }};
  const nativeReadableStreamChunkBytes = (value) => {{
    if (typeof value === "string") return blobUtf8Bytes(value);
    if (value instanceof ArrayBuffer) return Array.from(new Uint8Array(value));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value))
      return Array.from(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
    throw new TypeError("native Request stream chunks must be byte data");
  }};
  const nativeDrainReadableStream = (stream, claimed, responseBody) => {{
    const state = readableStreamState(stream);
    if (!claimed && (state.locked || state.disturbed))
      return Promise.reject(new TypeError("native Request body stream is unusable"));
    if (responseBody) {{
      state.consumedByResponse = true;
      state.consumingResponseBody = true;
    }} else {{
      state.consumedByRequest = true;
      state.consumingRequestBody = true;
    }}
    if (!claimed) markReadableStreamDisturbed(state);
    let reader;
    try {{ reader = stream.getReader(); }}
    catch (error) {{
      state.consumingRequestBody = false;
      return Promise.reject(error);
    }}
    const chunks = [];
    let totalBytes = 0;
    const release = () => {{
      if (responseBody) state.consumingResponseBody = false;
      else state.consumingRequestBody = false;
      reader.releaseLock();
    }};
    const readNext = () => reader.read().then(
      result => {{
        if (result.done) {{
          release();
          const bytes = [];
          for (const chunk of chunks) bytes.push(...chunk);
          return bytes;
        }}
        let bytes;
        try {{ bytes = nativeReadableStreamChunkBytes(result.value); }}
        catch (error) {{ release(); throw error; }}
        totalBytes += bytes.length;
        if (totalBytes > nativeFormBodyLimit) {{
          release();
          throw new RangeError("native Request stream body exceeds its limit");
        }}
        chunks.push(bytes);
        return readNext();
      }},
      error => {{ release(); throw error; }},
    );
    return readNext();
  }};
  const fetchNative = (input, options, internalDestination) => {{
    const sourceRequest = input && input.__glassRequest === true ? input : null;
    const sourceUrl = input && input.__glassUrl === true ? input : null;
    if (typeof input !== "string" && !sourceRequest && !sourceUrl)
      throw new TypeError("native fetch requires a URL string, URL, or Request");
    const hasBodyOverride = options && typeof options === "object" && Object.prototype.hasOwnProperty.call(options, "body");
    const sourceBody = sourceRequest && !hasBodyOverride ? sourceRequest.body : null;
    if (sourceBody && sourceBody.__glassReadableStream === true) {{
      if (!nativeRequestBodyUse(sourceRequest)) return Promise.reject(new TypeError("native Request body is unusable"));
      return fetchNativeDispatch(input, options, undefined, sourceBody, internalDestination);
    }}
    const settings = Object.assign(
      {{}},
      sourceRequest ? sourceRequest._settings : {{}},
      options && typeof options === "object" ? options : {{}},
    );
    const body = settings.body;
    if (body && body.__glassReadableStream === true)
      return fetchNativeDispatch(input, options, undefined, body, internalDestination);
    return fetchNativeDispatch(input, options, undefined, undefined, internalDestination);
  }};
  RequestNative.prototype.text = function() {{
    return nativeRequestBodyPromise(this, bytes => utf8TextFromBytes(bytes));
  }};
  RequestNative.prototype.json = function() {{
    return nativeRequestBodyPromise(this, bytes => JSON.parse(utf8TextFromBytes(bytes)));
  }};
  RequestNative.prototype.blob = function() {{
    return nativeRequestBodyPromise(this, (bytes, payload) => responseBodyBlobFromBytes(
      bytes,
      payload.contentType || this.headers.get("content-type") || "",
    ));
  }};
  RequestNative.prototype.arrayBuffer = function() {{
    return nativeRequestBodyPromise(this, bytes => new Uint8Array(bytes).buffer);
  }};
  RequestNative.prototype.bytes = function() {{
    return nativeRequestBodyPromise(this, bytes => new Uint8Array(bytes));
  }};
  RequestNative.prototype.formData = function() {{
    return nativeRequestBodyPromise(this, (bytes, payload) => nativeRequestFormData(bytes, payload, this));
  }};
  const responseInitEntries = (input) => {{
    if (input === undefined || input === null) return [];
    if (input.__glassHeaders === true) return input._entries.map(entry => [entry[0], entry[1]]);
    if (Array.isArray(input)) return input.map(entry => {{
      if (!Array.isArray(entry) || entry.length !== 2) throw new TypeError("native Response header pairs must contain two values");
      return [entry[0], entry[1]];
    }});
    if (typeof input !== "object") throw new TypeError("native Response headers must be an object or pairs");
    return Object.keys(input).map(name => [name, input[name]]);
  }};
  const responseConstructorPayload = (body, options) => {{
    const settings = options && typeof options === "object" ? options : {{}};
    const bodyNull = body === undefined || body === null;
    const bodyPayload = bodyNull
      ? {{ body: "", bodyBase64: encodeBase64([]), contentType: null }}
      : body && body.__glassReadableStream === true
        ? {{ body: "", bodyBase64: null, contentType: null, bodyStream: body }}
      : body && body.__glassNativeBlob === true
        ? {{ body: body._text, bodyBase64: encodeBase64(blobBytes(body)), contentType: body.type || null }}
        : body && body.__glassUrlSearchParams === true
          ? {{ body: body.toString(), bodyBase64: encodeBase64(blobUtf8Bytes(body.toString())), contentType: "application/x-www-form-urlencoded;charset=UTF-8" }}
          : body instanceof ArrayBuffer
            ? {{ body: utf8TextFromBytes(Array.from(new Uint8Array(body))), bodyBase64: encodeBase64(Array.from(new Uint8Array(body))), contentType: null }}
            : typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(body)
              ? {{ body: utf8TextFromBytes(Array.from(new Uint8Array(body.buffer, body.byteOffset, body.byteLength))), bodyBase64: encodeBase64(Array.from(new Uint8Array(body.buffer, body.byteOffset, body.byteLength))), contentType: null }}
              : {{ body: String(body), bodyBase64: encodeBase64(blobUtf8Bytes(String(body))), contentType: null }};
    const status = settings.status === undefined ? 200 : Number(settings.status);
    if (!Number.isInteger(status) || status < 200 || status > 599) throw new RangeError("native Response status is outside the bounded range");
    const headers = responseInitEntries(settings.headers);
    return {{
      url: "",
      status,
      statusText: settings.statusText === undefined ? "" : String(settings.statusText),
      headers,
      contentType: bodyPayload.contentType,
      body: bodyPayload.body,
      bodyBase64: bodyPayload.bodyBase64,
      bodyStream: bodyPayload.bodyStream || null,
      bodyNull,
      redirected: false,
      opaque: false,
      opaqueRedirect: false,
    }};
  }};
  const ResponseNative = function(body, options) {{
    if (!(this instanceof ResponseNative)) throw new TypeError("native Response requires new");
    if (body && body.__glassReadableStream === true) {{
      const state = readableStreamState(body);
      if (state.locked || state.disturbed) throw new TypeError("native Response body stream is unusable");
    }}
    return responseFromFetch(responseConstructorPayload(body, options));
  }};
  ResponseNative.prototype.constructor = ResponseNative;
  ResponseNative.json = function(data, options) {{
    const settings = options && typeof options === "object" ? Object.assign({{}}, options) : {{}};
    const headers = new HeadersNative(settings.headers);
    if (!headers.has("content-type")) headers.set("content-type", "application/json");
    settings.headers = headers;
    return new ResponseNative(JSON.stringify(data), settings);
  }};
  ResponseNative.error = function() {{
    const response = Object.create(ResponseNative.prototype);
    Object.assign(response, {{
      type: "error",
      ok: false,
      status: 0,
      statusText: "",
      url: "",
      redirected: false,
      headers: responseHeaders([], null),
      body: null,
      clone() {{ return ResponseNative.error(); }},
      text() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      json() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      blob() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      arrayBuffer() {{ return Promise.reject(nativeOpaqueResponseError()); }},
      bytes() {{ return Promise.reject(nativeOpaqueResponseError()); }},
    }});
    defineResponseBodyState(response);
    return Object.freeze(response);
  }};
  ResponseNative.redirect = function(url, status) {{
    const code = status === undefined ? 302 : Number(status);
    if (![301, 302, 303, 307, 308].includes(code)) throw new RangeError("native Response redirect status is unsupported");
    const location = url && url.__glassUrl === true ? url.href : String(url);
    return responseFromFetch({{
      url: "",
      status: code,
      statusText: "",
      headers: [["location", location]],
      contentType: null,
      body: "",
      bodyBase64: encodeBase64([]),
      bodyNull: true,
      redirected: false,
      opaque: false,
      opaqueRedirect: false,
    }});
  }};
  globalThis.Response = ResponseNative;
  const responseFromFetch = (payload) => {{
    const opaque = payload && payload.opaque === true;
    const opaqueRedirect = payload && payload.opaqueRedirect === true;
    const error = payload && payload.error === true;
    const filtered = opaque || opaqueRedirect || error;
    const bodyNull = payload && payload.bodyNull === true;
    const bodyStream = payload && payload.bodyStream && payload.bodyStream.__glassReadableStream === true
      ? payload.bodyStream
      : null;
    const streamId = payload && Number.isSafeInteger(Number(payload.bodyStreamId))
      ? Number(payload.bodyStreamId)
      : null;
    const opaqueBody = () => Promise.reject(nativeOpaqueResponseError());
    const responseBodyPromise = (response) => {{
      if (!responseBodyUse(response)) return responseBodyUnusable();
      if (response.__glassResponseStreamBody)
        return nativeDrainReadableStream(response.body, true, true);
      if (response.__glassFetchStreamId === null)
        return Promise.resolve(responseBodyBytes(response.__glassPayload));
      const group = fetchStreamGroup(response.__glassFetchStreamId);
      if (group.error !== null) return Promise.reject(new Error(group.error));
      if (group.done) return Promise.resolve(flattenFetchStream(group));
      return new Promise((resolve, reject) => {{
        group.waiters.push({{ resolve, reject }});
        try {{ requestFetchStreamRead(group); }}
        catch (error) {{ group.waiters.pop(); reject(error); }}
      }});
    }};
    const response = Object.create(ResponseNative.prototype);
    Object.assign(response, {{
      type: error ? "error" : opaqueRedirect ? "opaqueredirect" : (opaque ? "opaque" : "basic"),
      ok: !filtered && payload.status >= 200 && payload.status < 300,
      status: filtered ? 0 : payload.status,
      statusText: filtered ? "" : (payload.statusText === undefined ? String(payload.status) : String(payload.statusText)),
      url: filtered ? "" : payload.url,
      redirected: opaqueRedirect ? false : payload.redirected === true,
      headers: filtered ? responseHeaders([], null) : responseHeaders(payload.headers, payload.contentType),
      body: filtered || bodyNull
        ? null
        : bodyStream
          ? bodyStream
        : streamId === null
          ? new ReadableStreamNative(responseBodyBytes(payload))
          : new ReadableStreamNative([], streamId),
      __glassPayload: payload,
      __glassFetchStreamId: streamId,
      __glassResponseStreamBody: bodyStream !== null,
      clone() {{
        const body = this.body;
        if (body && (responseBodyIsUsed(this) || readableStreamState(body).locked))
          throw new TypeError("native Response body is unusable");
        if (this.__glassResponseStreamBody) {{
          const [sourceBody, cloneBody] = body.tee();
          this.__glassPayload.bodyStream = sourceBody;
          this.__glassResponseBodyState.stream = sourceBody;
          return responseFromFetch(Object.assign({{}}, this.__glassPayload, {{ bodyStream: cloneBody }}));
        }}
        return responseFromFetch(payload);
      }},
      text() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => utf8TextFromBytes(bytes)); }},
      json() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => JSON.parse(utf8TextFromBytes(bytes))); }},
      blob() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => responseBodyBlobFromBytes(bytes, payload.contentType)); }},
      arrayBuffer() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => responseBodyBlobFromBytes(bytes, payload.contentType).arrayBuffer()); }},
      bytes() {{ return filtered ? opaqueBody() : responseBodyPromise(this).then(bytes => new Uint8Array(bytes)); }},
    }});
    if (bodyStream) {{
      const responseBodyState = {{ stream: bodyStream }};
      Object.defineProperty(response, "__glassResponseBodyState", {{ value: responseBodyState }});
      Object.defineProperty(response, "body", {{
        configurable: true,
        enumerable: true,
        get() {{ return responseBodyState.stream; }},
      }});
    }}
    defineResponseBodyState(response);
    if (streamId !== null && !filtered && !bodyNull) {{
      const group = fetchStreamGroup(streamId);
      if (!group.started) {{
        group.started = true;
        requestFetchStreamRead(group);
      }}
    }}
    return Object.freeze(response);
  }};
  const nativeXhrEventTypes = ["readystatechange", "loadstart", "progress", "load", "error", "timeout", "abort", "loadend"];
  const nativeXhrDispatch = (target, type, extra) => {{
    const event = extra && Object.prototype.hasOwnProperty.call(extra, "loaded")
      && typeof globalThis.ProgressEvent === "function"
      ? new globalThis.ProgressEvent(type, extra)
      : createEvent(type, extra);
    Object.assign(event, extra || {{}});
    return dispatchTarget(target, event);
  }};
  const NativeXMLHttpRequestUpload = typeof globalThis.__glassXmlHttpRequestUploadConstructor === "function"
    ? globalThis.__glassXmlHttpRequestUploadConstructor
    : function() {{
      EventTargetNative.call(this);
      for (const type of nativeXhrEventTypes) installEventHandlerProperty(this, type);
    }};
  globalThis.__glassXmlHttpRequestUploadConstructor = NativeXMLHttpRequestUpload;
  globalThis.XMLHttpRequestUpload = NativeXMLHttpRequestUpload;
  const nativeXhrBodyLength = (body) => {{
    if (body === undefined || body === null) return 0;
    if (body.__glassFormData === true) {{
      const serialized = serializeFormData(body, nextFetchRequestId);
      return serialized.bodyBase64 === null
        ? blobUtf8Bytes(serialized.body).length
        : decodeBase64(serialized.bodyBase64, nativeFormBodyLimit).length;
    }}
    if (body.__glassUrlSearchParams === true) return blobUtf8Bytes(body.toString()).length;
    if (body.__glassNativeBlob === true) return blobBytes(body).length;
    if (body instanceof ArrayBuffer) return body.byteLength;
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(body)) return body.byteLength;
    return blobUtf8Bytes(String(body)).length;
  }};
  const nativeXhrStartUpload = (xhr, body) => {{
    const total = nativeXhrBodyLength(body);
    xhr._uploadStarted = true;
    xhr._uploadFinished = false;
    xhr._uploadTotal = total;
    xhr._uploadLoaded = 0;
    const progress = {{ lengthComputable: true, loaded: 0, total }};
    nativeXhrDispatch(xhr, "loadstart", {{
      lengthComputable: false,
      loaded: 0,
      total: 0,
    }});
    nativeXhrDispatch(xhr.upload, "loadstart", progress);
    xhr._uploadLoaded = total;
    nativeXhrDispatch(xhr.upload, "progress", {{ lengthComputable: true, loaded: total, total }});
  }};
  const nativeXhrFinishUpload = (xhr, type) => {{
    if (!xhr._uploadStarted || xhr._uploadFinished) return;
    xhr._uploadFinished = true;
    const loaded = type === "load" ? xhr._uploadTotal : xhr._uploadLoaded;
    nativeXhrDispatch(xhr.upload, type, {{
      lengthComputable: true,
      loaded,
      total: xhr._uploadTotal,
    }});
    nativeXhrDispatch(xhr.upload, "loadend", {{
      lengthComputable: true,
      loaded,
      total: xhr._uploadTotal,
    }});
  }};
  const nativeXhrResponseProgress = (xhr, value, loadedOverride) => {{
    const header = xhr._responseHeaders.get("content-length");
    const total = header === null || header === undefined ? -1 : Number(header);
    const lengthComputable = Number.isSafeInteger(total) && total >= 0 && total <= nativeXmlMaxBytes;
    const loaded = loadedOverride === undefined
      ? (typeof value === "string"
        ? blobUtf8Bytes(value).length
        : value instanceof ArrayBuffer
          ? value.byteLength
          : value && typeof value.size === "number"
            ? value.size
            : lengthComputable ? total : 0)
      : loadedOverride;
    nativeXhrDispatch(xhr, "progress", {{
      lengthComputable,
      loaded,
      total: lengthComputable ? total : 0,
    }});
  }};
  const nativeXhrUtf8SafePrefix = (bytes) => {{
    let start = bytes.length - 1;
    while (start >= 0 && bytes[start] >= 0x80 && bytes[start] <= 0xbf) start -= 1;
    if (start < 0) return bytes.length;
    const first = bytes[start];
    let width = 1;
    if (first >= 0xc2 && first <= 0xdf) width = 2;
    else if (first >= 0xe0 && first <= 0xef) width = 3;
    else if (first >= 0xf0 && first <= 0xf4) width = 4;
    return width > bytes.length - start ? start : bytes.length;
  }};
  const nativeXhrAppendResponseText = (xhr, responseType, bytes, final) => {{
    if (!["", "text"].includes(responseType)) return;
    const pending = xhr._responseUtf8Pending.concat(bytes);
    const safeLength = final ? pending.length : nativeXhrUtf8SafePrefix(pending);
    if (safeLength > 0) xhr._responseText += utf8TextFromBytes(pending.slice(0, safeLength));
    xhr._responseUtf8Pending = pending.slice(safeLength);
  }};
  const nativeXhrReadResponse = (xhr, response, responseType, controller) => {{
    const stream = response && response.body;
    if (!stream || typeof stream.getReader !== "function") {{
      if (xhr._controller !== controller || xhr._aborted) return Promise.resolve(null);
      const bytes = response && response.__glassPayload
        ? responseBodyBytes(response.__glassPayload)
        : [];
      nativeXhrAppendResponseText(xhr, responseType, bytes, true);
      xhr._readyState = 3;
      xhr._notifyReadyState();
      if (xhr._controller !== controller || xhr._aborted) return Promise.resolve(null);
      nativeXhrResponseProgress(xhr, null, bytes.length);
      return Promise.resolve(bytes);
    }}
    let reader;
    try {{ reader = stream.getReader(); }}
    catch (error) {{ return Promise.reject(error); }}
    xhr._responseReader = reader;
    const bytes = [];
    const release = () => {{
      if (xhr._responseReader === reader) xhr._responseReader = null;
      reader.releaseLock();
    }};
    const readNext = () => {{
      if (xhr._controller !== controller || xhr._aborted) {{
        release();
        return Promise.resolve(null);
      }}
      return reader.read().then(
      result => {{
        if (result.done) {{
          nativeXhrAppendResponseText(xhr, responseType, [], true);
          if (bytes.length === 0) {{
            xhr._readyState = 3;
            xhr._notifyReadyState();
            nativeXhrResponseProgress(xhr, null, 0);
          }}
          release();
          return bytes;
        }}
        let chunk;
        try {{ chunk = nativeReadableStreamChunkBytes(result.value); }}
        catch (error) {{ release(); throw error; }}
        if (chunk.length === 0) return readNext();
        bytes.push(...chunk);
        nativeXhrAppendResponseText(xhr, responseType, chunk, false);
        xhr._readyState = 3;
        xhr._notifyReadyState();
        if (xhr._controller !== controller || xhr._aborted) {{
          release();
          return null;
        }}
        nativeXhrResponseProgress(xhr, null, bytes.length);
        return readNext();
      }},
      error => {{ release(); throw error; }},
      );
    }};
    return readNext();
  }};
  const nativeSyncXhrRequestPayload = (xhr, body) => {{
    const payload = nativeRequestBodyPayload({{ body }});
    if (payload.stream)
      throw new TypeError("native synchronous XMLHttpRequest cannot use a ReadableStream body");
    if (nativeBodylessMethods.includes(xhr._method) && !payload.bodyNull)
      throw new TypeError("native " + xhr._method + " XMLHttpRequest requests must not have a body");
    const headers = {{}};
    let contentType = payload.contentType;
    for (const entry of xhr._headers._entries) {{
      const name = String(entry[0]).toLowerCase();
      if (name === "content-type") {{
        if (contentType !== null && contentType !== undefined)
          throw new TypeError("native XMLHttpRequest body chooses its own Content-Type");
        contentType = String(entry[1]);
      }} else headers[name] = String(entry[1]);
    }}
    const bytes = payload.bodyNull ? [] : payload.bytes;
    return {{
      documentUrl: host.url,
      href: xhr._url,
      method: xhr._method,
      headers,
      bodyBase64: payload.bodyNull ? null : encodeBase64(bytes, nativeFormBodyLimit),
      contentType: contentType === undefined ? null : contentType,
      credentials: xhr.withCredentials,
      timeoutMs: xhr._timeout === 0 ? null : xhr._timeout,
      cookieWrites: typeof globalThis.__glassTakePendingCookieWrites === "function"
        ? globalThis.__glassTakePendingCookieWrites()
        : [],
    }};
  }};
  const nativeXhrDecodeDocumentBytes = (bytes, responseContentType, overrideMimeType) => {{
    const decodeHost = globalThis.__glassDecodeHtmlDocumentHost;
    if (typeof decodeHost !== "function")
      throw new Error("native HTML document decoder is unavailable");
    const decoded = decodeHost(new Uint8Array(bytes), responseContentType, overrideMimeType);
    if (typeof decoded !== "string")
      throw new RangeError("native HTML response document exceeds its byte limit");
    return decoded;
  }};
  const nativeXhrNormalizeOverrideMimeType = (value) => {{
    const text = String(value).trim();
    const essence = text.split(";", 1)[0].trim();
    if (text.length === 0 || text.length > nativeXmlMaxBytes
        || /[\u0000-\u001f\u007f]/.test(text)
        || !/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+\/[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(essence))
      return "application/octet-stream";
    return text;
  }};
  const nativeXhrOpenMethod = (method) => {{
    const normalized = String(method).toUpperCase();
    if (!requestHeaderNameNative.test(normalized))
      throw new DOMExceptionNative("native XMLHttpRequest method is invalid", "SyntaxError");
    if (["CONNECT", "TRACE", "TRACK"].includes(normalized))
      throw new DOMExceptionNative("native XMLHttpRequest method is forbidden", "SecurityError");
    if (normalized.length > {fetch_method_limit})
      throw new TypeError("native XMLHttpRequest method is unsupported");
    return normalized;
  }};
  const nativeXhrOpenUrl = (value, username, password) => {{
    const source = value && value.__glassUrl === true ? value.href : String(value);
    let parts;
    try {{
      parts = nativeUrlParts(nativeUrlResolve(source, host.url));
    }} catch (_) {{
      throw new DOMExceptionNative("native XMLHttpRequest URL is invalid", "SyntaxError");
    }}
    if (parts.authority && (username !== undefined && username !== null
        || password !== undefined && password !== null)) {{
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
    }}
    return parts.href;
  }};
  const nativeXhrApplySyncResponse = (xhr, payload, responseType) => {{
    const fail = (error, eventType) => {{
      xhr._controller = null;
      xhr._responseReader = null;
      xhr._sent = false;
      xhr._status = 0;
      xhr._statusText = "";
      xhr._responseText = "";
      xhr._responseUtf8Pending = [];
      xhr._responseURL = "";
      xhr._response = "";
      xhr._responseXML = null;
      xhr._responseContentType = null;
      xhr._responseHeaders = responseHeaders([], null);
      xhr._readyState = 4;
      xhr._notifyReadyState();
      nativeXhrFinishUpload(xhr, eventType);
      nativeXhrDispatch(xhr, eventType, {{ error }});
      nativeXhrDispatch(xhr, "loadend");
    }};
    if (!payload || payload.error === true) {{
      const error = new Error(String(payload && payload.errorMessage || "native synchronous XMLHttpRequest failed"));
      error.name = payload && payload.timeout === true ? "TimeoutError" : "NetworkError";
      fail(error, payload && payload.timeout === true ? "timeout" : "error");
      return;
    }}
    let bytes;
    try {{
      bytes = decodeBase64(String(payload.bodyBase64 || ""), nativeXmlMaxBytes);
      const contentType = payload.contentType === null || payload.contentType === undefined
        ? null
        : String(payload.contentType);
      const responseContentType = xhr._overrideMimeType || contentType;
      xhr._status = Number(payload.status) || 0;
      xhr._statusText = payload.statusText === undefined ? String(xhr._status) : String(payload.statusText);
      xhr._responseURL = payload.url === undefined ? "" : String(payload.url);
      xhr._responseContentType = responseContentType;
      xhr._responseHeaders = responseHeaders(payload.headers, contentType);
      const value = responseType === "json"
        ? JSON.parse(utf8TextFromBytes(bytes))
        : responseType === "arraybuffer"
          ? new Uint8Array(bytes).buffer
          : responseType === "blob"
            ? responseBodyBlobFromBytes(bytes, responseContentType)
            : utf8TextFromBytes(bytes);
      const xmlContent = typeof globalThis.__glassIsXmlMime === "function"
        && globalThis.__glassIsXmlMime(responseContentType);
      const htmlContent = typeof globalThis.__glassIsHtmlMime === "function"
        && globalThis.__glassIsHtmlMime(responseContentType);
      if (responseType === "document") {{
        const parseDocument = xmlContent && typeof globalThis.__glassParseXmlDocument === "function"
          ? globalThis.__glassParseXmlDocument
          : htmlContent && typeof globalThis.__glassParseHtmlDocument === "function"
            ? globalThis.__glassParseHtmlDocument
            : null;
        xhr._responseXML = parseDocument
          ? parseDocument(htmlContent
            ? nativeXhrDecodeDocumentBytes(
              bytes,
              xhr._responseHeaders.get("content-type"),
              xhr._overrideMimeType,
            )
            : utf8TextFromBytes(bytes), xhr.responseURL, responseContentType)
          : null;
        xhr._responseText = "";
        xhr._response = xhr._responseXML;
      }} else if (responseType === "" && xmlContent) {{
        xhr._responseXML = typeof globalThis.__glassParseXmlDocument === "function"
          ? globalThis.__glassParseXmlDocument(utf8TextFromBytes(bytes), xhr.responseURL, responseContentType)
          : null;
        xhr._responseText = utf8TextFromBytes(bytes);
        xhr._response = value;
      }} else {{
        xhr._responseXML = null;
        xhr._responseText = typeof value === "string" ? value : "";
        xhr._response = value;
      }}
    }} catch (error) {{
      fail(error, "error");
      return;
    }}
    xhr._controller = null;
    xhr._responseReader = null;
    xhr._sent = false;
    xhr._readyState = 4;
    xhr._notifyReadyState();
    nativeXhrFinishUpload(xhr, "load");
    nativeXhrDispatch(xhr, "load");
    nativeXhrDispatch(xhr, "loadend");
  }};
  const XMLHttpRequestNative = function() {{
    EventTargetNative.call(this);
    this._readyState = 0;
    this._status = 0;
    this._statusText = "";
    this._responseText = "";
    this._responseURL = "";
    this._response = "";
    this._responseXML = null;
    this._responseType = "";
    this._overrideMimeType = null;
    this._withCredentials = false;
    for (const type of nativeXhrEventTypes) installEventHandlerProperty(this, type);
    this._method = "GET";
    this._url = "";
    this._async = true;
    this._headers = new HeadersNative();
    this._responseContentType = null;
    this._responseHeaders = responseHeaders([], null);
    this._controller = null;
    this._responseReader = null;
    this._responseUtf8Pending = [];
    this._aborted = false;
    this._sent = false;
    this._timeout = 0;
    this._upload = new NativeXMLHttpRequestUpload();
    this._uploadStarted = false;
    this._uploadFinished = false;
    this._uploadTotal = 0;
    this._uploadLoaded = 0;
  }};
  XMLHttpRequestNative.prototype._notifyReadyState = function() {{
    nativeXhrDispatch(this, "readystatechange");
  }};
  Object.defineProperties(XMLHttpRequestNative.prototype, {{
    readyState: {{ configurable: true, get() {{ return this._readyState; }} }},
    status: {{ configurable: true, get() {{ return this._status; }} }},
    statusText: {{ configurable: true, get() {{ return this._statusText; }} }},
    responseURL: {{ configurable: true, get() {{ return this._responseURL; }} }},
    response: {{ configurable: true, get() {{ return this._response; }} }},
    upload: {{ configurable: true, get() {{ return this._upload; }} }},
  }});
  Object.defineProperty(XMLHttpRequestNative.prototype, "timeout", {{
    get() {{ return this._timeout; }},
    set(value) {{
      if (!this._async)
        throw new DOMExceptionNative("native synchronous XMLHttpRequest cannot use timeout", "InvalidAccessError");
      const numeric = Number(value);
      if (!Number.isFinite(numeric) || numeric < 0 || numeric > {max_native_xhr_timeout_ms})
        throw new RangeError("native XMLHttpRequest timeout is outside the bounded range");
      this._timeout = Math.trunc(numeric);
    }},
  }});
  Object.defineProperty(XMLHttpRequestNative.prototype, "withCredentials", {{
    configurable: true,
    get() {{ return this._withCredentials; }},
    set(value) {{
      if ((this.readyState !== 0 && this.readyState !== 1) || this._sent)
        throw new DOMExceptionNative("native XMLHttpRequest credentials cannot change after opening", "InvalidStateError");
      this._withCredentials = Boolean(value);
    }},
  }});
  Object.defineProperty(XMLHttpRequestNative.prototype, "responseText", {{
    configurable: true,
    get() {{
      const responseType = String(this.responseType || "").toLowerCase();
      if (!["", "text"].includes(responseType))
        throw new DOMExceptionNative("native XMLHttpRequest responseText is unavailable for this response type", "InvalidStateError");
      return this._responseText;
    }},
  }});
  Object.defineProperty(XMLHttpRequestNative.prototype, "responseType", {{
    configurable: true,
    get() {{ return this._responseType; }},
    set(value) {{
      if (this.readyState === 3 || this.readyState === 4)
        throw new DOMExceptionNative("native XMLHttpRequest responseType cannot change after loading", "InvalidStateError");
      if (!this._async)
        throw new DOMExceptionNative("native synchronous XMLHttpRequest cannot set responseType", "InvalidAccessError");
      const normalized = String(value).toLowerCase();
      if (!["", "text", "json", "arraybuffer", "blob", "document"].includes(normalized))
        throw new TypeError("native XMLHttpRequest responseType is unsupported");
      this._responseType = normalized;
    }},
  }});
  Object.defineProperty(XMLHttpRequestNative.prototype, "responseXML", {{
    configurable: true,
    get() {{
      const responseType = String(this.responseType || "").toLowerCase();
      if (!["", "document"].includes(responseType))
        throw new DOMExceptionNative("native XMLHttpRequest responseXML is unavailable for this response type", "InvalidStateError");
      return this.readyState === 4 ? this._responseXML : null;
    }},
  }});
  XMLHttpRequestNative.prototype.overrideMimeType = function(value) {{
    if (this.readyState === 3 || this.readyState === 4)
      throw new DOMExceptionNative("native XMLHttpRequest MIME type cannot change after loading", "InvalidStateError");
    this._overrideMimeType = nativeXhrNormalizeOverrideMimeType(value);
  }};
  XMLHttpRequestNative.prototype.open = function(method, url, async, username, password) {{
    const normalizedMethod = nativeXhrOpenMethod(method);
    const resolvedUrl = nativeXhrOpenUrl(url, username, password);
    const nextAsync = async === undefined ? true : Boolean(async);
    if (!nextAsync && (this._timeout !== 0 || this._responseType !== ""))
      throw new DOMExceptionNative("native synchronous XMLHttpRequest configuration is invalid", "InvalidAccessError");
    const previousController = this._controller;
    const previousReader = this._responseReader;
    this._controller = null;
    this._responseReader = null;
    if (previousController) previousController.abort();
    if (previousReader) {{ try {{ previousReader.cancel(); }} catch (_) {{}} }}
    this._method = normalizedMethod;
    this._url = resolvedUrl;
    this._async = nextAsync;
    this._headers = new HeadersNative();
    this._controller = null;
    this._aborted = false;
    this._sent = false;
    this._status = 0;
    this._statusText = "";
    this._responseText = "";
    this._responseUtf8Pending = [];
    this._responseURL = "";
    this._response = "";
    this._responseXML = null;
    this._responseContentType = null;
    this._responseHeaders = responseHeaders([], null);
    this._uploadStarted = false;
    this._uploadFinished = false;
    this._uploadTotal = 0;
    this._uploadLoaded = 0;
    this._readyState = 1;
    this._notifyReadyState();
  }};
  XMLHttpRequestNative.prototype.setRequestHeader = function(name, value) {{
    if (this.readyState !== 1 || this._sent)
      throw new TypeError("native XMLHttpRequest is not open");
    this._headers.append(name, value);
  }};
  XMLHttpRequestNative.prototype.abort = function() {{
    const active = this.readyState !== 0 && this.readyState !== 4;
    const controller = this._controller;
    const reader = this._responseReader;
    this._aborted = true;
    this._controller = null;
    this._responseReader = null;
    this._sent = false;
    if (controller) controller.abort();
    if (reader) {{ try {{ reader.cancel(); }} catch (_) {{}} }}
    if (!active) return;
    this._readyState = 0;
    this._status = 0;
    this._statusText = "";
    this._responseText = "";
    this._responseUtf8Pending = [];
    this._responseURL = "";
    this._response = "";
    this._responseXML = null;
    this._responseContentType = null;
    this._responseHeaders = responseHeaders([], null);
    this._notifyReadyState();
    nativeXhrFinishUpload(this, "abort");
    nativeXhrDispatch(this, "abort");
    nativeXhrDispatch(this, "loadend");
  }};
  XMLHttpRequestNative.prototype.getResponseHeader = function(name) {{
    if (this.readyState < 2 || this.readyState === 0) return null;
    return this._responseHeaders.get(name);
  }};
  XMLHttpRequestNative.prototype.getAllResponseHeaders = function() {{
    if (this.readyState < 2 || this.readyState === 0) return "";
    return Array.from(this._responseHeaders.entries())
      .map(entry => entry[0] + ": " + entry[1] + "\r\n")
      .join("");
  }};
  XMLHttpRequestNative.prototype.send = function(body) {{
    if (this.readyState !== 1 || this._sent) throw new TypeError("native XMLHttpRequest is not open");
    const responseType = String(this.responseType || "").toLowerCase();
    if (!["", "text", "json", "arraybuffer", "blob", "document"].includes(responseType)) throw new TypeError("native XMLHttpRequest responseType is unsupported");
    const requestBody = body && (body.__glassFormData === true || body.__glassUrlSearchParams === true || body.__glassNativeBlob === true)
      ? body
      : body instanceof ArrayBuffer || (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(body))
        ? body
        : body === undefined || body === null ? null : String(body);
    if (!this._async) {{
      if (this._timeout !== 0)
        throw new DOMExceptionNative("native synchronous XMLHttpRequest cannot use timeout", "InvalidAccessError");
      const request = nativeSyncXhrRequestPayload(this, requestBody);
      this._sent = true;
      nativeXhrStartUpload(this, requestBody);
      this._aborted = false;
      const objectUrlEntry = nativeObjectUrlRegistry.get(this._url);
      if (objectUrlEntry && nativeBodylessMethods.includes(this._method) && requestBody === null) {{
        const objectBytes = this._method === "HEAD" ? [] : blobBytes(objectUrlEntry.blob);
        const objectContentType = objectUrlEntry.blob.type || null;
        const objectHeaders = [["content-length", String(objectUrlEntry.blob.size)]];
        if (objectContentType) objectHeaders.unshift(["content-type", objectContentType]);
        nativeXhrApplySyncResponse(this, {{
          error: false,
          url: this._url,
          status: 200,
          statusText: "OK",
          contentType: objectContentType,
          headers: objectHeaders,
          bodyBase64: encodeBase64(objectBytes),
        }}, responseType);
        return;
      }}
      let payload;
      try {{
        payload = JSON.parse(globalThis.__glassSyncXhr(JSON.stringify(request)));
      }} catch (error) {{
        this._sent = false;
        nativeXhrFinishUpload(this, "error");
        throw error;
      }}
      if (typeof payload.documentCookie === "string"
          && typeof globalThis.__glassApplySyncCookieProjection === "function")
        globalThis.__glassApplySyncCookieProjection(payload.documentCookie);
      nativeXhrApplySyncResponse(this, payload, responseType);
      return;
    }}
    this._sent = true;
    nativeXhrStartUpload(this, requestBody);
    const controller = new AbortControllerNative();
    this._controller = controller;
    this._aborted = false;
    const request = fetchNative(this._url, {{
      method: this._method,
      body: requestBody,
      headers: this._headers,
      credentials: this.withCredentials ? "include" : "same-origin",
      signal: controller.signal,
      __glassTimeoutMs: this._timeout,
    }});
    request.then(response => {{
      if (this._controller !== controller || this._aborted) return null;
      this._status = response.status;
      this._statusText = response.statusText;
      this._responseURL = response.url;
      this._responseContentType = this._overrideMimeType || response.headers.get("content-type");
      this._responseHeaders = response.headers;
      this._readyState = 2;
      this._notifyReadyState();
      return nativeXhrReadResponse(this, response, responseType, controller);
    }}).then(bytes => {{
      if (bytes === null || this._controller !== controller || this._aborted) return;
      this._controller = null;
      const value = responseType === "json"
        ? JSON.parse(utf8TextFromBytes(bytes))
        : responseType === "arraybuffer"
          ? new Uint8Array(bytes).buffer
          : responseType === "blob"
            ? responseBodyBlobFromBytes(bytes, this._responseContentType)
            : utf8TextFromBytes(bytes);
      const xmlContent = typeof globalThis.__glassIsXmlMime === "function"
        && globalThis.__glassIsXmlMime(this._responseContentType);
      const htmlContent = typeof globalThis.__glassIsHtmlMime === "function"
        && globalThis.__glassIsHtmlMime(this._responseContentType);
      if (responseType === "document") {{
        const parseDocument = xmlContent && typeof globalThis.__glassParseXmlDocument === "function"
          ? globalThis.__glassParseXmlDocument
          : htmlContent && typeof globalThis.__glassParseHtmlDocument === "function"
            ? globalThis.__glassParseHtmlDocument
            : null;
        this._responseXML = parseDocument
          ? parseDocument(htmlContent
            ? nativeXhrDecodeDocumentBytes(
              bytes,
              this._responseHeaders.get("content-type"),
              this._overrideMimeType,
            )
            : utf8TextFromBytes(bytes), this.responseURL, this._responseContentType)
          : null;
        this._responseText = "";
        this._response = this._responseXML;
      }} else if (responseType === "" && xmlContent) {{
        this._responseXML = typeof globalThis.__glassParseXmlDocument === "function"
          ? globalThis.__glassParseXmlDocument(utf8TextFromBytes(bytes), this.responseURL, this._responseContentType)
          : null;
        this._responseText = utf8TextFromBytes(bytes);
        this._response = value;
      }} else {{
        this._responseXML = null;
        this._responseText = typeof value === "string" ? value : "";
        this._response = value;
      }}
      this._sent = false;
      this._readyState = 4;
      this._notifyReadyState();
      nativeXhrFinishUpload(this, "load");
      nativeXhrDispatch(this, "load");
      nativeXhrDispatch(this, "loadend");
    }}).catch(error => {{
      if (this._controller !== controller || this._aborted) return;
      this._controller = null;
      if (error && error.name === "TimeoutError") {{
      this._sent = false;
      this._status = 0;
      this._statusText = "";
      this._responseText = "";
      this._responseUtf8Pending = [];
      this._responseURL = "";
        this._response = "";
        this._responseXML = null;
      this._responseContentType = null;
        this._responseHeaders = responseHeaders([], null);
        this._readyState = 4;
        this._notifyReadyState();
        nativeXhrFinishUpload(this, "timeout");
        nativeXhrDispatch(this, "timeout");
        nativeXhrDispatch(this, "loadend");
        return;
      }}
      this._sent = false;
      this._readyState = 4;
      this._notifyReadyState();
      nativeXhrFinishUpload(this, "error");
      nativeXhrDispatch(this, "error", {{ error }});
      nativeXhrDispatch(this, "loadend");
    }});
  }};
  XMLHttpRequestNative.UNSENT = 0;
  XMLHttpRequestNative.OPENED = 1;
  XMLHttpRequestNative.HEADERS_RECEIVED = 2;
  XMLHttpRequestNative.LOADING = 3;
  XMLHttpRequestNative.DONE = 4;
  XMLHttpRequestNative.prototype.UNSENT = 0;
  XMLHttpRequestNative.prototype.OPENED = 1;
  XMLHttpRequestNative.prototype.HEADERS_RECEIVED = 2;
  XMLHttpRequestNative.prototype.LOADING = 3;
  XMLHttpRequestNative.prototype.DONE = 4;
  globalThis.XMLHttpRequest = XMLHttpRequestNative;
  globalThis.__glassFetchRequests = fetchRequests;
  globalThis.__glassNextFetchRequestId = nextFetchRequestId;
  globalThis.fetch = fetchNative;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("module_response_xhr");
