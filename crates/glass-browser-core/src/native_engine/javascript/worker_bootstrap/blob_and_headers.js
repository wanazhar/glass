  const workerBlobBytes = (part) => {
    if (part && part.__glassWorkerBlob === true) return part._bytes.slice();
    if (typeof part === "string") return workerUtf8Bytes(part);
    if (part instanceof ArrayBuffer) return Array.from(new Uint8Array(part));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(part))
      return Array.from(new Uint8Array(part.buffer, part.byteOffset, part.byteLength));
    throw new TypeError("native Worker Blob supports text or buffers");
  };
  const WorkerBlob = function(parts, options) {
    if (!(this instanceof WorkerBlob)) throw new TypeError("native Worker Blob requires new");
    const values = parts === undefined || parts === null ? [] : parts;
    if (!Array.isArray(values)) throw new TypeError("native Worker Blob parts must be an array");
    this._bytes = [];
    for (const part of values) {
      const bytes = workerBlobBytes(part);
      if (this._bytes.length + bytes.length > __GLASS_WORKER_FETCH_BODY_LIMIT__)
        throw new RangeError("native Worker Blob size limit exceeded");
      this._bytes.push(...bytes);
    }
    this.__glassWorkerBlob = true;
    this.size = this._bytes.length;
    const type = options && typeof options.type === "string" ? options.type.toLowerCase() : "";
    this.type = /^[\x20-\x7e]*$/.test(type) ? type : "";
  };
  WorkerBlob.prototype.text = function() { return Promise.resolve(workerUtf8Text(this._bytes)); };
  WorkerBlob.prototype.arrayBuffer = function() { return Promise.resolve(new Uint8Array(this._bytes).buffer); };
  WorkerBlob.prototype.bytes = function() { return Promise.resolve(new Uint8Array(this._bytes)); };
  WorkerBlob.prototype.slice = function(start, end, contentType) {
    const length = this._bytes.length;
    const normalize = (value, fallback) => {
      if (value === undefined) return fallback;
      const number = Number(value);
      if (!Number.isFinite(number)) return fallback;
      return number < 0 ? Math.max(length + Math.trunc(number), 0) : Math.min(Math.trunc(number), length);
    };
    const begin = normalize(start, 0);
    const finish = normalize(end, length);
    const blob = new WorkerBlob([], { type: contentType === undefined ? this.type : contentType });
    blob._bytes = begin > finish ? [] : this._bytes.slice(begin, finish);
    blob.size = blob._bytes.length;
    return blob;
  };
  globalThis.Blob = WorkerBlob;
  const WorkerFile = function(parts, name, options) {
    if (!(this instanceof WorkerFile)) throw new TypeError("native Worker File requires new");
    WorkerBlob.call(this, parts, options);
    this.name = String(name);
    this.lastModified = options && Number.isFinite(Number(options.lastModified))
      ? Math.max(0, Number(options.lastModified)) : 0;
    this.__glassWorkerFile = true;
  };
  WorkerFile.prototype = Object.create(WorkerBlob.prototype);
  WorkerFile.prototype.constructor = WorkerFile;
  globalThis.File = WorkerFile;
  globalThis.__glassGetObjectUrlResource = (value) => {
    const entry = workerObjectUrlRegistry.get(String(value));
    if (!entry) return null;
    return {
      contentType: entry.blob.type || null,
      bodyBase64: encodeWorkerBase64(workerBlobBytes(entry.blob), __GLASS_WORKER_FETCH_BODY_LIMIT__),
    };
  };
  globalThis.__glassInstallObjectUrlTransfers = (transfers) => {
    if (!Array.isArray(transfers)) throw new TypeError("native Worker object URL transfers are invalid");
    for (const transfer of transfers) {
      if (!transfer || typeof transfer !== "object")
        throw new TypeError("native Worker object URL transfer is invalid");
      const href = String(transfer.href || "");
      if (!href.startsWith("blob:"))
        throw new TypeError("native Worker object URL transfer scheme is invalid");
      const resource = transfer.resource;
      if (!resource || typeof resource !== "object")
        throw new TypeError("native Worker object URL transfer resource is invalid");
      const bytes = decodeWorkerBase64(String(resource.body || ""), __GLASS_WORKER_FETCH_BODY_LIMIT__);
      const contentType = resource.contentType === null || resource.contentType === undefined
        ? ""
        : String(resource.contentType);
      if (!workerObjectUrlRegistry.has(href) && workerObjectUrlRegistry.size >= __GLASS_WORKER_MAX_COMMANDS__)
        throw new RangeError("native Worker object URL limit exceeded");
      const blob = new WorkerBlob([new Uint8Array(bytes)], { type: contentType });
      workerObjectUrlRegistry.set(href, { blob, origin: workerObjectUrlOrigin() });
    }
    return null;
  };
  const workerResponseBytes = (payload) => payload && typeof payload.bodyBase64 === "string"
    ? decodeWorkerBase64(payload.bodyBase64, __GLASS_WORKER_FETCH_BODY_LIMIT__)
    : workerUtf8Bytes(String(payload && payload.body || ""));
  const workerResponseHeaders = (rawEntries, contentType) => {
    const entries = [];
    const byName = new Map();
    if (Array.isArray(rawEntries)) for (const rawEntry of rawEntries) {
      if (!Array.isArray(rawEntry) || rawEntry.length !== 2) continue;
      const rawName = String(rawEntry[0]);
      const name = rawName.toLowerCase();
      const value = String(rawEntry[1]);
      if (!workerRequestHeaderName.test(rawName)
          || name.length > __GLASS_WORKER_FETCH_HEADER_NAME_LIMIT__
          || value.length > __GLASS_WORKER_FETCH_HEADER_VALUE_LIMIT__
          || /[\u0000-\u0008\u000a-\u001f\u007f]/.test(value)) continue;
      const existing = byName.get(name);
      if (existing) existing[1] += ", " + value;
      else {
        const entry = [name, value];
        byName.set(name, entry);
        entries.push(entry);
      }
    }
    if (!byName.has("content-type") && contentType !== null && contentType !== undefined) {
      const value = String(contentType);
      if (value.length <= __GLASS_WORKER_FETCH_HEADER_VALUE_LIMIT__
          && !/[\u0000-\u0008\u000a-\u001f\u007f]/.test(value))
        entries.push(["content-type", value]);
    }
    entries.sort((left, right) => left[0] < right[0] ? -1 : left[0] > right[0] ? 1 : 0);
    const headers = new WorkerHeadersNative();
    headers._entries = entries;
    headers.get = function(name) {
      const normalized = workerResponseHeaderNameForRead(name);
      const entry = this._entries.find(candidate => candidate[0] === normalized);
      return entry ? entry[1] : null;
    };
    headers.has = function(name) { return this.get(name) !== null; };
    Object.defineProperty(headers, "__glassWorkerHeadersImmutable", { value: true });
    Object.freeze(headers);
    return headers;
  };
  const workerHeaderNameForRead = (name) => {
    const value = String(name);
    const normalized = value.toLowerCase();
    if (!workerRequestHeaderName.test(value) || normalized.length > __GLASS_WORKER_FETCH_HEADER_NAME_LIMIT__)
      throw new TypeError("native Worker Headers name is invalid");
    return normalized;
  };
  const workerResponseHeaderNameForRead = (name) => {
    const value = String(name);
    const normalized = value.toLowerCase();
    if (!workerRequestHeaderName.test(value) || normalized.length > __GLASS_WORKER_FETCH_HEADER_NAME_LIMIT__)
      throw new WorkerDOMExceptionNative("native Worker response header name is invalid", "SyntaxError");
    return normalized;
  };
  const workerHeaderEntryForRequest = (name, value) => {
    const normalizedName = workerHeaderNameForRead(name);
    if (forbiddenWorkerRequestHeader(normalizedName))
      throw new TypeError("native Worker Headers name is forbidden");
    const normalizedValue = String(value);
    if (normalizedValue.length > __GLASS_WORKER_FETCH_HEADER_VALUE_LIMIT__
        || /[\u0000-\u001f\u007f]/.test(normalizedValue))
      throw new TypeError("native Worker Headers value is invalid");
    return [normalizedName, normalizedValue];
  };
  const workerHeadersInputPairs = (input) => {
    if (input === undefined || input === null) return [];
    if (input.__glassWorkerHeaders === true)
      return input._entries.map(entry => [entry[0], entry[1]]);
    if (Array.isArray(input)) return input.map(entry => {
      if (!Array.isArray(entry) || entry.length !== 2)
        throw new TypeError("native Worker Headers pairs must contain two values");
      return [entry[0], entry[1]];
    });
    if (typeof input !== "object")
      throw new TypeError("native Worker Headers accepts records, pairs, or Headers");
    return Object.keys(input).map(name => [name, input[name]]);
  };
  const WorkerHeadersNative = typeof globalThis.__glassWorkerHeadersConstructor === "function"
    ? globalThis.__glassWorkerHeadersConstructor
    : function(init) {
    if (!(this instanceof WorkerHeadersNative)) throw new TypeError("native Worker Headers requires new");
    Object.defineProperty(this, "__glassWorkerHeaders", { value: true });
    this._entries = [];
    for (const [name, value] of workerHeadersInputPairs(init)) this.append(name, value);
  };
  const workerHeadersEnsureMutable = (headers) => {
    if (headers.__glassWorkerHeadersImmutable === true)
      throw new TypeError("native Worker response Headers are immutable");
  };
  WorkerHeadersNative.prototype.append = function(name, value) {
    workerHeadersEnsureMutable(this);
    const normalized = workerHeaderEntryForRequest(name, value);
    const existing = this._entries.find(entry => entry[0] === normalized[0]);
    const nextValue = existing ? existing[1] + ", " + normalized[1] : normalized[1];
    const nextEntries = existing
      ? this._entries.map(entry => entry[0] === normalized[0] ? [entry[0], nextValue] : entry)
      : this._entries.concat([[normalized[0], normalized[1]]]);
    if (nextEntries.length > __GLASS_WORKER_FETCH_HEADER_COUNT_LIMIT__)
      throw new RangeError("native Worker Headers limit exceeded");
    const totalBytes = nextEntries.reduce((total, entry) => total + entry[0].length + entry[1].length, 0);
    if (totalBytes > __GLASS_WORKER_FETCH_HEADER_BYTES_LIMIT__)
      throw new RangeError("native Worker Headers exceed their limit");
    this._entries = nextEntries;
  };
  WorkerHeadersNative.prototype.set = function(name, value) {
    workerHeadersEnsureMutable(this);
    const normalized = workerHeaderEntryForRequest(name, value);
    const nextEntries = this._entries.filter(entry => entry[0] !== normalized[0]);
    nextEntries.push(normalized);
    if (nextEntries.length > __GLASS_WORKER_FETCH_HEADER_COUNT_LIMIT__)
      throw new RangeError("native Worker Headers limit exceeded");
    const totalBytes = nextEntries.reduce((total, entry) => total + entry[0].length + entry[1].length, 0);
    if (totalBytes > __GLASS_WORKER_FETCH_HEADER_BYTES_LIMIT__)
      throw new RangeError("native Worker Headers exceed their limit");
    this._entries = nextEntries;
  };
  WorkerHeadersNative.prototype.delete = function(name) {
    workerHeadersEnsureMutable(this);
    const normalizedName = workerHeaderNameForRead(name);
    this._entries = this._entries.filter(entry => entry[0] !== normalizedName);
  };
  WorkerHeadersNative.prototype.get = function(name) {
    const normalizedName = workerHeaderNameForRead(name);
    const entry = this._entries.find(candidate => candidate[0] === normalizedName);
    return entry ? entry[1] : null;
  };
  WorkerHeadersNative.prototype.has = function(name) { return this.get(name) !== null; };
  const workerHeaderIterator = (owner, kind) => {
    const entries = owner._entries.map(entry => [entry[0], entry[1]]);
    let index = 0;
    return {
      next() {
        if (index >= entries.length) return { value: undefined, done: true };
        const entry = entries[index++];
        if (kind === "keys") return { value: entry[0], done: false };
        if (kind === "values") return { value: entry[1], done: false };
        return { value: entry, done: false };
      },
      [Symbol.iterator]() { return this; },
    };
  };
  WorkerHeadersNative.prototype.entries = function() { return workerHeaderIterator(this, "entries"); };
  WorkerHeadersNative.prototype.keys = function() { return workerHeaderIterator(this, "keys"); };
  WorkerHeadersNative.prototype.values = function() { return workerHeaderIterator(this, "values"); };
  WorkerHeadersNative.prototype.forEach = function(callback, thisArg) {
    if (typeof callback !== "function") throw new TypeError("native Worker Headers callback must be callable");
    this._entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], this));
  };
  Object.defineProperty(WorkerHeadersNative.prototype, "size", {
    configurable: true,
    get() { return this._entries.length; },
  });
  WorkerHeadersNative.prototype[Symbol.iterator] = WorkerHeadersNative.prototype.entries;
  globalThis.__glassWorkerHeadersConstructor = WorkerHeadersNative;
  globalThis.Headers = WorkerHeadersNative;
