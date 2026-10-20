  const workerUrlSearchParamsEntryLimit = __GLASS_WORKER_WORKER_URL_SEARCH_PARAMS_ENTRIES_LIMIT__;
  const workerUrlSearchParamsBytesLimit = __GLASS_WORKER_WORKER_URL_SEARCH_PARAMS_BYTES_LIMIT__;
  const workerSearchParamsDecode = (value) => {
    try { return decodeURIComponent(String(value).replace(/\\+/g, " ")); }
    catch (_) { return String(value); }
  };
  const workerSearchParamsEncode = (value) => encodeURIComponent(String(value))
    .replace(/[!'()~]/g, character => "%" + character.charCodeAt(0).toString(16).toUpperCase())
    .replace(/%20/g, "+");
  const WorkerURLSearchParamsNative = function(init) {
    this.__glassWorkerSearchParams = true;
    this._entries = [];
    const append = (name, value) => {
      const pair = [String(name), String(value)];
      const nextBytes = this._entries.reduce((total, entry) => total + entry[0].length + entry[1].length, 0)
        + pair[0].length + pair[1].length;
      if (this._entries.length >= workerUrlSearchParamsEntryLimit)
        throw new RangeError("native Worker URLSearchParams entry limit exceeded");
      if (nextBytes > workerUrlSearchParamsBytesLimit)
        throw new RangeError("native Worker URLSearchParams exceeds its byte limit");
      this._entries.push(pair);
    };
    if (init === undefined || init === null) return;
    const source = init && init.__glassWorkerSearchParams === true ? init._entries : init;
    if (typeof source === "string") {
      const query = source.startsWith("?") ? source.slice(1) : source;
      if (query) query.split("&").forEach(part => {
        const separator = part.indexOf("=");
        const name = separator < 0 ? part : part.slice(0, separator);
        const value = separator < 0 ? "" : part.slice(separator + 1);
        append(workerSearchParamsDecode(name), workerSearchParamsDecode(value));
      });
      return;
    }
    if (source && typeof source[Symbol.iterator] === "function") {
      for (const entry of source) {
        const pair = Array.from(entry || []);
        if (pair.length !== 2) throw new TypeError("native Worker URLSearchParams pairs must contain two values");
        append(pair[0], pair[1]);
      }
      return;
    }
    if (typeof source !== "object") throw new TypeError("native Worker URLSearchParams input is invalid");
    for (const name of Object.keys(source)) append(name, source[name]);
  };
  WorkerURLSearchParamsNative.prototype.append = function(name, value) {
    const current = new WorkerURLSearchParamsNative(this._entries);
    current._entries.push([String(name), String(value)]);
    if (current._entries.length > workerUrlSearchParamsEntryLimit)
      throw new RangeError("native Worker URLSearchParams entry limit exceeded");
    const bytes = current._entries.reduce((total, entry) => total + entry[0].length + entry[1].length, 0);
    if (bytes > workerUrlSearchParamsBytesLimit)
      throw new RangeError("native Worker URLSearchParams exceeds its byte limit");
    this._entries = current._entries;
  };
  WorkerURLSearchParamsNative.prototype.set = function(name, value) {
    const key = String(name);
    const replacement = [key, String(value)];
    const index = this._entries.findIndex(entry => entry[0] === key);
    const nextEntries = index < 0
      ? this._entries.concat([replacement])
      : this._entries.map((entry, entryIndex) => entry[0] === key
        ? (entryIndex === index ? replacement : null)
        : entry).filter(Boolean);
    if (nextEntries.length > workerUrlSearchParamsEntryLimit)
      throw new RangeError("native Worker URLSearchParams entry limit exceeded");
    const bytes = nextEntries.reduce((total, entry) => total + entry[0].length + entry[1].length, 0);
    if (bytes > workerUrlSearchParamsBytesLimit)
      throw new RangeError("native Worker URLSearchParams exceeds its byte limit");
    this._entries = nextEntries;
  };
  WorkerURLSearchParamsNative.prototype.delete = function(name, value) {
    const key = String(name);
    if (arguments.length < 2) this._entries = this._entries.filter(entry => entry[0] !== key);
    else this._entries = this._entries.filter(entry => entry[0] !== key || entry[1] !== String(value));
  };
  WorkerURLSearchParamsNative.prototype.get = function(name) {
    const entry = this._entries.find(candidate => candidate[0] === String(name));
    return entry ? entry[1] : null;
  };
  WorkerURLSearchParamsNative.prototype.getAll = function(name) {
    return this._entries.filter(entry => entry[0] === String(name)).map(entry => entry[1]);
  };
  WorkerURLSearchParamsNative.prototype.has = function(name, value) {
    const key = String(name);
    return arguments.length < 2
      ? this._entries.some(entry => entry[0] === key)
      : this._entries.some(entry => entry[0] === key && entry[1] === String(value));
  };
  WorkerURLSearchParamsNative.prototype.toString = function() {
    return this._entries.map(entry => workerSearchParamsEncode(entry[0]) + "=" + workerSearchParamsEncode(entry[1])).join("&");
  };
  WorkerURLSearchParamsNative.prototype.entries = function() {
    let index = 0;
    const owner = this;
    return {
      next() {
        if (index >= owner._entries.length) return { value: undefined, done: true };
        return { value: owner._entries[index++].slice(), done: false };
      },
      [Symbol.iterator]() { return this; },
    };
  };
  WorkerURLSearchParamsNative.prototype.keys = function() {
    const iterator = this.entries();
    return { next() { const step = iterator.next(); return step.done ? step : { value: step.value[0], done: false }; }, [Symbol.iterator]() { return this; } };
  };
  WorkerURLSearchParamsNative.prototype.values = function() {
    const iterator = this.entries();
    return { next() { const step = iterator.next(); return step.done ? step : { value: step.value[1], done: false }; }, [Symbol.iterator]() { return this; } };
  };
  WorkerURLSearchParamsNative.prototype.forEach = function(callback, thisArg) {
    if (typeof callback !== "function") throw new TypeError("native Worker URLSearchParams callback must be callable");
    this._entries.slice().forEach(entry => callback.call(thisArg, entry[1], entry[0], this));
  };
  WorkerURLSearchParamsNative.prototype.sort = function() {
    this._entries = this._entries
      .map((entry, index) => [entry, index])
      .sort((left, right) => left[0][0] < right[0][0] ? -1 : left[0][0] > right[0][0] ? 1 : left[1] - right[1])
      .map(entry => entry[0]);
  };
  WorkerURLSearchParamsNative.prototype[Symbol.iterator] = WorkerURLSearchParamsNative.prototype.entries;
  Object.defineProperty(WorkerURLSearchParamsNative.prototype, "size", { get() { return this._entries.length; } });
  WorkerURLSearchParamsNative.prototype.toJSON = function() { return this.toString(); };
  globalThis.URLSearchParams = WorkerURLSearchParamsNative;
  const workerUrlParts = (input) => {
    const source = String(input);
    const hashIndex = source.indexOf("#");
    const hash = hashIndex < 0 ? "" : source.slice(hashIndex);
    const withoutHash = hashIndex < 0 ? source : source.slice(0, hashIndex);
    const queryIndex = withoutHash.indexOf("?");
    const search = queryIndex < 0 ? "" : withoutHash.slice(queryIndex);
    const main = queryIndex < 0 ? withoutHash : withoutHash.slice(0, queryIndex);
    const match = main.match(/^([A-Za-z][A-Za-z0-9+.-]*:)(?:\/\/([^/]*))?(.*)$/);
    if (!match) throw new TypeError("native Worker URL requires an absolute or resolvable URL");
    const protocol = match[1].toLowerCase();
    const authority = match[2] || "";
    const hasAuthorityDelimiter = main.slice(match[1].length).startsWith("//");
    let pathname = match[3] || "";
    if (authority && !pathname && ["http:", "https:"].includes(protocol)) pathname = "/";
    const at = authority.lastIndexOf("@");
    const userInfo = at < 0 ? "" : authority.slice(0, at);
    const credentials = userInfo.indexOf(":");
    const username = credentials < 0 ? userInfo : userInfo.slice(0, credentials);
    const password = credentials < 0 ? "" : userInfo.slice(credentials + 1);
    const host = at < 0 ? authority : authority.slice(at + 1);
    let hostname = host;
    let port = "";
    if (host.startsWith("[")) {
      const closing = host.indexOf("]");
      if (closing >= 0) {
        hostname = host.slice(0, closing + 1);
        if (host.slice(closing + 1, closing + 2) === ":") port = host.slice(closing + 2);
      }
    } else {
      const portIndex = host.lastIndexOf(":");
      if (portIndex > 0 && /^\d*$/.test(host.slice(portIndex + 1))) {
        hostname = host.slice(0, portIndex);
        port = host.slice(portIndex + 1);
      }
    }
    const prefix = authority || (protocol === "file:" && hasAuthorityDelimiter)
      ? protocol + "//" + authority
      : protocol;
    const originProtocol = protocol === "ws:" ? "http:" : protocol === "wss:" ? "https:" : protocol;
    const origin = ["http:", "https:"].includes(originProtocol) && host
      ? originProtocol + "//" + host.toLowerCase()
      : "null";
    return { protocol, authority, host, hostname, port, username, password, pathname, search, hash, origin, hasAuthorityDelimiter, href: prefix + pathname + search + hash };
  };
  const workerUrlResolve = (input, base) => {
    const value = String(input);
    const baseHref = base === undefined
      ? undefined
      : base && base.__glassUrl === true ? base.href : String(base);
    try {
      return __glassNativeCanonicalUrl(value, baseHref);
    } catch (_) {
      throw new TypeError("native Worker URL is invalid");
    }
  };
  const WorkerURLNative = function(input, base) {
    const source = input && input.__glassUrl === true ? input.href : input;
    const parts = workerUrlParts(workerUrlResolve(source, base));
    const state = {
      hasAuthority: Boolean(parts.authority),
      prefix: parts.authority || (parts.protocol === "file:" && parts.hasAuthorityDelimiter)
        ? parts.protocol + "//" + parts.authority
        : parts.protocol,
      origin: parts.origin,
      protocol: parts.protocol,
      username: parts.username,
      password: parts.password,
      host: parts.host,
      hostname: parts.hostname,
      port: parts.port,
      pathname: parts.pathname,
      search: parts.search,
      hash: parts.hash,
    };
    const searchParams = new WorkerURLSearchParamsNative(parts.search);
    const updateParts = next => {
      state.hasAuthority = Boolean(next.authority);
      state.prefix = next.authority || (next.protocol === "file:" && next.hasAuthorityDelimiter)
        ? next.protocol + "//" + next.authority
        : next.protocol;
      state.origin = next.origin;
      state.protocol = next.protocol;
      state.username = next.username;
      state.password = next.password;
      state.host = next.host;
      state.hostname = next.hostname;
      state.port = next.port;
      state.pathname = next.pathname;
      state.search = next.search;
      state.hash = next.hash;
      searchParams._entries = new WorkerURLSearchParamsNative(next.search)._entries;
    };
    const credentialsFor = (username, password) => username || password
      ? username + (password ? ":" + password : "") + "@"
      : "";
    const currentCredentials = () => credentialsFor(state.username, state.password);
    const currentAuthority = () => currentCredentials() + state.host;
    const requireAuthority = () => {
      if (!state.hasAuthority) throw new TypeError("native Worker URL authority mutation requires a host");
    };
    const replaceAuthority = (authority, protocol = state.protocol) => {
      requireAuthority();
      updateParts(workerUrlParts(workerUrlResolve(protocol + "//" + authority + state.pathname + state.search + state.hash)));
    };
    const validateHost = value => {
      const source = String(value);
      if (!source || /[\/\?#@\s]/.test(source)) throw new TypeError("native Worker URL host is invalid");
      if (!source.startsWith("[") && source.includes(":") && !/:[0-9]+$/.test(source))
        throw new TypeError("native Worker URL host port is invalid");
      const parsed = workerUrlParts(workerUrlResolve(state.protocol + "//" + source + "/"));
      if (!parsed.host || (parsed.port && Number(parsed.port) > 65535))
        throw new TypeError("native Worker URL host is invalid");
      return parsed.host;
    };
    const normalizeCredential = value => encodeURIComponent(String(value));
    const currentHref = () => state.prefix + state.pathname + state.search + state.hash;
    const syncSearch = () => {
      const encoded = searchParams.toString();
      state.search = encoded ? "?" + encoded : "";
    };
    for (const method of ["append", "set", "delete", "sort"]) {
      const original = searchParams[method];
      searchParams[method] = function(...args) {
        const result = original.apply(searchParams, args);
        syncSearch();
        return result;
      };
    }
    Object.defineProperty(this, "__glassUrl", { value: true });
    Object.defineProperty(this, "href", {
      enumerable: true,
      get: currentHref,
      set: value => updateParts(workerUrlParts(workerUrlResolve(value, currentHref()))),
    });
    const define = (name, getter) => Object.defineProperty(this, name, { enumerable: true, get: getter });
    define("origin", () => state.origin);
    Object.defineProperty(this, "protocol", {
      enumerable: true,
      get: () => state.protocol,
      set: value => {
        const source = String(value).toLowerCase();
        const protocol = source.endsWith(":") ? source : source + ":";
        if (!["http:", "https:"].includes(protocol)) throw new TypeError("native Worker URL protocol is unsupported");
        if (protocol !== state.protocol) replaceAuthority(currentAuthority(), protocol);
      },
    });
    Object.defineProperty(this, "username", {
      enumerable: true,
      get: () => state.username,
      set: value => replaceAuthority(credentialsFor(normalizeCredential(value), state.password) + state.host),
    });
    Object.defineProperty(this, "password", {
      enumerable: true,
      get: () => state.password,
      set: value => replaceAuthority(credentialsFor(state.username, normalizeCredential(value)) + state.host),
    });
    Object.defineProperty(this, "host", {
      enumerable: true,
      get: () => state.host,
      set: value => replaceAuthority(currentCredentials() + validateHost(value)),
    });
    Object.defineProperty(this, "hostname", {
      enumerable: true,
      get: () => state.hostname,
      set: value => {
        const source = String(value);
        if (!source || /[\/\?#@\s]/.test(source) || (!source.startsWith("[") && source.includes(":")))
          throw new TypeError("native Worker URL hostname is invalid");
        if (source.startsWith("[") && !/^\[[^\]]+\]$/.test(source))
          throw new TypeError("native Worker URL hostname is invalid");
        const host = source + (state.port ? ":" + state.port : "");
        replaceAuthority(currentCredentials() + validateHost(host));
      },
    });
    Object.defineProperty(this, "port", {
      enumerable: true,
      get: () => state.port,
      set: value => {
        const source = String(value);
        if (source && (!/^[0-9]+$/.test(source) || Number(source) > 65535))
          throw new TypeError("native Worker URL port is invalid");
        const host = state.hostname + (source ? ":" + source : "");
        replaceAuthority(currentCredentials() + validateHost(host));
      },
    });
    Object.defineProperty(this, "pathname", {
      enumerable: true,
      get: () => state.pathname,
      set: value => {
        const source = String(value).replace(/\\/g, "%5C").replace(/[\?#]/g, character => encodeURIComponent(character));
        const path = source.startsWith("/") ? source : "/" + source;
        const canonical = __glassNativeCanonicalUrl("http://glass.invalid" + path, undefined);
        state.pathname = workerUrlParts(canonical).pathname;
      },
    });
    Object.defineProperty(this, "search", {
      enumerable: true,
      get: () => state.search,
      set: value => {
        const source = String(value);
        searchParams._entries = new WorkerURLSearchParamsNative(source)._entries;
        syncSearch();
      },
    });
    Object.defineProperty(this, "hash", {
      enumerable: true,
      get: () => state.hash,
      set: value => {
        const source = String(value);
        const canonical = __glassNativeCanonicalUrl("http://glass.invalid/" + (!source ? "" : source.startsWith("#") ? source : "#" + source), undefined);
        state.hash = workerUrlParts(canonical).hash;
      },
    });
    define("searchParams", () => searchParams);
    Object.freeze(this);
  };
  WorkerURLNative.prototype.toString = function() { return this.href; };
  WorkerURLNative.prototype.toJSON = function() { return this.href; };
  globalThis.URL = WorkerURLNative;
  const workerObjectUrlRegistry = globalThis.__glassWorkerObjectUrlRegistry instanceof Map
    ? globalThis.__glassWorkerObjectUrlRegistry
    : new Map();
  let workerNextObjectUrlId = Number.isSafeInteger(globalThis.__glassWorkerNextObjectUrlId)
    ? globalThis.__glassWorkerNextObjectUrlId
    : 1;
  const workerObjectUrlOrigin = () => {
    try { return workerUrlParts(workerUrlResolve(workerUrl, undefined)).origin; }
    catch (_) { return "null"; }
  };
  WorkerURLNative.createObjectURL = function(object) {
    if (!object || object.__glassWorkerBlob !== true)
      throw new TypeError("native Worker URL.createObjectURL requires a Blob");
    if (workerObjectUrlRegistry.size >= __GLASS_WORKER_MAX_COMMANDS__)
      throw new RangeError("native Worker object URL limit exceeded");
    const origin = workerObjectUrlOrigin();
    let url;
    do {
      url = "blob:" + origin + "/glass-worker-" + workerNextObjectUrlId;
      workerNextObjectUrlId += 1;
    } while (workerObjectUrlRegistry.has(url));
    workerObjectUrlRegistry.set(url, { blob: object, origin });
    globalThis.__glassWorkerNextObjectUrlId = workerNextObjectUrlId;
    return url;
  };
  WorkerURLNative.revokeObjectURL = function(value) {
    workerObjectUrlRegistry.delete(String(value));
  };
  globalThis.__glassWorkerObjectUrlRegistry = workerObjectUrlRegistry;
  const workerLocationUrl = new WorkerURLNative(workerUrl);
  const workerLocation = {};
  for (const name of ["href", "protocol", "host", "hostname", "port", "pathname", "search", "hash", "origin"]) {
    Object.defineProperty(workerLocation, name, { enumerable: true, get: () => workerLocationUrl[name] });
  }
  workerLocation.toString = () => workerLocationUrl.href;
  globalThis.location = Object.freeze(workerLocation);
  const workerNavigator = globalThis.__glassWorkerNavigator instanceof Object
    ? globalThis.__glassWorkerNavigator
    : Object.freeze({
        userAgent: "GlassNative",
        language: "en-US",
        languages: Object.freeze(["en-US"]),
        onLine: true,
        cookieEnabled: true,
        hardwareConcurrency: 1,
        platform: "GlassNative",
        product: "Gecko",
        vendor: "",
      });
  globalThis.__glassWorkerNavigator = workerNavigator;
  globalThis.navigator = workerNavigator;
