pub(in crate::native_engine::javascript) const NATIVE_SERVICE_WORKER_BOOTSTRAP: &str = r###"
  globalThis.__glassReportWorkerMessagePortCallbackException = (error) =>
    reportWorkerCallbackException(error, false);
  const rejectServiceWorkerDynamicImport = (specifier, options) => {
    const intrinsics = globalThis.__glassDynamicImportIntrinsics;
    return new intrinsics.PromiseConstructor((resolve, reject) => {
      try {
        intrinsics.normalize(specifier, options);
      } catch (error) {
        reject(error);
        return;
      }
      reject(new intrinsics.TypeErrorConstructor(
        "dynamic import is not permitted in Service Workers",
      ));
    });
  };
  if (globalThis.__glassRejectDynamicImport === undefined) {
    Object.defineProperty(globalThis, "__glassRejectDynamicImport", {
      value: rejectServiceWorkerDynamicImport,
      enumerable: false,
      configurable: false,
      writable: false,
    });
  }
  globalThis.__glassServiceWorkerClientState = null;
  globalThis.__glassServiceWorkerClients = [];
  globalThis.__glassSetServiceWorkerClients = (clients) => {
    if (!Array.isArray(clients)) throw new TypeError("native service worker clients are invalid");
    globalThis.__glassServiceWorkerClients = clients;
    return null;
  };
  const serviceWorkerCachePendingRequests = globalThis.__glassServiceWorkerCachePendingRequests instanceof Map
    ? globalThis.__glassServiceWorkerCachePendingRequests
    : new Map();
  let nextServiceWorkerCacheRequestId = Number.isSafeInteger(globalThis.__glassNextServiceWorkerCacheRequestId)
    ? globalThis.__glassNextServiceWorkerCacheRequestId
    : 1;
  const serviceWorkerOpenWindowPendingRequests = globalThis.__glassServiceWorkerOpenWindowPendingRequests instanceof Map
    ? globalThis.__glassServiceWorkerOpenWindowPendingRequests
    : new Map();
  let nextServiceWorkerOpenWindowRequestId = Number.isSafeInteger(globalThis.__glassNextServiceWorkerOpenWindowRequestId)
    ? globalThis.__glassNextServiceWorkerOpenWindowRequestId
    : 1;
  const serviceWorkerFetchResponse = (response) => {
    if (!response || typeof response.status !== "number" || typeof response.clone !== "function")
      return Promise.reject(new TypeError("service worker fetch handler must resolve to a Response"));
    const clone = response.clone();
    const bodyPromise = clone.body === null
      ? Promise.resolve(new Uint8Array([]))
      : clone.arrayBuffer().then(buffer => new Uint8Array(buffer));
    return bodyPromise.then(bytes => {
      if (bytes.length > __GLASS_SERVICE_WORKER_BODY_LIMIT__)
        throw new RangeError("service worker response body exceeds its limit");
      return {
        status: Number(response.status),
        statusText: String(response.statusText || ""),
        url: String(response.url || ""),
        redirected: response.redirected === true,
        headers: response.headers && typeof response.headers.entries === "function"
          ? Array.from(response.headers.entries()) : [],
        contentType: response.headers && typeof response.headers.get === "function"
          ? response.headers.get("content-type") : null,
        bodyNull: clone.body === null,
        bodyBase64: encodeWorkerBase64(Array.from(bytes), __GLASS_SERVICE_WORKER_BODY_LIMIT__),
      };
    });
  };
  const serviceWorkerCacheQueueRequest = (kind, payload) => {
    const requestId = nextServiceWorkerCacheRequestId++;
    globalThis.__glassNextServiceWorkerCacheRequestId = nextServiceWorkerCacheRequestId;
    return new Promise((resolve, reject) => {
      serviceWorkerCachePendingRequests.set(requestId, { resolve, reject, kind, cacheName: payload.cache_name });
      try { pushCommand(Object.assign({ kind, request_id: requestId }, payload)); }
      catch (error) {
        serviceWorkerCachePendingRequests.delete(requestId);
        reject(error);
      }
    });
  };
  const serviceWorkerWindowClientFromState = (state) => {
    if (!state || typeof state !== "object") return null;
    const client = {
      id: String(state.clientId || ""),
      url: String(state.clientUrl || ""),
      type: String(state.clientType || "window"),
      frameType: String(state.frameType || "top-level"),
      visibilityState: String(state.visibilityState || "visible"),
      focused: state.focused === true,
      postMessage(message, options) {
        const workerId = Number.isSafeInteger(globalThis.__glassWorkerId)
          ? globalThis.__glassWorkerId : 0;
        if (!workerId) throw new Error("native service worker client is unavailable");
        const envelope = glassMessageCloneWithTransfers(
          message,
          glassMessageTransferList(options),
        );
        pushCommand({
          kind: "serviceWorkerClientPostMessage",
          worker_id: workerId,
          client_id: this.id,
          data: envelope.data,
          transfer_ports: envelope.transfer_ports,
        });
      },
    };
    return Object.freeze(client);
  };
  const serviceWorkerCacheRequestKey = (input) => {
    const source = input && input.__glassWorkerRequest === true
      ? input
      : new WorkerRequestNative(
          input && input.__glassUrl === true ? input.href : workerUrlResolve(input, workerUrl),
        );
    const url = new WorkerURLNative(source.url, workerUrl);
    if (!["http:", "https:"].includes(url.protocol))
      throw new TypeError("native Cache request URL must use HTTP(S)");
    const hash = url.href.indexOf("#");
    return {
      url: hash < 0 ? url.href : url.href.slice(0, hash),
      method: source.method,
      headers: source.headers && typeof source.headers.entries === "function"
        ? Array.from(source.headers.entries()) : [],
    };
  };
  const serviceWorkerCacheRequire = (value) => {
    if (!value || value.__glassServiceWorkerCache !== true)
      throw new TypeError("native Cache receiver is invalid");
    return value;
  };
  const ServiceWorkerCacheNative = typeof globalThis.__glassServiceWorkerCacheConstructor === "function"
    ? globalThis.__glassServiceWorkerCacheConstructor
    : function Cache() { throw new TypeError("Illegal constructor"); };
  const serviceWorkerMakeCache = (name) => {
    const cache = Object.create(ServiceWorkerCacheNative.prototype);
    Object.defineProperty(cache, "__glassServiceWorkerCache", { configurable: false, enumerable: false, value: true });
    Object.defineProperty(cache, "name", { configurable: false, enumerable: true, value: String(name) });
    return Object.freeze(cache);
  };
  ServiceWorkerCacheNative.prototype.match = function(request, options) {
    const cache = serviceWorkerCacheRequire(this);
    const key = serviceWorkerCacheRequestKey(request);
    const settings = options && typeof options === "object" ? options : {};
    return serviceWorkerCacheQueueRequest("serviceWorkerCacheMatch", {
      cache_name: cache.name,
      request_url: key.url,
      request_method: key.method,
      request_headers: key.headers,
      ignore_search: Boolean(settings.ignoreSearch),
      ignore_method: Boolean(settings.ignoreMethod),
      ignore_vary: Boolean(settings.ignoreVary),
    }).then(payload => payload && payload.found === true
      ? responseFromWorkerFetch(payload.response)
      : undefined);
  };
  ServiceWorkerCacheNative.prototype.put = function(request, response) {
    const cache = serviceWorkerCacheRequire(this);
    const key = serviceWorkerCacheRequestKey(request);
    if (key.method !== "GET")
      return Promise.reject(new TypeError("native Cache.put only supports GET requests"));
    if (!response || typeof response.clone !== "function")
      return Promise.reject(new TypeError("native Cache.put requires a Response"));
    if (response.type === "error" || response.type === "opaque" || response.type === "opaqueredirect"
      || response.status === 206)
      return Promise.reject(new TypeError("native Cache.put received a non-cacheable response"));
    const vary = response.headers && response.headers.get("vary");
    if (vary && vary.split(",").some(field => field.trim() === "*"))
      return Promise.reject(new TypeError("native Cache.put received a response with Vary: *"));
    return serviceWorkerFetchResponse(response).then(payload =>
      serviceWorkerCacheQueueRequest("serviceWorkerCachePut", {
        cache_name: cache.name,
        request_url: key.url,
        request_method: key.method,
        request_headers: key.headers,
        response: payload,
      }).then(() => undefined));
  };
  ServiceWorkerCacheNative.prototype.delete = function(request, options) {
    const cache = serviceWorkerCacheRequire(this);
    const key = serviceWorkerCacheRequestKey(request);
    const settings = options && typeof options === "object" ? options : {};
    return serviceWorkerCacheQueueRequest("serviceWorkerCacheDeleteRequest", {
      cache_name: cache.name,
      request_url: key.url,
      request_method: key.method,
      request_headers: key.headers,
      ignore_search: Boolean(settings.ignoreSearch),
      ignore_method: Boolean(settings.ignoreMethod),
      ignore_vary: Boolean(settings.ignoreVary),
    }).then(payload => payload && payload.deleted === true);
  };
  ServiceWorkerCacheNative.prototype.keys = function(request, options) {
    const cache = serviceWorkerCacheRequire(this);
    const settings = options && typeof options === "object" ? options : {};
    let key = null;
    if (request !== undefined) {
      try { key = serviceWorkerCacheRequestKey(request); }
      catch (error) { return Promise.reject(error); }
    }
    return serviceWorkerCacheQueueRequest("serviceWorkerCacheEntries", {
      cache_name: cache.name,
      request_url: key && key.url,
      request_method: key && key.method,
      request_headers: key ? key.headers : [],
      ignore_search: Boolean(settings.ignoreSearch),
      ignore_method: Boolean(settings.ignoreMethod),
      ignore_vary: Boolean(settings.ignoreVary),
    }).then(payload => (Array.isArray(payload && payload.entries) ? payload.entries : [])
      .map(entry => new WorkerRequestNative(String(entry.url || ""), {
        method: String(entry.method || "GET"),
        headers: Array.isArray(entry.headers) ? entry.headers : [],
      })));
  };
  ServiceWorkerCacheNative.prototype.add = function(request) {
    const cache = serviceWorkerCacheRequire(this);
    return workerFetchNative(request).then(response => {
      if (!response.ok) throw new TypeError("native Cache.add received a non-success response");
      return cache.put(request, response);
    });
  };
  ServiceWorkerCacheNative.prototype.addAll = function(requests) {
    const cache = serviceWorkerCacheRequire(this);
    let values;
    try {
      if (!requests || typeof requests[Symbol.iterator] !== "function")
        throw new TypeError("native Cache.addAll requires an iterable");
      values = Array.from(requests);
    } catch (error) {
      return Promise.reject(error);
    }
    if (values.length > __GLASS_SERVICE_WORKER_CACHE_ENTRY_LIMIT__)
      return Promise.reject(new RangeError("native Cache.addAll request limit exceeded"));
    if (values.length === 0) return Promise.resolve(undefined);
    let keys;
    try {
      keys = values.map(request => {
        const key = serviceWorkerCacheRequestKey(request);
        if (key.method !== "GET")
          throw new TypeError("native Cache.addAll only supports GET requests");
        return key;
      });
    } catch (error) {
      return Promise.reject(error);
    }
    return Promise.all(values.map(request => workerFetchNative(request)))
      .then(responses => Promise.all(responses.map((response, index) => {
        if (!response || response.type === "error" || response.type === "opaque"
          || response.type === "opaqueredirect" || !response.ok || response.status === 206)
          throw new TypeError("native Cache.addAll received a non-cacheable response");
        const vary = response.headers && response.headers.get("vary");
        if (vary && vary.split(",").some(field => field.trim() === "*"))
          throw new TypeError("native Cache.addAll received a response with Vary: *");
        return serviceWorkerFetchResponse(response).then(payload => ({
          request_url: keys[index].url,
          request_method: keys[index].method,
          request_headers: keys[index].headers,
          response: payload,
        }));
      })))
      .then(entries => serviceWorkerCacheQueueRequest("serviceWorkerCachePutAll", {
        cache_name: cache.name,
        entries,
      }).then(() => undefined));
  };
  globalThis.__glassServiceWorkerCacheConstructor = ServiceWorkerCacheNative;
  globalThis.Cache = ServiceWorkerCacheNative;
  const CacheStorageNative = typeof globalThis.__glassCacheStorageConstructor === "function"
    ? globalThis.__glassCacheStorageConstructor
    : function CacheStorage() { throw new TypeError("Illegal constructor"); };
  const serviceWorkerCacheStorage = globalThis.caches && typeof globalThis.caches === "object"
    ? globalThis.caches
    : Object.create(CacheStorageNative.prototype);
  serviceWorkerCacheStorage.open = (name) => {
    const cacheName = String(name);
    if (!cacheName || cacheName.length > __GLASS_SERVICE_WORKER_CACHE_NAME_LIMIT__)
      return Promise.reject(new TypeError("native Cache name is invalid"));
    return serviceWorkerCacheQueueRequest("serviceWorkerCacheOpen", { cache_name: cacheName })
      .then(() => serviceWorkerMakeCache(cacheName));
  };
  serviceWorkerCacheStorage.delete = (name) => {
    const cacheName = String(name);
    if (!cacheName || cacheName.length > __GLASS_SERVICE_WORKER_CACHE_NAME_LIMIT__)
      return Promise.reject(new TypeError("native Cache name is invalid"));
    return serviceWorkerCacheQueueRequest("serviceWorkerCacheDelete", { cache_name: cacheName })
      .then(payload => payload && payload.deleted === true);
  };
  serviceWorkerCacheStorage.has = (name) => {
    const cacheName = String(name);
    if (!cacheName || cacheName.length > __GLASS_SERVICE_WORKER_CACHE_NAME_LIMIT__)
      return Promise.reject(new TypeError("native Cache name is invalid"));
    return serviceWorkerCacheQueueRequest("serviceWorkerCacheHas", { cache_name: cacheName })
      .then(payload => payload && payload.has === true);
  };
  serviceWorkerCacheStorage.keys = () => serviceWorkerCacheQueueRequest("serviceWorkerCacheKeys", {})
    .then(payload => Array.isArray(payload && payload.keys) ? payload.keys.map(String) : []);
  serviceWorkerCacheStorage.match = (request, options) =>
    serviceWorkerCacheStorage.keys().then(names => {
      let chain = Promise.resolve(undefined);
      for (const name of names) {
        chain = chain.then(found => found === undefined
          ? serviceWorkerCacheStorage.open(name).then(cache => cache.match(request, options))
          : found);
      }
      return chain;
    });
  globalThis.__glassCacheStorageConstructor = CacheStorageNative;
  globalThis.caches = serviceWorkerCacheStorage;
  const serviceWorkerClientFromState = (state) => {
    if (!state || typeof state !== "object") return null;
    const client = {
      id: String(state.clientId || ""),
      url: String(state.clientUrl || ""),
      type: String(state.clientType || "window"),
      frameType: String(state.frameType || "top-level"),
      visibilityState: String(state.visibilityState || "visible"),
          focused: state.focused === true,
      postMessage(message, options) {
        const workerId = Number.isSafeInteger(globalThis.__glassWorkerId)
          ? globalThis.__glassWorkerId : 0;
        if (!workerId) throw new Error("native service worker client is unavailable");
        const envelope = glassMessageCloneWithTransfers(
          message,
          glassMessageTransferList(options),
        );
        pushCommand({
          kind: "serviceWorkerClientPostMessage",
          worker_id: workerId,
          client_id: this.id,
          data: envelope.data,
          transfer_ports: envelope.transfer_ports,
        });
      },
    };
    return Object.freeze(client);
  };
  const ServiceWorkerExtendableEventNative = typeof globalThis.__glassExtendableEventConstructor === "function"
    ? globalThis.__glassExtendableEventConstructor
    : function ExtendableEvent(type, init) {
        if (!(this instanceof ServiceWorkerExtendableEventNative))
          throw new TypeError("ExtendableEvent requires new");
        WorkerEventNative.call(this, type, init);
      };
  if (!globalThis.__glassExtendableEventConstructor) {
    ServiceWorkerExtendableEventNative.prototype = Object.create(WorkerEventNative.prototype);
    ServiceWorkerExtendableEventNative.prototype.constructor = ServiceWorkerExtendableEventNative;
    Object.defineProperty(ServiceWorkerExtendableEventNative.prototype, "waitUntil", {
      configurable: true,
      writable: true,
      value() {
        throw new WorkerDOMExceptionNative(
          "Service Worker event is not active",
          "InvalidStateError",
        );
      },
    });
  }
  const ServiceWorkerExtendableMessageEventNative = typeof globalThis.__glassExtendableMessageEventConstructor === "function"
    ? globalThis.__glassExtendableMessageEventConstructor
    : function ExtendableMessageEvent(type, init) {
        if (!(this instanceof ServiceWorkerExtendableMessageEventNative))
          throw new TypeError("ExtendableMessageEvent requires new");
        const settings = init && typeof init === "object" ? init : {};
        WorkerEventNative.call(this, type, settings);
        Object.defineProperties(this, {
          data: { value: settings.data === undefined ? null : settings.data, enumerable: true },
          origin: { value: String(settings.origin || ""), enumerable: true },
          lastEventId: { value: String(settings.lastEventId || ""), enumerable: true },
          source: { value: settings.source === undefined ? null : settings.source, enumerable: true },
          ports: {
            value: Object.freeze(Array.isArray(settings.ports) ? settings.ports.slice() : []),
            enumerable: true,
          },
        });
      };
  if (!globalThis.__glassExtendableMessageEventConstructor) {
    ServiceWorkerExtendableMessageEventNative.prototype = Object.create(
      ServiceWorkerExtendableEventNative.prototype,
    );
    ServiceWorkerExtendableMessageEventNative.prototype.constructor =
      ServiceWorkerExtendableMessageEventNative;
  }
  globalThis.__glassExtendableEventConstructor = ServiceWorkerExtendableEventNative;
  globalThis.__glassExtendableMessageEventConstructor = ServiceWorkerExtendableMessageEventNative;
  globalThis.ExtendableEvent = ServiceWorkerExtendableEventNative;
  globalThis.ExtendableMessageEvent = ServiceWorkerExtendableMessageEventNative;
  const serviceWorkerPromiseConstructor = globalThis.__glassServiceWorkerPromiseConstructor
    || Promise;
  if (!globalThis.__glassServiceWorkerPromiseConstructor) {
    Object.defineProperty(globalThis, "__glassServiceWorkerPromiseConstructor", {
      value: serviceWorkerPromiseConstructor,
      configurable: false,
      writable: false,
    });
  }
  if (!globalThis.__glassServiceWorkerPromiseResolve) {
    const promiseResolve = serviceWorkerPromiseConstructor.resolve;
    Object.defineProperty(globalThis, "__glassServiceWorkerPromiseResolve", {
      value: value => promiseResolve.call(serviceWorkerPromiseConstructor, value),
      configurable: false,
      writable: false,
    });
  }
  const serviceWorkerPromiseResolve = globalThis.__glassServiceWorkerPromiseResolve;
  const serviceWorkerNavigationPreloadPendingResponses =
    globalThis.__glassServiceWorkerNavigationPreloadPendingResponses instanceof Map
      ? globalThis.__glassServiceWorkerNavigationPreloadPendingResponses
      : new Map();
  globalThis.__glassServiceWorkerNavigationPreloadPendingResponses =
    serviceWorkerNavigationPreloadPendingResponses;
  const serviceWorkerFetchLifetimes =
    globalThis.__glassServiceWorkerFetchLifetimes instanceof Set
      ? globalThis.__glassServiceWorkerFetchLifetimes
      : new Set();
  globalThis.__glassServiceWorkerFetchLifetimes = serviceWorkerFetchLifetimes;
  const serviceWorkerDispatchMessage = (payload) => {
    const source = serviceWorkerClientFromState(payload && payload.source);
    if (!source) throw new TypeError("native Service Worker message source is unavailable");
    let type = "message";
    let data = null;
    let ports = [];
    try {
      const envelope = glassMessageDecodeEnvelope(payload);
      data = envelope.data;
      ports = Array.isArray(envelope.ports) ? envelope.ports : [];
    } catch (_error) {
      type = "messageerror";
    }
    const event = new ServiceWorkerExtendableMessageEventNative(type, {
      data,
      origin: String(payload.source_origin || ""),
      lastEventId: "",
      source,
      ports,
      bubbles: false,
      cancelable: false,
    });
    let resolveLifetime;
    const lifetime = new serviceWorkerPromiseConstructor(resolve => {
      resolveLifetime = resolve;
    });
    let dispatching = true;
    let pending = 0;
    let completed = false;
    const settleLifetime = () => {
      if (completed || dispatching || pending !== 0) return;
      completed = true;
      resolveLifetime(undefined);
    };
    const waitUntil = function(value) {
      if (this !== event)
        throw new TypeError("ExtendableEvent.waitUntil called on an incompatible receiver");
      if (!dispatching && pending === 0)
        throw new WorkerDOMExceptionNative(
          "Service Worker event is not active",
          "InvalidStateError",
        );
      const promise = serviceWorkerPromiseResolve(value);
      pending += 1;
      promise.then(
        () => { pending -= 1; settleLifetime(); },
        () => { pending -= 1; settleLifetime(); },
      );
    };
    Object.defineProperty(event, "isTrusted", {
      value: true,
      configurable: false,
      writable: false,
    });
    Object.defineProperty(event, "waitUntil", {
      value: waitUntil,
      configurable: false,
      writable: false,
    });
    event.target = globalThis;
    event.currentTarget = globalThis;
    event.eventPhase = 2;
    try {
      dispatch(type, event);
    } catch (error) {
      reportWorkerCallbackException(error, false);
    } finally {
      event.currentTarget = null;
      event.eventPhase = 0;
      dispatching = false;
      settleLifetime();
    }
    return lifetime;
  };
  globalThis.__glassDispatchServiceWorkerMessage = serviceWorkerDispatchMessage;
  const serviceWorkerDispatchFetch = (payload) => {
    globalThis.__glassServiceWorkerClientState = payload && typeof payload === "object"
      ? {
          clientId: String(payload.clientId || ""),
          clientUrl: String(payload.clientUrl || ""),
          clientType: String(payload.clientType || "window"),
          frameType: String(payload.frameType || "top-level"),
          visibilityState: String(payload.visibilityState || "visible"),
          focused: payload.focused === true,
          controlled: payload.controlled === true,
        }
      : null;
    if (payload && Array.isArray(payload.clients)) {
      globalThis.__glassServiceWorkerClients = payload.clients;
    }
    const body = payload && payload.bodyNull === true
      ? undefined
      : payload && typeof payload.bodyBase64 === "string"
        ? new Uint8Array(decodeWorkerBase64(payload.bodyBase64, __GLASS_SERVICE_WORKER_BODY_LIMIT__))
        : String(payload && payload.body || "");
    const requestHeaders = Array.isArray(payload && payload.headers)
      ? payload.headers.map(entry => Array.isArray(entry) ? [entry[0], entry[1]] : entry)
      : [];
    if (payload && payload.contentType !== undefined && payload.contentType !== null
        && !requestHeaders.some(entry => Array.isArray(entry)
          && String(entry[0]).toLowerCase() === "content-type"))
      requestHeaders.push(["content-type", String(payload.contentType)]);
    const preloadRequestId = payload && Number.isSafeInteger(payload.navigationPreloadRequestId)
      ? Number(payload.navigationPreloadRequestId)
      : 0;
    let preloadResponse = Promise.resolve(undefined);
    if (preloadRequestId > 0) {
      if (serviceWorkerNavigationPreloadPendingResponses.size >= __GLASS_SERVICE_WORKER_PRELOAD_LIMIT__)
        throw new RangeError("native ServiceWorker navigation preload limit exceeded");
      preloadResponse = new serviceWorkerPromiseConstructor((resolve, reject) => {
        serviceWorkerNavigationPreloadPendingResponses.set(preloadRequestId, { resolve, reject });
      });
    }
    const requestMode = String(payload && payload.mode || "same-origin");
    const request = new WorkerRequestNative(String(payload && payload.url || workerUrl), {
      method: String(payload && payload.method || "GET"),
      headers: requestHeaders,
      body,
      mode: requestMode,
      redirect: String(payload && payload.redirect || "follow"),
      credentials: payload && typeof payload.credentialsMode === "string"
        ? payload.credentialsMode
        : payload && payload.credentials === true ? "include" : "omit",
      referrer: payload && typeof payload.referrer === "string" ? payload.referrer : "about:client",
      referrerPolicy: payload && typeof payload.referrerPolicy === "string"
        ? payload.referrerPolicy : "",
      destination: String(payload && payload.destination || ""),
    }, requestMode === "navigate" ? workerNavigationRequestToken : undefined);
    workerRequestReferrerState.set(request, {
      referrer: request.referrer,
      url: payload && typeof payload.referrerUrl === "string" ? payload.referrerUrl : workerUrl,
    });
    let responded = false;
    let immediatePropagationStopped = false;
    let responsePromise = null;
    let dispatching = true;
    let pendingLifetimePromises = 0;
    let lifetimeSettled = false;
    let resolveLifetime;
    const lifetimePromise = new serviceWorkerPromiseConstructor(resolve => {
      resolveLifetime = resolve;
    });
    const settleLifetime = () => {
      if (lifetimeSettled || dispatching || pendingLifetimePromises !== 0) return;
      lifetimeSettled = true;
      resolveLifetime(undefined);
    };
    const extendLifetime = value => {
      if (!dispatching && pendingLifetimePromises === 0)
        throw new DOMExceptionNative("Service Worker event is not active", "InvalidStateError");
      pendingLifetimePromises += 1;
      serviceWorkerPromiseResolve(value).then(
        () => { pendingLifetimePromises -= 1; settleLifetime(); },
        () => { pendingLifetimePromises -= 1; settleLifetime(); },
      );
    };
    const event = {
      type: "fetch",
      request,
      clientId: String(payload && payload.clientId || ""),
      resultingClientId: String(payload && payload.resultingClientId || ""),
      isReload: false,
      isHistoryNavigation: false,
      preloadResponse,
      waitUntil(value) {
        if (this !== event)
          throw new TypeError("ExtendableEvent.waitUntil called on an incompatible receiver");
        extendLifetime(value);
      },
      stopImmediatePropagation() {
        if (this !== event)
          throw new TypeError("Event.stopImmediatePropagation called on an incompatible receiver");
        immediatePropagationStopped = true;
      },
      respondWith(value) {
        if (!dispatching)
          throw new DOMExceptionNative("FetchEvent is not being dispatched", "InvalidStateError");
        if (responded) throw new DOMExceptionNative("service worker fetch already responded", "InvalidStateError");
        responded = true;
        immediatePropagationStopped = true;
        responsePromise = serviceWorkerPromiseResolve(value);
        extendLifetime(responsePromise);
      },
    };
    Object.defineProperty(event, "preloadResponse", {
      value: preloadResponse,
      enumerable: true,
      configurable: false,
      writable: false,
    });
    const callbacks = (listeners.get("fetch") || []).slice();
    for (const callback of callbacks) {
      try {
        if (typeof callback === "function") callback.call(globalThis, event);
        else if (callback && typeof callback.handleEvent === "function") callback.handleEvent(event);
      } catch (error) { reportWorkerCallbackException(error, false); }
      if (immediatePropagationStopped) break;
    }
    dispatching = false;
    serviceWorkerFetchLifetimes.add(lifetimePromise);
    lifetimePromise.then(
      () => { serviceWorkerFetchLifetimes.delete(lifetimePromise); },
      () => { serviceWorkerFetchLifetimes.delete(lifetimePromise); },
    );
    settleLifetime();
    if (!responded) return serviceWorkerPromiseResolve({ handled: false });
    return responsePromise.then(response => serviceWorkerFetchResponse(response))
      .then(response => ({ handled: true, response }));
  };
  let serviceWorkerFetchHandlerSlots =
    globalThis.__glassServiceWorkerFetchHandlerSlots instanceof Map
      ? globalThis.__glassServiceWorkerFetchHandlerSlots
      : null;
  if (!serviceWorkerFetchHandlerSlots) {
    serviceWorkerFetchHandlerSlots = new Map();
    Object.defineProperty(globalThis, "__glassServiceWorkerFetchHandlerSlots", {
      value: serviceWorkerFetchHandlerSlots,
      configurable: false,
      enumerable: false,
      writable: false,
    });
  }
  const setServiceWorkerFetchHandler = (value) => {
    const handlerValue = typeof value === "function" ? value : null;
    const current = serviceWorkerFetchHandlerSlots.get("fetch") || null;
    if (handlerValue === null) {
      if (current) {
        current.active = false;
        const callbacks = listeners.get("fetch") || [];
        listeners.set("fetch", callbacks.filter((callback) => callback !== current.listener));
        serviceWorkerFetchHandlerSlots.delete("fetch");
      }
      globalThis.__glassServiceWorkerOnFetch = null;
      return;
    }
    if (current && current.active) {
      current.value = handlerValue;
      globalThis.__glassServiceWorkerOnFetch = handlerValue;
      return;
    }
    const slot = { active: true, value: handlerValue, listener: null };
    slot.listener = (event) => {
      if (!slot.active || typeof slot.value !== "function") return undefined;
      return slot.value.call(globalThis, event);
    };
    const callbacks = listeners.get("fetch") || [];
    callbacks.push(slot.listener);
    listeners.set("fetch", callbacks);
    serviceWorkerFetchHandlerSlots.set("fetch", slot);
    globalThis.__glassServiceWorkerOnFetch = handlerValue;
  };
  if (!Object.prototype.hasOwnProperty.call(globalThis, "onfetch")) {
    Object.defineProperty(globalThis, "onfetch", {
      configurable: true,
      enumerable: true,
      get() {
        const slot = serviceWorkerFetchHandlerSlots.get("fetch");
        return slot && slot.active ? slot.value : null;
      },
      set(value) { setServiceWorkerFetchHandler(value); },
    });
  }
  globalThis.__glassHasServiceWorkerFetchListener = () => {
    const callbacks = listeners.get("fetch") || [];
    return typeof globalThis.onfetch === "function" || callbacks.length > 0;
  };
  globalThis.__glassResolveServiceWorkerNavigationPreload = (requestId, payload) => {
    const id = Number(requestId);
    if (!Number.isSafeInteger(id) || id <= 0)
      throw new TypeError("native ServiceWorker navigation preload request id is invalid");
    const pending = serviceWorkerNavigationPreloadPendingResponses.get(id);
    if (!pending) return false;
    serviceWorkerNavigationPreloadPendingResponses.delete(id);
    if (payload && typeof payload.error === "string") {
      pending.reject(new TypeError(payload.error));
      return true;
    }
    try {
      pending.resolve(responseFromWorkerFetch(payload));
    } catch (error) {
      pending.reject(new TypeError(String(error && error.message || error)));
    }
    return true;
  };
  globalThis.__glassDispatchServiceWorkerFetch = serviceWorkerDispatchFetch;
  globalThis.skipWaiting = () => {
    const workerId = Number.isSafeInteger(globalThis.__glassWorkerId)
      ? globalThis.__glassWorkerId : 0;
    if (!workerId) return Promise.reject(new Error("native service worker is unavailable"));
    pushCommand({ kind: "serviceWorkerSkipWaiting", worker_id: workerId });
    return Promise.resolve(undefined);
  };
  const serviceWorkerClients = globalThis.clients && typeof globalThis.clients === "object"
    ? globalThis.clients : {};
  serviceWorkerClients.claim = () => {
    const workerId = Number.isSafeInteger(globalThis.__glassWorkerId)
      ? globalThis.__glassWorkerId : 0;
    if (!workerId) return Promise.reject(new Error("native service worker is unavailable"));
    pushCommand({ kind: "serviceWorkerClientsClaim", worker_id: workerId });
    return Promise.resolve(undefined);
  };
  serviceWorkerClients.matchAll = (options) => {
    const settings = options && typeof options === "object" ? options : {};
    const type = settings.type === undefined ? "window" : String(settings.type);
    if (!["window", "worker", "sharedworker", "all"].includes(type))
      return Promise.reject(new TypeError("service worker clients.matchAll type is invalid"));
    if (type !== "window" && type !== "all") return Promise.resolve([]);
    const states = Array.isArray(globalThis.__glassServiceWorkerClients)
      ? globalThis.__glassServiceWorkerClients : [];
    const clients = states
      .filter(state => type === "all" || String(state.clientType || "window") === type)
      .filter(state => settings.includeUncontrolled === true || state.controlled === true)
      .map(serviceWorkerClientFromState)
      .filter(Boolean);
    return Promise.resolve(clients);
  };
  serviceWorkerClients.openWindow = (value) => {
    const workerId = Number.isSafeInteger(globalThis.__glassWorkerId)
      ? globalThis.__glassWorkerId : 0;
    if (!workerId) return Promise.reject(new Error("native service worker is unavailable"));
    let target;
    try {
      target = new WorkerURLNative(String(value), workerUrl);
      if (!["http:", "https:"].includes(target.protocol) || target.origin !== workerLocation.origin)
        throw new DOMExceptionNative("service worker openWindow URL must be same-origin HTTP(S)", "SecurityError");
      if (target.username || target.password)
        throw new DOMExceptionNative("service worker openWindow URL must not contain credentials", "SecurityError");
    } catch (error) {
      return Promise.reject(error);
    }
    const requestId = nextServiceWorkerOpenWindowRequestId++;
    globalThis.__glassNextServiceWorkerOpenWindowRequestId = nextServiceWorkerOpenWindowRequestId;
    return new Promise((resolve, reject) => {
      serviceWorkerOpenWindowPendingRequests.set(requestId, { resolve, reject });
      try {
        pushCommand({ kind: "serviceWorkerOpenWindow", request_id: requestId, worker_id: workerId, url: target.href });
      } catch (error) {
        serviceWorkerOpenWindowPendingRequests.delete(requestId);
        reject(error);
      }
    });
  };
  globalThis.clients = serviceWorkerClients;
  globalThis.registration = globalThis.registration || {
    scope: workerUrl.slice(0, workerUrl.lastIndexOf("/") + 1),
    update: () => Promise.resolve(undefined),
    unregister: () => Promise.resolve(undefined),
  };
  globalThis.__glassDispatchServiceWorkerLifecycle = (type) => {
    const waitUntilPromises = [];
    const event = {
      type: String(type),
      waitUntil(value) { waitUntilPromises.push(Promise.resolve(value)); },
    };
    const callbacks = listeners.get(String(type)) || [];
    for (const callback of callbacks.slice()) {
      try {
        if (typeof callback === "function") callback.call(globalThis, event);
        else if (callback && typeof callback.handleEvent === "function") callback.handleEvent(event);
      } catch (error) {
        reportWorkerCallbackException(error, false);
      }
    }
    return Promise.all(waitUntilPromises).then(() => null);
  };
  let serviceWorkerLifecycleHandlerSlots =
    globalThis.__glassServiceWorkerLifecycleHandlerSlots instanceof Map
      ? globalThis.__glassServiceWorkerLifecycleHandlerSlots
      : null;
  if (!serviceWorkerLifecycleHandlerSlots) {
    serviceWorkerLifecycleHandlerSlots = new Map();
    Object.defineProperty(globalThis, "__glassServiceWorkerLifecycleHandlerSlots", {
      value: serviceWorkerLifecycleHandlerSlots,
      configurable: false,
      enumerable: false,
      writable: false,
    });
  }
  const setServiceWorkerLifecycleHandler = (type, value) => {
    const isObjectOrFunction = value !== null
      && (typeof value === "object" || typeof value === "function");
    const handlerValue = isObjectOrFunction ? value : null;
    const current = serviceWorkerLifecycleHandlerSlots.get(type) || null;
    if (handlerValue === null) {
      if (current) {
        current.active = false;
        const callbacks = listeners.get(type) || [];
        listeners.set(type, callbacks.filter((callback) => callback !== current.listener));
        serviceWorkerLifecycleHandlerSlots.delete(type);
      }
      return;
    }
    if (current && current.active) {
      current.value = handlerValue;
      return;
    }
    const slot = { active: true, value: handlerValue, listener: null };
    slot.listener = (event) => {
      if (!slot.active || typeof slot.value !== "function") return undefined;
      return slot.value.call(globalThis, event);
    };
    const callbacks = listeners.get(type) || [];
    callbacks.push(slot.listener);
    listeners.set(type, callbacks);
    serviceWorkerLifecycleHandlerSlots.set(type, slot);
  };
  for (const [property, type] of [["oninstall", "install"], ["onactivate", "activate"]]) {
    Object.defineProperty(globalThis, property, {
      configurable: true,
      enumerable: true,
      get() {
        const slot = serviceWorkerLifecycleHandlerSlots.get(type);
        return slot && slot.active ? slot.value : null;
      },
      set(value) { setServiceWorkerLifecycleHandler(type, value); },
    });
  }
  globalThis.__glassResolveServiceWorkerCache = (requestId, payload) => {
    const pending = serviceWorkerCachePendingRequests.get(Number(requestId));
    if (!pending) return null;
    serviceWorkerCachePendingRequests.delete(Number(requestId));
    if (payload && payload.error) {
      pending.reject(new Error(String(payload.error)));
      return null;
    }
    pending.resolve(payload || null);
    return null;
  };
  globalThis.__glassResolveServiceWorkerOpenWindow = (requestId, payload) => {
    const pending = serviceWorkerOpenWindowPendingRequests.get(Number(requestId));
    if (!pending) return null;
    serviceWorkerOpenWindowPendingRequests.delete(Number(requestId));
    if (payload && payload.error) {
      pending.reject(new Error(String(payload.error)));
      return null;
    }
    const client = serviceWorkerWindowClientFromState(payload);
    if (!client || !client.id || !client.url) {
      pending.reject(new Error("native service worker openWindow response was invalid"));
      return null;
    }
    pending.resolve(client);
    return null;
  };
  globalThis.__glassServiceWorkerCachePendingRequests = serviceWorkerCachePendingRequests;
  globalThis.__glassNextServiceWorkerCacheRequestId = nextServiceWorkerCacheRequestId;
  globalThis.__glassServiceWorkerOpenWindowPendingRequests = serviceWorkerOpenWindowPendingRequests;
  globalThis.__glassNextServiceWorkerOpenWindowRequestId = nextServiceWorkerOpenWindowRequestId;
"###;
