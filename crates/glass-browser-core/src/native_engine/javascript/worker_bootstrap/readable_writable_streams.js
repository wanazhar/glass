  const workerReadableStreamState = (stream) => {
    if (!stream || stream.__glassWorkerReadableStream !== true || !stream._state)
      throw new TypeError("native Worker ReadableStream receiver is invalid");
    return stream._state;
  };
  const workerReadableStreamSetDone = (state) => {
    if (!state.done) state.done = true;
    if (state.closedWaiters.length === 0) return;
    const waiters = state.closedWaiters.splice(0);
    for (const waiter of waiters) {
      if (state.error !== null && waiter && typeof waiter.reject === "function")
        waiter.reject(new Error(state.error));
      else if (waiter && typeof waiter.resolve === "function") waiter.resolve();
      else if (typeof waiter === "function") waiter();
    }
  };
  const workerReadableStreamDisturb = (state) => {
    if (state.disturbed) return;
    state.disturbed = true;
    if (typeof state.onDisturb === "function") state.onDisturb();
  };
  const workerReadableStreamConsume = (state) => {
    if (state.locked || state.disturbed && state.consumedByResponse) return false;
    workerReadableStreamDisturb(state);
    state.consumedByResponse = true;
    state.offset = state.bytes.length;
    workerReadableStreamSetDone(state);
    return true;
  };
  const workerReadableStreamRemoteBytes = (value) => {
    if (value instanceof ArrayBuffer) return new Uint8Array(value).slice();
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value))
      return new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();
    throw new TypeError("native transferred Worker ReadableStream chunk is not binary");
  };
  const workerReadableStreamEmptyByob = (view) => typeof view.subarray === "function"
    ? view.subarray(0, 0)
    : new Uint8Array(view.buffer, view.byteOffset, 0);
  const workerReadableStreamRemoteResult = (state, pending, value) => {
    if (!state.byteMode) return { value, done: false };
    const bytes = workerReadableStreamRemoteBytes(value);
    if (!pending.byobView) return { value: new Uint8Array(bytes), done: false };
    const view = pending.byobView;
    const bytesPerElement = Number(view.BYTES_PER_ELEMENT) || 1;
    const capacity = Math.floor(view.byteLength / bytesPerElement) * bytesPerElement;
    if (capacity === 0) throw new TypeError("native Worker BYOB view is too small");
    const count = Math.min(bytes.length, capacity);
    new Uint8Array(view.buffer, view.byteOffset, count).set(bytes.subarray(0, count));
    if (count < bytes.length) state.remoteQueue.unshift(bytes.slice(count));
    const elements = Math.floor(count / bytesPerElement);
    return {
      value: typeof view.subarray === "function"
        ? view.subarray(0, elements)
        : new Uint8Array(view.buffer, view.byteOffset, count),
      done: false,
    };
  };
  const workerReadableStreamRemoteDone = (state) => {
    state.remoteDone = true;
    if (state.remotePendingRead) {
      const pending = state.remotePendingRead;
      state.remotePendingRead = null;
      pending.resolve(pending.byobView
        ? { value: workerReadableStreamEmptyByob(pending.byobView), done: true }
        : { value: undefined, done: true });
    }
    workerReadableStreamSetDone(state);
  };
  const workerReadableStreamRemoteFail = (state, message) => {
    state.error = String(message || "native transferred ReadableStream failed");
    state.remoteDone = true;
    if (state.remotePendingRead) {
      const pending = state.remotePendingRead;
      state.remotePendingRead = null;
      pending.reject(new Error(state.error));
    }
    workerReadableStreamSetDone(state);
  };
  const workerReadableStreamCancelRemote = (state, reason) => {
    if (state.remoteDone) return Promise.resolve(undefined);
    state.remoteDone = true;
    state.cancelled = true;
    state.remoteQueue = [];
    if (state.remotePendingRead) {
      const pending = state.remotePendingRead;
      state.remotePendingRead = null;
      pending.resolve(pending.byobView
        ? { value: workerReadableStreamEmptyByob(pending.byobView), done: true }
        : { value: undefined, done: true });
    }
    try { state.remotePort.postMessage({ __glassReadableStreamControl: "cancel", reason }); }
    catch (_error) {}
    workerReadableStreamSetDone(state);
    return Promise.resolve(undefined);
  };
  const workerReadableStreamDone = (state) => state.cancelled || state.done || state.error !== null;
  const workerReadableStreamByteView = (value) => {
    if (value instanceof ArrayBuffer) return new Uint8Array(value.slice(0));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value))
      return new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();
    throw new TypeError("native Worker byte stream chunks must be ArrayBuffer or views");
  };
  const workerReadableStreamValueByteLength = (value) => value instanceof ArrayBuffer
    ? value.byteLength
    : typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value)
      ? value.byteLength
      : 1;
  const workerReadableStreamStrategy = (strategy, byteMode) => {
    const value = strategy === undefined || strategy === null ? {} : strategy;
    if (typeof value !== "object" && typeof value !== "function")
      throw new TypeError("native Worker ReadableStream strategy is invalid");
    let highWaterMark = value.highWaterMark === undefined
      ? __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__
      : Number(value.highWaterMark);
    if (!Number.isFinite(highWaterMark) || highWaterMark < 0)
      throw new RangeError("native Worker ReadableStream highWaterMark is invalid");
    highWaterMark = Math.min(highWaterMark, __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__);
    const size = value.size === undefined
      ? (chunk => byteMode ? workerReadableStreamValueByteLength(chunk) : 1)
      : value.size;
    if (typeof size !== "function")
      throw new TypeError("native Worker ReadableStream size algorithm is invalid");
    return { highWaterMark, size };
  };
  const workerReadableStreamChunkSize = (state, value) => {
    const size = Number(state.strategy.size(value));
    if (!Number.isFinite(size) || size < 0)
      throw new RangeError("native Worker ReadableStream chunk size is invalid");
    return size;
  };
  const workerReadableStreamByobResult = (view, byteLength) => {
    const bytesPerElement = Number(view.BYTES_PER_ELEMENT) || 1;
    const elements = Math.floor(byteLength / bytesPerElement);
    return typeof view.subarray === "function"
      ? view.subarray(0, elements)
      : new Uint8Array(view.buffer, view.byteOffset, elements * bytesPerElement);
  };
  const workerReadableStreamClearPending = (state) => {
    const pending = state.pendingRead;
    state.pendingRead = null;
    state.byobRequest = null;
    return pending;
  };
  const workerReadableStreamResolvePending = (state, value, done) => {
    const pending = state.pendingRead;
    if (!pending) return false;
    if (pending.byobView) {
      if (done) {
        const current = workerReadableStreamClearPending(state);
        current.resolve({ value: workerReadableStreamByobResult(current.byobView, 0), done: true });
        return true;
      }
      let bytes;
      try { bytes = workerReadableStreamByteView(value); }
      catch (error) {
        const current = workerReadableStreamClearPending(state);
        current.reject(error);
        return true;
      }
      const bytesPerElement = Number(pending.byobView.BYTES_PER_ELEMENT) || 1;
      const capacity = Math.floor(pending.byobView.byteLength / bytesPerElement) * bytesPerElement;
      const count = Math.min(bytes.length, capacity);
      if (count === 0) return false;
      new Uint8Array(pending.byobView.buffer, pending.byobView.byteOffset, count)
        .set(bytes.subarray(0, count));
      if (count < bytes.length) {
        const remainder = bytes.slice(count);
        state.queued.unshift(remainder);
        const remainderSize = workerReadableStreamChunkSize(state, remainder);
        state.queueSizes.unshift(remainderSize);
        state.queueSize += remainderSize;
      }
      const current = workerReadableStreamClearPending(state);
      current.resolve({ value: workerReadableStreamByobResult(current.byobView, count), done: false });
      return true;
    }
    const current = workerReadableStreamClearPending(state);
    current.resolve({
      value: value === undefined ? undefined : state.byteMode ? new Uint8Array(workerReadableStreamByteView(value)) : value,
      done: Boolean(done),
    });
    return true;
  };
  const workerReadableStreamReadQueued = (state) => {
    if (state.remotePort) return null;
    if (state.fetchGroup) {
      if (state.queued.length === 0) return null;
      const value = state.queued.shift();
      state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
      return { value: new Uint8Array(workerReadableStreamByteView(value)), done: false };
    }
    if (state.underlyingSource === null) {
      if (state.offset >= state.bytes.length) return null;
      const value = new Uint8Array(state.bytes.slice(state.offset));
      state.offset = state.bytes.length;
      return { value, done: false };
    }
    if (state.queued.length === 0) return null;
    const value = state.queued.shift();
    state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
    return { value: state.byteMode ? new Uint8Array(workerReadableStreamByteView(value)) : value, done: false };
  };
  const workerReadableStreamReadByteQueued = (state, view) => {
    const target = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
    if (state.remotePort) return null;
    if (state.fetchGroup) {
      if (state.queued.length === 0) return null;
      const value = workerReadableStreamByteView(state.queued[0]);
      const count = Math.min(target.byteLength, value.byteLength);
      target.set(value.subarray(0, count));
      if (count === value.byteLength) {
        state.queued.shift();
        state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
      } else {
        const remainder = value.slice(count);
        state.queued[0] = remainder;
        state.queueSize = Math.max(0, state.queueSize - state.queueSizes[0]);
        state.queueSizes[0] = workerReadableStreamChunkSize(state, remainder);
        state.queueSize += state.queueSizes[0];
      }
      return { value: workerReadableStreamByobResult(view, count), done: false };
    }
    if (state.underlyingSource === null) {
      if (state.offset >= state.bytes.length) return null;
      const count = Math.min(target.byteLength, state.bytes.length - state.offset);
      target.set(state.bytes.slice(state.offset, state.offset + count));
      state.offset += count;
      return { value: workerReadableStreamByobResult(view, count), done: false };
    }
    if (state.queued.length === 0) return null;
    const value = workerReadableStreamByteView(state.queued[0]);
    const count = Math.min(target.byteLength, value.byteLength);
    target.set(value.subarray(0, count));
    if (count === value.byteLength) {
      state.queued.shift();
      state.queueSize = Math.max(0, state.queueSize - state.queueSizes.shift());
    } else {
      const remainder = value.slice(count);
      state.queued[0] = remainder;
      state.queueSize = Math.max(0, state.queueSize - state.queueSizes[0]);
      state.queueSizes[0] = workerReadableStreamChunkSize(state, remainder);
      state.queueSize += state.queueSizes[0];
    }
    return { value: workerReadableStreamByobResult(view, count), done: false };
  };
  const workerReadableStreamFail = (state, error) => {
    if (state.error !== null || state.cancelled) return;
    state.error = error instanceof Error ? error.message : String(error);
    state.done = true;
    if (state.pendingRead) {
      const pending = workerReadableStreamClearPending(state);
      pending.reject(new Error(state.error));
    }
    workerReadableStreamSetDone(state);
  };
  const workerReadableStreamClose = (state) => {
    if (state.done || state.cancelled || state.error !== null) return;
    state.done = true;
    if (state.pendingRead) workerReadableStreamResolvePending(state, undefined, true);
    workerReadableStreamSetDone(state);
  };
  const workerReadableStreamEnqueue = (state, value) => {
    if (state.done || state.cancelled || state.error !== null)
      throw new TypeError("native Worker ReadableStream controller is closed");
    state.sourceProduced = true;
    const queuedValue = state.byteMode ? workerReadableStreamByteView(value) : value;
    if (workerReadableStreamValueByteLength(queuedValue) > __GLASS_WORKER_FETCH_BODY_LIMIT__)
      throw new RangeError("native Worker ReadableStream chunk exceeds its limit");
    const size = workerReadableStreamChunkSize(state, queuedValue);
    if (size > __GLASS_WORKER_FETCH_BODY_LIMIT__)
      throw new RangeError("native Worker ReadableStream chunk exceeds its limit");
    if (state.pendingRead) {
      if (workerReadableStreamResolvePending(state, queuedValue, false)) return;
    }
    if (state.queued.length >= __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__)
      throw new RangeError("native Worker ReadableStream queue limit exceeded");
    state.queued.push(queuedValue);
    state.queueSizes.push(size);
    state.queueSize += size;
  };
  const workerReadableStreamByobRequest = (state) => {
    const pending = state.pendingRead;
    if (!pending || !pending.byobView || state.done || state.cancelled || state.error !== null)
      return null;
    if (state.byobRequest) return state.byobRequest;
    const request = {
      get view() { return pending.byobView; },
      respond(bytesWritten) {
        const count = Number(bytesWritten);
        if (!Number.isInteger(count) || count < 0 || count > pending.byobView.byteLength)
          throw new RangeError("native Worker BYOB response is invalid");
        if (count === 0 && !state.done)
          throw new TypeError("native Worker BYOB response is empty");
        const current = workerReadableStreamClearPending(state);
        current.resolve({ value: workerReadableStreamByobResult(current.byobView, count), done: false });
      },
      respondWithNewView(view) {
        if (!view || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(view))
          throw new TypeError("native Worker BYOB view is invalid");
        const bytes = workerReadableStreamByteView(view);
        if (bytes.byteLength > pending.byobView.byteLength)
          throw new RangeError("native Worker BYOB view exceeds its request");
        new Uint8Array(pending.byobView.buffer, pending.byobView.byteOffset, bytes.byteLength).set(bytes);
        const current = workerReadableStreamClearPending(state);
        current.resolve({ value: workerReadableStreamByobResult(current.byobView, bytes.byteLength), done: false });
      },
    };
    state.byobRequest = Object.freeze(request);
    return state.byobRequest;
  };
  const workerReadableStreamPullSource = (state) => {
    const source = state.underlyingSource;
    if (!source || state.sourceStarting || state.sourcePulling || state.done || state.cancelled || state.error !== null)
      return;
    if (typeof source.pull !== "function") return;
    state.sourceProduced = false;
    state.sourcePulling = true;
    let result;
    try { result = source.pull(state.sourceController); }
    catch (error) {
      state.sourcePulling = false;
      workerReadableStreamFail(state, error);
      return;
    }
    Promise.resolve(result).then(
      () => {
        const produced = state.sourceProduced;
        state.sourceProduced = false;
        state.sourcePulling = false;
        if (produced && state.pendingRead && !state.done && !state.cancelled && state.error === null)
          workerReadableStreamPullSource(state);
      },
      error => { state.sourcePulling = false; workerReadableStreamFail(state, error); },
    );
  };
  const workerReadableStreamStartSource = (state) => {
    const source = state.underlyingSource;
    if (!source || typeof source.start !== "function") {
      state.sourceStarting = false;
      return;
    }
    let result;
    try { result = source.start(state.sourceController); }
    catch (error) {
      state.sourceStarting = false;
      workerReadableStreamFail(state, error);
      return;
    }
    Promise.resolve(result).then(
      () => { state.sourceStarting = false; workerReadableStreamPullSource(state); },
      error => { state.sourceStarting = false; workerReadableStreamFail(state, error); },
    );
  };
  const workerReadableStreamCancelSource = (state, reason) => {
    if (state.cancelled) return Promise.resolve(undefined);
    workerReadableStreamDisturb(state);
    state.cancelled = true;
    state.done = true;
    state.queued = [];
    state.queueSizes = [];
    state.queueSize = 0;
    if (state.pendingRead) workerReadableStreamResolvePending(state, undefined, true);
    workerReadableStreamSetDone(state);
    if (!state.underlyingSource || typeof state.underlyingSource.cancel !== "function")
      return Promise.resolve(undefined);
    try { return Promise.resolve(state.underlyingSource.cancel(reason)); }
    catch (error) { return Promise.reject(error); }
  };
  const workerReadableStreamWatchClosed = (state, resolve, reject) => {
    if (state.error !== null) reject(new Error(state.error));
    else if (workerReadableStreamDone(state)) resolve();
    else state.closedWaiters.push({ resolve, reject });
  };
  const workerFetchStreamGroups = globalThis.__glassWorkerFetchStreamGroups instanceof Map
    ? globalThis.__glassWorkerFetchStreamGroups
    : new Map();
  const workerFetchStreamGroup = (streamId) => {
    const id = Number(streamId);
    if (!Number.isSafeInteger(id) || id <= 0)
      throw new TypeError("native Worker fetch stream identifier is invalid");
    let group = workerFetchStreamGroups.get(id);
    if (!group) {
      group = {
        id,
        chunks: [],
        totalBytes: 0,
        done: false,
        error: null,
        readRequested: false,
        cancelRequested: false,
        streams: [],
      };
      workerFetchStreamGroups.set(id, group);
    }
    return group;
  };
  const requestWorkerFetchStreamRead = (group) => {
    if (!group || group.done || group.error !== null || group.cancelRequested || group.readRequested)
      return;
    group.readRequested = true;
    pushCommand({
      kind: "fetchStreamRead",
      worker_id: workerId,
      stream_id: Number(group.id),
    });
  };
  const cancelWorkerFetchStreamGroup = (group) => {
    if (!group || group.done || group.cancelRequested) return;
    group.cancelRequested = true;
    group.done = true;
    for (const state of group.streams) {
      state.done = true;
      if (state.pendingRead) {
        const pending = state.pendingRead;
        state.pendingRead = null;
        state.byobRequest = null;
        pending.resolve(pending.byobView
          ? { value: workerReadableStreamEmptyByob(pending.byobView), done: true }
          : { value: undefined, done: true });
      }
      workerReadableStreamSetDone(state);
    }
    pushCommand({
      kind: "fetchStreamCancel",
      worker_id: workerId,
      stream_id: Number(group.id),
    });
  };
  const maybeCancelWorkerFetchStreamGroup = (group) => {
    if (!group || group.streams.some(state => !state.cancelled)) return;
    cancelWorkerFetchStreamGroup(group);
  };
  const WorkerReadableStreamNative = typeof globalThis.__glassWorkerReadableStreamConstructor === "function"
    ? globalThis.__glassWorkerReadableStreamConstructor
    : function(bytes, streamOptions, onDisturb) {
    if (!(this instanceof WorkerReadableStreamNative)) throw new TypeError("native Worker ReadableStream requires new");
    const transfer = bytes && bytes.__glassWorkerReadableStreamTransfer === true ? bytes : null;
    const hostStreamId = !transfer && bytes && bytes.__glassWorkerFetchStreamId !== undefined
      ? Number(bytes.__glassWorkerFetchStreamId)
      : null;
    if (hostStreamId !== null && (!Number.isSafeInteger(hostStreamId) || hostStreamId <= 0))
      throw new TypeError("native Worker fetch stream identifier is invalid");
    const underlyingSource = !transfer && hostStreamId === null
      && bytes && typeof bytes === "object" && !Array.isArray(bytes)
      ? bytes
      : null;
    const sourceOptions = underlyingSource && streamOptions && typeof streamOptions === "object"
      ? streamOptions
      : {};
    const staticBytes = !transfer && hostStreamId === null && Array.isArray(bytes);
    if (!transfer && hostStreamId === null && bytes !== undefined && bytes !== null
        && !staticBytes && !underlyingSource)
      throw new TypeError("native Worker ReadableStream source is invalid");
    if (underlyingSource && underlyingSource.type !== undefined && underlyingSource.type !== "bytes")
      throw new TypeError("native Worker ReadableStream type is unsupported");
    if (transfer && (!transfer.port || typeof transfer.port.postMessage !== "function"))
      throw new TypeError("native Worker ReadableStream transfer endpoint is invalid");
    const byteMode = hostStreamId !== null
      ? true
      : transfer
        ? transfer.byteMode === true
      : underlyingSource
        ? underlyingSource.type === "bytes"
        : staticBytes;
    const strategy = workerReadableStreamStrategy(
      transfer ? undefined : underlyingSource ? sourceOptions : undefined,
      byteMode,
    );
    const values = transfer ? [] : staticBytes ? bytes.slice() : [];
    if (values.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
      throw new RangeError("native Worker ReadableStream body exceeds its limit");
    for (const value of values) {
      if (!Number.isInteger(value) || value < 0 || value > 255)
        throw new TypeError("native Worker ReadableStream bytes are invalid");
    }
    const fetchGroup = hostStreamId === null ? null : workerFetchStreamGroup(hostStreamId);
    const queued = fetchGroup
      ? fetchGroup.chunks.map(chunk => new Uint8Array(chunk.slice()))
      : [];
    const queueSizes = queued.map(value => value.byteLength);
    const state = {
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
      disturbed: false,
      consumedByResponse: false,
      consumingResponseBody: false,
      consumedByRequest: false,
      consumingRequestBody: false,
      cancelled: false,
      done: fetchGroup ? fetchGroup.done : false,
      error: fetchGroup ? fetchGroup.error : null,
      closedWaiters: [],
      pendingRead: null,
      byobRequest: null,
      onDisturb: typeof streamOptions === "function"
        ? streamOptions
        : typeof onDisturb === "function" ? onDisturb : null,
      remotePort: transfer ? transfer.port : null,
      byteMode,
      remoteDone: transfer ? false : true,
      remoteQueue: [],
      remotePendingRead: null,
      fetchGroup,
      fetchStreamId: hostStreamId,
    };
    if (fetchGroup) fetchGroup.streams.push(state);
    if (underlyingSource) {
      state.sourceController = Object.freeze({
        get desiredSize() { return state.strategy.highWaterMark - state.queueSize; },
        get byobRequest() { return workerReadableStreamByobRequest(state); },
        enqueue(value) { workerReadableStreamEnqueue(state, value); },
        close() { workerReadableStreamClose(state); },
        error(error) { workerReadableStreamFail(state, error); },
      });
    }
    if (state.remotePort) {
      state.remotePort.onmessage = event => {
        const message = event && event.data;
        if (!message || typeof message !== "object" || state.remoteDone) return;
        const kind = String(message.__glassReadableStreamControl || "");
        if (kind === "chunk") {
          let result;
          try {
            result = state.remotePendingRead
              ? workerReadableStreamRemoteResult(state, state.remotePendingRead, message.value)
              : state.byteMode
                ? { value: new Uint8Array(workerReadableStreamRemoteBytes(message.value)), done: false }
                : { value: message.value, done: false };
          } catch (error) {
            workerReadableStreamRemoteFail(
              state,
              error instanceof Error ? error.message : String(error),
            );
            return;
          }
          if (state.remotePendingRead) {
            const pending = state.remotePendingRead;
            state.remotePendingRead = null;
            pending.resolve(result);
          } else if (state.remoteQueue.length < __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__) {
            state.remoteQueue.push(result.value);
          } else {
            workerReadableStreamRemoteFail(
              state,
              "native transferred Worker ReadableStream queue limit exceeded",
            );
          }
        } else if (kind === "close") {
          workerReadableStreamRemoteDone(state);
        } else if (kind === "error") {
          workerReadableStreamRemoteFail(state, message.message);
        }
      };
      state.remotePort.start();
    }
    Object.defineProperty(this, "__glassWorkerReadableStream", { value: true });
    Object.defineProperty(this, "_state", { value: state });
    if (underlyingSource) workerReadableStreamStartSource(state);
    Object.freeze(this);
  };
  Object.defineProperty(WorkerReadableStreamNative.prototype, "locked", {
    configurable: true,
    get() { return workerReadableStreamState(this).locked; },
  });
  WorkerReadableStreamNative.prototype.getReader = function(options) {
    const state = workerReadableStreamState(this);
    if (state.locked) throw new TypeError("native Worker ReadableStream is already locked");
    if (state.consumedByResponse && !state.consumingResponseBody)
      throw new TypeError("native Worker ReadableStream body is unusable");
    if (state.consumedByRequest && !state.consumingRequestBody)
      throw new TypeError("native Worker ReadableStream body is unusable");
    const settings = options === undefined || options === null ? {} : options;
    if (typeof settings !== "object" && typeof settings !== "function")
      throw new TypeError("native Worker ReadableStream reader options are invalid");
    const mode = settings.mode === undefined ? undefined : String(settings.mode);
    if (mode !== undefined && mode !== "byob")
      throw new TypeError("native Worker ReadableStream reader mode is unsupported");
    const byob = mode === "byob";
    if (byob && !state.byteMode)
      throw new TypeError("native Worker BYOB reader requires a byte stream");
    state.locked = true;
    let released = false;
    let resolveClosed;
    let rejectClosed;
    const closed = new Promise((resolve, reject) => {
      resolveClosed = resolve;
      rejectClosed = reject;
    });
    workerReadableStreamWatchClosed(state, resolveClosed, rejectClosed);
    const release = () => {
      if (released) return;
      released = true;
      state.locked = false;
    };
    const read = (view) => {
      if (released) return Promise.reject(new TypeError("native Worker ReadableStream reader is released"));
      if (state.consumedByResponse && !state.consumingResponseBody)
        return Promise.reject(new TypeError("native Worker body is unusable"));
      if (state.consumedByRequest && !state.consumingRequestBody)
        return Promise.reject(new TypeError("native Worker body is unusable"));
      if (byob) {
        if (!view || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(view) || view.byteLength === 0)
          throw new TypeError("native Worker BYOB read requires a non-empty view");
      } else if (view !== undefined) {
        throw new TypeError("native Worker default reader read does not accept a view");
      }
      workerReadableStreamDisturb(state);
      if (state.remotePort) {
        if (state.remoteQueue.length > 0) {
          const value = state.remoteQueue.shift();
          return Promise.resolve(byob
            ? workerReadableStreamRemoteResult(state, { byobView: view }, value)
            : state.byteMode
              ? { value: new Uint8Array(workerReadableStreamByteView(value)), done: false }
              : { value, done: false });
        }
        if (state.error !== null) return Promise.reject(new Error(state.error));
        if (state.remoteDone || state.done) {
          workerReadableStreamSetDone(state);
          return Promise.resolve(byob
            ? { value: workerReadableStreamEmptyByob(view), done: true }
            : { value: undefined, done: true });
        }
        if (state.remotePendingRead)
          return Promise.reject(new TypeError("native transferred Worker ReadableStream read is already pending"));
        const promise = new Promise((resolve, reject) => {
          state.remotePendingRead = { resolve, reject, byobView: byob ? view : null };
        });
        try { state.remotePort.postMessage({ __glassReadableStreamControl: "pull" }); }
        catch (error) {
          workerReadableStreamRemoteFail(
            state,
            error instanceof Error ? error.message : String(error),
          );
          return Promise.reject(error);
        }
        return promise;
      }
      if (state.fetchGroup) {
        if (state.error !== null || state.fetchGroup.error !== null)
          return Promise.reject(new Error(state.error || state.fetchGroup.error));
        const queued = byob
          ? workerReadableStreamReadByteQueued(state, view)
          : workerReadableStreamReadQueued(state);
        if (queued) return Promise.resolve(queued);
        if (state.fetchGroup.done || state.done) {
          workerReadableStreamSetDone(state);
          return Promise.resolve(byob
            ? { value: workerReadableStreamEmptyByob(view), done: true }
            : { value: undefined, done: true });
        }
        if (state.pendingRead)
          return Promise.reject(new TypeError("native Worker fetch stream read is already pending"));
        const promise = new Promise((resolve, reject) => {
          state.pendingRead = { resolve, reject, byobView: byob ? view : null };
        });
        try { requestWorkerFetchStreamRead(state.fetchGroup); }
        catch (error) { state.pendingRead = null; return Promise.reject(error); }
        return promise;
      }
      if (state.error !== null) return Promise.reject(new Error(state.error));
      const queued = byob
        ? workerReadableStreamReadByteQueued(state, view)
        : workerReadableStreamReadQueued(state);
      if (queued) {
        if (state.underlyingSource && state.queued.length < state.strategy.highWaterMark)
          workerReadableStreamPullSource(state);
        return Promise.resolve(queued);
      }
      if (workerReadableStreamDone(state) || state.underlyingSource === null && state.offset >= state.bytes.length) {
        workerReadableStreamSetDone(state);
        return Promise.resolve(byob
          ? { value: workerReadableStreamEmptyByob(view), done: true }
          : { value: undefined, done: true });
      }
      if (state.pendingRead)
        return Promise.reject(new TypeError("native Worker ReadableStream read is already pending"));
      const promise = new Promise((resolve, reject) => {
        state.pendingRead = { resolve, reject, byobView: byob ? view : null };
      });
      try {
        if (state.underlyingSource) workerReadableStreamPullSource(state);
      }
      catch (error) {
        state.pendingRead = null;
        return Promise.reject(error);
      }
      return promise;
    };
    const cancel = reason => {
      if (released) return Promise.reject(new TypeError("native Worker ReadableStream reader is released"));
      workerReadableStreamDisturb(state);
      if (state.remotePort) return workerReadableStreamCancelRemote(state, reason);
      if (state.underlyingSource) return workerReadableStreamCancelSource(state, reason);
      if (state.fetchGroup) {
        state.cancelled = true;
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) workerReadableStreamResolvePending(state, undefined, true);
        workerReadableStreamSetDone(state);
        maybeCancelWorkerFetchStreamGroup(state.fetchGroup);
        return Promise.resolve(undefined);
      }
      state.cancelled = true;
      state.offset = state.bytes.length;
      workerReadableStreamSetDone(state);
      return Promise.resolve(undefined);
    };
    const reader = {
      read,
      cancel,
      releaseLock: release,
      return() {
        if (released) return Promise.resolve({ value: undefined, done: true });
        return cancel().then(() => { release(); return { value: undefined, done: true }; });
      },
      [Symbol.asyncIterator]() { return this; },
    };
    Object.defineProperty(reader, "closed", { value: closed });
    return Object.freeze(reader);
  };
  WorkerReadableStreamNative.prototype.cancel = function(reason) {
    const state = workerReadableStreamState(this);
    if (state.locked) return Promise.reject(new TypeError("native Worker ReadableStream is locked"));
    if (state.remotePort) return workerReadableStreamCancelRemote(state, reason);
    if (state.underlyingSource) return workerReadableStreamCancelSource(state, reason);
    if (state.fetchGroup) {
      state.cancelled = true;
      state.done = true;
      state.queued = [];
      state.queueSizes = [];
      state.queueSize = 0;
      workerReadableStreamDisturb(state);
      workerReadableStreamSetDone(state);
      maybeCancelWorkerFetchStreamGroup(state.fetchGroup);
      return Promise.resolve(undefined);
    }
    state.cancelled = true;
    state.offset = state.bytes.length;
    workerReadableStreamDisturb(state);
    workerReadableStreamSetDone(state);
    return Promise.resolve(undefined);
  };
  WorkerReadableStreamNative.prototype.tee = function() {
    const sourceState = workerReadableStreamState(this);
    if (sourceState.locked || sourceState.consumedByResponse || sourceState.consumedByRequest)
      throw new TypeError("native Worker ReadableStream is unusable");
    workerReadableStreamDisturb(sourceState);
    const upstreamReader = this.getReader();
    const tee = {
      upstreamReader,
      reading: false,
      done: false,
      error: null,
      upstreamReleased: false,
      branches: [],
    };
    const releaseUpstream = () => {
      if (tee.upstreamReleased) return;
      tee.upstreamReleased = true;
      tee.upstreamReader.releaseLock();
    };
    const cancelUpstream = reason => {
      if (tee.upstreamReleased) return Promise.resolve(undefined);
      tee.done = true;
      return Promise.resolve(tee.upstreamReader.cancel(reason)).then(
        () => { releaseUpstream(); },
        error => { releaseUpstream(); throw error; },
      );
    };
    const failTee = error => {
      if (tee.error !== null) return Promise.resolve(undefined);
      const normalized = error instanceof Error ? error : new Error(String(error));
      tee.error = normalized.message;
      tee.done = true;
      for (const branch of tee.branches) {
        if (!branch.cancelled && branch.controller) branch.controller.error(normalized);
      }
      return cancelUpstream(normalized).catch(() => undefined);
    };
    const finishTee = () => {
      if (tee.done) return;
      tee.done = true;
      for (const branch of tee.branches) {
        if (!branch.cancelled && branch.controller) branch.controller.close();
      }
      releaseUpstream();
    };
    const maybePullTee = () => {
      if (tee.reading || tee.done || tee.error !== null) return Promise.resolve(undefined);
      const activeBranches = tee.branches.filter(branch => !branch.cancelled);
      if (activeBranches.length === 0) return cancelUpstream();
      if (activeBranches.some(branch => branch.state && branch.state.queued.length >= __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__))
        return Promise.resolve(undefined);
      tee.reading = true;
      return tee.upstreamReader.read().then(
        result => {
          tee.reading = false;
          if (result.done) {
            finishTee();
            return;
          }
          for (const branch of tee.branches) {
            if (branch.cancelled || !branch.controller) continue;
            try { branch.controller.enqueue(result.value); }
            catch (error) { return failTee(error); }
          }
        },
        error => {
          tee.reading = false;
          return failTee(error);
        },
      );
    };
    const cancelBranch = (branch, reason) => {
      if (branch.cancelled) return Promise.resolve(undefined);
      branch.cancelled = true;
      if (tee.branches.every(candidate => candidate.cancelled)) return cancelUpstream(reason);
      return Promise.resolve(undefined);
    };
    const createBranch = () => {
      const branch = { controller: null, stream: null, state: null, cancelled: false };
      const branchSource = {
        start(controller) { branch.controller = controller; },
        pull() { return maybePullTee(); },
        cancel(reason) { return cancelBranch(branch, reason); },
      };
      if (sourceState.byteMode) branchSource.type = "bytes";
      branch.stream = new WorkerReadableStreamNative(branchSource);
      branch.state = workerReadableStreamState(branch.stream);
      tee.branches.push(branch);
      return branch.stream;
    };
    return [createBranch(), createBranch()];
  };
  const workerWritableStreamState = (stream) => {
    if (!stream || stream.__glassWorkerWritableStream !== true || !stream._state)
      throw new TypeError("native WritableStream receiver is invalid");
    return stream._state;
  };
  const settleWorkerWritableStreamClosed = (state) => {
    if (!state.closedWaiters || state.closedWaiters.length === 0) return;
    const waiters = state.closedWaiters.splice(0);
    if (state.error !== null) {
      const error = new Error(state.error);
      for (const waiter of waiters) waiter.reject(error);
    } else if (state.closed) {
      for (const waiter of waiters) waiter.resolve();
    } else {
      state.closedWaiters.push(...waiters);
    }
  };
  const watchWorkerWritableStreamClosed = (state, resolve, reject) => {
    if (state.error !== null) reject(new Error(state.error));
    else if (state.closed) resolve();
    else state.closedWaiters.push({ resolve, reject });
  };
  const failWorkerWritableStream = (state, error) => {
    if (state.closed) return;
    state.error = error instanceof Error ? error.message : String(error);
    state.closed = true;
    state.closing = true;
    settleWorkerWritableStreamClosed(state);
  };
  const finishWorkerWritableStream = (state) => {
    if (state.closed) return;
    state.closed = true;
    state.closing = true;
    settleWorkerWritableStreamClosed(state);
  };
  const WorkerWritableStreamNative = typeof globalThis.__glassWorkerWritableStreamConstructor === "function"
    ? globalThis.__glassWorkerWritableStreamConstructor
    : function(underlyingSink) {
    if (!(this instanceof WorkerWritableStreamNative)) throw new TypeError("native WritableStream requires new");
    const sink = underlyingSink === undefined || underlyingSink === null ? {} : underlyingSink;
    if (typeof sink !== "object" && typeof sink !== "function")
      throw new TypeError("native WritableStream sink is invalid");
    const state = {
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
    };
    let startResult;
    try { startResult = typeof sink.start === "function" ? sink.start() : undefined; }
    catch (error) {
      state.error = error instanceof Error ? error.message : String(error);
      state.closed = true;
      state.closing = true;
      startResult = Promise.reject(error);
    }
    state.startPromise = Promise.resolve(startResult).then(
      () => undefined,
      error => { failWorkerWritableStream(state, error); throw error; },
    );
    state.startPromise.catch(() => undefined);
    Object.defineProperty(this, "__glassWorkerWritableStream", { value: true });
    Object.defineProperty(this, "_state", { value: state });
    Object.freeze(this);
  };
  Object.defineProperty(WorkerWritableStreamNative.prototype, "locked", {
    configurable: true,
    get() { return workerWritableStreamState(this).locked; },
  });
  const workerWritableStreamWrite = (state, chunk) => {
    if (state.error !== null) return Promise.reject(new Error(state.error));
    if (state.closed || state.closing) return Promise.reject(new TypeError("native WritableStream is closed"));
    if (state.writeQueue >= __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__)
      return Promise.reject(new RangeError("native WritableStream queue limit exceeded"));
    state.writeQueue += 1;
    const operation = state.writeTail
      .then(() => state.startPromise)
      .then(() => {
        if (state.error !== null) throw new Error(state.error);
        if (typeof state.sink.write !== "function") return undefined;
        return state.sink.write(chunk);
      })
      .then(
        () => { state.writeQueue -= 1; },
        error => { state.writeQueue -= 1; failWorkerWritableStream(state, error); throw error; },
      );
    state.writeTail = operation.catch(() => undefined);
    return operation;
  };
  const workerWritableStreamClose = (state) => {
    if (state.error !== null) return Promise.reject(new Error(state.error));
    if (state.closePromise) return state.closePromise;
    if (state.closed) return Promise.resolve(undefined);
    state.closing = true;
    state.closePromise = state.writeTail
      .then(() => state.startPromise)
      .then(() => {
        if (state.error !== null) throw new Error(state.error);
        return typeof state.sink.close === "function" ? state.sink.close() : undefined;
      })
      .then(
        () => { finishWorkerWritableStream(state); },
        error => { failWorkerWritableStream(state, error); throw error; },
      );
    return state.closePromise;
  };
  const workerWritableStreamAbort = (state, reason) => {
    if (state.aborted) return state.error === null
      ? Promise.resolve(undefined)
      : Promise.reject(new Error(state.error));
    if (state.closed && state.error === null) return Promise.resolve(undefined);
    state.aborted = true;
    state.closing = true;
    const error = reason instanceof Error ? reason : new Error(String(reason || "native WritableStream aborted"));
    state.error = error.message;
    state.closed = true;
    settleWorkerWritableStreamClosed(state);
    let result;
    try { result = typeof state.sink.abort === "function" ? state.sink.abort(reason) : undefined; }
    catch (abortError) { return Promise.reject(abortError); }
    return Promise.resolve(result);
  };
  WorkerWritableStreamNative.prototype.getWriter = function() {
    const state = workerWritableStreamState(this);
    if (state.locked) throw new TypeError("native WritableStream is already locked");
    state.locked = true;
    let released = false;
    let resolveClosed;
    let rejectClosed;
    const closed = new Promise((resolve, reject) => {
      resolveClosed = resolve;
      rejectClosed = reject;
    });
    watchWorkerWritableStreamClosed(state, resolveClosed, rejectClosed);
    const writer = {
      get ready() { return Promise.resolve(undefined); },
      get desiredSize() { return state.closed || state.error !== null ? null : __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__ - state.writeQueue; },
      get closed() { return closed; },
      write(chunk) {
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return workerWritableStreamWrite(state, chunk);
      },
      close() {
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return workerWritableStreamClose(state);
      },
      abort(reason) {
        if (released) return Promise.reject(new TypeError("native WritableStream writer is released"));
        return workerWritableStreamAbort(state, reason);
      },
      releaseLock() {
        if (released) return;
        released = true;
        state.locked = false;
      },
    };
    return Object.freeze(writer);
  };
  WorkerWritableStreamNative.prototype.abort = function(reason) {
    const state = workerWritableStreamState(this);
    if (state.locked) return Promise.reject(new TypeError("native WritableStream is locked"));
    return workerWritableStreamAbort(state, reason);
  };
  WorkerReadableStreamNative.prototype.pipeTo = function(destination, options) {
    const sourceState = workerReadableStreamState(this);
    const destinationState = workerWritableStreamState(destination);
    if (sourceState.locked) throw new TypeError("native ReadableStream is locked");
    if (destinationState.locked) throw new TypeError("native WritableStream is locked");
    const settings = options && typeof options === "object" ? options : {};
    const preventClose = settings.preventClose === true;
    const preventAbort = settings.preventAbort === true;
    const preventCancel = settings.preventCancel === true;
    const signal = settings.signal === undefined ? null : settings.signal;
    if (signal !== null && (!signal || typeof signal !== "object" || typeof signal.aborted !== "boolean" || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
      throw new TypeError("native pipeTo signal is invalid");
    if (signal && signal.aborted) return Promise.reject(signal.reason === undefined ? nativeWorkerAbortError() : signal.reason);
    const reader = this.getReader();
    const writer = destination.getWriter();
    let abortListener = null;
    const cleanup = () => {
      if (signal && abortListener) signal.removeEventListener("abort", abortListener);
      reader.releaseLock();
      writer.releaseLock();
    };
    const readNext = () => reader.read().then(result => {
      if (result.done) {
        reader.releaseLock();
        return preventClose ? undefined : writer.close();
      }
      return writer.write(result.value).then(readNext);
    });
    const abortPromise = signal
      ? new Promise((_, reject) => {
          abortListener = () => reject(signal.reason === undefined ? nativeWorkerAbortError() : signal.reason);
          signal.addEventListener("abort", abortListener);
        })
      : new Promise(() => {});
    const operation = Promise.race([readNext(), abortPromise]);
    return operation.then(
      value => { cleanup(); return value; },
      error => {
        const actions = [];
        if (!preventAbort) actions.push(writer.abort(error).catch(() => undefined));
        if (!preventCancel) actions.push(reader.cancel(error).catch(() => undefined));
        return Promise.all(actions).then(() => { cleanup(); throw error; });
      },
    );
  };
  WorkerReadableStreamNative.prototype.pipeThrough = function(transform, options) {
    if (!transform || typeof transform !== "object") throw new TypeError("native pipeThrough transform is invalid");
    const writable = transform.writable;
    const readable = transform.readable;
    workerWritableStreamState(writable);
    workerReadableStreamState(readable);
    this.pipeTo(writable, options);
    return readable;
  };
  const WorkerTransformStreamNative = typeof globalThis.__glassWorkerTransformStreamConstructor === "function"
    ? globalThis.__glassWorkerTransformStreamConstructor
    : function(transformer) {
    if (!(this instanceof WorkerTransformStreamNative)) throw new TypeError("native TransformStream requires new");
    const source = transformer === undefined || transformer === null ? {} : transformer;
    if (typeof source !== "object" && typeof source !== "function")
      throw new TypeError("native TransformStream transformer is invalid");
    const transformState = { readable: null, controller: null, errored: null, terminated: false };
    const readable = new WorkerReadableStreamNative({
      start(controller) { transformState.controller = controller; },
    });
    transformState.readable = readable;
    const transformController = Object.freeze({
      get desiredSize() {
        if (transformState.errored !== null || transformState.terminated) return null;
        const outputState = workerReadableStreamState(readable);
        return outputState.strategy.highWaterMark - outputState.queueSize;
      },
      enqueue(value) {
        if (transformState.errored !== null || transformState.terminated)
          throw new TypeError("native TransformStream controller is closed");
        transformState.controller.enqueue(value);
      },
      error(reason) {
        if (transformState.errored !== null) return;
        transformState.errored = reason instanceof Error ? reason.message : String(reason);
        transformState.controller.error(reason);
      },
      terminate() {
        if (transformState.errored !== null || transformState.terminated) return;
        transformState.terminated = true;
        transformState.controller.close();
      },
    });
    const invoke = (name, args) => {
      if (transformState.errored !== null) return Promise.reject(new Error(transformState.errored));
      if (transformState.terminated && name === "transform")
        return Promise.reject(new TypeError("native TransformStream is terminated"));
      const callback = source[name];
      if (typeof callback !== "function") return Promise.resolve(undefined);
      try { return Promise.resolve(callback.call(source, ...args)); }
      catch (error) {
        transformState.errored = error instanceof Error ? error.message : String(error);
        if (!transformState.terminated) transformState.controller.error(error);
        return Promise.reject(error);
      }
    };
    const writable = new WorkerWritableStreamNative({
      start() { return invoke("start", [transformController]); },
      write(chunk) {
        if (typeof source.transform === "function") return invoke("transform", [chunk, transformController]);
        transformController.enqueue(chunk);
        return undefined;
      },
      close() {
        return invoke("flush", [transformController]).then(() => {
          if (transformState.errored !== null || transformState.terminated) return;
          transformState.controller.close();
        });
      },
      abort(reason) {
        if (!transformState.terminated && transformState.errored === null) transformState.controller.error(reason);
        return undefined;
      },
    });
    this.readable = readable;
    this.writable = writable;
    Object.freeze(this);
  };
  globalThis.__glassWorkerTransformStreamConstructor = WorkerTransformStreamNative;
  globalThis.TransformStream = WorkerTransformStreamNative;
  globalThis.__glassWorkerWritableStreamConstructor = WorkerWritableStreamNative;
  globalThis.WritableStream = WorkerWritableStreamNative;

  WorkerReadableStreamNative.prototype[Symbol.asyncIterator] = function() {
    const reader = this.getReader();
    return Object.freeze({
      next() { return reader.read(); },
      return() { return reader.return(); },
      [Symbol.asyncIterator]() { return this; },
    });
  };
  globalThis.__glassWorkerReadableStreamConstructor = WorkerReadableStreamNative;
  globalThis.ReadableStream = WorkerReadableStreamNative;
  globalThis.__glassNativeReadableStreamIsReadableStream = value => {
    try { return workerReadableStreamState(value) !== null; }
    catch (_error) { return false; }
  };
  globalThis.__glassNativeReadableStreamIsByteStream = value => {
    try { return workerReadableStreamState(value).byteMode === true; }
    catch (_error) { return false; }
  };
  globalThis.__glassNativeReadableStreamFromTransfer = (descriptor, port) => {
    return new WorkerReadableStreamNative({
      __glassWorkerReadableStreamTransfer: true,
      port,
      byteMode: descriptor && descriptor.byte_mode === true,
    });
  };
  const workerReadableStreamChunkBytes = (value) => {
    if (typeof value === "string") return workerUtf8Bytes(value);
    if (value && value.__glassWorkerBlob === true) return value._bytes.slice();
    if (value instanceof ArrayBuffer) return Array.from(new Uint8Array(value));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value))
      return Array.from(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
    throw new TypeError("native Worker body stream chunks must be byte data");
  };
  const workerDrainReadableStream = (stream, claimed, responseBody) => {
    const state = workerReadableStreamState(stream);
    if (!claimed && (state.locked || state.disturbed))
      return Promise.reject(new TypeError("native Worker body stream is unusable"));
    if (responseBody) {
      state.consumedByResponse = true;
      state.consumingResponseBody = true;
    } else {
      state.consumedByRequest = true;
      state.consumingRequestBody = true;
    }
    workerReadableStreamDisturb(state);
    let reader;
    try { reader = stream.getReader(); }
    catch (error) {
      if (responseBody) state.consumingResponseBody = false;
      else state.consumingRequestBody = false;
      return Promise.reject(error);
    }
    const chunks = [];
    let totalBytes = 0;
    const release = () => {
      if (responseBody) state.consumingResponseBody = false;
      else state.consumingRequestBody = false;
      reader.releaseLock();
    };
    const readNext = () => reader.read().then(
      result => {
        if (result.done) {
          release();
          const bytes = [];
          for (const chunk of chunks) bytes.push(...chunk);
          return bytes;
        }
        let bytes;
        try { bytes = workerReadableStreamChunkBytes(result.value); }
        catch (error) { release(); throw error; }
        totalBytes += bytes.length;
        if (totalBytes > __GLASS_WORKER_FETCH_BODY_LIMIT__) {
          release();
          throw new RangeError("native Worker stream body exceeds its limit");
        }
        chunks.push(bytes);
        return readNext();
      },
      error => { release(); throw error; },
    );
    return readNext();
  };
  globalThis.__glassDispatchWorkerFetchStreamEvent = (streamId, payload) => {
    const group = workerFetchStreamGroup(streamId);
    if (!payload || typeof payload !== "object" || group.done) return null;
    const type = String(payload.type || "");
    group.readRequested = false;
    if (type === "chunk") {
      let bytes;
      try { bytes = decodeWorkerBase64(String(payload.dataBase64 || ""), __GLASS_WORKER_FETCH_STREAM_CHUNK_LIMIT__); }
      catch (_) {
        group.error = "native Worker fetch response stream chunk is invalid";
        group.done = true;
        for (const state of group.streams) {
          state.error = group.error;
          state.done = true;
          state.queued = [];
          state.queueSizes = [];
          state.queueSize = 0;
          if (state.pendingRead) {
            const pending = workerReadableStreamClearPending(state);
            pending.reject(new Error(group.error));
          }
          workerReadableStreamSetDone(state);
        }
        return null;
      }
      if (group.totalBytes + bytes.length > __GLASS_WORKER_FETCH_STREAM_BODY_LIMIT__) {
        group.error = "native Worker fetch response stream exceeds its limit";
        group.done = true;
        for (const state of group.streams) {
          state.error = group.error;
          state.done = true;
          state.queued = [];
          state.queueSizes = [];
          state.queueSize = 0;
          if (state.pendingRead) {
            const pending = workerReadableStreamClearPending(state);
            pending.reject(new Error(group.error));
          }
          workerReadableStreamSetDone(state);
        }
        return null;
      }
      const chunk = new Uint8Array(bytes);
      group.chunks.push(chunk.slice());
      group.totalBytes += bytes.length;
      for (const state of group.streams) {
        if (state.cancelled || (state.consumedByResponse && !state.consumingResponseBody)) continue;
        state.done = false;
        if (state.pendingRead && workerReadableStreamResolvePending(state, chunk, false)) continue;
        if (state.queued.length >= __GLASS_WORKER_FETCH_STREAM_QUEUE_LIMIT__) {
          state.error = "native Worker fetch response stream queue limit exceeded";
          state.done = true;
          workerReadableStreamSetDone(state);
          continue;
        }
        const queued = chunk.slice();
        state.queued.push(queued);
        state.queueSizes.push(queued.byteLength);
        state.queueSize += queued.byteLength;
      }
    } else if (type === "end") {
      group.done = true;
      for (const state of group.streams) {
        state.done = true;
        if (state.pendingRead) workerReadableStreamResolvePending(state, undefined, true);
        workerReadableStreamSetDone(state);
      }
    } else if (type === "error") {
      group.error = String(payload.message || "native Worker fetch response stream failed");
      group.done = true;
      for (const state of group.streams) {
        state.error = group.error;
        state.done = true;
        state.queued = [];
        state.queueSizes = [];
        state.queueSize = 0;
        if (state.pendingRead) {
          const pending = workerReadableStreamClearPending(state);
          pending.reject(new Error(group.error));
        }
        workerReadableStreamSetDone(state);
      }
    }
    return null;
  };
  globalThis.__glassWorkerFetchStreamGroups = workerFetchStreamGroups;
  const workerFetchUploadGroups = globalThis.__glassWorkerFetchUploadGroups instanceof Map
    ? globalThis.__glassWorkerFetchUploadGroups
    : new Map();
  const workerFetchUploadGroup = (streamId) => {
    const id = Number(streamId);
    if (!Number.isSafeInteger(id) || id <= 0)
      throw new TypeError("native Worker fetch upload stream identifier is invalid");
    let group = workerFetchUploadGroups.get(id);
    if (!group) {
      group = {
        id,
        reader: null,
        streamState: null,
        demandPending: false,
        done: false,
        cancelled: false,
        error: null,
        totalBytes: 0,
      };
      workerFetchUploadGroups.set(id, group);
    }
    return group;
  };
  const releaseWorkerFetchUploadReader = (group) => {
    if (!group || !group.reader) return;
    const reader = group.reader;
    group.reader = null;
    if (group.streamState) group.streamState.consumingRequestBody = false;
    try { reader.releaseLock(); } catch (_) {}
  };
  const startWorkerFetchUpload = (stream, streamId) => {
    const state = workerReadableStreamState(stream);
    if (state.locked) throw new TypeError("native Worker fetch request body stream is locked");
    if (!state.consumedByRequest) {
      if (state.disturbed) throw new TypeError("native Worker fetch request body stream is unusable");
      state.consumedByRequest = true;
      workerReadableStreamDisturb(state);
    }
    if (state.consumedByResponse)
      throw new TypeError("native Worker fetch request body stream is unusable");
    const group = workerFetchUploadGroup(streamId);
    if (group.reader || group.done)
      throw new TypeError("native Worker fetch upload stream is already active");
    state.consumingRequestBody = true;
    try { group.reader = stream.getReader(); }
    catch (error) { state.consumingRequestBody = false; throw error; }
    group.streamState = state;
    return group;
  };
  const cancelWorkerFetchUploadGroup = (group, reason) => {
    if (!group || group.done) return;
    group.cancelled = true;
    group.done = true;
    group.error = reason === undefined ? "native Worker fetch request body was cancelled" : String(reason);
    const reader = group.reader;
    if (group.streamState) group.streamState.consumingRequestBody = false;
    group.reader = null;
    if (reader) {
      try { Promise.resolve(reader.cancel(reason)).catch(() => {}); } catch (_) {}
    }
    pushCommand({
      kind: "fetchUploadCancel",
      worker_id: workerId,
      stream_id: Number(group.id),
    });
  };
  globalThis.__glassDispatchWorkerFetchUploadEvent = (streamId, payload) => {
    const group = workerFetchUploadGroup(streamId);
    if (!payload || typeof payload !== "object" || group.done || !group.reader) return null;
    if (String(payload.type || "") !== "demand" || group.demandPending) return null;
    group.demandPending = true;
    const fail = error => {
      if (group.done) return;
      group.demandPending = false;
      group.done = true;
      group.error = String(error && error.message !== undefined ? error.message : error).slice(0, 4096);
      releaseWorkerFetchUploadReader(group);
      try {
        pushCommand({
          kind: "fetchUploadError",
          worker_id: workerId,
          stream_id: Number(group.id),
          message: group.error,
        });
      } catch (_) {}
    };
    let read;
    try { read = group.reader.read(); }
    catch (error) { fail(error); return null; }
    Promise.resolve(read).then(
      result => {
        if (group.done || group.cancelled) return;
        group.demandPending = false;
        if (!result || result.done) {
          group.done = true;
          releaseWorkerFetchUploadReader(group);
          try {
            pushCommand({
              kind: "fetchUploadEnd",
              worker_id: workerId,
              stream_id: Number(group.id),
            });
          } catch (error) { fail(error); }
          return;
        }
        let bytes;
        try { bytes = workerReadableStreamChunkBytes(result.value); }
        catch (error) { fail(error); return; }
        const nextTotal = group.totalBytes + bytes.length;
        if (nextTotal > __GLASS_WORKER_FETCH_BODY_LIMIT__) {
          fail(new RangeError("native Worker fetch request body exceeds its limit"));
          return;
        }
        group.totalBytes = nextTotal;
        try {
          pushCommand({
            kind: "fetchUploadChunk",
            worker_id: workerId,
            stream_id: Number(group.id),
            data_base64: encodeWorkerBase64(bytes, __GLASS_WORKER_FETCH_BODY_LIMIT__),
          });
        } catch (error) { fail(error); }
      },
      fail,
    );
    return null;
  };
  globalThis.__glassWorkerFetchUploadGroups = workerFetchUploadGroups;
