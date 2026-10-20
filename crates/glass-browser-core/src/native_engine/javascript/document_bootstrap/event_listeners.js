  const listeners = globalThis.__glassHostListeners instanceof Map
    ? globalThis.__glassHostListeners
    : new Map();
  globalThis.__glassHostListeners = listeners;
  const eventHandlers = globalThis.__glassHostEventHandlers instanceof Map
    ? globalThis.__glassHostEventHandlers
    : new Map();
  globalThis.__glassHostEventHandlers = eventHandlers;
  const inlineAttributeListeners = globalThis.__glassHostInlineAttributeListeners instanceof Map
    ? globalThis.__glassHostInlineAttributeListeners
    : new Map();
  globalThis.__glassHostInlineAttributeListeners = inlineAttributeListeners;
  const normalizeEventType = (type) => {{
    const value = String(type).toLowerCase();
    if (!value || value.length > 128) throw new TypeError("invalid native event type");
    return value;
  }};
  const listenerKey = (owner, type) => String(owner) + ":" + normalizeEventType(type);
  // Script-created nodes use temporary indexes in listener keys until their
  // first host mutation commits them to the native document arena.
  const rebindNodeEventOwner = (previousIndex, nextIndex) => {{
    if (!Number.isSafeInteger(previousIndex) || !Number.isSafeInteger(nextIndex)
        || previousIndex < 0 || nextIndex < 0 || previousIndex === nextIndex) return;
    const previousOwner = "node:" + previousIndex;
    const nextOwner = "node:" + nextIndex;
    const prefix = previousOwner + ":";
    for (const registry of [listeners, eventHandlers]) {{
      for (const [key, value] of Array.from(registry.entries())) {{
        if (!key.startsWith(prefix)) continue;
        const reboundKey = nextOwner + key.slice(previousOwner.length);
        if (registry.has(reboundKey))
          throw new TypeError("native event owner index collision");
        registry.set(reboundKey, value);
        registry.delete(key);
      }}
    }}
    for (const [key, record] of Array.from(inlineAttributeListeners.entries())) {{
      if (!key.startsWith(prefix)) continue;
      const reboundKey = nextOwner + key.slice(previousOwner.length);
      if (inlineAttributeListeners.has(reboundKey))
        throw new TypeError("native inline event owner index collision");
      inlineAttributeListeners.set(reboundKey, {{ ...record, owner: nextOwner }});
      inlineAttributeListeners.delete(key);
    }}
  }};
  const listenerCount = () => {{
    let count = 0;
    for (const callbacks of listeners.values()) count += callbacks.length;
    return count;
  }};
  const listenerOptions = (options) => {{
    if (options === true) return {{ capture: true, once: false }};
    if (!options || typeof options !== "object") return {{ capture: false, once: false }};
    return {{ capture: Boolean(options.capture), once: Boolean(options.once) }};
  }};
  const normalizeEventListener = (callback) => {{
    if (typeof callback === "function") return callback;
    if (callback && typeof callback.handleEvent === "function") {{
      return event => callback.handleEvent(event);
    }}
    throw new TypeError("event listener must be callable");
  }};
  const addListener = (owner, type, callback, options) => {{
    if (callback === null || callback === undefined) return;
    const listener = normalizeEventListener(callback);
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key) || [];
    const settings = listenerOptions(options);
    if (callbacks.some((record) =>
      (record.listener === callback || record.callback === callback)
        && record.capture === settings.capture
    )) return;
    if (listenerCount() >= {max_listeners}) throw new RangeError("native event listener limit exceeded");
    callbacks.push({{ callback: listener, listener: callback, capture: settings.capture, once: settings.once }});
    listeners.set(key, callbacks);
  }};
  const removeListener = (owner, type, callback, options) => {{
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key);
    if (!callbacks) return;
    const capture = listenerOptions(options).capture;
    const index = callbacks.findIndex((record) =>
      (record.listener === callback || record.callback === callback) && record.capture === capture
    );
    if (index < 0) return;
    callbacks.splice(index, 1);
    if (callbacks.length === 0) listeners.delete(key);
  }};
  const nativeWindowEventHandlerTypes = [
    "afterprint", "beforeprint", "beforeunload", "hashchange", "languagechange",
    "message", "messageerror", "offline", "online", "pagehide", "pagereveal",
    "pageshow", "pageswap", "popstate", "rejectionhandled", "storage",
    "unhandledrejection", "unload",
  ];
  const nativeWindowReflectingBodyEventHandlerTypes = new Set([
    "blur", "error", "focus", "load", "resize", "scroll",
  ]);
  const nativeWindowTargetedBodyEventHandlerTypes = new Set([
    ...nativeWindowEventHandlerTypes,
    ...nativeWindowReflectingBodyEventHandlerTypes,
  ]);
  const nativeEventHandlerTargetFor = (exposedTarget, type) => {{
    if (!exposedTarget || !nativeWindowTargetedBodyEventHandlerTypes.has(type)) return exposedTarget;
    const tagName = String(exposedTarget.tagName || "").toUpperCase();
    if (exposedTarget.namespaceURI !== HTML_NAMESPACE || !["BODY", "FRAMESET"].includes(tagName))
      return exposedTarget;
    const ownerDocument = exposedTarget.ownerDocument;
    return ownerDocument && ownerDocument.defaultView ? ownerDocument.defaultView : null;
  }};
  const installEventHandlerProperty = (element, type, onPrototype = false) => {{
    const targetFor = (receiver) => nativeEventHandlerTargetFor(onPrototype ? receiver : element, type);
    Object.defineProperty(element, "on" + type, {{
      enumerable: true,
      configurable: onPrototype,
      get() {{
        const target = targetFor(this);
        if (!target) return null;
        return eventHandlerValueFor(target, type);
      }},
      set(next) {{
        const target = targetFor(this);
        if (!target) return;
        const handler = typeof next === "function" ? next : null;
        setEventHandlerValue(target, type, handler, false);
      }},
    }});
  }};
