  const responseHeaders = (rawEntries, contentType) => {{
    const entries = [];
    const byName = new Map();
    if (Array.isArray(rawEntries)) for (const rawEntry of rawEntries) {{
      if (!Array.isArray(rawEntry) || rawEntry.length !== 2) continue;
      const rawName = String(rawEntry[0]);
      const name = rawName.toLowerCase();
      const value = String(rawEntry[1]);
      if (!requestHeaderNameNative.test(rawName)
          || name.length > {fetch_header_name_limit}
          || value.length > {fetch_header_value_limit}
          || /[\u0000-\u0008\u000a-\u001f\u007f]/.test(value)) continue;
      const existing = byName.get(name);
      if (existing) existing[1] += ", " + value;
      else {{
        const entry = [name, value];
        byName.set(name, entry);
        entries.push(entry);
      }}
    }}
    if (!byName.has("content-type") && contentType !== null && contentType !== undefined) {{
      const entry = ["content-type", String(contentType)];
      byName.set("content-type", entry);
      entries.push(entry);
    }}
    entries.sort((left, right) => left[0] < right[0] ? -1 : left[0] > right[0] ? 1 : 0);
    const iterator = values => values[Symbol.iterator]();
    const responseHeaderName = (name) => {{
      const value = String(name);
      const normalized = value.toLowerCase();
      if (!requestHeaderNameNative.test(value) || normalized.length > {fetch_header_name_limit})
        throw new DOMExceptionNative("native response header name is invalid", "SyntaxError");
      return normalized;
    }};
    const headers = Object.create(HeadersNative.prototype);
    Object.defineProperty(headers, "__glassHeaders", {{ value: true }});
    Object.defineProperty(headers, "_entries", {{ value: entries }});
    const readOnly = () => {{ throw new TypeError("native response Headers are immutable"); }};
    headers.append = readOnly;
    headers.set = readOnly;
    headers.delete = readOnly;
    Object.assign(headers, {{
      get(name) {{
        const key = responseHeaderName(name);
        const entry = entries.find(candidate => candidate[0] === key);
        return entry ? entry[1] : null;
      }},
      has(name) {{ return this.get(name) !== null; }},
      entries() {{ return iterator(entries.map(entry => [entry[0], entry[1]])); }},
      keys() {{ return iterator(entries.map(entry => entry[0])); }},
      values() {{ return iterator(entries.map(entry => entry[1])); }},
      forEach(callback, thisArg) {{
        if (typeof callback !== "function") throw new TypeError("native response header callback must be callable");
        entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], headers));
      }},
      [Symbol.iterator]() {{ return this.entries(); }},
    }});
    return Object.freeze(headers);
  }};
  const fetchStreamGroups = globalThis.__glassFetchStreamGroups instanceof Map
    ? globalThis.__glassFetchStreamGroups
    : new Map();
  const fetchUploadGroups = globalThis.__glassFetchUploadGroups instanceof Map
    ? globalThis.__glassFetchUploadGroups
    : new Map();
  const fetchUploadGroup = (streamId) => {{
    const id = Number(streamId);
    if (!Number.isSafeInteger(id) || id <= 0)
      throw new TypeError("native fetch upload stream identifier is invalid");
    let group = fetchUploadGroups.get(id);
    if (!group) {{
      group = {{
        id,
        reader: null,
        streamState: null,
        demandPending: false,
        done: false,
        cancelled: false,
        error: null,
        totalBytes: 0,
      }};
      fetchUploadGroups.set(id, group);
    }}
    return group;
  }};
  const releaseFetchUploadReader = (group) => {{
    if (!group || !group.reader) return;
    const reader = group.reader;
    group.reader = null;
    if (group.streamState) group.streamState.consumingRequestBody = false;
    try {{ reader.releaseLock(); }} catch (_) {{}}
  }};
  const startFetchUpload = (stream, streamId) => {{
    const state = readableStreamState(stream);
    if (state.locked) throw new TypeError("native fetch request body stream is locked");
    if (!state.consumedByRequest) {{
      if (state.disturbed) throw new TypeError("native fetch request body stream is unusable");
      state.consumedByRequest = true;
      markReadableStreamDisturbed(state);
    }}
    if (state.consumedByResponse) throw new TypeError("native fetch request body stream is unusable");
    const group = fetchUploadGroup(streamId);
    if (group.reader || group.done) throw new TypeError("native fetch upload stream is already active");
    state.consumingRequestBody = true;
    try {{ group.reader = stream.getReader(); }}
    catch (error) {{
      state.consumingRequestBody = false;
      throw error;
    }}
    group.streamState = state;
    return group;
  }};
  const cancelFetchUploadGroup = (group, reason) => {{
    if (!group || group.done) return;
    group.cancelled = true;
    group.done = true;
    group.error = reason === undefined ? "native fetch request body was cancelled" : String(reason);
    const reader = group.reader;
    if (group.streamState) group.streamState.consumingRequestBody = false;
    group.reader = null;
    if (reader) {{
      try {{ Promise.resolve(reader.cancel(reason)).catch(() => {{}}); }} catch (_) {{}}
    }}
    pushCommand({{ kind: "fetchUploadCancel", stream_id: Number(group.id) }});
  }};
  const fetchStreamGroup = (streamId) => {{
    const id = Number(streamId);
    if (!Number.isSafeInteger(id) || id <= 0) throw new TypeError("native fetch stream identifier is invalid");
    let group = fetchStreamGroups.get(id);
    if (!group) {{
      group = {{ id, chunks: [], totalBytes: 0, done: false, error: null, streams: [], waiters: [], started: false, readRequested: false, cancelRequested: false }};
      fetchStreamGroups.set(id, group);
    }}
    return group;
  }};
  const fetchStreamCanAcceptDemand = (group) => group && group.streams
    .filter(state => !state.cancelled && !state.consumedByResponse)
    .every(state => state.queued.length < {fetch_stream_queue_limit});
  const requestFetchStreamRead = (group) => {{
    if (!group || group.done || group.error !== null || group.cancelRequested || group.readRequested || !fetchStreamCanAcceptDemand(group)) return;
    group.readRequested = true;
    pushCommand({{ kind: "fetchStreamRead", stream_id: Number(group.id) }});
  }};
  const cancelFetchStreamGroup = (group) => {{
    if (group.done || group.cancelRequested) return;
    group.cancelRequested = true;
    group.done = true;
    for (const state of group.streams) {{
      state.done = true;
      if (state.pendingRead) {{ state.pendingRead.resolve({{ value: undefined, done: true }}); state.pendingRead = null; }}
      settleReadableStreamClosed(state);
    }}
    settleFetchStreamWaiters(group);
    pushCommand({{ kind: "fetchStreamCancel", stream_id: Number(group.id) }});
  }};
  const maybeCancelFetchStreamGroup = (group) => {{
    if (!group) return;
    if (group.streams.some(state => !state.cancelled && !state.consumedByResponse) || group.waiters.length > 0) return;
    cancelFetchStreamGroup(group);
  }};
  const flattenFetchStream = (group) => {{
    const bytes = [];
    for (const chunk of group.chunks) bytes.push(...chunk);
    return bytes;
  }};
  const settleFetchStreamWaiters = (group) => {{
    const waiters = group.waiters.splice(0);
    const bytes = flattenFetchStream(group);
    for (const waiter of waiters) waiter.resolve(bytes.slice());
  }};
  const rejectFetchStreamWaiters = (group) => {{
    const waiters = group.waiters.splice(0);
    const error = new Error(group.error || "native fetch response stream failed");
    for (const waiter of waiters) waiter.reject(error);
  }};
  const readableStreamState = (stream) => {{
    if (!stream || stream.__glassReadableStream !== true || !stream._state)
      throw new TypeError("native ReadableStream receiver is invalid");
    return stream._state;
  }};
  const readableStreamStrategy = (strategy, byteMode) => {{
    const value = strategy === undefined || strategy === null ? {{}} : strategy;
    if (typeof value !== "object" && typeof value !== "function")
      throw new TypeError("native ReadableStream strategy is invalid");
    let highWaterMark = value.highWaterMark === undefined
      ? {fetch_stream_queue_limit}
      : Number(value.highWaterMark);
    if (!Number.isFinite(highWaterMark) || highWaterMark < 0)
      throw new RangeError("native ReadableStream highWaterMark is invalid");
    highWaterMark = Math.min(highWaterMark, {fetch_stream_queue_limit});
    const size = value.size === undefined
      ? (chunk => byteMode
        ? (chunk instanceof ArrayBuffer || ArrayBuffer.isView(chunk) ? chunk.byteLength : 1)
        : 1)
      : value.size;
    if (typeof size !== "function")
      throw new TypeError("native ReadableStream size algorithm is invalid");
    return {{ highWaterMark, size }};
  }};
  const readableByteView = (value) => {{
    if (value instanceof ArrayBuffer)
      return new Uint8Array(value.slice(0));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value))
      return new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();
    throw new TypeError("native byte stream chunks must be ArrayBuffer or views");
  }};
  const readableValueByteLength = (value) => value instanceof ArrayBuffer
    ? value.byteLength
    : typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value)
      ? value.byteLength
      : typeof value === "string"
        ? blobUtf8Bytes(value).length
        : 1;
  const readableChunkSize = (state, value) => {{
    const size = Number(state.strategy.size(value));
    if (!Number.isFinite(size) || size < 0)
      throw new RangeError("native ReadableStream chunk size is invalid");
    return size;
  }};
  const readableByobResultView = (view, byteLength) => {{
    const bytesPerElement = Number(view.BYTES_PER_ELEMENT) || 1;
    const elements = Math.floor(byteLength / bytesPerElement);
    if (typeof view.subarray === "function") return view.subarray(0, elements);
    return new Uint8Array(view.buffer, view.byteOffset, elements * bytesPerElement);
  }};
  const clearReadablePendingRead = (state) => {{
    const pending = state.pendingRead;
    state.pendingRead = null;
    state.byobRequest = null;
    return pending;
  }};
  const resolveReadablePending = (state, value, done) => {{
    const pending = state.pendingRead;
    if (!pending) return false;
    if (pending.byobView) {{
      if (done) {{
        const current = clearReadablePendingRead(state);
        current.resolve({{ value: readableByobResultView(current.byobView, 0), done: true }});
        return true;
      }}
      let bytes;
      try {{ bytes = readableByteView(value); }}
      catch (error) {{
        const current = clearReadablePendingRead(state);
        current.reject(error);
        return true;
      }}
      const bytesPerElement = Number(pending.byobView.BYTES_PER_ELEMENT) || 1;
      const capacity = Math.floor(pending.byobView.byteLength / bytesPerElement) * bytesPerElement;
      const count = Math.min(bytes.length, capacity);
      if (count === 0) return false;
      new Uint8Array(pending.byobView.buffer, pending.byobView.byteOffset, count)
        .set(bytes.subarray(0, count));
      if (count < bytes.length) {{
        const remainder = bytes.slice(count);
        state.queued.unshift(remainder);
        const remainderSize = readableChunkSize(state, remainder);
        state.queueSizes.unshift(remainderSize);
        state.queueSize += remainderSize;
      }}
      const current = clearReadablePendingRead(state);
      current.resolve({{ value: readableByobResultView(current.byobView, count), done: false }});
      return true;
    }}
    const current = clearReadablePendingRead(state);
    current.resolve({{
      value: value === undefined ? undefined : state.byteMode ? new Uint8Array(readableByteView(value)) : value,
      done: Boolean(done),
    }});
    return true;
  }};
  const readableByobRequest = (state) => {{
    const pending = state.pendingRead;
    if (!pending || !pending.byobView || state.done || state.cancelled || state.error !== null)
      return null;
    if (state.byobRequest) return state.byobRequest;
    const request = {{
      get view() {{ return pending.byobView; }},
      respond(bytesWritten) {{
        const count = Number(bytesWritten);
        if (!Number.isInteger(count) || count < 0 || count > pending.byobView.byteLength)
          throw new RangeError("native ReadableStream BYOB response is invalid");
        if (count === 0 && !state.done)
          throw new TypeError("native ReadableStream BYOB response is empty");
        state.sourceProduced = true;
        const current = clearReadablePendingRead(state);
        current.resolve({{ value: readableByobResultView(current.byobView, count), done: false }});
      }},
      respondWithNewView(view) {{
        if (!view || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(view))
          throw new TypeError("native ReadableStream BYOB view is invalid");
        const bytes = readableByteView(view);
        if (bytes.byteLength > pending.byobView.byteLength)
          throw new RangeError("native ReadableStream BYOB view exceeds its request");
        new Uint8Array(pending.byobView.buffer, pending.byobView.byteOffset, bytes.byteLength)
          .set(bytes);
        state.sourceProduced = true;
        const current = clearReadablePendingRead(state);
        current.resolve({{ value: readableByobResultView(current.byobView, bytes.byteLength), done: false }});
      }},
    }};
    state.byobRequest = Object.freeze(request);
    return state.byobRequest;
  }};
  const settleReadableStreamClosed = (state) => {{
    if (!state.closedWaiters || state.closedWaiters.length === 0) return;
    const waiters = state.closedWaiters.splice(0);
    if (state.error !== null) {{
      const error = new Error(state.error);
      for (const waiter of waiters) waiter.reject(error);
    }} else if (readableStreamDone(state)) {{
      for (const waiter of waiters) waiter.resolve();
    }} else {{
      state.closedWaiters.push(...waiters);
    }}
  }};
  const watchReadableStreamClosed = (state, resolve, reject) => {{
    if (state.error !== null) {{
      reject(new Error(state.error));
    }} else if (readableStreamDone(state)) {{
      resolve();
    }} else {{
      state.closedWaiters.push({{ resolve, reject }});
    }}
  }};
  const markReadableStreamDisturbed = (state) => {{
    if (state.disturbed) return;
    state.disturbed = true;
    if (typeof state.onDisturb === "function") state.onDisturb();
  }};
  const failReadableStreamSource = (state, error) => {{
    if (state.done || state.cancelled || state.error !== null) return;
    state.error = error instanceof Error ? error.message : String(error);
    state.done = true;
    if (state.pendingRead) {{
      const pending = clearReadablePendingRead(state);
      pending.reject(new Error(state.error));
    }}
    settleReadableStreamClosed(state);
  }};
  const closeReadableStreamSource = (state) => {{
    if (state.done || state.cancelled || state.error !== null) return;
    state.done = true;
    if (state.pendingRead) {{
      resolveReadablePending(state, undefined, true);
    }}
    settleReadableStreamClosed(state);
  }};
  const enqueueReadableStreamSource = (state, value) => {{
    if (state.done || state.cancelled || state.error !== null)
      throw new TypeError("native ReadableStream controller is closed");
    state.sourceProduced = true;
    const queuedValue = state.byteMode ? readableByteView(value) : value;
    const byteLength = readableValueByteLength(queuedValue);
    if (byteLength > storageValueLimit) throw new RangeError("native ReadableStream chunk exceeds its limit");
    const size = readableChunkSize(state, queuedValue);
    if (size > storageValueLimit) throw new RangeError("native ReadableStream chunk exceeds its limit");
    if (state.pendingRead) {{
      if (resolveReadablePending(state, queuedValue, false)) return;
    }} else {{
      if (state.queued.length >= {fetch_stream_queue_limit})
        throw new RangeError("native ReadableStream queue limit exceeded");
      state.queued.push(queuedValue);
      state.queueSizes.push(size);
      state.queueSize += size;
    }}
  }};
  const pullReadableStreamSource = (state) => {{
    const source = state.underlyingSource;
    if (!source || state.sourceStarting || state.sourcePulling || state.done || state.cancelled || state.error !== null)
      return;
    if (typeof source.pull !== "function") return;
    state.sourceProduced = false;
    state.sourcePulling = true;
    let result;
    try {{
      result = source.pull(state.sourceController);
    }} catch (error) {{
      state.sourcePulling = false;
      failReadableStreamSource(state, error);
      return;
    }}
    Promise.resolve(result).then(
      () => {{
        const produced = state.sourceProduced;
        state.sourceProduced = false;
        state.sourcePulling = false;
        if (produced && state.pendingRead && !state.done && !state.cancelled && state.error === null)
          pullReadableStreamSource(state);
      }},
      error => {{ state.sourcePulling = false; failReadableStreamSource(state, error); }},
    );
  }};
  const startReadableStreamSource = (state) => {{
    const source = state.underlyingSource;
    if (!source || typeof source.start !== "function") {{
      state.sourceStarting = false;
      return;
    }}
    let result;
    try {{
      result = source.start(state.sourceController);
    }} catch (error) {{
      state.sourceStarting = false;
      failReadableStreamSource(state, error);
      return;
    }}
    Promise.resolve(result).then(
      () => {{ state.sourceStarting = false; pullReadableStreamSource(state); }},
      error => {{ state.sourceStarting = false; failReadableStreamSource(state, error); }},
    );
  }};
  const cancelReadableStreamSource = (state, reason) => {{
    if (state.cancelled) return Promise.resolve(undefined);
    markReadableStreamDisturbed(state);
    state.cancelled = true;
    state.done = true;
    state.queued = [];
    state.queueSizes = [];
    state.queueSize = 0;
    if (state.pendingRead) {{
      resolveReadablePending(state, undefined, true);
    }}
    settleReadableStreamClosed(state);
    if (!state.underlyingSource || typeof state.underlyingSource.cancel !== "function")
      return Promise.resolve(undefined);
    try {{ return Promise.resolve(state.underlyingSource.cancel(reason)); }}
    catch (error) {{ return Promise.reject(error); }}
  }};
  const readableStreamDone = (state) => state.cancelled || state.done || state.error !== null;
  const responseBodyIsUsed = (response) => {{
    if (!response || !response.__glassBodyState) return false;
    const body = response.body;
    if (!body) return response.__glassBodyState.used === true;
    return response.__glassBodyState.used === true || readableStreamState(body).disturbed === true;
  }};
  const responseBodyUse = (response) => {{
    const body = response && response.body;
    if (!body) return true;
    const state = readableStreamState(body);
    if (responseBodyIsUsed(response) || state.locked) return false;
    response.__glassBodyState.used = true;
    markReadableStreamDisturbed(state);
    state.consumedByResponse = true;
    if (!response.__glassResponseStreamBody) state.queued = [];
    return true;
  }};
  const responseBodyUnusable = () => Promise.reject(new TypeError("native Response body is unusable"));
  const defineResponseBodyState = (response) => {{
    Object.defineProperty(response, "__glassBodyState", {{ value: {{ used: false }} }});
    Object.defineProperty(response, "bodyUsed", {{
      configurable: true,
      get() {{ return responseBodyIsUsed(response); }},
    }});
  }};
  const ReadableStreamNative = typeof globalThis.__glassReadableStreamConstructor === "function"
    ? globalThis.__glassReadableStreamConstructor
    : function(bytes, streamId, onDisturb) {{
    if (!(this instanceof ReadableStreamNative)) throw new TypeError("native ReadableStream requires new");
    const underlyingSource = bytes && typeof bytes === "object" && !Array.isArray(bytes)
      && (streamId === undefined || (streamId && typeof streamId === "object"))
      ? bytes
      : null;
    const streamOptions = underlyingSource && streamId && typeof streamId === "object" ? streamId : {{}};
    if (underlyingSource && underlyingSource.type !== undefined && underlyingSource.type !== "bytes")
      throw new TypeError("native ReadableStream type is unsupported");
    const byteMode = (underlyingSource && underlyingSource.type === "bytes")
      || (underlyingSource === null && Array.isArray(bytes))
      || (underlyingSource === null && streamId !== undefined && streamId !== null);
    const strategy = readableStreamStrategy(streamOptions, byteMode);
    const hostId = underlyingSource || streamId === undefined || streamId === null ? null : Number(streamId);
    const values = hostId === null && underlyingSource === null && Array.isArray(bytes) ? bytes.slice() : [];
    if (values.length > {fetch_stream_body_limit}) throw new RangeError("native ReadableStream body limit exceeded");
    for (const value of values) {{
      if (!Number.isInteger(value) || value < 0 || value > 255)
        throw new TypeError("native ReadableStream bytes are invalid");
    }}
    const group = hostId === null ? null : fetchStreamGroup(hostId);
    const queued = group ? group.chunks.map(chunk => chunk.slice()) : [];
    const queueSizes = queued.map(value => value.byteLength);
    const state = {{
      bytes: values,
      offset: 0,
      queued,
      queueSizes,
      queueSize: queueSizes.reduce((total, size) => total + size, 0),
      strategy,
      underlyingSource,
      sourceController: null,
      sourceStarting: Boolean(underlyingSource && typeof underlyingSource.start === "function"),
      sourcePulling: false,
      sourceProduced: false,
      locked: false,
      cancelled: false,
      disturbed: false,
      consumedByResponse: false,
      consumingResponseBody: false,
      consumedByRequest: false,
      consumingRequestBody: false,
      done: underlyingSource ? false : group ? group.done : true,
      error: group ? group.error : null,
      streamId: hostId,
      group,
      pendingRead: null,
      byobRequest: null,
      closedWaiters: [],
      onDisturb: typeof onDisturb === "function" ? onDisturb : null,
      byteMode,
    }};
    if (underlyingSource) {{
      state.sourceController = Object.freeze({{
        get desiredSize() {{ return state.strategy.highWaterMark - state.queueSize; }},
        get byobRequest() {{ return readableByobRequest(state); }},
        enqueue(value) {{ enqueueReadableStreamSource(state, value); }},
        close() {{ closeReadableStreamSource(state); }},
        error(error) {{ failReadableStreamSource(state, error); }},
      }});
    }}
    if (group) group.streams.push(state);
    Object.defineProperty(this, "__glassReadableStream", {{ value: true }});
    Object.defineProperty(this, "_state", {{ value: state }});
    if (underlyingSource) startReadableStreamSource(state);
    Object.freeze(this);
  }};
  Object.defineProperty(ReadableStreamNative.prototype, "locked", {{
    configurable: true,
    get() {{ return readableStreamState(this).locked; }},
  }});
  ReadableStreamNative.prototype.getReader = function(options) {{
    const state = readableStreamState(this);
    if (state.locked) throw new TypeError("native ReadableStream is already locked");
    const readerOptions = options === undefined || options === null ? {{}} : options;
    if (typeof readerOptions !== "object" && typeof readerOptions !== "function")
      throw new TypeError("native ReadableStream reader options are invalid");
    const mode = readerOptions.mode === undefined ? undefined : String(readerOptions.mode);
    if (mode !== undefined && mode !== "byob")
      throw new TypeError("native ReadableStream reader mode is unsupported");
    const byob = mode === "byob";
    if (byob && !state.byteMode)
      throw new TypeError("native BYOB reader requires a byte stream");
    state.locked = true;
    let released = false;
    let resolveClosed;
    let rejectClosed;
    const closedPromise = new Promise((resolve, reject) => {{
      resolveClosed = resolve;
      rejectClosed = reject;
    }});
    watchReadableStreamClosed(state, resolveClosed, rejectClosed);
    const release = () => {{
      if (released) return;
      released = true;
      state.locked = false;
    }};
    const readQueued = () => {{
      if (state.cancelled) return null;
      if (state.streamId === null && state.underlyingSource === null) {{
        if (state.offset >= state.bytes.length) return null;
        const value = new Uint8Array(state.bytes.slice(state.offset));
        state.offset = state.bytes.length;
        return {{ value, done: false }};
      }}
      if (state.queued.length === 0) return null;
      const value = state.queued.shift();
      state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
      return {{ value: state.byteMode ? new Uint8Array(value) : value, done: false }};
    }};
    const readByteQueued = (view) => {{
      if (state.cancelled) return null;
      const target = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
      if (state.streamId === null && state.underlyingSource === null) {{
        if (state.offset >= state.bytes.length) return null;
        const count = Math.min(target.byteLength, state.bytes.length - state.offset);
        target.set(state.bytes.slice(state.offset, state.offset + count));
        state.offset += count;
        return {{ value: readableByobResultView(view, count), done: false }};
      }}
      if (state.queued.length === 0) return null;
      const value = new Uint8Array(state.queued[0]);
      const count = Math.min(target.byteLength, value.byteLength);
      target.set(value.subarray(0, count));
      if (count === value.byteLength) {{
        state.queued.shift();
        state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
      }} else {{
        const remainder = value.slice(count);
        state.queued[0] = remainder;
        state.queueSize = Math.max(0, state.queueSize - state.queueSizes[0]);
        state.queueSizes[0] = readableChunkSize(state, remainder);
        state.queueSize += state.queueSizes[0];
      }}
      return {{ value: readableByobResultView(view, count), done: false }};
    }};
    const resolveQueuedRead = () => {{
      const pending = state.pendingRead;
      if (!pending) return false;
      const result = pending.byobView ? readByteQueued(pending.byobView) : readQueued();
      if (!result) return false;
      const current = clearReadablePendingRead(state);
      current.resolve(result);
      return true;
    }};
    const read = (view) => {{
      if (state.consumedByResponse && !state.consumingResponseBody) return responseBodyUnusable();
      if (state.consumedByRequest && !state.consumingRequestBody) return Promise.reject(new TypeError("native Request body is unusable"));
      if (byob) {{
        if (!view || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(view) || view.byteLength === 0)
          throw new TypeError("native BYOB read requires a non-empty view");
      }} else if (view !== undefined) {{
        throw new TypeError("native default reader read does not accept a view");
      }}
      markReadableStreamDisturbed(state);
      const queued = byob ? readByteQueued(view) : readQueued();
      if (queued) {{
        requestFetchStreamRead(state.group);
        return Promise.resolve(queued);
      }}
      if (state.error !== null) return Promise.reject(new Error(state.error));
      if (readableStreamDone(state)) {{
        return Promise.resolve(byob
          ? {{ value: readableByobResultView(view, 0), done: true }}
          : {{ value: undefined, done: true }});
      }}
      if (state.pendingRead) return Promise.reject(new TypeError("native ReadableStream read is already pending"));
      const promise = new Promise((resolve, reject) => {{ state.pendingRead = {{ resolve, reject, byobView: byob ? view : null }}; }});
      try {{
        if (state.underlyingSource) pullReadableStreamSource(state);
        else requestFetchStreamRead(state.group);
      }}
      catch (error) {{ state.pendingRead = null; return Promise.reject(error); }}
      return promise;
    }};
    const reader = {{
      read(view) {{
        if (released) return Promise.reject(new TypeError("native ReadableStream reader is released"));
        return read(view);
      }},
      cancel(reason) {{
        if (released) return Promise.reject(new TypeError("native ReadableStream reader is released"));
        if (state.underlyingSource) return cancelReadableStreamSource(state, reason);
        state.cancelled = true;
        markReadableStreamDisturbed(state);
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) resolveReadablePending(state, undefined, true);
        settleReadableStreamClosed(state);
        maybeCancelFetchStreamGroup(state.group);
        return Promise.resolve(undefined);
      }},
      releaseLock() {{ release(); }},
      return() {{
        if (released) return Promise.resolve({{ value: undefined, done: true }});
        if (state.underlyingSource) {{
          const cancellation = cancelReadableStreamSource(state);
          release();
          return cancellation.then(() => ({{ value: undefined, done: true }}));
        }}
        state.cancelled = true;
        markReadableStreamDisturbed(state);
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) resolveReadablePending(state, undefined, true);
        settleReadableStreamClosed(state);
        release();
        maybeCancelFetchStreamGroup(state.group);
        return Promise.resolve({{ value: undefined, done: true }});
      }},
      [Symbol.asyncIterator]() {{ return this; }},
    }};
    Object.defineProperty(reader, "closed", {{ value: closedPromise }});
    return Object.freeze(reader);
  }};
  ReadableStreamNative.prototype.cancel = function(reason) {{
    const state = readableStreamState(this);
    if (state.locked) return Promise.reject(new TypeError("native ReadableStream is locked"));
    if (state.underlyingSource) return cancelReadableStreamSource(state, reason);
    state.cancelled = true;
    markReadableStreamDisturbed(state);
    state.done = true;
    state.queued = [];
    state.queueSizes = [];
    state.queueSize = 0;
    settleReadableStreamClosed(state);
    maybeCancelFetchStreamGroup(state.group);
    return Promise.resolve(undefined);
  }};
  ReadableStreamNative.prototype[Symbol.asyncIterator] = function() {{
    const reader = this.getReader();
    return Object.freeze({{
      next() {{ return reader.read(); }},
      return() {{ return reader.return(); }},
      [Symbol.asyncIterator]() {{ return this; }},
    }});
  }};
  ReadableStreamNative.prototype.tee = function() {{
    const sourceState = readableStreamState(this);
    if (sourceState.locked) throw new TypeError("native ReadableStream is locked");
    markReadableStreamDisturbed(sourceState);
    const upstreamReader = this.getReader();
    const tee = {{
      upstreamReader,
      reading: false,
      done: false,
      error: null,
      upstreamReleased: false,
      branches: [],
    }};
    const releaseUpstream = () => {{
      if (tee.upstreamReleased) return;
      tee.upstreamReleased = true;
      tee.upstreamReader.releaseLock();
    }};
    const cancelUpstream = reason => {{
      if (tee.upstreamReleased) return Promise.resolve(undefined);
      tee.done = true;
      return Promise.resolve(tee.upstreamReader.cancel(reason)).then(
        () => {{ releaseUpstream(); }},
        error => {{ releaseUpstream(); throw error; }},
      );
    }};
    const failTee = error => {{
      if (tee.error !== null) return Promise.resolve(undefined);
      const normalized = error instanceof Error ? error : new Error(String(error));
      tee.error = normalized.message;
      tee.done = true;
      for (const branch of tee.branches) {{
        if (!branch.cancelled && branch.controller) branch.controller.error(normalized);
      }}
      return cancelUpstream(normalized).catch(() => undefined);
    }};
    const finishTee = () => {{
      if (tee.done) return;
      tee.done = true;
      for (const branch of tee.branches) {{
        if (!branch.cancelled && branch.controller) branch.controller.close();
      }}
      releaseUpstream();
    }};
    const maybePullTee = () => {{
      if (tee.reading || tee.done || tee.error !== null) return Promise.resolve(undefined);
      const activeBranches = tee.branches.filter(branch => !branch.cancelled);
      if (activeBranches.length === 0) return cancelUpstream();
      if (activeBranches.some(branch => branch.state && branch.state.queued.length >= {fetch_stream_queue_limit}))
        return Promise.resolve(undefined);
      tee.reading = true;
      return tee.upstreamReader.read().then(
        result => {{
          tee.reading = false;
          if (result.done) {{
            finishTee();
            return;
          }}
          for (const branch of tee.branches) {{
            if (branch.cancelled || !branch.controller) continue;
            try {{ branch.controller.enqueue(result.value); }}
            catch (error) {{ return failTee(error); }}
          }}
        }},
        error => {{
          tee.reading = false;
          return failTee(error);
        }},
      );
    }};
    const cancelBranch = (branch, reason) => {{
      if (branch.cancelled) return Promise.resolve(undefined);
      branch.cancelled = true;
      if (tee.branches.every(candidate => candidate.cancelled)) return cancelUpstream(reason);
      return Promise.resolve(undefined);
    }};
    const createBranch = () => {{
      const branch = {{ controller: null, stream: null, state: null, cancelled: false }};
      const branchSource = {{
        start(controller) {{ branch.controller = controller; }},
        pull() {{ return maybePullTee(); }},
        cancel(reason) {{ return cancelBranch(branch, reason); }},
      }};
      if (sourceState.byteMode) branchSource.type = "bytes";
      branch.stream = new ReadableStreamNative(branchSource);
      branch.state = readableStreamState(branch.stream);
      tee.branches.push(branch);
      return branch.stream;
    }};
    const branches = [createBranch(), createBranch()];
    return branches;
  }};
  const writableStreamState = (stream) => {{
    if (!stream || stream.__glassWritableStream !== true || !stream._state)
      throw new TypeError("native WritableStream receiver is invalid");
    return stream._state;
  }};
  const settleWritableStreamClosed = (state) => {{
    if (!state.closedWaiters || state.closedWaiters.length === 0) return;
    const waiters = state.closedWaiters.splice(0);
    if (state.error !== null) {{
      const error = new Error(state.error);
      for (const waiter of waiters) waiter.reject(error);
    }} else if (state.closed) {{
      for (const waiter of waiters) waiter.resolve();
    }} else {{
      state.closedWaiters.push(...waiters);
    }}
  }};
  const watchWritableStreamClosed = (state, resolve, reject) => {{
    if (state.error !== null) reject(new Error(state.error));
    else if (state.closed) resolve();
    else state.closedWaiters.push({{ resolve, reject }});
  }};
  const failWritableStream = (state, error) => {{
    if (state.closed) return;
    state.error = error instanceof Error ? error.message : String(error);
    state.closed = true;
    state.closing = true;
    settleWritableStreamClosed(state);
  }};
  const finishWritableStream = (state) => {{
    if (state.closed) return;
    state.closed = true;
    state.closing = true;
    settleWritableStreamClosed(state);
  }};
  const WritableStreamNative = function(underlyingSink) {{
    if (!(this instanceof WritableStreamNative)) throw new TypeError("native WritableStream requires new");
    const sink = underlyingSink === undefined || underlyingSink === null ? {{}} : underlyingSink;
    if (typeof sink !== "object" && typeof sink !== "function")
      throw new TypeError("native WritableStream sink is invalid");
    const state = {{
      sink,
      locked: false,
      closing: false,
      closed: false,
      error: null,
      writeQueue: 0,
      writeTail: Promise.resolve(undefined),
      closePromise: null,
      startPromise: null,
      aborted: false,
      closedWaiters: [],
    }};
    let startResult;
    try {{ startResult = typeof sink.start === "function" ? sink.start() : undefined; }}
    catch (error) {{
      state.error = error instanceof Error ? error.message : String(error);
      state.closed = true;
      state.closing = true;
      startResult = Promise.reject(error);
    }}
    state.startPromise = Promise.resolve(startResult).then(
      () => undefined,
      error => {{ failWritableStream(state, error); throw error; }},
    );
    state.startPromise.catch(() => undefined);
    Object.defineProperty(this, "__glassWritableStream", {{ value: true }});
    Object.defineProperty(this, "_state", {{ value: state }});
    Object.freeze(this);
  }};
  Object.defineProperty(WritableStreamNative.prototype, "locked", {{
    configurable: true,
    get() {{ return writableStreamState(this).locked; }},
  }});
  const writableStreamWrite = (state, chunk) => {{
    if (state.error !== null) return Promise.reject(new Error(state.error));
    if (state.closed || state.closing) return Promise.reject(new TypeError("native WritableStream is closed"));
    if (state.writeQueue >= {fetch_stream_queue_limit})
      return Promise.reject(new RangeError("native WritableStream queue limit exceeded"));
    state.writeQueue += 1;
    const operation = state.writeTail
      .then(() => state.startPromise)
      .then(() => {{
        if (state.error !== null) throw new Error(state.error);
        if (typeof state.sink.write !== "function") return undefined;
        return state.sink.write(chunk);
      }})
      .then(
        () => {{ state.writeQueue -= 1; }},
        error => {{ state.writeQueue -= 1; failWritableStream(state, error); throw error; }},
      );
    state.writeTail = operation.catch(() => undefined);
    return operation;
  }};
  const writableStreamClose = (state) => {{
    if (state.error !== null) return Promise.reject(new Error(state.error));
    if (state.closePromise) return state.closePromise;
    if (state.closed) return Promise.resolve(undefined);
    state.closing = true;
    state.closePromise = state.writeTail
      .then(() => state.startPromise)
      .then(() => {{
        if (state.error !== null) throw new Error(state.error);
        return typeof state.sink.close === "function" ? state.sink.close() : undefined;
      }})
      .then(
        () => {{ finishWritableStream(state); }},
        error => {{ failWritableStream(state, error); throw error; }},
      );
    return state.closePromise;
  }};
  const writableStreamAbort = (state, reason) => {{
    if (state.aborted) return state.error === null
      ? Promise.resolve(undefined)
      : Promise.reject(new Error(state.error));
    if (state.closed && state.error === null) return Promise.resolve(undefined);
    state.aborted = true;
    state.closing = true;
    const error = reason instanceof Error ? reason : new Error(String(reason || "native WritableStream aborted"));
    state.error = error.message;
    state.closed = true;
    settleWritableStreamClosed(state);
    let result;
    try {{ result = typeof state.sink.abort === "function" ? state.sink.abort(reason) : undefined; }}
    catch (abortError) {{ return Promise.reject(abortError); }}
    return Promise.resolve(result);
  }};
  WritableStreamNative.prototype.getWriter = function() {{
    const state = writableStreamState(this);
    if (state.locked) throw new TypeError("native WritableStream is already locked");
    state.locked = true;
    let released = false;
    let resolveClosed;
    let rejectClosed;
    const closed = new Promise((resolve, reject) => {{
      resolveClosed = resolve;
      rejectClosed = reject;
    }});
    watchWritableStreamClosed(state, resolveClosed, rejectClosed);
    const writer = {{
      get ready() {{ return Promise.resolve(undefined); }},
      get desiredSize() {{ return state.closed || state.error !== null ? null : {fetch_stream_queue_limit} - state.writeQueue; }},
      get closed() {{ return closed; }},
      write(chunk) {{
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return writableStreamWrite(state, chunk);
      }},
      close() {{
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return writableStreamClose(state);
      }},
      abort(reason) {{
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return writableStreamAbort(state, reason);
      }},
      releaseLock() {{
        if (released) return;
        released = true;
        state.locked = false;
      }},
    }};
    return Object.freeze(writer);
  }};
  WritableStreamNative.prototype.abort = function(reason) {{
    const state = writableStreamState(this);
    if (state.locked) return Promise.reject(new TypeError("native WritableStream is locked"));
    return writableStreamAbort(state, reason);
  }};
  ReadableStreamNative.prototype.pipeTo = function(destination, options) {{
    const sourceState = readableStreamState(this);
    const destinationState = writableStreamState(destination);
    if (sourceState.locked) throw new TypeError("native ReadableStream is locked");
    if (destinationState.locked) throw new TypeError("native WritableStream is locked");
    const settings = options && typeof options === "object" ? options : {{}};
    const preventClose = settings.preventClose === true;
    const preventAbort = settings.preventAbort === true;
    const preventCancel = settings.preventCancel === true;
    const signal = settings.signal === undefined ? null : settings.signal;
    if (signal !== null && (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
      throw new TypeError("native pipeTo signal is invalid");
    if (signal && signal.aborted) return Promise.reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
    const reader = this.getReader();
    const writer = destination.getWriter();
    let abortListener = null;
    const cleanup = () => {{
      if (signal && abortListener) signal.removeEventListener("abort", abortListener);
      reader.releaseLock();
      writer.releaseLock();
    }};
    const readNext = () => reader.read().then(result => {{
      if (result.done) {{
        reader.releaseLock();
        return preventClose ? undefined : writer.close();
      }}
      return writer.write(result.value).then(readNext);
    }});
    const abortPromise = signal
      ? new Promise((_, reject) => {{
          abortListener = () => reject(signal.reason === undefined ? nativeAbortError() : signal.reason);
          signal.addEventListener("abort", abortListener);
        }})
      : new Promise(() => {{}});
    const operation = Promise.race([readNext(), abortPromise]);
    return operation.then(
      value => {{ cleanup(); return value; }},
      error => {{
        const actions = [];
        if (!preventAbort) actions.push(writer.abort(error).catch(() => undefined));
        if (!preventCancel) actions.push(reader.cancel(error).catch(() => undefined));
        return Promise.all(actions).then(() => {{ cleanup(); throw error; }});
      }},
    );
  }};
  ReadableStreamNative.prototype.pipeThrough = function(transform, options) {{
    if (!transform || typeof transform !== "object") throw new TypeError("native pipeThrough transform is invalid");
    const writable = transform.writable;
    const readable = transform.readable;
    writableStreamState(writable);
    readableStreamState(readable);
    this.pipeTo(writable, options);
    return readable;
  }};
  const TransformStreamNative = function(transformer) {{
    if (!(this instanceof TransformStreamNative)) throw new TypeError("native TransformStream requires new");
    const source = transformer === undefined || transformer === null ? {{}} : transformer;
    if (typeof source !== "object" && typeof source !== "function")
      throw new TypeError("native TransformStream transformer is invalid");
    const transformState = {{ readable: null, controller: null, errored: null, terminated: false }};
    const readable = new ReadableStreamNative({{
      start(controller) {{ transformState.controller = controller; }},
    }});
    transformState.readable = readable;
    const transformController = Object.freeze({{
      get desiredSize() {{
        if (transformState.errored !== null || transformState.terminated) return null;
        const outputState = readableStreamState(readable);
        return outputState.strategy.highWaterMark - outputState.queueSize;
      }},
      enqueue(value) {{
        if (transformState.errored !== null || transformState.terminated)
          throw new TypeError("native TransformStream controller is closed");
        transformState.controller.enqueue(value);
      }},
      error(reason) {{
        if (transformState.errored !== null) return;
        transformState.errored = reason instanceof Error ? reason.message : String(reason);
        transformState.controller.error(reason);
      }},
      terminate() {{
        if (transformState.errored !== null || transformState.terminated) return;
        transformState.terminated = true;
        transformState.controller.close();
      }},
    }});
    const invoke = (name, args) => {{
      if (transformState.errored !== null) return Promise.reject(new Error(transformState.errored));
      if (transformState.terminated && name === "transform")
        return Promise.reject(new TypeError("native TransformStream is terminated"));
      const callback = source[name];
      if (typeof callback !== "function") return Promise.resolve(undefined);
      try {{ return Promise.resolve(callback.call(source, ...args)); }}
      catch (error) {{
        transformState.errored = error instanceof Error ? error.message : String(error);
        if (!transformState.terminated) transformState.controller.error(error);
        return Promise.reject(error);
      }}
    }};
    const writable = new WritableStreamNative({{
      start() {{ return invoke("start", [transformController]); }},
      write(chunk) {{
        if (typeof source.transform === "function") return invoke("transform", [chunk, transformController]);
        transformController.enqueue(chunk);
        return undefined;
      }},
      close() {{
        return invoke("flush", [transformController]).then(() => {{
          if (transformState.errored !== null || transformState.terminated) return;
          transformState.controller.close();
        }});
      }},
      abort(reason) {{
        if (!transformState.terminated && transformState.errored === null) transformState.controller.error(reason);
        return undefined;
      }},
    }});
    this.readable = readable;
    this.writable = writable;
    Object.freeze(this);
  }};
  globalThis.TransformStream = TransformStreamNative;
  globalThis.WritableStream = WritableStreamNative;
  globalThis.__glassDispatchFetchUploadEvent = (streamId, payload) => {{
    const group = fetchUploadGroup(streamId);
    if (!payload || typeof payload !== "object" || group.done || !group.reader) return null;
    if (String(payload.type || "") !== "demand" || group.demandPending) return null;
    group.demandPending = true;
    const fail = error => {{
      if (group.done) return;
      group.demandPending = false;
      group.done = true;
      group.error = String(error && error.message !== undefined ? error.message : error).slice(0, 4096);
      releaseFetchUploadReader(group);
      try {{
        pushCommand({{ kind: "fetchUploadError", stream_id: Number(group.id), message: group.error }});
      }} catch (_) {{}}
    }};
    let read;
    try {{ read = group.reader.read(); }}
    catch (error) {{ fail(error); return null; }}
    Promise.resolve(read).then(
      result => {{
        if (group.done || group.cancelled) return;
        group.demandPending = false;
        if (!result || result.done) {{
          group.done = true;
          releaseFetchUploadReader(group);
          try {{ pushCommand({{ kind: "fetchUploadEnd", stream_id: Number(group.id) }}); }}
          catch (error) {{ fail(error); }}
          return;
        }}
        let bytes;
        try {{ bytes = nativeReadableStreamChunkBytes(result.value); }}
        catch (error) {{ fail(error); return; }}
        const nextTotal = group.totalBytes + bytes.length;
        if (nextTotal > nativeFormBodyLimit) {{
          fail(new RangeError("native fetch request body exceeds its limit"));
          return;
        }}
        group.totalBytes = nextTotal;
        try {{
          pushCommand({{
            kind: "fetchUploadChunk",
            stream_id: Number(group.id),
            data_base64: encodeBase64(bytes, nativeFormBodyLimit),
          }});
        }} catch (error) {{ fail(error); }}
      }},
      fail,
    );
    return null;
  }};
  globalThis.__glassDispatchFetchStreamEvent = (streamId, payload) => {{
    const group = fetchStreamGroup(streamId);
    if (!payload || typeof payload !== "object" || group.done) return null;
    const type = String(payload.type || "");
    group.readRequested = false;
    if (type === "chunk") {{
      let bytes;
      try {{ bytes = decodeBase64(String(payload.dataBase64 || ""), {fetch_stream_chunk_limit}); }}
      catch (_) {{ group.error = "native fetch response stream chunk is invalid"; group.done = true; rejectFetchStreamWaiters(group); return null; }}
      if (group.totalBytes + bytes.length > {fetch_stream_body_limit}) {{
        group.error = "native fetch response stream exceeds its limit";
        group.done = true;
        for (const state of group.streams) {{
          state.error = group.error;
          state.done = true;
          state.queued = [];
          state.queueSizes = [];
          state.queueSize = 0;
          if (state.pendingRead) {{
            const pending = clearReadablePendingRead(state);
            pending.reject(new Error(group.error));
          }}
        }}
        rejectFetchStreamWaiters(group);
        return null;
      }}
      group.chunks.push(bytes.slice());
      group.totalBytes += bytes.length;
      for (const state of group.streams) {{
        if (state.cancelled || state.consumedByResponse) continue;
        state.done = false;
        if (state.pendingRead) {{
          resolveReadablePending(state, new Uint8Array(bytes), false);
        }} else {{
          const queued = new Uint8Array(bytes.slice());
          state.queued.push(queued);
          state.queueSizes.push(queued.byteLength);
          state.queueSize += queued.byteLength;
        }}
      }}
      if (group.waiters.length > 0) requestFetchStreamRead(group);
    }} else if (type === "end") {{
      group.done = true;
      for (const state of group.streams) {{
        state.done = true;
        if (state.pendingRead) resolveReadablePending(state, undefined, true);
        settleReadableStreamClosed(state);
      }}
      settleFetchStreamWaiters(group);
    }} else if (type === "error") {{
      group.error = String(payload.message || "native fetch response stream failed");
      group.done = true;
      for (const state of group.streams) {{
        state.error = group.error;
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) {{
          const pending = clearReadablePendingRead(state);
          pending.reject(new Error(group.error));
        }}
        settleReadableStreamClosed(state);
      }}
      rejectFetchStreamWaiters(group);
    }}
    return null;
  }};
  globalThis.__glassFetchStreamGroups = fetchStreamGroups;
  globalThis.__glassFetchUploadGroups = fetchUploadGroups;
  globalThis.__glassReadableStreamConstructor = ReadableStreamNative;
  globalThis.ReadableStream = ReadableStreamNative;
  globalThis.__glassNativeReadableStreamIsReadableStream = value => {{
    try {{ return readableStreamState(value) !== null; }}
    catch (_error) {{ return false; }}
  }};
  globalThis.__glassNativeReadableStreamIsByteStream = value => {{
    try {{ return readableStreamState(value).byteMode === true; }}
    catch (_error) {{ return false; }}
  }};
  globalThis.__glassNativeReadableStreamFromTransfer = (descriptor, port) => {{
    if (!descriptor || typeof descriptor !== "object"
        || typeof port !== "object" || typeof port.postMessage !== "function")
      throw new TypeError("native ReadableStream transfer endpoint is invalid");
    let pending = null;
    let remoteDone = false;
    let remoteError = null;
    const remoteQueue = [];
    const source = {{
      start(controller) {{
        port.onmessage = event => {{
          const message = event && event.data;
          if (!message || typeof message !== "object" || remoteDone) return;
          const kind = String(message.__glassReadableStreamControl || "");
          if (kind === "chunk") {{
            if (pending) {{
              const current = pending;
              pending = null;
              try {{ controller.enqueue(message.value); current.resolve(); }}
              catch (error) {{ current.reject(error); }}
            }} else if (remoteQueue.length < {fetch_stream_queue_limit}) {{
              remoteQueue.push(message.value);
            }} else {{
              remoteError = new Error("native transferred ReadableStream queue limit exceeded");
              remoteDone = true;
              controller.error(remoteError);
            }}
          }} else if (kind === "close") {{
            remoteDone = true;
            if (pending) {{
              const current = pending;
              pending = null;
              try {{ controller.close(); current.resolve(); }}
              catch (error) {{ current.reject(error); }}
            }} else controller.close();
          }} else if (kind === "error") {{
            remoteError = new Error(String(message.message || "native transferred ReadableStream failed"));
            remoteDone = true;
            if (pending) {{
              const current = pending;
              pending = null;
              controller.error(remoteError);
              current.reject(remoteError);
            }} else controller.error(remoteError);
          }}
        }};
        port.start();
      }},
      pull(controller) {{
        if (remoteQueue.length > 0) {{
          controller.enqueue(remoteQueue.shift());
          return Promise.resolve();
        }}
        if (remoteError !== null) {{ controller.error(remoteError); return Promise.reject(remoteError); }}
        if (remoteDone) {{ controller.close(); return Promise.resolve(); }}
        if (pending) return Promise.reject(new TypeError("native transferred ReadableStream read is already pending"));
        return new Promise((resolve, reject) => {{
          pending = {{ controller, resolve, reject }};
          try {{ port.postMessage({{ __glassReadableStreamControl: "pull" }}); }}
          catch (error) {{ pending = null; remoteDone = true; reject(error); }}
        }});
      }},
      cancel(reason) {{
        remoteDone = true;
        const current = pending;
        pending = null;
        if (current) current.resolve();
        try {{ port.postMessage({{ __glassReadableStreamControl: "cancel", reason }}); }}
        catch (_error) {{}}
        try {{ port.close(); }} catch (_error) {{}}
        return Promise.resolve(undefined);
      }},
    }};
    if (descriptor.byte_mode === true) source.type = "bytes";
    return new ReadableStreamNative(source);
  }};
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("module_fetch_streams");
