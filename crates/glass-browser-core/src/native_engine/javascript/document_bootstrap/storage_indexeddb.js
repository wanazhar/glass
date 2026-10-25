  const storageEntryLimit = {storage_entry_limit};
  const storageKeyLimit = {storage_key_limit};
  const storageValueLimit = {storage_value_limit};
  const HTML_NAMESPACE = "http://www.w3.org/1999/xhtml";
  const SVG_NAMESPACE = "http://www.w3.org/2000/svg";
  const MATHML_NAMESPACE = "http://www.w3.org/1998/Math/MathML";
  const XML_NAMESPACE = "http://www.w3.org/XML/1998/namespace";
  const XMLNS_NAMESPACE = "http://www.w3.org/2000/xmlns/";
  const XLINK_NAMESPACE = "http://www.w3.org/1999/xlink";
  const boundedStorageText = (value, limit, field) => {{
    const text = String(value);
    if (text.length > limit) throw new RangeError("native storage " + field + " exceeds its limit");
    return text;
  }};
  const createStorage = (mapSlot, objectSlot, initialValues) => {{
    const hasExistingValues = globalThis[mapSlot] instanceof Map;
    const values = hasExistingValues ? globalThis[mapSlot] : new Map();
    if (!hasExistingValues && initialValues && typeof initialValues === "object") {{
      for (const key of Object.keys(initialValues)) {{
        values.set(key, String(initialValues[key]));
      }}
    }}
    globalThis[mapSlot] = values;
    const existing = globalThis[objectSlot];
    if (existing && existing.__glassNativeStorage === true) return existing;
    const store = {{
      get length() {{ return values.size; }},
      key(index) {{
        const position = Number(index);
        if (!Number.isInteger(position) || position < 0) return null;
        return Array.from(values.keys())[position] ?? null;
      }},
      getItem(key) {{
        const value = values.get(String(key));
        return value === undefined ? null : value;
      }},
      setItem(key, value) {{
        const normalizedKey = boundedStorageText(key, storageKeyLimit, "key");
        const normalizedValue = boundedStorageText(value, storageValueLimit, "value");
        if (!values.has(normalizedKey) && values.size >= storageEntryLimit) {{
          throw new RangeError("native storage entry limit exceeded");
        }}
        values.set(normalizedKey, normalizedValue);
        pushCommand({{ kind: "storageSet", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session", key: normalizedKey, value: normalizedValue }});
      }},
      removeItem(key) {{
        const normalizedKey = String(key);
        if (!values.delete(normalizedKey)) return;
        pushCommand({{ kind: "storageRemove", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session", key: normalizedKey }});
      }},
      clear() {{
        if (values.size === 0) return;
        values.clear();
        pushCommand({{ kind: "storageClear", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session" }});
      }},
    }};
    Object.defineProperty(store, "__glassNativeStorage", {{
      value: true,
      enumerable: false,
      configurable: false,
    }});
    globalThis[objectSlot] = store;
    return store;
  }};
  globalThis.localStorage = createStorage("__glassLocalStorageValues", "__glassLocalStorageObject", host.storage.local);
  globalThis.sessionStorage = createStorage("__glassSessionStorageValues", "__glassSessionStorageObject", host.storage.session);
  const indexedDbHostState = host.indexed_db && typeof host.indexed_db === "object"
    ? host.indexed_db
    : {{ databases: {{}} }};
  if (!indexedDbHostState.databases || typeof indexedDbHostState.databases !== "object") indexedDbHostState.databases = {{}};
  globalThis.__glassIndexedDbState = indexedDbHostState;
  const indexedDbState = () => globalThis.__glassIndexedDbState;
  const indexedDbConnections = globalThis.__glassIndexedDbConnections instanceof Map
    ? globalThis.__glassIndexedDbConnections
    : new Map();
  const indexedDbPendingOpens = globalThis.__glassIndexedDbPendingOpens instanceof Map
    ? globalThis.__glassIndexedDbPendingOpens
    : new Map();
  const indexedDbPendingDeletes = globalThis.__glassIndexedDbPendingDeletes instanceof Map
    ? globalThis.__glassIndexedDbPendingDeletes
    : new Map();
  const indexedDbTransactionQueues = globalThis.__glassIndexedDbTransactionQueues instanceof Map
    ? globalThis.__glassIndexedDbTransactionQueues
    : new Map();
  globalThis.__glassIndexedDbConnections = indexedDbConnections;
  globalThis.__glassIndexedDbPendingOpens = indexedDbPendingOpens;
  globalThis.__glassIndexedDbPendingDeletes = indexedDbPendingDeletes;
  globalThis.__glassIndexedDbTransactionQueues = indexedDbTransactionQueues;
  if (!globalThis.__glassNativeIndexedDBInstalled) {{
  const indexedDbDatabaseLimit = {indexed_db_database_limit};
  const indexedDbStoreLimit = {indexed_db_store_limit};
  const indexedDbIndexLimit = {indexed_db_index_limit};
  const indexedDbRecordLimit = {indexed_db_record_limit};
  const indexedDbValueLimit = {indexed_db_value_limit};
  const indexedDbSchedule = (callback) => Promise.resolve().then(callback);
  const indexedDbError = (name, message) => {{
    const error = new Error(message);
    error.name = name;
    return error;
  }};
  const indexedDbName = (value, field) => {{
    const name = String(value);
    if (!name || name.length > storageKeyLimit) throw indexedDbError("TypeError", field + " is outside the native limit");
    return name;
  }};
  const indexedDbTypeKey = "__glassNativeIndexedDbType";
  const indexedDbTypedArrayNames = new Set([
    "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array",
    "Uint16Array", "Int32Array", "Uint32Array", "Float32Array", "Float64Array",
    "BigInt64Array", "BigUint64Array",
  ]);
  const indexedDbEncode = (value, seen = new Set(), depth = 0) => {{
    if (depth > 64) throw indexedDbError("DataCloneError", "native IndexedDB value is too deeply nested");
    if (value === undefined) return {{ [indexedDbTypeKey]: "undefined" }};
    if (value === null || typeof value === "boolean" || typeof value === "string") return value;
    if (typeof value === "number") {{
      if (Number.isFinite(value) && !Object.is(value, -0)) return value;
      return {{
        [indexedDbTypeKey]: "number",
        value: Number.isNaN(value) ? "NaN" : (value === Infinity ? "Infinity" : (value === -Infinity ? "-Infinity" : "-0")),
      }};
    }}
    if (typeof value === "bigint" || typeof value === "function" || typeof value === "symbol") throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    if (seen.has(value)) throw indexedDbError("DataCloneError", "cyclic value cannot be cloned by native IndexedDB");
    seen.add(value);
    let encoded;
    if (typeof SharedArrayBuffer === "function" && value instanceof SharedArrayBuffer) throw indexedDbError("DataCloneError", "shared buffers cannot be cloned by native IndexedDB");
    if (value instanceof ArrayBuffer) {{
      encoded = {{
        [indexedDbTypeKey]: "arrayBuffer",
        bytes: Array.from(new Uint8Array(value)),
      }};
    }} else if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(value)) {{
      const constructorName = value.constructor && value.constructor.name;
      if (constructorName !== "DataView" && !indexedDbTypedArrayNames.has(constructorName)) throw indexedDbError("DataCloneError", "typed array cannot be cloned by native IndexedDB");
      encoded = {{
        [indexedDbTypeKey]: "typedArray",
        constructor: constructorName,
        bytes: Array.from(new Uint8Array(value.buffer, value.byteOffset, value.byteLength)),
      }};
    }} else if (value && value.__glassNativeBlob === true) {{
      encoded = {{
        [indexedDbTypeKey]: "blob",
        text: String(value._text || ""),
        type: String(value.type || ""),
        file: value.__glassNativeFile === true,
        name: value.__glassNativeFile === true ? String(value.name || "") : "",
        lastModified: value.__glassNativeFile === true ? Number(value.lastModified || 0) : 0,
      }};
    }} else if (value instanceof Date) {{
      const timestamp = value.getTime();
      if (!Number.isFinite(timestamp)) throw indexedDbError("DataCloneError", "invalid date cannot be cloned by native IndexedDB");
      encoded = {{ [indexedDbTypeKey]: "date", value: timestamp }};
    }} else if (value instanceof RegExp) {{
      encoded = {{ [indexedDbTypeKey]: "regexp", source: value.source, flags: value.flags }};
    }} else if (value instanceof Map) {{
      encoded = {{
        [indexedDbTypeKey]: "map",
        entries: Array.from(value.entries(), entry => [
          indexedDbEncode(entry[0], seen, depth + 1),
          indexedDbEncode(entry[1], seen, depth + 1),
        ]),
      }};
    }} else if (value instanceof Set) {{
      encoded = {{
        [indexedDbTypeKey]: "set",
        values: Array.from(value.values(), entry => indexedDbEncode(entry, seen, depth + 1)),
      }};
    }} else if (Array.isArray(value)) {{
      encoded = Array.from(value, entry => indexedDbEncode(entry, seen, depth + 1));
    }} else {{
      if (Object.prototype.hasOwnProperty.call(value, indexedDbTypeKey)) throw indexedDbError("DataCloneError", "reserved native IndexedDB value tag is not writable");
      encoded = Object.create(null);
      for (const key of Object.keys(value)) encoded[key] = indexedDbEncode(value[key], seen, depth + 1);
    }}
    seen.delete(value);
    return encoded;
  }};
  const indexedDbDecode = (value) => {{
    if (Array.isArray(value)) return value.map(entry => indexedDbDecode(entry));
    if (!value || typeof value !== "object") return value;
    const type = value[indexedDbTypeKey];
    if (type === "undefined") return undefined;
    if (type === "number") return value.value === "NaN" ? NaN : (value.value === "Infinity" ? Infinity : (value.value === "-Infinity" ? -Infinity : -0));
    if (type === "date") return new Date(value.value);
    if (type === "regexp") return new RegExp(value.source, value.flags);
    if (type === "map") return new Map((value.entries || []).map(entry => [indexedDbDecode(entry[0]), indexedDbDecode(entry[1])]));
    if (type === "set") return new Set((value.values || []).map(entry => indexedDbDecode(entry)));
    if (type === "arrayBuffer") return new Uint8Array(value.bytes || []).buffer;
    if (type === "typedArray") {{
      const buffer = new Uint8Array(value.bytes || []).buffer;
      if (value.constructor === "DataView") return new DataView(buffer);
      if (!indexedDbTypedArrayNames.has(value.constructor) || typeof globalThis[value.constructor] !== "function") throw indexedDbError("DataCloneError", "typed array cannot be reconstructed by native IndexedDB");
      return new globalThis[value.constructor](buffer);
    }}
    if (type === "blob") return value.file
      ? new File([value.text], value.name, {{ type: value.type, lastModified: value.lastModified }})
      : new Blob([value.text], {{ type: value.type }});
    const decoded = {{}};
    for (const key of Object.keys(value)) Object.defineProperty(decoded, key, {{
      value: indexedDbDecode(value[key]),
      enumerable: true,
      configurable: true,
      writable: true,
    }});
    return decoded;
  }};
  const indexedDbStoredClone = (value) => indexedDbDecode(value);
  const indexedDbClone = (value) => {{
    let serialized;
    try {{ serialized = JSON.stringify(indexedDbEncode(value)); }} catch (error) {{
      if (error && (error.name === "DataCloneError" || error.name === "QuotaExceededError")) throw error;
      throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    }}
    if (serialized === undefined) throw indexedDbError("DataCloneError", "value cannot be cloned by native IndexedDB");
    if (serialized.length > indexedDbValueLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB value limit exceeded");
    return indexedDbDecode(JSON.parse(serialized));
  }};
  const indexedDbKeyToken = (key, allowUndefined) => {{
    if (key === undefined && allowUndefined) return undefined;
    if (typeof key === "string") {{
      if (key.length > storageKeyLimit) throw indexedDbError("DataError", "native IndexedDB key limit exceeded");
      return "s:" + key;
    }}
    if (typeof key === "number" && Number.isFinite(key)) {{
      const normalized = Object.is(key, -0) ? 0 : key;
      return "n:" + String(normalized);
    }}
    throw indexedDbError("DataError", "native IndexedDB supports only string and finite number keys");
  }};
  const indexedDbKeyValue = (token) => token.startsWith("s:")
    ? token.slice(2)
    : Number(token.slice(2));
  const indexedDbCompareTokens = (left, right) => {{
    const leftNumber = left.startsWith("n:");
    const rightNumber = right.startsWith("n:");
    if (leftNumber !== rightNumber) return leftNumber ? -1 : 1;
    const leftValue = indexedDbKeyValue(left);
    const rightValue = indexedDbKeyValue(right);
    return leftValue < rightValue ? -1 : (leftValue > rightValue ? 1 : 0);
  }};
  const indexedDbRangeIncludes = (range, token) => {{
    if (!range) return true;
    if (range.__lowerToken !== undefined) {{
      const comparison = indexedDbCompareTokens(token, range.__lowerToken);
      if (comparison < 0 || (comparison === 0 && range.lowerOpen)) return false;
    }}
    if (range.__upperToken !== undefined) {{
      const comparison = indexedDbCompareTokens(token, range.__upperToken);
      if (comparison > 0 || (comparison === 0 && range.upperOpen)) return false;
    }}
    return true;
  }};
  const makeIndexedDbKeyRange = (lower, upper, lowerOpen, upperOpen) => {{
    if (lower !== undefined && upper !== undefined) {{
      const comparison = indexedDbCompareTokens(lower, upper);
      if (comparison > 0 || (comparison === 0 && (lowerOpen || upperOpen))) throw indexedDbError("DataError", "native IndexedDB key range bounds are invalid");
    }}
    const range = {{
      lower: lower === undefined ? undefined : indexedDbKeyValue(lower),
      upper: upper === undefined ? undefined : indexedDbKeyValue(upper),
      lowerOpen: Boolean(lowerOpen),
      upperOpen: Boolean(upperOpen),
    }};
    Object.defineProperties(range, {{
      __glassNativeKeyRange: {{ value: true }},
      __lowerToken: {{ value: lower }},
      __upperToken: {{ value: upper }},
    }});
    range.includes = (value) => indexedDbRangeIncludes(range, indexedDbKeyToken(value, false));
    return range;
  }};
  const indexedDbQueryRange = (query) => {{
    if (query === undefined) return null;
    if (query && query.__glassNativeKeyRange === true) return query;
    const token = indexedDbKeyToken(query, false);
    return makeIndexedDbKeyRange(token, token, false, false);
  }};
  const indexedDbQueryLimit = (count) => {{
    if (count === undefined) return indexedDbRecordLimit;
    const number = Number(count);
    if (!Number.isInteger(number) || number < 0) throw indexedDbError("TypeError", "native IndexedDB count must be a non-negative integer");
    return Math.min(indexedDbRecordLimit, number);
  }};
  const indexedDbDirection = (direction) => {{
    const value = direction === undefined ? "next" : String(direction);
    if (!["next", "nextunique", "prev", "prevunique"].includes(value)) throw indexedDbError("TypeError", "native IndexedDB cursor direction is unsupported");
    return value;
  }};
  const indexedDbReadKeyPath = (value, keyPath) => {{
    if (!keyPath) return undefined;
    let current = value;
    for (const part of keyPath.split(".")) {{
      if (current === null || current === undefined || typeof current !== "object") return undefined;
      current = current[part];
    }}
    return current;
  }};
  const indexedDbWriteKeyPath = (value, keyPath, key) => {{
    if (!keyPath || value === null || typeof value !== "object") throw indexedDbError("DataError", "auto-increment key path requires an object");
    const parts = keyPath.split(".");
    let current = value;
    for (let index = 0; index < parts.length - 1; index += 1) {{
      const part = parts[index];
      if (!current[part] || typeof current[part] !== "object") current[part] = {{}};
      current = current[part];
    }}
    current[parts[parts.length - 1]] = key;
  }};
  const indexedDbIndexKeys = (value, index) => {{
    const extracted = indexedDbReadKeyPath(indexedDbDecode(value), index.key_path);
    if (extracted === undefined) return [];
    const candidates = index.multi_entry && Array.isArray(extracted) ? extracted : [extracted];
    const keys = [];
    for (const candidate of candidates) {{
      const token = indexedDbKeyToken(candidate, false);
      if (!keys.includes(token)) keys.push(token);
    }}
    return keys;
  }};
  const indexedDbObjectEntries = (store) => Object.keys(store.records)
    .map(primaryToken => ({{ indexToken: primaryToken, primaryToken, value: store.records[primaryToken] }}))
    .sort((left, right) => indexedDbCompareTokens(left.primaryToken, right.primaryToken));
  const indexedDbIndexEntries = (store, index) => {{
    const entries = [];
    for (const primaryToken of Object.keys(store.records)) {{
      for (const indexToken of indexedDbIndexKeys(store.records[primaryToken], index)) {{
        entries.push({{ indexToken, primaryToken, value: store.records[primaryToken] }});
      }}
    }}
    return entries.sort((left, right) =>
      indexedDbCompareTokens(left.indexToken, right.indexToken)
      || indexedDbCompareTokens(left.primaryToken, right.primaryToken));
  }};
  const indexedDbOrderedEntries = (entries, range, direction) => {{
    const ordered = entries.filter(entry => indexedDbRangeIncludes(range, entry.indexToken));
    if (direction.startsWith("prev")) ordered.reverse();
    if (direction.endsWith("unique")) {{
      const seen = new Set();
      return ordered.filter(entry => {{
        if (seen.has(entry.indexToken)) return false;
        seen.add(entry.indexToken);
        return true;
      }});
    }}
    return ordered;
  }};
  const makeIndexedDbRequest = () => ({{
    result: undefined,
    error: null,
    readyState: "pending",
    onupgradeneeded: null,
    onblocked: null,
    onsuccess: null,
    onerror: null,
  }});
  const finishIndexedDbRequest = (request, value, error, after) => {{
    indexedDbSchedule(() => {{
      request.readyState = "done";
      if (error) {{
        request.error = error;
        if (typeof request.onerror === "function") request.onerror.call(request, {{ target: request }});
      }} else {{
        request.result = value;
        if (typeof request.onsuccess === "function") request.onsuccess.call(request, {{ target: request }});
      }}
      if (typeof after === "function") after();
    }});
  }};
  const finishIndexedDbTransaction = (transaction) => {{
    if (transaction.__finished || transaction.__pending !== 0) return;
    transaction.__finished = true;
    releaseIndexedDbTransaction(transaction);
    if (transaction.__aborted) {{
      if (!transaction.__abortDispatched) {{
        transaction.__abortDispatched = true;
        if (typeof transaction.onabort === "function") transaction.onabort.call(transaction, {{ target: transaction }});
      }}
      return;
    }}
    if (typeof transaction.oncomplete === "function") transaction.oncomplete.call(transaction, {{ target: transaction }});
  }};
  const maybeFinishIndexedDbTransaction = (transaction) => {{
    if (transaction.__pending !== 0 || transaction.__finished || transaction.__finishScheduled) return;
    transaction.__finishScheduled = true;
    indexedDbSchedule(() => {{
      transaction.__finishScheduled = false;
      finishIndexedDbTransaction(transaction);
    }});
  }};
  const drainIndexedDbTransactions = (databaseName) => {{
    const queue = indexedDbTransactionQueues.get(databaseName);
    if (!queue || queue.length === 0) return;
    const transaction = queue[0];
    if (transaction.__running || transaction.__operationQueue.length === 0) return;
    const next = transaction.__operationQueue.shift();
    transaction.__running = true;
    indexedDbSchedule(() => {{
      if (transaction.__initialState === null && !transaction.__aborted) transaction.__initialState = cloneIndexedDbDatabaseState(transaction.__databaseState);
      let value;
      let error = null;
      if (transaction.__aborted) {{
        error = indexedDbError("AbortError", "native IndexedDB transaction was aborted");
      }} else {{
        try {{ value = next.operation(); }} catch (caught) {{
          error = caught instanceof Error ? caught : indexedDbError("UnknownError", String(caught));
          transaction.__aborted = true;
          transaction.error = error;
          transaction.__rollback();
          if (typeof transaction.onerror === "function") transaction.onerror.call(transaction, {{ target: transaction }});
        }}
      }}
      finishIndexedDbRequest(next.request, value, error, () => {{
        transaction.__pending -= 1;
        transaction.__running = false;
        drainIndexedDbTransactions(databaseName);
        maybeFinishIndexedDbTransaction(transaction);
      }});
    }});
  }};
  const releaseIndexedDbTransaction = (transaction) => {{
    const queue = indexedDbTransactionQueues.get(transaction.db.name);
    if (!queue) return;
    const position = queue.indexOf(transaction);
    if (position >= 0) queue.splice(position, 1);
    if (queue.length === 0) indexedDbTransactionQueues.delete(transaction.db.name);
    else drainIndexedDbTransactions(transaction.db.name);
  }};
  const queueIndexedDbRequest = (transaction, operation) => {{
    if (transaction.__finished || transaction.__aborted) throw indexedDbError("TransactionInactiveError", "native IndexedDB transaction is inactive");
    const request = makeIndexedDbRequest();
    transaction.__pending += 1;
    transaction.__operationQueue.push({{ request, operation }});
    let queue = indexedDbTransactionQueues.get(transaction.db.name);
    if (!queue) {{
      queue = [];
      indexedDbTransactionQueues.set(transaction.db.name, queue);
    }}
    if (!queue.includes(transaction)) queue.push(transaction);
    drainIndexedDbTransactions(transaction.db.name);
    return request;
  }};
  const cloneIndexedDbDatabaseState = (databaseState) => JSON.parse(JSON.stringify(databaseState));
  const restoreIndexedDbDatabaseState = (databaseState, snapshot) => {{
    const stores = databaseState.stores || {{}};
    const snapshotStores = snapshot.stores || {{}};
    for (const name of Object.keys(stores)) {{
      if (!Object.prototype.hasOwnProperty.call(snapshotStores, name)) delete stores[name];
    }}
    for (const [name, snapshotStore] of Object.entries(snapshotStores)) {{
      const currentStore = stores[name];
      if (!currentStore) {{
        stores[name] = snapshotStore;
        continue;
      }}
      for (const key of Object.keys(currentStore)) delete currentStore[key];
      for (const [key, value] of Object.entries(snapshotStore)) currentStore[key] = value;
    }}
    databaseState.version = snapshot.version;
    databaseState.stores = stores;
  }};
  const makeIndexedDbTransaction = (database, databaseState, storeNames, mode, upgrade) => {{
    const transaction = {{
      db: database,
      mode,
      error: null,
      __storeNames: storeNames,
      __databaseState: databaseState,
      __initialState: null,
      __rolledBack: false,
      get objectStoreNames() {{ return storeNames.slice().sort(); }},
      oncomplete: null,
      onerror: null,
      onabort: null,
      __upgrade: upgrade,
      __pending: 0,
      __aborted: false,
      __finished: false,
      __finishScheduled: false,
      __abortDispatched: false,
      __operationQueue: [],
      __running: false,
      __rollback() {{
        if (this.__rolledBack) return;
        if (this.__initialState !== null) restoreIndexedDbDatabaseState(databaseState, this.__initialState);
        this.__rolledBack = true;
      }},
      abort() {{
        if (this.__finished) throw indexedDbError("InvalidStateError", "native IndexedDB transaction is inactive");
        if (this.__aborted) return;
        this.__aborted = true;
        this.error = indexedDbError("AbortError", "native IndexedDB transaction was aborted");
        this.__rollback();
        maybeFinishIndexedDbTransaction(this);
      }},
      objectStore(name) {{
        const normalizedName = indexedDbName(name, "object store name");
        if (!storeNames.includes(normalizedName) || !databaseState.stores[normalizedName]) throw indexedDbError("NotFoundError", "object store is not in this transaction");
        const store = databaseState.stores[normalizedName];
        return makeIndexedDbObjectStore(this, store, normalizedName);
      }},
    }};
    indexedDbSchedule(() => indexedDbSchedule(() => maybeFinishIndexedDbTransaction(transaction)));
    return transaction;
  }};
  const makeIndexedDbCursor = (transaction, request, entries, indexCursor, keyOnly, direction, source, store, resolveCursorRecord, ensureIndexConstraints) => {{
    let position = 0;
    let pending = false;
    let done = false;
    const currentEntry = () => entries[position];
    const currentKeyToken = () => {{
      const entry = currentEntry();
      return indexCursor ? entry.indexToken : entry.primaryToken;
    }};
    const queuePosition = (nextPosition) => {{
      if (done || pending || transaction.__finished || transaction.__aborted) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is inactive");
      pending = true;
      transaction.__pending += 1;
      indexedDbSchedule(() => {{
        pending = false;
        position = nextPosition;
        const entry = currentEntry();
        if (!entry) {{
          done = true;
          request.result = null;
        }} else {{
          request.result = cursor;
        }}
        request.readyState = "done";
        if (typeof request.onsuccess === "function") request.onsuccess.call(request, {{ target: request }});
        transaction.__pending -= 1;
        maybeFinishIndexedDbTransaction(transaction);
      }});
    }};
    const seekToKey = (token, start) => {{
      const forward = direction.startsWith("next");
      let candidate = start;
      while (candidate < entries.length) {{
        const comparison = indexedDbCompareTokens(currentKeyTokenFor(entries[candidate]), token);
        if ((forward && comparison >= 0) || (!forward && comparison <= 0)) break;
        candidate += 1;
      }}
      return candidate;
    }};
    const seekToPair = (indexToken, primaryToken, start) => {{
      const forward = direction.startsWith("next");
      let candidate = start;
      while (candidate < entries.length) {{
        const entry = entries[candidate];
        const indexComparison = indexedDbCompareTokens(entry.indexToken, indexToken);
        const comparison = indexComparison || indexedDbCompareTokens(entry.primaryToken, primaryToken);
        if ((forward && comparison >= 0) || (!forward && comparison <= 0)) break;
        candidate += 1;
      }}
      return candidate;
    }};
    const cursor = {{
      source,
      get direction() {{ return direction; }},
      get key() {{ const entry = currentEntry(); return entry ? indexedDbKeyValue(entry.indexToken) : undefined; }},
      get primaryKey() {{ const entry = currentEntry(); return entry ? indexedDbKeyValue(entry.primaryToken) : undefined; }},
      get request() {{ return request; }},
      get value() {{
        if (keyOnly) return undefined;
        const entry = currentEntry();
        return entry ? indexedDbStoredClone(entry.value) : undefined;
      }},
      continue(key) {{
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        let nextPosition = position + 1;
        if (key !== undefined) {{
          const token = indexedDbKeyToken(key, false);
          const comparison = indexedDbCompareTokens(token, currentKeyToken());
          if ((direction.startsWith("next") && comparison <= 0) || (direction.startsWith("prev") && comparison >= 0)) throw indexedDbError("DataError", "native IndexedDB cursor key must advance");
          nextPosition = seekToKey(token, nextPosition);
        }}
        queuePosition(nextPosition);
      }},
      advance(count) {{
        if (!Number.isInteger(Number(count)) || Number(count) < 1) throw indexedDbError("TypeError", "native IndexedDB cursor advance count must be positive");
        queuePosition(position + Number(count));
      }},
      continuePrimaryKey(key, primaryKey) {{
        if (!indexCursor) throw indexedDbError("InvalidAccessError", "native IndexedDB primary-key continuation requires an index cursor");
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        const indexToken = indexedDbKeyToken(key, false);
        const primaryToken = indexedDbKeyToken(primaryKey, false);
        const comparison = indexedDbCompareTokens(indexToken, currentKeyToken()) || indexedDbCompareTokens(primaryToken, currentEntry().primaryToken);
        if ((direction.startsWith("next") && comparison <= 0) || (direction.startsWith("prev") && comparison >= 0)) throw indexedDbError("DataError", "native IndexedDB cursor key must advance");
        queuePosition(seekToPair(indexToken, primaryToken, position + 1));
      }},
      update(value) {{
        if (keyOnly) throw indexedDbError("InvalidStateError", "native IndexedDB key cursor cannot update records");
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        if (transaction.mode === "readonly") throw indexedDbError("ReadOnlyError", "native IndexedDB transaction is read-only");
        const primaryToken = currentEntry().primaryToken;
        return queueIndexedDbRequest(transaction, () => {{
          const record = resolveCursorRecord(value, primaryToken);
          ensureIndexConstraints(record.token, record.value);
          store.records[record.token] = record.value;
          currentEntry().value = record.value;
          return record.key;
        }});
      }},
      delete() {{
        if (!currentEntry()) throw indexedDbError("InvalidStateError", "native IndexedDB cursor is exhausted");
        if (transaction.mode === "readonly") throw indexedDbError("ReadOnlyError", "native IndexedDB transaction is read-only");
        const primaryToken = currentEntry().primaryToken;
        return queueIndexedDbRequest(transaction, () => {{ delete store.records[primaryToken]; return undefined; }});
      }},
    }};
    const currentKeyTokenFor = (entry) => indexCursor ? entry.indexToken : entry.primaryToken;
    return cursor;
  }};
  const makeIndexedDbIndex = (transaction, store, name, metadata, resolveCursorRecord, ensureIndexConstraints) => {{
    const queryEntries = (query, direction) => indexedDbOrderedEntries(indexedDbIndexEntries(store, metadata), query, direction);
    let index;
    index = {{
      name,
      keyPath: metadata.key_path,
      multiEntry: metadata.multi_entry,
      unique: metadata.unique,
      get(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = queryEntries(range, "next")[0];
          return entry ? indexedDbStoredClone(entry.value) : undefined;
        }});
      }},
      getKey(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = queryEntries(range, "next")[0];
          return entry ? indexedDbKeyValue(entry.primaryToken) : undefined;
        }});
      }},
      getAll(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => queryEntries(range, "next").slice(0, limit).map(entry => indexedDbStoredClone(entry.value)));
      }},
      getAllKeys(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => queryEntries(range, "next").slice(0, limit).map(entry => indexedDbKeyValue(entry.primaryToken)));
      }},
      count(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => queryEntries(range, "next").length);
      }},
      openCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = queryEntries(range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, true, false, selectedDirection, index, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
      openKeyCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = queryEntries(range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, true, true, selectedDirection, index, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
    }};
    return index;
  }};
  const makeIndexedDbObjectStore = (transaction, store, name) => {{
    const requireWritable = () => {{
      if (transaction.mode === "readonly") throw indexedDbError("ReadOnlyError", "native IndexedDB transaction is read-only");
    }};
    const resolveRecord = (value, key) => {{
      const cloned = indexedDbClone(value);
      if (store.key_path && key !== undefined) throw indexedDbError("InvalidAccessError", "key path stores do not accept an explicit key");
      let resolved = store.key_path ? indexedDbReadKeyPath(cloned, store.key_path) : key;
      if (resolved === undefined && store.auto_increment) {{
        if (!Number.isSafeInteger(store.next_key) || store.next_key <= 0) throw indexedDbError("QuotaExceededError", "native IndexedDB key space exhausted");
        resolved = store.next_key;
        store.next_key += 1;
        indexedDbWriteKeyPath(cloned, store.key_path, resolved);
      }}
      const token = indexedDbKeyToken(resolved, false);
      return {{ token, value: indexedDbEncode(cloned), key: indexedDbKeyValue(token) }};
    }};
    const resolveCursorRecord = (value, primaryToken) => {{
      const cloned = indexedDbClone(value);
      const token = store.key_path
        ? indexedDbKeyToken(indexedDbReadKeyPath(cloned, store.key_path), false)
        : primaryToken;
      if (token !== primaryToken) throw indexedDbError("DataError", "native IndexedDB cursor update cannot change the primary key");
      return {{ token, value: indexedDbEncode(cloned), key: indexedDbKeyValue(token) }};
    }};
    const ensureIndexConstraints = (primaryToken, value) => {{
      for (const index of Object.values(store.indexes)) {{
        if (!index.unique) continue;
        const candidateKeys = indexedDbIndexKeys(value, index);
        for (const existingToken of Object.keys(store.records)) {{
          if (existingToken === primaryToken) continue;
          const existingKeys = indexedDbIndexKeys(store.records[existingToken], index);
          if (candidateKeys.some(candidate => existingKeys.includes(candidate))) throw indexedDbError("ConstraintError", "native IndexedDB unique index constraint failed");
        }}
      }}
    }};
    let objectStore;
    objectStore = {{
      name,
      keyPath: store.key_path,
      autoIncrement: store.auto_increment,
      get indexNames() {{ return Object.keys(store.indexes).sort(); }},
      createIndex(indexName, keyPath, options) {{
        if (!transaction.__upgrade || transaction.__finished) throw indexedDbError("InvalidStateError", "indexes can only be created during upgrade");
        const normalizedName = indexedDbName(indexName, "index name");
        if (store.indexes[normalizedName]) throw indexedDbError("ConstraintError", "index already exists");
        if (Object.keys(store.indexes).length >= indexedDbIndexLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB index limit exceeded");
        if (Array.isArray(keyPath)) throw indexedDbError("TypeError", "native IndexedDB compound index key paths are unsupported");
        const normalizedKeyPath = indexedDbName(keyPath, "index key path");
        const metadata = {{
          key_path: normalizedKeyPath,
          unique: Boolean(options && options.unique),
          multi_entry: Boolean(options && options.multiEntry),
        }};
        const seen = new Set();
        for (const value of Object.values(store.records)) {{
          for (const token of indexedDbIndexKeys(value, metadata)) {{
            if (metadata.unique && seen.has(token)) throw indexedDbError("ConstraintError", "native IndexedDB unique index constraint failed");
            seen.add(token);
          }}
        }}
        store.indexes[normalizedName] = metadata;
        return makeIndexedDbIndex(transaction, store, normalizedName, metadata, resolveCursorRecord, ensureIndexConstraints);
      }},
      deleteIndex(indexName) {{
        if (!transaction.__upgrade || transaction.__finished) throw indexedDbError("InvalidStateError", "indexes can only be deleted during upgrade");
        const normalizedName = indexedDbName(indexName, "index name");
        if (!store.indexes[normalizedName]) throw indexedDbError("NotFoundError", "index does not exist");
        delete store.indexes[normalizedName];
      }},
      get(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = indexedDbObjectEntries(store).find(candidate => indexedDbRangeIncludes(range, candidate.indexToken));
          return entry ? indexedDbStoredClone(entry.value) : undefined;
        }});
      }},
      getKey(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          const entry = indexedDbObjectEntries(store).find(candidate => indexedDbRangeIncludes(range, candidate.indexToken));
          return entry ? indexedDbKeyValue(entry.primaryToken) : undefined;
        }});
      }},
      getAll(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => indexedDbObjectEntries(store)
          .filter(entry => indexedDbRangeIncludes(range, entry.indexToken))
          .slice(0, limit)
          .map(entry => indexedDbStoredClone(entry.value)));
      }},
      getAllKeys(query, count) {{
        const range = indexedDbQueryRange(query);
        const limit = indexedDbQueryLimit(count);
        return queueIndexedDbRequest(transaction, () => indexedDbObjectEntries(store)
          .filter(entry => indexedDbRangeIncludes(range, entry.indexToken))
          .slice(0, limit)
          .map(entry => indexedDbKeyValue(entry.primaryToken)));
      }},
      count(query) {{
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => indexedDbObjectEntries(store)
          .filter(entry => indexedDbRangeIncludes(range, entry.indexToken)).length);
      }},
      openCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = indexedDbOrderedEntries(indexedDbObjectEntries(store), range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, false, false, selectedDirection, () => objectStore, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
      openKeyCursor(query, direction) {{
        const range = indexedDbQueryRange(query);
        const selectedDirection = indexedDbDirection(direction);
        const entries = indexedDbOrderedEntries(indexedDbObjectEntries(store), range, selectedDirection);
        const request = queueIndexedDbRequest(transaction, () => entries.length === 0 ? null : makeIndexedDbCursor(transaction, request, entries, false, true, selectedDirection, () => objectStore, store, resolveCursorRecord, ensureIndexConstraints));
        return request;
      }},
      index(indexName) {{
        const normalizedName = indexedDbName(indexName, "index name");
        const metadata = store.indexes[normalizedName];
        if (!metadata) throw indexedDbError("NotFoundError", "index does not exist");
        return makeIndexedDbIndex(transaction, store, normalizedName, metadata, resolveCursorRecord, ensureIndexConstraints);
      }},
      put(value, key) {{
        requireWritable();
        return queueIndexedDbRequest(transaction, () => {{
          const record = resolveRecord(value, key);
          if (store.records[record.token] === undefined && Object.keys(store.records).length >= indexedDbRecordLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB record limit exceeded");
          ensureIndexConstraints(record.token, record.value);
          store.records[record.token] = record.value;
          return record.key;
        }});
      }},
      add(value, key) {{
        requireWritable();
        return queueIndexedDbRequest(transaction, () => {{
          const record = resolveRecord(value, key);
          if (store.records[record.token] !== undefined) throw indexedDbError("ConstraintError", "native IndexedDB key already exists");
          if (Object.keys(store.records).length >= indexedDbRecordLimit) throw indexedDbError("QuotaExceededError", "native IndexedDB record limit exceeded");
          ensureIndexConstraints(record.token, record.value);
          store.records[record.token] = record.value;
          return record.key;
        }});
      }},
      delete(query) {{
        requireWritable();
        const range = indexedDbQueryRange(query);
        return queueIndexedDbRequest(transaction, () => {{
          for (const entry of indexedDbObjectEntries(store)) if (indexedDbRangeIncludes(range, entry.indexToken)) delete store.records[entry.primaryToken];
          return undefined;
        }});
      }},
      clear() {{
        requireWritable();
        return queueIndexedDbRequest(transaction, () => {{ store.records = {{}}; return undefined; }});
      }},
    }};
    return objectStore;
  }};
  const removeIndexedDbConnection = (database) => {{
    const connections = indexedDbConnections.get(database.name) || [];
    const remaining = connections.filter(candidate => candidate !== database && !candidate.__closed);
    if (remaining.length === 0) indexedDbConnections.delete(database.name);
    else indexedDbConnections.set(database.name, remaining);
    const pendingDelete = indexedDbPendingDeletes.get(database.name);
    if (pendingDelete && remaining.length === 0) {{
      indexedDbPendingDeletes.delete(database.name);
      indexedDbSchedule(() => processIndexedDbDelete(pendingDelete, database.name));
      return;
    }}
    const pending = indexedDbPendingOpens.get(database.name);
    if (pending && remaining.length === 0) {{
      indexedDbPendingOpens.delete(database.name);
      indexedDbSchedule(() => processIndexedDbOpen(pending.request, database.name, pending.requestedVersion));
    }}
  }};
  const registerIndexedDbConnection = (database) => {{
    const connections = indexedDbConnections.get(database.name) || [];
    connections.push(database);
    indexedDbConnections.set(database.name, connections);
  }};
  const makeIndexedDbDatabase = (name, databaseState, upgradeTransaction) => {{
    let closed = false;
    const database = {{
      name,
      __upgradeTransaction: upgradeTransaction,
      get __closed() {{ return closed; }},
      onversionchange: null,
      onclose: null,
      get version() {{ return databaseState.version; }},
      get objectStoreNames() {{ return Object.keys(databaseState.stores).sort(); }},
      createObjectStore(storeName, options) {{
        const activeUpgradeTransaction = database.__upgradeTransaction;
        if (!activeUpgradeTransaction || activeUpgradeTransaction.__finished) throw indexedDbError("InvalidStateError", "object stores can only be created during upgrade");
        const normalizedName = indexedDbName(storeName, "object store name");
        if (Object.keys(databaseState.stores).length >= indexedDbStoreLimit && !databaseState.stores[normalizedName]) throw indexedDbError("QuotaExceededError", "native IndexedDB object store limit exceeded");
        if (databaseState.stores[normalizedName]) throw indexedDbError("ConstraintError", "native IndexedDB object store already exists");
        const keyPath = options && options.keyPath !== undefined && options.keyPath !== null
          ? indexedDbName(options.keyPath, "key path")
          : null;
        const autoIncrement = Boolean(options && options.autoIncrement);
        if (autoIncrement && !keyPath) throw indexedDbError("InvalidAccessError", "auto-increment stores require a key path");
        databaseState.stores[normalizedName] = {{
          key_path: keyPath,
          auto_increment: autoIncrement,
          next_key: 1,
          indexes: {{}},
          records: {{}},
        }};
        if (!activeUpgradeTransaction.__storeNames.includes(normalizedName)) activeUpgradeTransaction.__storeNames.push(normalizedName);
        return makeIndexedDbObjectStore(activeUpgradeTransaction, databaseState.stores[normalizedName], normalizedName);
      }},
      deleteObjectStore(storeName) {{
        const activeUpgradeTransaction = database.__upgradeTransaction;
        if (!activeUpgradeTransaction || activeUpgradeTransaction.__finished) throw indexedDbError("InvalidStateError", "object stores can only be deleted during upgrade");
        const normalizedName = indexedDbName(storeName, "object store name");
        if (!databaseState.stores[normalizedName]) throw indexedDbError("NotFoundError", "object store does not exist");
        delete databaseState.stores[normalizedName];
        const position = activeUpgradeTransaction.__storeNames.indexOf(normalizedName);
        if (position >= 0) activeUpgradeTransaction.__storeNames.splice(position, 1);
      }},
      transaction(storeNames, mode) {{
        if (closed) throw indexedDbError("InvalidStateError", "native IndexedDB database is closed");
        const names = Array.isArray(storeNames) ? storeNames.map(value => indexedDbName(value, "object store name")) : [indexedDbName(storeNames, "object store name")];
        const selectedMode = mode === undefined ? "readonly" : String(mode);
        if (!["readonly", "readwrite"].includes(selectedMode)) throw indexedDbError("TypeError", "native IndexedDB transaction mode is unsupported");
        for (const storeName of names) if (!databaseState.stores[storeName]) throw indexedDbError("NotFoundError", "object store does not exist");
        return makeIndexedDbTransaction(database, databaseState, names, selectedMode, false);
      }},
      close() {{
        if (closed) return;
        closed = true;
        removeIndexedDbConnection(database);
        if (typeof database.onclose === "function") database.onclose.call(database, {{ target: database }});
      }},
    }};
    return database;
  }};
  const processIndexedDbOpen = (request, normalizedName, requestedVersion) => {{
    const databaseState = indexedDbState().databases[normalizedName];
    const oldVersion = databaseState ? Number(databaseState.version) : 0;
    const nextVersion = requestedVersion === undefined ? (databaseState ? oldVersion : 1) : requestedVersion;
    if (databaseState && nextVersion < oldVersion) {{
      finishIndexedDbRequest(request, undefined, indexedDbError("VersionError", "native IndexedDB version is older than the database"), null);
      return;
    }}
    const upgraded = nextVersion > oldVersion;
    if (upgraded) {{
      const connections = indexedDbConnections.get(normalizedName) || [];
      for (const connection of connections) {{
        if (connection.__closed || typeof connection.onversionchange !== "function") continue;
        try {{ connection.onversionchange.call(connection, {{ target: connection, oldVersion, newVersion: nextVersion }}); }} catch (_error) {{}}
      }}
      const remaining = (indexedDbConnections.get(normalizedName) || []).filter(connection => !connection.__closed);
      if (remaining.length > 0) {{
        indexedDbConnections.set(normalizedName, remaining);
        if (!request.__blocked) {{
          request.__blocked = true;
          indexedDbPendingOpens.set(normalizedName, {{ request, requestedVersion }});
          indexedDbSchedule(() => {{
            if (typeof request.onblocked === "function") request.onblocked.call(request, {{ target: request, oldVersion, newVersion: nextVersion }});
          }});
        }}
        return;
      }}
      indexedDbConnections.delete(normalizedName);
    }}
    let nextDatabaseState = databaseState;
    if (!nextDatabaseState) {{
      if (Object.keys(indexedDbState().databases).length >= indexedDbDatabaseLimit) {{
        finishIndexedDbRequest(request, undefined, indexedDbError("QuotaExceededError", "native IndexedDB database limit exceeded"), null);
        return;
      }}
      nextDatabaseState = {{ version: nextVersion, stores: {{}} }};
      indexedDbState().databases[normalizedName] = nextDatabaseState;
    }} else if (upgraded) {{
      nextDatabaseState.version = nextVersion;
    }}
    const database = makeIndexedDbDatabase(normalizedName, nextDatabaseState, null);
    registerIndexedDbConnection(database);
    if (upgraded) {{
      const upgradeTransaction = makeIndexedDbTransaction(database, nextDatabaseState, Object.keys(nextDatabaseState.stores), "versionchange", true);
      database.__upgradeTransaction = upgradeTransaction;
      request.result = database;
      request.transaction = upgradeTransaction;
      try {{
        if (typeof request.onupgradeneeded === "function") request.onupgradeneeded.call(request, {{ target: request, oldVersion, newVersion: nextVersion }});
        finishIndexedDbRequest(request, database, null, null);
      }} catch (error) {{
        finishIndexedDbRequest(request, undefined, error instanceof Error ? error : indexedDbError("AbortError", String(error)), null);
      }}
    }} else {{
      finishIndexedDbRequest(request, database, null, null);
    }}
  }};
  const processIndexedDbDelete = (request, normalizedName) => {{
    const databaseState = indexedDbState().databases[normalizedName];
    if (!databaseState) {{
      finishIndexedDbRequest(request, undefined, null, null);
      return;
    }}
    const oldVersion = Number(databaseState.version);
    const connections = indexedDbConnections.get(normalizedName) || [];
    if (!request.__versionChangeNotified) {{
      request.__versionChangeNotified = true;
      for (const connection of connections) {{
        if (connection.__closed || typeof connection.onversionchange !== "function") continue;
        try {{ connection.onversionchange.call(connection, {{ target: connection, oldVersion, newVersion: null }}); }} catch (_error) {{}}
      }}
    }}
    const remaining = (indexedDbConnections.get(normalizedName) || []).filter(connection => !connection.__closed);
    if (remaining.length > 0) {{
      indexedDbConnections.set(normalizedName, remaining);
      if (!request.__blocked) {{
        request.__blocked = true;
        indexedDbPendingDeletes.set(normalizedName, request);
        indexedDbSchedule(() => {{
          if (typeof request.onblocked === "function") request.onblocked.call(request, {{ target: request, oldVersion, newVersion: null }});
        }});
      }}
      return;
    }}
    indexedDbConnections.delete(normalizedName);
    delete indexedDbState().databases[normalizedName];
    finishIndexedDbRequest(request, undefined, null, null);
  }};
  const nativeIndexedDB = {{
    __glassNativeIndexedDB: true,
    open(name, version) {{
      const normalizedName = indexedDbName(name, "database name");
      if (version !== undefined && (!Number.isSafeInteger(Number(version)) || Number(version) < 1)) throw indexedDbError("TypeError", "native IndexedDB version must be a positive integer");
      const requestedVersion = version === undefined ? undefined : Number(version);
      const request = makeIndexedDbRequest();
      indexedDbSchedule(() => processIndexedDbOpen(request, normalizedName, requestedVersion));
      return request;
    }},
    deleteDatabase(name) {{
      const normalizedName = indexedDbName(name, "database name");
      const request = makeIndexedDbRequest();
      indexedDbSchedule(() => processIndexedDbDelete(request, normalizedName));
      return request;
    }},
    databases() {{
      return Promise.resolve(Object.keys(indexedDbState().databases).map(name => ({{ name, version: indexedDbState().databases[name].version }})));
    }},
  }};
  const nativeIDBKeyRange = {{
    only(value) {{ const token = indexedDbKeyToken(value, false); return makeIndexedDbKeyRange(token, token, false, false); }},
    lowerBound(value, open) {{ return makeIndexedDbKeyRange(indexedDbKeyToken(value, false), undefined, Boolean(open), false); }},
    upperBound(value, open) {{ return makeIndexedDbKeyRange(undefined, indexedDbKeyToken(value, false), false, Boolean(open)); }},
    bound(lower, upper, lowerOpen, upperOpen) {{ return makeIndexedDbKeyRange(indexedDbKeyToken(lower, false), indexedDbKeyToken(upper, false), Boolean(lowerOpen), Boolean(upperOpen)); }},
  }};
  globalThis.indexedDB = nativeIndexedDB;
  globalThis.__glassNativeIndexedDB = nativeIndexedDB;
  globalThis.IDBKeyRange = nativeIDBKeyRange;
  globalThis.__glassNativeIndexedDBInstalled = true;
  }} else if (globalThis.indexedDB !== globalThis.__glassNativeIndexedDB) {{
    globalThis.indexedDB = globalThis.__glassNativeIndexedDB;
  }}
  globalThis.__glassTimers = timers;
  globalThis.__glassRunningTimers = runningTimers;
  globalThis.__glassNextTimerId = nextTimerId;
  globalThis.__glassAnimationFrames = animationFrames;
  globalThis.__glassNextAnimationFrameId = nextAnimationFrameId;
  globalThis.__glassIdleCallbacks = idleCallbacks;
  globalThis.__glassNextIdleCallbackId = nextIdleCallbackId;
  globalThis.setTimeout = setTimeoutNative;
  globalThis.setInterval = setIntervalNative;
  globalThis.clearTimeout = clearTimer;
  globalThis.clearInterval = clearTimer;
  globalThis.requestAnimationFrame = requestAnimationFrameNative;
  globalThis.cancelAnimationFrame = cancelAnimationFrameNative;
  globalThis.requestIdleCallback = requestIdleCallbackNative;
  globalThis.cancelIdleCallback = cancelIdleCallbackNative;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("module_storage_indexeddb");
