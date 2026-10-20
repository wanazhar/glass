  const beforeUnloadEventReturnValues = globalThis.__glassBeforeUnloadEventReturnValues instanceof WeakMap
    ? globalThis.__glassBeforeUnloadEventReturnValues
    : new WeakMap();
  globalThis.__glassBeforeUnloadEventReturnValues = beforeUnloadEventReturnValues;
  const beforeUnloadDOMString = (value) => {{
    if (typeof value === "symbol") throw new TypeError("Cannot convert a Symbol value to a DOMString");
    return `${{value}}`;
  }};
  const createEvent = (type, options) => {{
    const settings = options && typeof options === "object" ? options : {{}};
    const event = {{
      type: normalizeEventType(type),
      bubbles: Boolean(settings.bubbles),
      cancelable: Boolean(settings.cancelable),
      key: settings.key === undefined ? "" : String(settings.key),
      code: settings.code === undefined ? "" : String(settings.code),
      altKey: Boolean(settings.altKey),
      ctrlKey: Boolean(settings.ctrlKey),
      metaKey: Boolean(settings.metaKey),
      shiftKey: Boolean(settings.shiftKey),
      target: null,
      currentTarget: null,
      eventPhase: 0,
      defaultPrevented: false,
      returnValue: "",
      persisted: Boolean(settings.persisted),
      relatedTarget: settings.relatedTarget === undefined ? null : settings.relatedTarget,
      submitter: settings.submitter === undefined ? null : settings.submitter,
      state: settings.state === undefined ? null : settings.state,
      oldURL: settings.oldURL === undefined ? "" : String(settings.oldURL),
      newURL: settings.newURL === undefined ? "" : String(settings.newURL),
      preventDefault() {{
        if (this.cancelable) this.defaultPrevented = true;
      }},
      stopPropagation() {{ eventState.stopped = true; }},
      stopImmediatePropagation() {{
        eventState.stopped = true;
        eventState.immediate = true;
      }},
    }};
    const eventState = {{ stopped: false, immediate: false, dispatching: false }};
    Object.defineProperty(event, "__glassState", {{
      value: eventState,
      enumerable: false,
      configurable: false,
    }});
    if (typeof globalThis.Event === "function" && globalThis.Event.prototype) {{
      try {{ Object.setPrototypeOf(event, globalThis.Event.prototype); }} catch (_error) {{}}
    }}
    return event;
  }};
  const createBeforeUnloadEvent = (type, options) => {{
    const event = createEvent(type, options);
    beforeUnloadEventReturnValues.set(event, "");
    delete event.returnValue;
    const constructor = globalThis.__glassBeforeUnloadEventConstructor;
    if (typeof constructor === "function" && constructor.prototype) {{
      try {{ Object.setPrototypeOf(event, constructor.prototype); }} catch (_error) {{}}
    }}
    return event;
  }};
  const createFocusEvent = (type, options) => {{
    const settings = options && typeof options === "object" ? options : {{}};
    const event = createEvent(type, options);
    event.view = settings.view === undefined ? globalThis : settings.view;
    event.detail = Number.isFinite(Number(settings.detail)) ? Math.trunc(Number(settings.detail)) : 0;
    if (typeof globalThis.FocusEvent === "function" && globalThis.FocusEvent.prototype) {{
      try {{ Object.setPrototypeOf(event, globalThis.FocusEvent.prototype); }} catch (_error) {{}}
    }}
    return event;
  }};
  const ownerFor = (target) => {{
    if (target && typeof target.__glassEventOwner === "string") return target.__glassEventOwner;
    if (target === globalThis) return "window";
    if (target === document) return "document";
    return "node:" + target.nodeIndex;
  }};
  const eventHandlerKeyFor = (target, type) => listenerKey(ownerFor(target), type);
  const eventHandlerStateFor = (target, type, create = false) => {{
    const key = eventHandlerKeyFor(target, type);
    let state = eventHandlers.get(key);
    if (!state && create) {{
      state = {{ handler: null, active: false, registered: null, contentAttribute: false }};
      state.registered = event => {{
        const handler = state.handler;
        if (!handler) return;
        const currentTarget = event.currentTarget || target;
        const errorEventConstructor = globalThis.__glassErrorEventConstructor || globalThis.ErrorEvent;
        let specialWindowError = false;
        try {{
          specialWindowError = currentTarget === globalThis
            && type === "error"
            && typeof errorEventConstructor === "function"
            && event instanceof errorEventConstructor;
        }} catch (_error) {{}}
        let result;
        if (specialWindowError) {{
          result = handler.call(
            currentTarget,
            event.message || "",
            event.filename || "",
            Number(event.lineno) || 0,
            Number(event.colno) || 0,
            event.error || null,
          );
        }} else {{
          result = handler.call(currentTarget, event);
        }}
        if (specialWindowError && result === true) {{
          event.defaultPrevented = true;
        }} else if (type === "beforeunload" && beforeUnloadEventReturnValues.has(event)) {{
          const returnValue = result === null || result === undefined
            ? null
            : beforeUnloadDOMString(result);
          if (returnValue !== null) {{
            event.defaultPrevented = true;
            if (event.returnValue === "") event.returnValue = returnValue;
          }}
        }} else if (!specialWindowError && result === false) {{
          event.defaultPrevented = true;
        }}
        return result;
      }};
      eventHandlers.set(key, state);
    }}
    return {{ key, state }};
  }};
  const eventHandlerValueFor = (target, type) => {{
    const {{ state }} = eventHandlerStateFor(target, type);
    return state ? state.handler : null;
  }};
  const setEventHandlerValue = (target, type, handler, contentAttribute) => {{
    const {{ key, state: existing }} = eventHandlerStateFor(target, type);
    if (!existing && !handler) return;
    const state = existing || eventHandlerStateFor(target, type, true).state;
    state.handler = handler;
    state.contentAttribute = Boolean(contentAttribute);
    if (handler && !state.active) {{
      addListener(ownerFor(target), type, state.registered, false);
      state.active = true;
    }} else if (!handler && state.active) {{
      removeListener(ownerFor(target), type, state.registered, false);
      state.active = false;
    }}
    if (!handler) eventHandlers.delete(key);
  }};
  const nativeInlineEventType = (name) => {{
    const value = String(name).toLowerCase();
    if (!value.startsWith("on") || value.length <= 2 || !/^[a-z][a-z0-9]*$/.test(value.slice(2))) return null;
    try {{ return normalizeEventType(value.slice(2)); }} catch (_error) {{ return null; }}
  }};
  const nativeWindowInlineEventTargetFor = (element, type, namespaceURI = null) => {{
    if (namespaceURI !== null) return null;
    const target = nativeEventHandlerTargetFor(element, type);
    return target && target !== element ? target : null;
  }};
  const installNativeWindowInlineEventHandler = (type, value, target, records) => {{
    const previous = records.get(type);
    let allowed = true;
    if (typeof globalThis.__glassAllowsInlineEventHandler === "function") {{
      try {{ allowed = Boolean(globalThis.__glassAllowsInlineEventHandler(value)); }} catch (_error) {{ allowed = false; }}
    }}
    if (!allowed) {{
      if (previous && !previous.shared) removeListener(previous.owner, type, previous.registered, false);
      if (previous && previous.shared)
        setEventHandlerValue(previous.target, type, null, false);
      else if (!previous && eventHandlerStateFor(target, type).state?.contentAttribute)
        setEventHandlerValue(target, type, null, false);
      records.set(type, {{
        source: value,
        shared: true,
        target,
        handler: previous && previous.shared && previous.target === target ? previous.handler : null,
      }});
      return;
    }}
    let handler;
    try {{
      handler = type === "error"
        ? Function("event", "source", "lineno", "colno", "error", value)
        : Function("event", value);
    }} catch (_error) {{
      setEventHandlerValue(target, type, null, false);
      records.set(type, {{ source: value, shared: true, target, handler: null }});
      return;
    }}
    if (previous && !previous.shared) removeListener(previous.owner, type, previous.registered, false);
    if (previous && previous.shared && previous.target !== target)
      setEventHandlerValue(previous.target, type, null, false);
    setEventHandlerValue(target, type, handler, true);
    records.set(type, {{ source: value, shared: true, target, handler }});
  }};
  const createNativeWindowInlineAttributeController = (getElement, namespaceForName) => {{
    const records = new Map();
    const targetFor = (name, type) =>
      nativeWindowInlineEventTargetFor(getElement(), type, namespaceForName(name));
    const set = (name, source) => {{
      const type = nativeInlineEventType(name);
      if (!type) return;
      const target = targetFor(name, type);
      if (target) {{
        installNativeWindowInlineEventHandler(
          type,
          String(source === null || source === undefined ? "" : source),
          target,
          records,
        );
        return;
      }}
      const previous = records.get(type);
      if (previous && previous.shared) setEventHandlerValue(previous.target, type, null, false);
      records.delete(type);
    }};
    const remove = (name, hadAttribute = true) => {{
      if (!hadAttribute) return;
      const type = nativeInlineEventType(name);
      if (!type) return;
      const record = records.get(type);
      const target = record && record.shared ? record.target : targetFor(name, type);
      if (target) setEventHandlerValue(target, type, null, false);
      records.delete(type);
    }};
    const refresh = (attributes) => {{
      const current = new Map();
      for (const [name, value] of Object.entries(attributes || {{}})) {{
        const type = nativeInlineEventType(name);
        if (type) current.set(type, {{ name, value: String(value) }});
      }}
      for (const type of records.keys()) {{
        if (!current.has(type)) remove("on" + type);
      }}
      for (const [type, attribute] of current) {{
        const target = targetFor(attribute.name, type);
        const record = records.get(type);
        if (record && record.source === attribute.value && record.shared === Boolean(target)
            && (!target || record.target === target)) continue;
        set(attribute.name, attribute.value);
      }}
    }};
    return {{ set, remove, refresh }};
  }};
  const invokeListeners = (target, event, capture, phase) => {{
    const owner = ownerFor(target);
    const callbacks = (listeners.get(listenerKey(owner, event.type)) || []).slice();
    const eventState = event.__glassState;
    event.currentTarget = target;
    event.eventPhase = phase;
    for (const record of callbacks) {{
      if (record.capture !== capture) continue;
      try {{ record.callback.call(target, event); }} catch (_error) {{}}
      if (record.once) removeListener(owner, event.type, record.callback, capture);
      if (eventState.immediate) break;
    }}
  }};
  const dispatchTarget = (target, event) => {{
    if (!event || typeof event.type !== "string") throw new TypeError("invalid native event");
    normalizeEventType(event.type);
    const eventState = event.__glassState;
    if (!eventState || eventState.dispatching) throw new TypeError("event is already being dispatched");
    eventState.dispatching = true;
    event.target = target;
    const path = [target];
    const ownerDocument = target && target.nodeType === 9
      ? target
      : target && target.ownerDocument
        ? target.ownerDocument
        : null;
    const ownerWindow = ownerDocument && ownerDocument.defaultView
      ? ownerDocument.defaultView
      : globalThis;
    if (target !== ownerWindow && target !== ownerDocument && typeof target.nodeIndex === "number") {{
      let parent = target.parentNode || null;
      while (parent) {{
        path.push(parent);
        if (parent === ownerDocument) break;
        if (typeof parent.parentNode === "undefined") break;
        parent = parent.parentNode;
      }}
      if (path[path.length - 1] === ownerDocument && ownerWindow !== ownerDocument) {{
        path.push(ownerWindow);
      }}
    }} else if (target === ownerDocument && ownerWindow !== ownerDocument) {{
      path.push(ownerWindow);
    }}
    for (let index = path.length - 1; index > 0; index -= 1) {{
      invokeListeners(path[index], event, true, 1);
      if (eventState.stopped || eventState.immediate) break;
    }}
    if (!eventState.immediate) {{
      invokeListeners(target, event, true, 2);
      if (!eventState.immediate) invokeListeners(target, event, false, 2);
    }}
    if (event.bubbles && !eventState.stopped && !eventState.immediate) {{
      for (let index = 1; index < path.length; index += 1) {{
        invokeListeners(path[index], event, false, 3);
        if (eventState.stopped || eventState.immediate) break;
      }}
    }}
    if (event.type === "beforeunload"
        && beforeUnloadEventReturnValues.has(event)
        && event.returnValue !== "") event.defaultPrevented = true;
    event.currentTarget = null;
    event.eventPhase = 0;
    eventState.dispatching = false;
    return !event.defaultPrevented;
  }};
  const nativeTextDecoderBytes = (input) => {{
    if (input === undefined || input === null) return [];
    if (input instanceof ArrayBuffer) return Array.from(new Uint8Array(input));
    if (typeof ArrayBuffer.isView === "function" && ArrayBuffer.isView(input))
      return Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength));
    throw new TypeError("native TextDecoder input must be an ArrayBuffer or view");
  }};
  const TextEncoderNative = typeof globalThis.__glassTextEncoderConstructor === "function"
    ? globalThis.__glassTextEncoderConstructor
    : function TextEncoder() {{
        if (!(this instanceof TextEncoderNative)) throw new TypeError("TextEncoder requires new");
        this.encoding = "utf-8";
      }};
  TextEncoderNative.prototype.encode = function(input) {{
    return new Uint8Array(blobUtf8Bytes(String(input === undefined ? "" : input)));
  }};
  TextEncoderNative.prototype.encodeInto = function(input, destination) {{
    if (!destination || typeof ArrayBuffer.isView !== "function" || !ArrayBuffer.isView(destination)
        || destination.BYTES_PER_ELEMENT === undefined)
      throw new TypeError("native TextEncoder destination must be a typed array");
    const source = String(input === undefined ? "" : input);
    const target = new Uint8Array(destination.buffer, destination.byteOffset, destination.byteLength);
    let read = 0;
    const bytes = [];
    for (const character of Array.from(source)) {{
      const encoded = blobUtf8Bytes(character);
      if (bytes.length + encoded.length > target.length) break;
      bytes.push(...encoded);
      read += character.length;
    }}
    target.set(bytes);
    return {{ read, written: bytes.length }};
  }};
  globalThis.__glassTextEncoderConstructor = TextEncoderNative;
  globalThis.TextEncoder = TextEncoderNative;
  const TextDecoderNative = typeof globalThis.__glassTextDecoderConstructor === "function"
    ? globalThis.__glassTextDecoderConstructor
    : function TextDecoder(label, options) {{
        if (!(this instanceof TextDecoderNative)) throw new TypeError("TextDecoder requires new");
        const normalized = label === undefined ? "utf-8" : String(label).toLowerCase();
        if (!["utf-8", "utf8", "unicode-1-1-utf-8"].includes(normalized))
          throw new RangeError("native TextDecoder only supports UTF-8");
        const settings = options && typeof options === "object" ? options : {{}};
        this.encoding = "utf-8";
        this.fatal = settings.fatal === true;
        this.ignoreBOM = settings.ignoreBOM === true;
      }};
  TextDecoderNative.prototype.decode = function(input) {{
    const bytes = nativeTextDecoderBytes(input);
    if (bytes.length > {native_form_body_bytes}) throw new RangeError("native TextDecoder input exceeds its limit");
    const decoded = utf8TextFromBytes(bytes);
    if (this.fatal && decoded.includes("\ufffd")) throw new TypeError("native TextDecoder encountered invalid UTF-8");
    return decoded;
  }};
  globalThis.__glassTextDecoderConstructor = TextDecoderNative;
  globalThis.TextDecoder = TextDecoderNative;
  const nativeBinaryString = (bytes) => {{
    let value = "";
    for (const byte of bytes) value += String.fromCharCode(byte);
    return value;
  }};
  globalThis.atob = (value) => nativeBinaryString(decodeBase64(String(value), {native_form_body_bytes}));
  globalThis.btoa = (value) => {{
    const text = String(value);
    const bytes = [];
    for (let index = 0; index < text.length; index += 1) {{
      const code = text.charCodeAt(index);
      if (code > 255) throw new TypeError("native btoa input contains a non-Latin-1 character");
      bytes.push(code);
    }}
    return encodeBase64(bytes, {native_form_body_bytes});
  }};
  globalThis.structuredClone = (value, options) => cloneMessageData(value, options);
  let nextEventTargetId = Number.isSafeInteger(globalThis.__glassNextEventTargetId)
    ? globalThis.__glassNextEventTargetId
    : 1;
  const EventTargetNative = typeof globalThis.__glassEventTargetConstructor === "function"
    ? globalThis.__glassEventTargetConstructor
    : function EventTarget() {{
        if (!(this instanceof EventTargetNative)) throw new TypeError("EventTarget requires new");
        const owner = "event-target:" + nextEventTargetId;
        nextEventTargetId += 1;
        globalThis.__glassNextEventTargetId = nextEventTargetId;
        Object.defineProperty(this, "__glassEventOwner", {{
          configurable: false,
          enumerable: false,
          value: owner,
        }});
      }};
  EventTargetNative.prototype.addEventListener = function(type, callback, options) {{
    addListener(ownerFor(this), type, callback, options);
  }};
  EventTargetNative.prototype.removeEventListener = function(type, callback, options) {{
    removeListener(ownerFor(this), type, callback, options);
  }};
  EventTargetNative.prototype.dispatchEvent = function(event) {{
    return dispatchTarget(this, event);
  }};
  globalThis.__glassEventTargetConstructor = EventTargetNative;
  globalThis.EventTarget = EventTargetNative;
  if (typeof globalThis.XMLHttpRequestUpload === "function"
      && typeof globalThis.XMLHttpRequestUpload.prototype === "object") {{
    try {{ Object.setPrototypeOf(globalThis.XMLHttpRequestUpload.prototype, EventTargetNative.prototype); }} catch (_error) {{}}
  }}
  if (typeof globalThis.XMLHttpRequest === "function"
      && typeof globalThis.XMLHttpRequest.prototype === "object") {{
    try {{ Object.setPrototypeOf(globalThis.XMLHttpRequest.prototype, EventTargetNative.prototype); }} catch (_error) {{}}
  }}
