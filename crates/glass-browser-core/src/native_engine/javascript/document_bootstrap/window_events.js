  const EventNative = globalThis.__glassEventConstructor || function Event(type, options) {{
    return globalThis.__glassCreateEvent(type, options);
  }};
  const UIEventNative = globalThis.__glassUIEventConstructor || function UIEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    const settings = options && typeof options === "object" ? options : {{}};
    event.view = settings.view === undefined ? null : settings.view;
    const detail = Number(settings.detail);
    event.detail = Number.isFinite(detail) ? Math.trunc(detail) : 0;
    try {{ Object.setPrototypeOf(event, UIEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const FocusEventNative = globalThis.__glassFocusEventConstructor || function FocusEvent(type, options) {{
    const event = new UIEventNative(type, options);
    const settings = options && typeof options === "object" ? options : {{}};
    event.relatedTarget = settings.relatedTarget === undefined ? null : settings.relatedTarget;
    try {{ Object.setPrototypeOf(event, FocusEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  if (typeof globalThis.__glassUIEventConstructor !== "function") {{
    UIEventNative.prototype = Object.create(EventNative.prototype);
    UIEventNative.prototype.constructor = UIEventNative;
  }}
  if (typeof globalThis.__glassFocusEventConstructor !== "function") {{
    FocusEventNative.prototype = Object.create(UIEventNative.prototype);
    FocusEventNative.prototype.constructor = FocusEventNative;
  }}
  const hasProgressEventConstructor = typeof globalThis.__glassProgressEventConstructor === "function";
  const ProgressEventNative = hasProgressEventConstructor
    ? globalThis.__glassProgressEventConstructor
    : function ProgressEvent(type, options) {{
        const event = globalThis.__glassCreateEvent(type, options);
        const settings = options && typeof options === "object" ? options : {{}};
        event.lengthComputable = settings.lengthComputable === true;
        const loaded = Number(settings.loaded);
        const total = Number(settings.total);
        event.loaded = Number.isFinite(loaded) ? Math.max(0, Math.trunc(loaded)) : 0;
        event.total = Number.isFinite(total) ? Math.max(0, Math.trunc(total)) : 0;
        try {{ Object.setPrototypeOf(event, ProgressEventNative.prototype); }} catch (_error) {{}}
        return event;
      }};
  if (!hasProgressEventConstructor) {{
    ProgressEventNative.prototype = Object.create(EventNative.prototype);
    ProgressEventNative.prototype.constructor = ProgressEventNative;
  }}
  const CustomEventNative = globalThis.__glassCustomEventConstructor || function CustomEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    event.detail = options && typeof options === "object" ? options.detail : undefined;
    try {{ Object.setPrototypeOf(event, CustomEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const StorageEventNative = globalThis.__glassStorageEventConstructor || function StorageEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    event.key = options && options.key !== undefined ? options.key : null;
    event.oldValue = options && options.oldValue !== undefined ? options.oldValue : null;
    event.newValue = options && options.newValue !== undefined ? options.newValue : null;
    event.url = options && options.url !== undefined ? String(options.url) : "";
    event.storageArea = options && options.storageArea !== undefined ? options.storageArea : null;
    try {{ Object.setPrototypeOf(event, StorageEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const ErrorEventNative = globalThis.__glassErrorEventConstructor || function ErrorEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    const settings = options && typeof options === "object" ? options : {{}};
    event.message = settings.message === undefined ? "" : String(settings.message);
    event.filename = settings.filename === undefined ? "" : String(settings.filename);
    event.lineno = Number(settings.lineno) || 0;
    event.colno = Number(settings.colno) || 0;
    event.error = settings.error === undefined ? null : settings.error;
    try {{ Object.setPrototypeOf(event, ErrorEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const BeforeUnloadEventNative = function BeforeUnloadEvent() {{
    throw new TypeError("Illegal constructor");
  }};
  BeforeUnloadEventNative.prototype = Object.create(EventNative.prototype);
  BeforeUnloadEventNative.prototype.constructor = BeforeUnloadEventNative;
  Object.defineProperty(BeforeUnloadEventNative.prototype, "returnValue", {{
    configurable: true,
    enumerable: true,
    get() {{
      if (!beforeUnloadEventReturnValues.has(this)) throw new TypeError("Illegal invocation");
      return beforeUnloadEventReturnValues.get(this);
    }},
    set(value) {{
      if (!beforeUnloadEventReturnValues.has(this)) throw new TypeError("Illegal invocation");
      beforeUnloadEventReturnValues.set(this, beforeUnloadDOMString(value));
    }},
  }});
  if (typeof Symbol === "function" && Symbol.toStringTag) {{
    Object.defineProperty(BeforeUnloadEventNative.prototype, Symbol.toStringTag, {{
      configurable: true,
      value: "BeforeUnloadEvent",
    }});
  }}
  const PromiseRejectionEventNative = globalThis.__glassPromiseRejectionEventConstructor || function PromiseRejectionEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    const settings = options && typeof options === "object" ? options : {{}};
    event.promise = settings.promise === undefined ? null : settings.promise;
    event.reason = settings.reason === undefined ? null : settings.reason;
    try {{ Object.setPrototypeOf(event, PromiseRejectionEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  const SecurityPolicyViolationEventNative = globalThis.__glassSecurityPolicyViolationEventConstructor || function SecurityPolicyViolationEvent(type, options) {{
    const event = globalThis.__glassCreateEvent(type, options);
    const settings = options && typeof options === "object" ? options : {{}};
    event.documentURI = settings.documentURI === undefined ? "" : String(settings.documentURI);
    event.referrer = settings.referrer === undefined ? "" : String(settings.referrer);
    event.blockedURI = settings.blockedURI === undefined ? "" : String(settings.blockedURI);
    event.effectiveDirective = settings.effectiveDirective === undefined ? "" : String(settings.effectiveDirective);
    event.violatedDirective = settings.violatedDirective === undefined
      ? event.effectiveDirective
      : String(settings.violatedDirective);
    event.originalPolicy = settings.originalPolicy === undefined ? "" : String(settings.originalPolicy);
    event.sourceFile = settings.sourceFile === undefined ? "" : String(settings.sourceFile);
    event.sample = settings.sample === undefined ? "" : String(settings.sample);
    event.disposition = settings.disposition === undefined ? "enforce" : String(settings.disposition);
    event.statusCode = Number(settings.statusCode) || 0;
    event.lineNumber = Number(settings.lineNumber) || 0;
    event.columnNumber = Number(settings.columnNumber) || 0;
    try {{ Object.setPrototypeOf(event, SecurityPolicyViolationEventNative.prototype); }} catch (_error) {{}}
    return event;
  }};
  globalThis.__glassEventConstructor = EventNative;
  globalThis.__glassUIEventConstructor = UIEventNative;
  globalThis.__glassFocusEventConstructor = FocusEventNative;
  globalThis.__glassProgressEventConstructor = ProgressEventNative;
  globalThis.__glassCustomEventConstructor = CustomEventNative;
  globalThis.__glassStorageEventConstructor = StorageEventNative;
  globalThis.__glassErrorEventConstructor = ErrorEventNative;
  globalThis.__glassBeforeUnloadEventConstructor = BeforeUnloadEventNative;
  globalThis.__glassPromiseRejectionEventConstructor = PromiseRejectionEventNative;
  globalThis.__glassSecurityPolicyViolationEventConstructor = SecurityPolicyViolationEventNative;
  globalThis.__glassCreateEvent = createEvent;
  globalThis.Event = EventNative;
  globalThis.UIEvent = UIEventNative;
  globalThis.FocusEvent = FocusEventNative;
  globalThis.ProgressEvent = ProgressEventNative;
  globalThis.CustomEvent = CustomEventNative;
  globalThis.StorageEvent = StorageEventNative;
  globalThis.ErrorEvent = ErrorEventNative;
  globalThis.BeforeUnloadEvent = BeforeUnloadEventNative;
  globalThis.PromiseRejectionEvent = PromiseRejectionEventNative;
  globalThis.SecurityPolicyViolationEvent = SecurityPolicyViolationEventNative;
  {message_channel_script}
  try {{ delete globalThis.__glassNativeArrayBufferIsAttached; }} catch (_error) {{}}
  try {{ delete globalThis.__glassNativeDetachArrayBuffer; }} catch (_error) {{}}
  {service_worker_page_script}
  try {{ Object.setPrototypeOf(CustomEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(StorageEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(ErrorEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(PromiseRejectionEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(SecurityPolicyViolationEventNative.prototype, EventNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(document, DocumentNative.prototype); }} catch (_error) {{}}
  try {{ Object.setPrototypeOf(location, LocationNative.prototype); }} catch (_error) {{}}
  for (const element of elements) {{
    try {{ Object.setPrototypeOf(element, elementPrototypeFor(element.tagName)); }} catch (_error) {{}}
    if (!Object.prototype.hasOwnProperty.call(element, "ownerDocument")) {{
      Object.defineProperty(element, "ownerDocument", {{
        enumerable: false,
        configurable: false,
        get() {{ return globalThis.document || null; }},
      }});
    }}
  }}
  nativeCustomElementUpgradeTree(document, true);
  nativeFormAssociatedReconcile(document);
  for (const text of textNodes) {{
    try {{ Object.setPrototypeOf(text, TextNative.prototype); }} catch (_error) {{}}
  }}
  for (const comment of commentNodes) {{
    try {{ Object.setPrototypeOf(comment, CommentNative.prototype); }} catch (_error) {{}}
  }}
  for (const documentType of documentTypeNodes) {{
    try {{ Object.setPrototypeOf(documentType, DocumentTypeNative.prototype); }} catch (_error) {{}}
  }}
  try {{ Object.setPrototypeOf(globalThis, WindowNative.prototype); }} catch (_error) {{}}
  globalThis.self = globalThis;
  globalThis.top = relationshipTop || globalThis;
  globalThis.parent = relationshipParent || globalThis;
  globalThis.frames = globalThis;
  globalThis.length = frameBindings.length;
  Object.defineProperty(globalThis, "frameElement", {{
    configurable: true,
    enumerable: true,
    value: selectedFrameElement,
  }});
  for (const proxy of windowProxyCache.values()) {{
    try {{ Object.setPrototypeOf(proxy, WindowNative.prototype); }} catch (_error) {{}}
  }}
  globalThis.addEventListener = (type, callback, options) => addListener("window", type, callback, options);
  globalThis.removeEventListener = (type, callback, options) => removeListener("window", type, callback, options);
  globalThis.dispatchEvent = (event) => dispatchTarget(globalThis, event);
  if (!Object.prototype.hasOwnProperty.call(globalThis, "onerror")) installEventHandlerProperty(globalThis, "error");
  if (!Object.prototype.hasOwnProperty.call(globalThis, "onunhandledrejection")) installEventHandlerProperty(globalThis, "unhandledrejection");
  if (!Object.prototype.hasOwnProperty.call(globalThis, "onrejectionhandled")) installEventHandlerProperty(globalThis, "rejectionhandled");
  const makeStorageEvent = (descriptor) => {{
    const event = createEvent("storage");
    event.key = descriptor.key === null ? null : String(descriptor.key);
    event.oldValue = descriptor.old_value === null ? null : String(descriptor.old_value);
    event.newValue = descriptor.new_value === null ? null : String(descriptor.new_value);
    event.url = String(descriptor.url || "");
    event.storageArea = descriptor.scope === "session"
      ? globalThis.sessionStorage
      : globalThis.localStorage;
    return event;
  }};
  globalThis.__glassDispatchStorageEvents = (events) => events.map((descriptor) => {{
    const values = descriptor.scope === "session"
      ? globalThis.__glassSessionStorageValues
      : globalThis.__glassLocalStorageValues;
    if (descriptor.key === null) values.clear();
    else if (descriptor.new_value === null) values.delete(String(descriptor.key));
    else values.set(String(descriptor.key), String(descriptor.new_value));
    return dispatchTarget(globalThis, makeStorageEvent(descriptor));
  }});
  globalThis.console = globalThis.console || {{
    log() {{}}, info() {{}}, warn() {{}}, error() {{}}
  }};
  globalThis.__glassQueueResizeObserverChanges = queueResizeObserverChanges;
  globalThis.__glassQueueIntersectionObserverChanges = queueIntersectionObserverChanges;
  {xml_document_script}
  if (Array.isArray(host.storage_events) && host.storage_events.length > 0) {{
    globalThis.__glassDispatchStorageEvents(host.storage_events);
  }}
  if (bootstrapParams.runTimers) {{
    globalThis.__glassRunTimers(host.now_ms);
  }}
  {dynamic_import_bootstrap}
  cachedHostState = state;
  cachedGeometrySnapshot = state.geometry;
  cachedGeometryMap = geometryByIndex;
  if (typeof globalThis.__glassNativeHostViewMark === "function")
    globalThis.__glassNativeHostViewMark("installer_complete");
  }};
}})()
