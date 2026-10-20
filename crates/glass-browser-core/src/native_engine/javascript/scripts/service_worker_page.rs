pub(in crate::native_engine::javascript) const NATIVE_SERVICE_WORKER_PAGE_SCRIPT: &str = r###"
  const serviceWorkerRegistrations = globalThis.__glassServiceWorkerRegistrations instanceof Map
    ? globalThis.__glassServiceWorkerRegistrations
    : new Map();
  const serviceWorkerPendingRequests = globalThis.__glassServiceWorkerPendingRequests instanceof Map
    ? globalThis.__glassServiceWorkerPendingRequests
    : new Map();
  let nextServiceWorkerRequestId = Number.isSafeInteger(globalThis.__glassNextServiceWorkerRequestId)
    ? globalThis.__glassNextServiceWorkerRequestId
    : 1;
  const serviceWorkerClone = (value) => glassMessageClone(value);
  const serviceWorkerOrigin = String(host.origin || "null");
  const serviceWorkerUrl = (value) => {
    const url = new URLNative(String(value), host.url);
    if (!["http:", "https:"].includes(url.protocol) || url.origin !== serviceWorkerOrigin)
      throw new DOMExceptionNative("service worker URL must be same-origin HTTP(S)", "SecurityError");
    return url;
  };
  const serviceWorkerScopeHref = (url) => url.origin + url.pathname;
  const ServiceWorkerNative = typeof globalThis.__glassServiceWorkerConstructor === "function"
    ? globalThis.__glassServiceWorkerConstructor
    : function ServiceWorker() { throw new TypeError("Illegal constructor"); };
  const ServiceWorkerRegistrationNative = typeof globalThis.__glassServiceWorkerRegistrationConstructor === "function"
    ? globalThis.__glassServiceWorkerRegistrationConstructor
    : function ServiceWorkerRegistration() { throw new TypeError("Illegal constructor"); };
  const NavigationPreloadManagerNative = typeof globalThis.__glassNavigationPreloadManagerConstructor === "function"
    ? globalThis.__glassNavigationPreloadManagerConstructor
    : function NavigationPreloadManager() { throw new TypeError("Illegal constructor"); };
  const serviceWorkerNavigationPreloadManagerByRegistration = globalThis.__glassServiceWorkerNavigationPreloadManagerByRegistration instanceof WeakMap
    ? globalThis.__glassServiceWorkerNavigationPreloadManagerByRegistration
    : new WeakMap();
  globalThis.__glassServiceWorkerNavigationPreloadManagerByRegistration = serviceWorkerNavigationPreloadManagerByRegistration;
  const serviceWorkerRegistrationByNavigationPreloadManager = globalThis.__glassServiceWorkerRegistrationByNavigationPreloadManager instanceof WeakMap
    ? globalThis.__glassServiceWorkerRegistrationByNavigationPreloadManager
    : new WeakMap();
  globalThis.__glassServiceWorkerRegistrationByNavigationPreloadManager = serviceWorkerRegistrationByNavigationPreloadManager;
  if (!ServiceWorkerNative.prototype.postMessage) {
    ServiceWorkerNative.prototype.postMessage = function(message, options) {
      const envelope = glassMessageCloneWithTransfers(
        message,
        glassMessageTransferList(options),
      );
      pushCommand({
        kind: "serviceWorkerPostMessage",
        scope: String(this.__glassServiceWorkerScope || ""),
        source_origin: serviceWorkerOrigin,
        data: envelope.data,
        transfer_ports: envelope.transfer_ports,
      });
    };
  }
  globalThis.__glassServiceWorkerConstructor = ServiceWorkerNative;
  globalThis.__glassServiceWorkerRegistrationConstructor = ServiceWorkerRegistrationNative;
  globalThis.__glassNavigationPreloadManagerConstructor = NavigationPreloadManagerNative;
  globalThis.ServiceWorker = ServiceWorkerNative;
  globalThis.ServiceWorkerRegistration = ServiceWorkerRegistrationNative;
  globalThis.NavigationPreloadManager = NavigationPreloadManagerNative;
  const serviceWorkerListeners = (target) => {
    if (!(target.__glassServiceWorkerListeners instanceof Map)) {
      Object.defineProperty(target, "__glassServiceWorkerListeners", {
        configurable: false, enumerable: false, value: new Map(),
      });
    }
    return target.__glassServiceWorkerListeners;
  };
  const serviceWorkerAddEventListener = function(type, callback) {
    if (typeof callback !== "function" && !(callback && typeof callback.handleEvent === "function")) return;
    const listeners = serviceWorkerListeners(this);
    const name = String(type);
    const callbacks = listeners.get(name) || [];
    if (!callbacks.includes(callback)) callbacks.push(callback);
    listeners.set(name, callbacks);
  };
  const serviceWorkerRemoveEventListener = function(type, callback) {
    const listeners = serviceWorkerListeners(this);
    const name = String(type);
    const callbacks = listeners.get(name) || [];
    listeners.set(name, callbacks.filter(candidate => candidate !== callback));
  };
  const serviceWorkerDispatchEvent = (target, event) => {
    const name = String(event && event.type || "");
    const value = Object.assign({ type: name, target, currentTarget: target }, event || {});
    const listeners = serviceWorkerListeners(target);
    for (const callback of (listeners.get(name) || []).slice()) {
      try {
        if (typeof callback === "function") callback.call(target, value);
        else if (callback && typeof callback.handleEvent === "function") callback.handleEvent.call(callback, value);
      } catch (_) {}
    }
    const handler = target["on" + name];
    if (typeof handler === "function") {
      try { handler.call(target, value); } catch (_) {}
    }
    return true;
  };
  if (!ServiceWorkerNative.prototype.addEventListener) ServiceWorkerNative.prototype.addEventListener = serviceWorkerAddEventListener;
  if (!ServiceWorkerNative.prototype.removeEventListener) ServiceWorkerNative.prototype.removeEventListener = serviceWorkerRemoveEventListener;
  if (!ServiceWorkerNative.prototype.dispatchEvent) ServiceWorkerNative.prototype.dispatchEvent = function(event) {
    return serviceWorkerDispatchEvent(this, event);
  };
  if (!ServiceWorkerRegistrationNative.prototype.addEventListener) ServiceWorkerRegistrationNative.prototype.addEventListener = serviceWorkerAddEventListener;
  if (!ServiceWorkerRegistrationNative.prototype.removeEventListener) ServiceWorkerRegistrationNative.prototype.removeEventListener = serviceWorkerRemoveEventListener;
  if (!ServiceWorkerRegistrationNative.prototype.dispatchEvent) ServiceWorkerRegistrationNative.prototype.dispatchEvent = function(event) {
    return serviceWorkerDispatchEvent(this, event);
  };
  const serviceWorkerNavigationPreloadRegistration = (manager) => {
    const registration = serviceWorkerRegistrationByNavigationPreloadManager.get(manager);
    if (!registration) throw new TypeError("Illegal invocation");
    return registration;
  };
  const serviceWorkerNavigationPreloadRequest = (manager, operation, headerValue) => {
    const registration = serviceWorkerNavigationPreloadRegistration(manager);
    const payload = { scope: String(registration.scope), operation };
    if (headerValue !== undefined) payload.header_value = headerValue;
    return serviceWorkerQueueRequest("serviceWorkerNavigationPreload", payload);
  };
  NavigationPreloadManagerNative.prototype.enable = function() {
    return serviceWorkerNavigationPreloadRequest(this, "enable");
  };
  NavigationPreloadManagerNative.prototype.disable = function() {
    return serviceWorkerNavigationPreloadRequest(this, "disable");
  };
  NavigationPreloadManagerNative.prototype.setHeaderValue = function(value) {
    const byteString = "" + value;
    for (let index = 0; index < byteString.length; index += 1) {
      if (byteString.charCodeAt(index) > 255)
        throw new TypeError("NavigationPreloadManager header value is not a ByteString");
    }
    const headerValue = byteString.replace(/^[\t ]+|[\t ]+$/g, "");
    if (headerValue.length > __GLASS_SERVICE_WORKER_NAVIGATION_PRELOAD_HEADER_VALUE_LIMIT__)
      return Promise.reject(new RangeError("NavigationPreloadManager header value exceeds its byte limit"));
    if (/[\u0000\r\n]/.test(headerValue))
      return Promise.reject(new TypeError("NavigationPreloadManager header value is invalid"));
    return serviceWorkerNavigationPreloadRequest(this, "setHeaderValue", headerValue);
  };
  NavigationPreloadManagerNative.prototype.getState = function() {
    return serviceWorkerNavigationPreloadRequest(this, "getState");
  };
  const serviceWorkerNavigationPreloadManager = (registration) => {
    let manager = serviceWorkerNavigationPreloadManagerByRegistration.get(registration);
    if (!manager) {
      manager = Object.create(NavigationPreloadManagerNative.prototype);
      serviceWorkerNavigationPreloadManagerByRegistration.set(registration, manager);
      serviceWorkerRegistrationByNavigationPreloadManager.set(manager, registration);
    }
    return manager;
  };
  if (!Object.getOwnPropertyDescriptor(ServiceWorkerRegistrationNative.prototype, "navigationPreload"))
    Object.defineProperty(ServiceWorkerRegistrationNative.prototype, "navigationPreload", {
      configurable: true, enumerable: true,
      get() { return serviceWorkerNavigationPreloadManager(this); },
    });
  const serviceWorkerMakeWorker = (state, scope) => {
    const worker = Object.create(ServiceWorkerNative.prototype);
    worker.scriptURL = String(state && state.script_url || "");
    worker.state = String(state && state.state || "activated");
    worker.onstatechange = null;
    Object.defineProperty(worker, "__glassServiceWorkerListeners", {
      configurable: false, enumerable: false, value: new Map(),
    });
    Object.defineProperty(worker, "__glassServiceWorkerScope", {
      configurable: false, enumerable: false, value: scope,
    });
    return worker;
  };
  const serviceWorkerMakeRegistration = (state) => {
    const scope = String(state.scope || "");
    const hasExplicitActive = Object.prototype.hasOwnProperty.call(state, "active");
    const activeState = hasExplicitActive ? state.active : state;
    const registration = Object.create(ServiceWorkerRegistrationNative.prototype);
    registration.scope = scope;
    registration.__glassControlled = state.controlled === true;
    registration.installing = null;
    registration.waiting = state.waiting
      ? serviceWorkerMakeWorker(state.waiting, scope) : null;
    registration.active = activeState
      ? serviceWorkerMakeWorker(activeState, scope) : null;
    registration.onupdatefound = null;
    Object.defineProperty(registration, "__glassServiceWorkerListeners", {
      configurable: false, enumerable: false, value: new Map(),
    });
    registration.update = () => {
      const requestId = nextServiceWorkerRequestId++;
      globalThis.__glassNextServiceWorkerRequestId = nextServiceWorkerRequestId;
      return new Promise((resolve, reject) => {
        serviceWorkerPendingRequests.set(requestId, { resolve, reject, kind: "update", scope });
        try {
          pushCommand({ kind: "serviceWorkerUpdate", request_id: requestId, scope, referrer_url: host.creation_url, referrer_policy: documentReferrerPolicy.value });
        } catch (error) {
          serviceWorkerPendingRequests.delete(requestId);
          reject(error);
        }
      });
    };
    registration.unregister = () => {
      const requestId = nextServiceWorkerRequestId++;
      globalThis.__glassNextServiceWorkerRequestId = nextServiceWorkerRequestId;
      return new Promise((resolve, reject) => {
        serviceWorkerPendingRequests.set(requestId, { resolve, reject, kind: "unregister", scope });
        try {
          pushCommand({ kind: "serviceWorkerUnregister", request_id: requestId, scope });
        } catch (error) {
          serviceWorkerPendingRequests.delete(requestId);
          reject(error);
        }
      });
    };
    return registration;
  };
  const serviceWorkerSetState = (worker, state) => {
    const next = String(state || "");
    if (!next || worker.state === next) return;
    worker.state = next;
    serviceWorkerDispatchEvent(worker, { type: "statechange" });
  };
  const serviceWorkerApplyRegistration = (registration, state) => {
    const scope = String(state.scope || registration.scope || "");
    const wasControlled = registration.__glassControlled === true && !!registration.active;
    registration.__glassControlled = state.controlled === true;
    const hasExplicitActive = Object.prototype.hasOwnProperty.call(state, "active");
    const activeState = hasExplicitActive ? state.active : state;
    const waitingState = state.waiting || null;
    const lifecycle = Array.isArray(state.lifecycle)
      ? state.lifecycle.map(String).filter(value => ["installing", "installed", "activating", "activated"].includes(value))
      : [];
    const previousActive = registration.active;
    if (hasExplicitActive && !registration.active && activeState)
      registration.active = serviceWorkerMakeWorker(activeState, scope);
    if (lifecycle.length === 0) {
      registration.scope = scope;
      registration.active = activeState
        ? serviceWorkerMakeWorker(activeState, scope) : null;
      registration.installing = null;
      registration.waiting = waitingState
        ? serviceWorkerMakeWorker(waitingState, scope) : null;
      if (!wasControlled && registration.__glassControlled && registration.active)
        serviceWorkerContainer.dispatchEvent({ type: "controllerchange" });
      return;
    }
    const candidateState = waitingState || activeState || state;
    const worker = serviceWorkerMakeWorker({
      script_url: candidateState && candidateState.script_url,
      state: lifecycle[0],
    }, scope);
    registration.scope = scope;
    registration.installing = worker;
    registration.waiting = null;
    serviceWorkerDispatchEvent(registration, { type: "updatefound" });
    for (const next of lifecycle.slice(1)) {
      if (next === "installed") {
        registration.installing = null;
        registration.waiting = worker;
      } else if (next === "activating") {
        registration.waiting = null;
        registration.installing = worker;
      } else if (next === "activated") {
        registration.installing = null;
        registration.waiting = null;
        registration.active = worker;
      }
      serviceWorkerSetState(worker, next);
    }
    if (worker.state === "activated") {
      registration.installing = null;
      registration.waiting = null;
      registration.active = worker;
      let controllerChanged = false;
      if (previousActive && previousActive !== worker) {
        serviceWorkerSetState(previousActive, "redundant");
        controllerChanged = true;
      }
      if (!wasControlled && registration.__glassControlled) controllerChanged = true;
      if (controllerChanged && typeof serviceWorkerContainer !== "undefined")
        serviceWorkerContainer.dispatchEvent({ type: "controllerchange" });
    }
  };
  const serviceWorkerRefresh = () => {
    const states = Array.isArray(host.service_workers) ? host.service_workers : [];
    for (const state of states) {
      if (!state || typeof state !== "object" || !state.scope) continue;
      const scope = String(state.scope);
      let registration = serviceWorkerRegistrations.get(scope);
      if (!registration) {
        registration = serviceWorkerMakeRegistration(state);
        serviceWorkerRegistrations.set(scope, registration);
      } else {
        registration.scope = scope;
        registration.__glassControlled = state.controlled === true;
        const hasExplicitActive = Object.prototype.hasOwnProperty.call(state, "active");
        const activeState = hasExplicitActive ? state.active : state;
        if (!registration.active && activeState)
          registration.active = serviceWorkerMakeWorker(activeState, scope);
        if (hasExplicitActive) {
          registration.waiting = state.waiting
            ? serviceWorkerMakeWorker(state.waiting, scope) : null;
        }
      }
    }
    globalThis.__glassServiceWorkerRegistrations = serviceWorkerRegistrations;
    return states;
  };
  serviceWorkerRefresh();
  const serviceWorkerFind = (href) => {
    let best = null;
    for (const registration of serviceWorkerRegistrations.values()) {
      if (!registration || typeof registration.scope !== "string") continue;
      if (!href.startsWith(registration.scope)) continue;
      if (!best || registration.scope.length > best.scope.length) best = registration;
    }
    return best;
  };
  const serviceWorkerQueueRequest = (kind, payload) => {
    const requestId = nextServiceWorkerRequestId++;
    globalThis.__glassNextServiceWorkerRequestId = nextServiceWorkerRequestId;
    return new Promise((resolve, reject) => {
      serviceWorkerPendingRequests.set(requestId, { resolve, reject, kind });
      try { pushCommand(Object.assign({ kind, request_id: requestId }, payload)); }
      catch (error) {
        serviceWorkerPendingRequests.delete(requestId);
        reject(error);
      }
    });
  };
  const serviceWorkerRegister = (scriptURL, options) => {
    try {
      const script = serviceWorkerUrl(scriptURL);
      const settings = options === undefined ? {} : options;
      if (!settings || typeof settings !== "object")
        return Promise.reject(new TypeError("service worker registration options must be an object"));
      const requestedScope = settings.scope === undefined
        ? serviceWorkerScopeHref(new URLNative(script.href.slice(0, script.href.lastIndexOf("/") + 1)))
        : serviceWorkerScopeHref(serviceWorkerUrl(settings.scope));
      const scriptDirectory = script.pathname.slice(0, script.pathname.lastIndexOf("/") + 1);
      const scopeUrl = new URLNative(requestedScope);
      if (!scopeUrl.pathname.startsWith(scriptDirectory))
        return Promise.reject(new DOMExceptionNative("service worker scope exceeds script scope", "SecurityError"));
      const workerType = settings.type === undefined ? "classic" : String(settings.type).toLowerCase();
      if (workerType !== "classic" && workerType !== "module")
        return Promise.reject(new TypeError("service worker type must be classic or module"));
      return serviceWorkerQueueRequest("serviceWorkerRegister", {
        script_url: script.href,
        scope: requestedScope,
        worker_type: workerType,
        referrer_url: host.creation_url,
        referrer_policy: documentReferrerPolicy.value,
      });
    } catch (error) {
      return Promise.reject(error);
    }
  };
  const serviceWorkerContainer = globalThis.__glassServiceWorkerContainer || {};
  serviceWorkerContainer.register = serviceWorkerRegister;
  serviceWorkerContainer.getRegistration = (value) => {
    try {
      const url = serviceWorkerUrl(value === undefined ? host.url : value);
      return Promise.resolve(serviceWorkerFind(url.href) || undefined);
    } catch (error) { return Promise.reject(error); }
  };
  serviceWorkerContainer.getRegistrations = () => Promise.resolve(
    Array.from(serviceWorkerRegistrations.values()).filter(registration => registration)
  );
  serviceWorkerContainer.addEventListener = serviceWorkerContainer.addEventListener || function(type, callback) {
    if (!this.__glassListeners) this.__glassListeners = new Map();
    const callbacks = this.__glassListeners.get(String(type)) || [];
    if (typeof callback === "function" && !callbacks.includes(callback)) callbacks.push(callback);
    this.__glassListeners.set(String(type), callbacks);
  };
  serviceWorkerContainer.removeEventListener = serviceWorkerContainer.removeEventListener || function(type, callback) {
    const callbacks = this.__glassListeners && this.__glassListeners.get(String(type)) || [];
    if (this.__glassListeners) this.__glassListeners.set(String(type), callbacks.filter(candidate => candidate !== callback));
  };
  serviceWorkerContainer.dispatchEvent = serviceWorkerContainer.dispatchEvent || function(event) {
    const callbacks = this.__glassListeners && this.__glassListeners.get(String(event && event.type)) || [];
    for (const callback of callbacks.slice()) { try { callback.call(this, event); } catch (_) {} }
    const handler = this["on" + String(event && event.type || "")];
    if (typeof handler === "function") { try { handler.call(this, event); } catch (_) {} }
    return true;
  };
  Object.defineProperty(serviceWorkerContainer, "controller", {
    configurable: true, enumerable: true,
    get: () => {
      const registration = serviceWorkerFind(String(host.url));
      return registration && registration.__glassControlled && registration.active || null;
    },
  });
  Object.defineProperty(serviceWorkerContainer, "ready", {
    configurable: true, enumerable: true,
    get: () => Promise.resolve(serviceWorkerFind(String(host.url))),
  });
  serviceWorkerContainer.oncontrollerchange = serviceWorkerContainer.oncontrollerchange || null;
  if (serviceWorkerContainer.onmessage === undefined) serviceWorkerContainer.onmessage = null;
  if (serviceWorkerContainer.onmessageerror === undefined) serviceWorkerContainer.onmessageerror = null;
  serviceWorkerContainer.__glassServiceWorkerContainer = true;
  globalThis.__glassServiceWorkerContainer = serviceWorkerContainer;
  setNavigatorProperty("serviceWorker", serviceWorkerContainer);
  globalThis.__glassResolveServiceWorker = (requestId, payload) => {
    const pending = serviceWorkerPendingRequests.get(Number(requestId));
    if (!pending) return null;
    serviceWorkerPendingRequests.delete(Number(requestId));
    if (payload && payload.error) {
      const message = String(payload.error);
      pending.reject(payload.errorName
        ? new DOMExceptionNative(message, String(payload.errorName))
        : new Error(message));
      return null;
    }
    if (pending.kind === "serviceWorkerNavigationPreload") {
      if (payload && payload.operation === "getState") {
        const state = payload.state;
        if (!state || typeof state.enabled !== "boolean" || typeof state.headerValue !== "string") {
          pending.reject(new Error("native navigation preload state response was invalid"));
          return null;
        }
        pending.resolve({ enabled: state.enabled, headerValue: state.headerValue });
      } else {
        pending.resolve(undefined);
      }
      return null;
    }
    if (pending.kind === "serviceWorkerRegister" || pending.kind === "update") {
      const state = payload && payload.registration;
      if (!state || !state.scope) {
        pending.reject(new Error("native service worker registration response was invalid"));
        return null;
      }
      const scope = String(state.scope);
      let registration = serviceWorkerRegistrations.get(scope);
      if (!registration) {
        registration = serviceWorkerMakeRegistration(state);
        registration.active = null;
        registration.installing = null;
        registration.waiting = null;
        serviceWorkerRegistrations.set(scope, registration);
      }
      serviceWorkerApplyRegistration(registration, state);
      if (pending.kind === "update") pending.resolve(undefined);
      else pending.resolve(registration);
    } else {
      const unregistered = Boolean(payload && payload.unregistered);
      if (unregistered) {
        const requestedScope = String(pending.scope || "");
        const removals = [];
        for (const [scope, registration] of serviceWorkerRegistrations.entries()) {
          if (scope === requestedScope || registration.scope === requestedScope)
            removals.push([scope, registration]);
        }
        for (const [scope, registration] of removals) {
          if (registration.active) serviceWorkerSetState(registration.active, "redundant");
          registration.active = null;
          registration.installing = null;
          registration.waiting = null;
          serviceWorkerRegistrations.delete(scope);
        }
        if (removals.length > 0) serviceWorkerContainer.dispatchEvent({ type: "controllerchange" });
      }
      pending.resolve(unregistered);
    }
    return null;
  };
  globalThis.__glassServiceWorkerRegistrations = serviceWorkerRegistrations;
  globalThis.__glassServiceWorkerPendingRequests = serviceWorkerPendingRequests;
  globalThis.__glassDispatchServiceWorkerClientMessage = (payload) => {
    const registration = serviceWorkerFind(String(host.url));
    const source = registration && registration.active || null;
    const origin = source && source.scriptURL
      ? new URLNative(source.scriptURL).origin
      : "";
    let envelope;
    try {
      envelope = glassMessageDecodeEnvelope(payload || {});
    } catch (_error) {
      const MessageEventConstructor = globalThis.__glassWorkerMessageEventConstructor
        || globalThis.MessageEvent;
      const messageErrorEvent = new MessageEventConstructor("messageerror", {
        data: null,
        origin,
        source,
        ports: [],
        bubbles: false,
        cancelable: false,
      });
      messageErrorEvent.target = serviceWorkerContainer;
      messageErrorEvent.currentTarget = serviceWorkerContainer;
      messageErrorEvent.eventPhase = 2;
      serviceWorkerContainer.dispatchEvent(messageErrorEvent);
      messageErrorEvent.currentTarget = null;
      messageErrorEvent.eventPhase = 0;
      return null;
    }
    if (payload && Array.isArray(payload.object_urls))
      globalThis.__glassInstallObjectUrlTransfers(payload.object_urls);
    const MessageEventConstructor = globalThis.__glassWorkerMessageEventConstructor
      || globalThis.MessageEvent;
    const messageEvent = new MessageEventConstructor("message", {
      data: envelope.data,
      origin,
      source,
      ports: envelope.ports,
      bubbles: false,
      cancelable: false,
    });
    messageEvent.target = serviceWorkerContainer;
    messageEvent.currentTarget = serviceWorkerContainer;
    messageEvent.eventPhase = 2;
    serviceWorkerContainer.dispatchEvent(messageEvent);
    messageEvent.currentTarget = null;
    messageEvent.eventPhase = 0;
    return null;
  };
"###;
